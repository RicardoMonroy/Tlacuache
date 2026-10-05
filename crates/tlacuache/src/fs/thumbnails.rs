//! Miniaturas freedesktop (8.5). Se buscan en la caché compartida
//! (`core::thumbnail`) y, si no hay una válida, se generan en `large` con
//! gdk-pixbuf (imágenes) o poppler (PDF). Todo en dos hilos de trabajo que
//! atienden primero lo último pedido (lo que se ve ahora); el resultado
//! vuelve al hilo de GTK por un canal y se avisa con la señal `ready`.

use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use glib::subclass::Signal;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gdk_pixbuf, glib};
use tlacuache_core::filetype::FileCategory;
use tlacuache_core::thumbnail::{self, FAIL_DIR, GENERATED, ThumbSize};

/// Hilos que generan miniaturas.
const WORKERS: usize = 2;
/// Texturas en memoria antes de vaciar la caché.
const MEMORY_LIMIT: usize = 2000;

/// Una miniatura pedida.
#[derive(Debug, Clone)]
pub struct Job {
    pub key: String,
    pub path: PathBuf,
    pub uri: String,
    pub mtime: u64,
    pub category: FileCategory,
    /// Lado que se va a mostrar (px físicos).
    pub needed: u32,
}

/// Clave de la caché en memoria: misma URI y misma fecha.
pub fn key(uri: &str, mtime: u64) -> String {
    format!("{uri}\n{mtime}")
}

/// Píxeles de una miniatura (sin `gdk`: la textura se crea en el hilo de GTK).
pub struct Raw {
    width: i32,
    height: i32,
    stride: usize,
    has_alpha: bool,
    bytes: Vec<u8>,
}

type Queue = Arc<(Mutex<Vec<Job>>, Condvar)>;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Thumbnailer {
        pub textures: RefCell<HashMap<String, gdk::Texture>>,
        /// Pedidas y aún sin resultado (no se repiten).
        pub pending: RefCell<HashSet<String>>,
        /// No se pudieron generar (no se reintentan en esta sesión).
        pub failed: RefCell<HashSet<String>>,
        pub queue: OnceCell<Queue>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Thumbnailer {
        const NAME: &'static str = "TlacuacheThumbnailer";
        type Type = super::Thumbnailer;
    }

    impl ObjectImpl for Thumbnailer {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("ready")
                        .param_types([String::static_type()])
                        .build(),
                ]
            })
        }
    }
}

glib::wrapper! {
    pub struct Thumbnailer(ObjectSubclass<imp::Thumbnailer>);
}

thread_local! {
    static THUMBNAILER: OnceCell<Thumbnailer> = const { OnceCell::new() };
}

/// El miniaturizador de la app (hilo de GTK).
pub fn get() -> Thumbnailer {
    THUMBNAILER.with(|t| t.get_or_init(glib::Object::new).clone())
}

impl Thumbnailer {
    pub fn connect_ready<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "ready",
            false,
            glib::closure_local!(move |t: &Self, key: String| f(t, &key)),
        )
    }

    /// Textura ya cargada para `key`.
    pub fn lookup(&self, key: &str) -> Option<gdk::Texture> {
        self.imp().textures.borrow().get(key).cloned()
    }

    /// Pide la miniatura de `job` (si no está, ni pedida, ni fallida).
    pub fn request(&self, job: Job) {
        let imp = self.imp();
        if imp.textures.borrow().contains_key(&job.key)
            || imp.failed.borrow().contains(&job.key)
            || !imp.pending.borrow_mut().insert(job.key.clone())
        {
            return;
        }
        let (lock, ready) = &**self.queue();
        if let Ok(mut jobs) = lock.lock() {
            jobs.push(job);
            ready.notify_one();
        }
    }

    /// Olvida lo pedido que aún no empezó (al cambiar de carpeta).
    pub fn clear_queue(&self) {
        let (lock, _) = &**self.queue();
        if let Ok(mut jobs) = lock.lock() {
            let mut pending = self.imp().pending.borrow_mut();
            for job in jobs.drain(..) {
                pending.remove(&job.key);
            }
        }
    }

    /// La cola, con sus hilos y el receptor en el hilo de GTK (al primer uso).
    fn queue(&self) -> &Queue {
        self.imp().queue.get_or_init(|| {
            let queue: Queue = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
            let (tx, rx) = async_channel::unbounded::<(String, Result<Raw, String>)>();
            let cache = glib::user_cache_dir().join("thumbnails");
            for _ in 0..WORKERS {
                let (queue, tx, cache) = (queue.clone(), tx.clone(), cache.clone());
                std::thread::spawn(move || worker(&queue, &tx, &cache));
            }
            glib::spawn_future_local(glib::clone!(
                #[weak(rename_to = thumbnailer)]
                self,
                async move {
                    while let Ok((key, result)) = rx.recv().await {
                        thumbnailer.finish(key, result);
                    }
                }
            ));
            queue
        })
    }

    fn finish(&self, key: String, result: Result<Raw, String>) {
        let imp = self.imp();
        imp.pending.borrow_mut().remove(&key);
        match result {
            Ok(raw) => {
                let format = if raw.has_alpha {
                    gdk::MemoryFormat::R8g8b8a8
                } else {
                    gdk::MemoryFormat::R8g8b8
                };
                let texture = gdk::MemoryTexture::new(
                    raw.width,
                    raw.height,
                    format,
                    &glib::Bytes::from_owned(raw.bytes),
                    raw.stride,
                );
                let mut textures = imp.textures.borrow_mut();
                if textures.len() >= MEMORY_LIMIT {
                    textures.clear();
                }
                textures.insert(key.clone(), texture.upcast());
                drop(textures);
                self.emit_by_name::<()>("ready", &[&key]);
            }
            Err(err) => {
                tracing::debug!("sin miniatura: {err}");
                imp.failed.borrow_mut().insert(key);
            }
        }
    }
}

