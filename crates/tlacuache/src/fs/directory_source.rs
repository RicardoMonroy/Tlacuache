//! Listado de una carpeta (R.2/R.3, ADR-017); reemplaza a `gtk::DirectoryList`.
//!
//! - **Carga**: lee la carpeta de forma asíncrona y por lotes en un
//!   `gio::ListStore` de `FileItem`; la vista se llena mientras llegan.
//! - **Resincronización**: vuelve a leer la carpeta entera y aplica solo las
//!   diferencias (`core::listing_diff`): bajas, altas y cambios en el sitio.
//!   Las filas que no cambian siguen siendo los mismos objetos, así que la
//!   selección y el desplazamiento se conservan.
//! - **Monitor propio**: cualquier evento de la carpeta (también `CHANGED`,
//!   que `DirectoryList` ignoraba) programa una resincronización tras una
//!   pausa corta; así una descarga aparece sola y su tamaño crece en la vista.
//! - **Revisión periódica** (R.4): las carpetas que no avisan (sin monitor,
//!   remotas o FUSE, como OneDrive) se resincronizan cada pocos segundos,
//!   solo mientras su vista está en pantalla (`set_visible`).
//!
//! Con `RUST_LOG=tlacuache=debug` se registran los eventos del monitor y lo
//! que cambia en cada resincronización (diagnóstico de R.1).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::entry::FileEntry;
use tlacuache_core::folder_watch::{needs_polling, poll_interval};
use tlacuache_core::listing_diff::diff_listing;

use super::file_item::{FileItem, entry_from_info};
use super::listing::ATTRIBUTES;

/// Entradas por lote al leer la carpeta.
const BATCH: i32 = 200;
/// Pausa entre un aviso del monitor y la resincronización. Los avisos que
/// llegan mientras tanto se juntan en la misma lectura (una descarga que
/// crece se actualiza a este ritmo).
const RESYNC_DELAY: Duration = Duration::from_millis(300);

mod imp {
    use super::*;

    #[derive(glib::Properties)]
    #[properties(wrapper_type = super::DirectorySource)]
    pub struct DirectorySource {
        /// Leyendo la carpeta por primera vez.
        #[property(get)]
        pub loading: Cell<bool>,
        /// Error de la última carga.
        #[property(get, nullable)]
        pub error: RefCell<Option<glib::Error>>,
        pub store: gio::ListStore,
        pub file: RefCell<Option<gio::File>>,
        /// Carga o resincronización en curso.
        pub task: RefCell<Option<glib::JoinHandle<()>>>,
        pub busy: Cell<bool>,
        /// Hubo avisos durante la lectura en curso: hay que volver a leer.
        pub dirty: Cell<bool>,
        pub monitor: RefCell<Option<gio::FileMonitor>>,
        pub resync_timer: RefCell<Option<glib::SourceId>>,
        /// La carpeta no avisa de sus cambios: se revisa periódicamente.
        pub polled: Cell<bool>,
        /// Alguna vista la muestra en pantalla.
        pub visible: Cell<bool>,
        pub poll_timer: RefCell<Option<glib::SourceId>>,
        /// Lo que tardó la última lectura (fija el ritmo de revisión).
        pub last_read: Cell<Duration>,
    }