/// Hilo de trabajo: toma lo último pedido y devuelve el resultado.
fn worker(queue: &Queue, tx: &async_channel::Sender<(String, Result<Raw, String>)>, cache: &Path) {
    let (lock, ready) = &**queue;
    loop {
        let job = {
            let Ok(mut jobs) = lock.lock() else {
                return;
            };
            loop {
                if let Some(job) = jobs.pop() {
                    break job;
                }
                jobs = match ready.wait(jobs) {
                    Ok(jobs) => jobs,
                    Err(_) => return,
                };
            }
        };
        let result = load_or_create(cache, &job);
        if tx.send_blocking((job.key, result)).is_err() {
            return;
        }
    }
}

/// Miniatura válida de la caché o una nueva (que se guarda en ella).
pub fn load_or_create(cache: &Path, job: &Job) -> Result<Raw, String> {
    let md5 =
        glib::compute_checksum_for_string(glib::ChecksumType::Md5, &job.uri).ok_or("sin MD5")?;
    let name = thumbnail::file_name(&md5);
    for size in ThumbSize::sufficient_for(job.needed) {
        if let Some(pixbuf) = valid_thumbnail(&cache.join(size.dir()).join(&name), job) {
            return Ok(raw(&pixbuf));
        }
    }
    let fail = cache.join("fail").join(FAIL_DIR).join(&name);
    if valid_thumbnail(&fail, job).is_some() {
        return Err(format!("{} falló antes", job.uri));
    }
    match generate(job) {
        Ok(pixbuf) => {
            let target = cache.join(GENERATED.dir()).join(&name);
            if let Err(err) = save(&pixbuf, &target, job) {
                tracing::debug!("no se guardó la miniatura: {err}");
            }
            Ok(raw(&pixbuf))
        }
        Err(err) => {
            // Marca de fallo (1×1 con los metadatos) para no reintentar.
            if let Some(marker) =
                gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, true, 8, 1, 1)
            {
                let _ = save(&marker, &fail, job);
            }
            Err(err)
        }
    }
}

fn valid_thumbnail(path: &Path, job: &Job) -> Option<gdk_pixbuf::Pixbuf> {
    let pixbuf = gdk_pixbuf::Pixbuf::from_file(path).ok()?;
    let uri = pixbuf.option("tEXt::Thumb::URI");
    let mtime = pixbuf.option("tEXt::Thumb::MTime");
    thumbnail::is_valid(uri.as_deref(), mtime.as_deref(), &job.uri, job.mtime).then_some(pixbuf)
}

fn generate(job: &Job) -> Result<gdk_pixbuf::Pixbuf, String> {
    let side = GENERATED.pixels();
    match job.category {
        FileCategory::Pdf => pdf_thumbnail(&job.path, side),
        _ => {
            let (_, width, height) = gdk_pixbuf::Pixbuf::file_info(&job.path)
                .ok_or_else(|| format!("formato no reconocido: {}", job.path.display()))?;
            let (w, h) = thumbnail::fit(
                u32::try_from(width).unwrap_or(0),
                u32::try_from(height).unwrap_or(0),
                side,
            );
            let pixbuf = gdk_pixbuf::Pixbuf::from_file_at_scale(
                &job.path,
                i32::try_from(w).unwrap_or(1),
                i32::try_from(h).unwrap_or(1),
                true,
            )
            .map_err(|e| e.to_string())?;
            Ok(pixbuf.apply_embedded_orientation().unwrap_or(pixbuf))
        }
    }
}

/// Primera página con poppler; cairo da BGRA premultiplicado y la
/// miniatura (PNG) se guarda como RGBA.
fn pdf_thumbnail(path: &Path, side: u32) -> Result<gdk_pixbuf::Pixbuf, String> {
    let page = crate::ui::pdf_preview::render_page(path, 0, f64::from(side))?;
    let mut rgba = Vec::with_capacity(page.data.len());
    for row in page.data.chunks(page.stride) {
        let width = usize::try_from(page.width).unwrap_or(0);
        for &[b, g, r, a] in row[..width * 4].as_chunks::<4>().0 {
            let un = |c: u8| {
                if a == 0 {
                    0
                } else {
                    ((u16::from(c) * 255 + u16::from(a) / 2) / u16::from(a)) as u8
                }
            };
            rgba.extend_from_slice(&[un(r), un(g), un(b), a]);
        }
    }
    Ok(gdk_pixbuf::Pixbuf::from_bytes(
        &glib::Bytes::from_owned(rgba),
        gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        page.width,
        page.height,
        page.width * 4,
    ))
}

/// Guarda según la especificación: carpeta 0700, archivo 0600, escritura
/// atómica (temporal + renombrar) y metadatos `Thumb::URI`/`Thumb::MTime`.
fn save(pixbuf: &gdk_pixbuf::Pixbuf, target: &Path, job: &Job) -> Result<(), String> {
    let dir = target.parent().ok_or("ruta sin carpeta")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    let tmp = dir.join(format!(
        ".{}.tlacuache-{}",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("thumb"),
        std::process::id()
    ));
    let mtime = job.mtime.to_string();
    pixbuf
        .savev(
            &tmp,
            "png",
            &[
                ("tEXt::Thumb::URI", job.uri.as_str()),
                ("tEXt::Thumb::MTime", mtime.as_str()),
                ("tEXt::Software", "Tlacuache Browser"),
            ],
        )
        .map_err(|e| e.to_string())?;
    let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    std::fs::rename(&tmp, target).map_err(|e| e.to_string())
}

fn raw(pixbuf: &gdk_pixbuf::Pixbuf) -> Raw {
    Raw {
        width: pixbuf.width(),
        height: pixbuf.height(),
        stride: usize::try_from(pixbuf.rowstride()).unwrap_or(0),
        has_alpha: pixbuf.has_alpha(),
        bytes: pixbuf.read_pixel_bytes().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job_for(path: &Path, mtime: u64, category: FileCategory) -> Job {
        let uri = glib::filename_to_uri(path, None).unwrap().to_string();
        Job {
            key: key(&uri, mtime),
            path: path.to_owned(),
            uri,
            mtime,
            category,
            needed: 96,
        }
    }

    fn png(path: &Path, width: i32, height: i32) {
        let pixbuf =
            gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, false, 8, width, height).unwrap();
        pixbuf.fill(0x88c0d0ff);
        pixbuf.savev(path, "png", &[]).unwrap();
    }

    #[test]
    fn creates_once_then_reuses_and_regenerates_when_modified() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        let image = dir.path().join("foto grande.png");
        png(&image, 1000, 500);

        let job = job_for(&image, 100, FileCategory::Image);
        let first = load_or_create(&cache, &job).unwrap();
        assert_eq!((first.width, first.height), (256, 128));

        let md5 = glib::compute_checksum_for_string(glib::ChecksumType::Md5, &job.uri).unwrap();
        let stored = cache.join("large").join(thumbnail::file_name(&md5));
        let meta = std::fs::metadata(&stored).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        let saved = gdk_pixbuf::Pixbuf::from_file(&stored).unwrap();
        assert_eq!(
            saved.option("tEXt::Thumb::URI").as_deref(),
            Some(job.uri.as_str())
        );
        assert_eq!(saved.option("tEXt::Thumb::MTime").as_deref(), Some("100"));

        // Segunda vez: se lee de la caché sin reescribir el archivo.
        let modified = meta.modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        load_or_create(&cache, &job).unwrap();
        assert_eq!(
            std::fs::metadata(&stored).unwrap().modified().unwrap(),
            modified
        );

        // Otra fecha: deja de ser válida y se regenera.
        let newer = job_for(&image, 200, FileCategory::Image);
        load_or_create(&cache, &newer).unwrap();
        let saved = gdk_pixbuf::Pixbuf::from_file(&stored).unwrap();
        assert_eq!(saved.option("tEXt::Thumb::MTime").as_deref(), Some("200"));
    }

    #[test]
    fn small_images_are_not_enlarged_and_failures_are_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        let small = dir.path().join("icono.png");
        png(&small, 40, 30);
        let thumb = load_or_create(&cache, &job_for(&small, 1, FileCategory::Image)).unwrap();
        assert_eq!((thumb.width, thumb.height), (40, 30));

        let broken = dir.path().join("roto.png");
        std::fs::write(&broken, b"no es png").unwrap();
        let job = job_for(&broken, 1, FileCategory::Image);
        assert!(load_or_create(&cache, &job).is_err());
        let md5 = glib::compute_checksum_for_string(glib::ChecksumType::Md5, &job.uri).unwrap();
        assert!(
            cache
                .join("fail")
                .join(FAIL_DIR)
                .join(thumbnail::file_name(&md5))
                .exists()
        );
        let Err(again) = load_or_create(&cache, &job) else {
            panic!("debió fallar");
        };
        assert!(again.contains("falló antes"), "{again}");
    }
}