    impl Default for DirectorySource {
        fn default() -> Self {
            Self {
                loading: Cell::default(),
                error: RefCell::default(),
                store: gio::ListStore::new::<FileItem>(),
                file: RefCell::default(),
                task: RefCell::default(),
                busy: Cell::default(),
                dirty: Cell::default(),
                monitor: RefCell::default(),
                resync_timer: RefCell::default(),
                polled: Cell::default(),
                visible: Cell::default(),
                poll_timer: RefCell::default(),
                last_read: Cell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DirectorySource {
        const NAME: &'static str = "TlacuacheDirectorySource";
        type Type = super::DirectorySource;
    }

    #[glib::derived_properties]
    impl ObjectImpl for DirectorySource {
        fn dispose(&self) {
            self.obj().stop();
        }
    }
}

glib::wrapper! {
    pub struct DirectorySource(ObjectSubclass<imp::DirectorySource>);
}

impl DirectorySource {
    pub fn new(dir: &gio::File) -> Self {
        let source: Self = glib::Object::new();
        source.set_file(dir);
        source
    }

    /// Los `FileItem` de la carpeta, sin filtrar ni ordenar.
    pub fn store(&self) -> &gio::ListStore {
        &self.imp().store
    }

    pub fn file(&self) -> Option<gio::File> {
        self.imp().file.borrow().clone()
    }

    /// Muestra otra carpeta: descarta la lectura en curso y empieza de cero.
    pub fn set_file(&self, dir: &gio::File) {
        self.stop();
        let imp = self.imp();
        self.store().remove_all();
        imp.file.replace(Some(dir.clone()));
        if imp.error.replace(None).is_some() {
            self.notify_error();
        }
        self.watch(dir);
        self.load(dir.clone());
    }

    /// Vuelve a leer la carpeta y aplica las diferencias (Ctrl+R y avisos
    /// del monitor). Si ya hay una lectura en curso, se repite al terminar.
    pub fn resync(&self) {
        let imp = self.imp();
        if imp.busy.get() {
            imp.dirty.set(true);
            return;
        }
        let Some(dir) = self.file() else {
            return;
        };
        imp.busy.set(true);
        let handle = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = source)]
            self,
            async move {
                let started = Instant::now();
                let mut fresh = Vec::new();
                match read_dir(&dir, |batch| fresh.extend(batch)).await {
                    Ok(()) => source.apply(fresh),
                    // Se repite con el siguiente aviso, revisión o recarga;
                    // la vista conserva lo que tenía. En carpetas remotas
                    // (sin red, p. ej.) no se llena el registro de avisos.
                    Err(err) if source.imp().polled.get() => {
                        tracing::debug!("no se pudo releer {}: {err}", dir.uri());
                    }
                    Err(err) => tracing::warn!("no se pudo releer {}: {err}", dir.uri()),
                }
                source.imp().last_read.set(started.elapsed());
                source.finish_task();
            }
        ));
        imp.task.replace(Some(handle));
    }

    /// Primera lectura: los lotes se añaden según llegan.
    fn load(&self, dir: gio::File) {
        let imp = self.imp();
        imp.busy.set(true);
        imp.loading.set(true);
        self.notify_loading();
        let handle = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = source)]
            self,
            async move {
                let started = Instant::now();
                let store = source.store().clone();
                let result = read_dir(&dir, |batch| {
                    let items: Vec<FileItem> = batch.into_iter().map(FileItem::from_info).collect();
                    store.extend_from_slice(&items);
                })
                .await;
                if let Err(err) = result {
                    source.imp().error.replace(Some(err));
                    source.notify_error();
                }
                source.imp().last_read.set(started.elapsed());
                source.imp().loading.set(false);
                source.notify_loading();
                source.finish_task();
            }
        ));
        imp.task.replace(Some(handle));
    }

    /// Fin de una lectura: si llegaron avisos mientras tanto, otra.
    fn finish_task(&self) {
        let imp = self.imp();
        imp.task.replace(None);
        imp.busy.set(false);
        if imp.dirty.replace(false) {
            self.resync();
        }
        self.update_polling();
    }

    /// Alguna vista muestra (o dejó de mostrar) la carpeta en pantalla. Al
    /// volver a verse, una carpeta revisada periódicamente se lee enseguida.
    pub fn set_visible(&self, visible: bool) {
        let imp = self.imp();
        if imp.visible.replace(visible) == visible {
            return;
        }
        if visible && imp.polled.get() {
            self.resync();
        }
        self.update_polling();
    }

    /// Programa la siguiente revisión si la carpeta lo necesita y se ve; si
    /// no, quita la programada. Con una lectura en curso, se programa al
    /// terminar (`finish_task`).
    fn update_polling(&self) {
        let imp = self.imp();
        if !(imp.polled.get() && imp.visible.get()) {
            if let Some(timer) = imp.poll_timer.take() {
                timer.remove();
            }
            return;
        }
        if imp.poll_timer.borrow().is_some() || imp.busy.get() {
            return;
        }
        let timer = glib::timeout_add_local_once(
            poll_interval(imp.last_read.get()),
            glib::clone!(
                #[weak(rename_to = source)]
                self,
                move || {
                    source.imp().poll_timer.replace(None);
                    source.resync();
                }
            ),
        );
        imp.poll_timer.replace(Some(timer));
    }

    /// Decide si `dir` se revisa periódicamente según su sistema de archivos
    /// (consulta asíncrona; `monitored`: se pudo crear su monitor).
    fn check_polling(&self, dir: &gio::File, monitored: bool) {
        let dir = dir.clone();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = source)]
            self,
            async move {
                let attributes = format!(
                    "{},{}",
                    gio::FILE_ATTRIBUTE_FILESYSTEM_TYPE,
                    gio::FILE_ATTRIBUTE_FILESYSTEM_REMOTE
                );
                let info = dir
                    .query_filesystem_info_future(&attributes, glib::Priority::DEFAULT)
                    .await;
                // Se cambió de carpeta mientras tanto.
                if !source.file().is_some_and(|f| f.equal(&dir)) {
                    return;
                }
                let (remote, fs_type) = match &info {
                    Ok(info) => (
                        info.boolean(gio::FILE_ATTRIBUTE_FILESYSTEM_REMOTE),
                        info.attribute_string(gio::FILE_ATTRIBUTE_FILESYSTEM_TYPE)
                            .map(|t| t.to_string()),
                    ),
                    Err(_) => (false, None),
                };
                let polled = needs_polling(monitored, remote, fs_type.as_deref());
                if polled {
                    tracing::debug!(
                        "revisión periódica de {} (tipo {fs_type:?}, remoto {remote}, monitor {monitored})",
                        source.label()
                    );
                }
                source.imp().polled.set(polled);
                source.update_polling();
            }
        ));
    }

    /// Aplica la lectura nueva `fresh` al store: cambios en el sitio, bajas
    /// y altas.
    fn apply(&self, fresh: Vec<gio::FileInfo>) {
        let store = self.store();
        let current: Vec<FileItem> = (0..store.n_items())
            .filter_map(|i| store.item(i).and_downcast())
            .collect();
        let current_keys: Vec<(PathBuf, FileEntry)> = current
            .iter()
            .map(|item| (item.key().unwrap_or_default(), item.entry().clone()))
            .collect();
        let fresh_keys: Vec<(PathBuf, FileEntry)> = fresh
            .iter()
            .map(|info| (info.name(), entry_from_info(info)))
            .collect();
        let diff = diff_listing(&current_keys, &fresh_keys);
        if diff.is_empty() {
            return;
        }
        if tracing::enabled!(tracing::Level::DEBUG) {
            let names = |keys: &[(PathBuf, FileEntry)], idx: &mut dyn Iterator<Item = usize>| {
                idx.map(|i| keys[i].1.name.clone()).collect::<Vec<_>>()
            };
            tracing::debug!(
                "resincronizado {}: -{:?} +{:?} ~{:?}",
                self.label(),
                names(&current_keys, &mut diff.removed.iter().copied()),
                names(&fresh_keys, &mut diff.added.iter().copied()),
                names(&current_keys, &mut diff.changed.iter().map(|&(i, _)| i)),
            );
        }
        // Primero los cambios: las posiciones aún son las del listado actual.
        // Avisar del mismo objeto (-1 +1) hace que el filtro y el orden lo
        // reubiquen sin perder la selección; `changed` repinta sus celdas.
        for &(i, j) in &diff.changed {
            current[i].update(fresh[j].clone());
            let position = u32::try_from(i).unwrap_or(u32::MAX);
            store.items_changed(position, 1, 1);
        }
        for &i in diff.removed.iter().rev() {
            store.remove(u32::try_from(i).unwrap_or(u32::MAX));
        }
        let added: Vec<FileItem> = diff
            .added
            .iter()
            .map(|&j| FileItem::from_info(fresh[j].clone()))
            .collect();
        store.extend_from_slice(&added);
    }

    /// Vigila `dir`; cada aviso programa una resincronización.
    fn watch(&self, dir: &gio::File) {
        let monitor = match dir.monitor_directory(
            gio::FileMonitorFlags::WATCH_MOVES,
            None::<&gio::Cancellable>,
        ) {
            Ok(monitor) => monitor,
            Err(err) => {
                tracing::debug!("sin monitor para {}: {err}", dir.uri());
                self.check_polling(dir, false);
                return;
            }
        };
        self.check_polling(dir, true);
        monitor.connect_changed(glib::clone!(
            #[weak(rename_to = source)]
            self,
            move |_, file, other, event| {
                if tracing::enabled!(tracing::Level::DEBUG) {
                    let name = |f: &gio::File| f.basename().unwrap_or_default();
                    match other {
                        Some(other) => tracing::debug!(
                            "monitor {}: {event:?} {:?} → {:?}",
                            source.label(),
                            name(file),
                            name(other)
                        ),
                        None => {
                            tracing::debug!(
                                "monitor {}: {event:?} {:?}",
                                source.label(),
                                name(file)
                            )
                        }
                    }
                }
                source.schedule_resync();
            }
        ));
        self.imp().monitor.replace(Some(monitor));
    }

    /// Junta los avisos de los próximos `RESYNC_DELAY` en una sola lectura.
    fn schedule_resync(&self) {
        let imp = self.imp();
        if imp.resync_timer.borrow().is_some() {
            return;
        }
        let timer = glib::timeout_add_local_once(
            RESYNC_DELAY,
            glib::clone!(
                #[weak(rename_to = source)]
                self,
                move || {
                    source.imp().resync_timer.replace(None);
                    source.resync();
                }
            ),
        );
        imp.resync_timer.replace(Some(timer));
    }

    /// Detiene la lectura en curso, el monitor y la resincronización
    /// programada.
    fn stop(&self) {
        let imp = self.imp();
        if let Some(task) = imp.task.take() {
            task.abort();
        }
        imp.busy.set(false);
        imp.dirty.set(false);
        if imp.loading.replace(false) {
            self.notify_loading();
        }
        if let Some(monitor) = imp.monitor.take() {
            monitor.cancel();
        }
        if let Some(timer) = imp.resync_timer.take() {
            timer.remove();
        }
        imp.polled.set(false);
        if let Some(timer) = imp.poll_timer.take() {
            timer.remove();
        }
    }

    /// Ruta de la carpeta para el registro.
    fn label(&self) -> String {
        self.file()
            .map(|dir| {
                dir.path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| dir.uri().to_string())
            })
            .unwrap_or_default()
    }
}

/// Lee `dir` de forma asíncrona y entrega sus entradas por lotes, cada una
/// con su `gio::File` en `standard::file` (lo usa `FileItem::file`).
async fn read_dir(
    dir: &gio::File,
    mut on_batch: impl FnMut(Vec<gio::FileInfo>),
) -> Result<(), glib::Error> {
    let enumerator = dir
        .enumerate_children_future(
            ATTRIBUTES,
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await?;
    loop {
        let infos = enumerator
            .next_files_future(BATCH, glib::Priority::DEFAULT)
            .await?;
        if infos.is_empty() {
            break;
        }
        for info in &infos {
            info.set_attribute_object("standard::file", &enumerator.child(info));
        }
        on_batch(infos);
    }
    let _ = enumerator.close_future(glib::Priority::DEFAULT).await;
    Ok(())
}
