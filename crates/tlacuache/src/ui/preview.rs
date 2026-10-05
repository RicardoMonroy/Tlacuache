//! Vista previa y detalles de un panel (`docs/UI_SPEC.md` §3): a la
//! izquierda la vista previa (imagen, texto/código o el ícono grande del
//! tipo), a la derecha los detalles.
//!
//! Las imágenes se decodifican y escalan (lado mayor `MAX_DECODE_SIDE`) en
//! un hilo de trabajo con gdk-pixbuf; la textura se crea en el hilo de GTK.
//! Del texto se leen como mucho `[preview] max_text_bytes`, también en un
//! hilo de trabajo. De las carpetas se cuentan los elementos y se mide el
//! tamaño (recursivo, con avance en vivo, cancelable).
//!
//! `request` espera `preview::DEBOUNCE_MS` desde el último cambio de
//! selección; cada carga nueva aborta la anterior (su consulta de gio se
//! cancela al soltar el future).

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use adw::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{gdk, gdk_pixbuf, gio, glib, pango};
use tlacuache_core::age::Age;
use tlacuache_core::config::PreviewPosition;
use tlacuache_core::entry::FileEntry;
use tlacuache_core::filetype::{FileCategory, classify};
use tlacuache_core::perms;
use tlacuache_core::preview::{
    DEBOUNCE_MS, MAX_DECODE_SIDE, PreviewKind, Target, decode_size, format_duration, kind,
    scale_to_side, text_excerpt,
};
use tlacuache_core::summary::SelectionSummary;

use crate::fs::display::display_name;
use crate::fs::file_item::{SelectedInfo, entry_from_info};
use crate::fs::listing::ATTRIBUTES;
use crate::fs::size::{self, Measure};
use crate::strings;
use crate::ui::image_preview::ImagePreview;
use crate::ui::media_preview::MediaPreview;
use crate::ui::pdf_preview::PdfPreview;
use crate::ui::set_category_icon;
use crate::ui::text_preview::TextPreview;

/// Tamaño del ícono de la vista previa genérica.
const ICON_SIZE: i32 = 96;
/// Lado mayor (px) al rasterizar imágenes vectoriales.
const VECTOR_SIDE: i32 = 1024;
/// Alto máximo de los detalles con la vista previa a la derecha.
const DETAILS_MAX_HEIGHT: i32 = 260;
const PAGE_ICON: &str = "icon";
const PAGE_IMAGE: &str = "image";
const PAGE_TEXT: &str = "text";
const PAGE_PDF: &str = "pdf";
const PAGE_MEDIA: &str = "media";
/// Límite de texto si la config no dice otro.
const DEFAULT_MAX_TEXT_BYTES: u64 = 1 << 20;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PreviewPane {
        pub icon: gtk::Image,
        pub title: gtk::Label,
        pub details: gtk::Grid,
        pub details_scroll: gtk::ScrolledWindow,
        /// Páginas: `icon` (genérica) e `image`.
        pub stack: gtk::Stack,
        pub image: ImagePreview,
        pub text: TextPreview,
        pub pdf: PdfPreview,
        pub media: MediaPreview,
        /// `[preview] max_text_bytes`.
        pub max_text_bytes: Cell<u64>,
        /// Carga pendiente del debounce.
        pub timer: RefCell<Option<glib::SourceId>>,
        /// Carga en curso (se aborta al pedir otra).
        pub task: RefCell<Option<glib::JoinHandle<()>>>,
        /// Medición de carpeta en curso.
        pub measuring: RefCell<Option<gio::Cancellable>>,
        /// Filas usadas en `details`.
        pub rows: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreviewPane {
        const NAME: &'static str = "TlacuachePreviewPane";
        type Type = super::PreviewPane;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for PreviewPane {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }

        fn dispose(&self) {
            self.obj().cancel();
        }
    }
    impl WidgetImpl for PreviewPane {}
    impl BoxImpl for PreviewPane {}
}

glib::wrapper! {
    pub struct PreviewPane(ObjectSubclass<imp::PreviewPane>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for PreviewPane {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PreviewPane {
    fn build(&self) {
        let imp = self.imp();
        self.set_orientation(gtk::Orientation::Horizontal);
        self.add_css_class("tl-preview");

        imp.icon.set_pixel_size(ICON_SIZE);
        imp.title.add_css_class("tl-preview-title");
        imp.title.set_wrap(true);
        imp.title.set_wrap_mode(pango::WrapMode::WordChar);
        imp.title.set_justify(gtk::Justification::Center);
        imp.title.set_max_width_chars(30);
        let visual = gtk::Box::new(gtk::Orientation::Vertical, 8);
        visual.add_css_class("tl-preview-visual");
        visual.set_valign(gtk::Align::Center);
        visual.append(&imp.icon);
        visual.append(&imp.title);

        imp.details.add_css_class("tl-preview-details");
        imp.details.set_column_spacing(12);
        imp.details.set_row_spacing(4);
        imp.details.set_valign(gtk::Align::Start);
        let scroll = &imp.details_scroll;
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroll.set_child(Some(&imp.details));

        imp.stack.add_named(&visual, Some(PAGE_ICON));
        imp.stack.add_named(&imp.image, Some(PAGE_IMAGE));
        imp.stack.add_named(&imp.text, Some(PAGE_TEXT));
        imp.stack.add_named(&imp.pdf, Some(PAGE_PDF));
        imp.stack.add_named(&imp.media, Some(PAGE_MEDIA));
        imp.max_text_bytes.set(DEFAULT_MAX_TEXT_BYTES);
        self.append(&imp.stack);
        self.append(scroll);
        self.set_position(PreviewPosition::Bottom);
    }

    /// Abajo: visual y detalles lado a lado. A la derecha (panel estrecho):
    /// visual arriba y detalles debajo, a su altura natural.
    pub fn set_position(&self, position: PreviewPosition) {
        let imp = self.imp();
        let scroll = &imp.details_scroll;
        let right = position == PreviewPosition::Right;
        self.set_orientation(if right {
            gtk::Orientation::Vertical
        } else {
            gtk::Orientation::Horizontal
        });
        imp.stack.set_hexpand(!right);
        imp.stack.set_vexpand(right);
        scroll.set_hexpand(!right);
        scroll.set_vexpand(!right);
        scroll.set_propagate_natural_height(right);
        scroll.set_max_content_height(if right { DETAILS_MAX_HEIGHT } else { -1 });
        if right {
            self.add_css_class("right");
        } else {
            self.remove_css_class("right");
        }
    }

    /// Muestra `target` tras el debounce; `dir` es la carpeta actual (para
    /// `Target::Directory`).
    pub fn request(&self, target: Target<SelectedInfo>, dir: Option<gio::File>) {
        self.cancel();
        let id = glib::timeout_add_local_once(
            Duration::from_millis(DEBOUNCE_MS),
            glib::clone!(
                #[weak(rename_to = pane)]
                self,
                move || {
                    pane.imp().timer.take();
                    pane.load(target, dir);
                }
            ),
        );
        self.imp().timer.replace(Some(id));
    }

    /// Descarta la carga pendiente o en curso (al ocultar o cambiar).
    pub fn cancel(&self) {
        let imp = self.imp();
        if let Some(id) = imp.timer.take() {
            id.remove();
        }
        if let Some(task) = imp.task.take() {
            task.abort();
        }
        if let Some(cancellable) = imp.measuring.take() {
            cancellable.cancel();
        }
        // Nada sigue sonando al cambiar de selección o cerrar la vista previa.
        imp.media.stop();
    }

    fn load(&self, target: Target<SelectedInfo>, dir: Option<gio::File>) {
        let file = match target {
            Target::Many(summary) => {
                self.show_many(summary);
                return;
            }
            Target::Single(info) => info.file,
            Target::Directory => match dir {
                Some(dir) => dir,
                None => {
                    self.clear();
                    return;
                }
            },
        };
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = pane)]
            self,
            async move { pane.load_file(&file).await }
        ));
        self.imp().task.replace(Some(task));
    }

    async fn load_file(&self, file: &gio::File) {
        let attributes = format!("{ATTRIBUTES},unix::mode,owner::user,owner::group");
        let info = file
            .query_info_future(
                &attributes,
                gio::FileQueryInfoFlags::NONE,
                glib::Priority::DEFAULT,
            )
            .await;
        match info {
            Ok(info) => {
                self.show_file(file, &info);
                let content_type = info.content_type().map(|t| t.to_string());
                let name = info.display_name().to_string();
                let is_dir = info.file_type() == gio::FileType::Directory;
                // Solo archivos locales: los remotos se quedan con el ícono.
                match (kind(content_type.as_deref(), &name, is_dir), file.path()) {
                    (PreviewKind::Image, Some(path)) => self.load_image(path).await,
                    (PreviewKind::Text, Some(path)) => {
                        self.load_text(path, &name, content_type.as_deref()).await;
                    }
                    (PreviewKind::Pdf, Some(path)) => self.load_pdf(path).await,
                    (PreviewKind::Video, _) => self.load_media(file, true),
                    (PreviewKind::Audio, _) => self.load_media(file, false),
                    (PreviewKind::Folder, _) => self.measure_folder(file).await,
                    _ => {}
                }
            }
            Err(err) => {
                self.clear();
                self.imp().title.set_text(&display_name(file));
                self.add_row(strings::PROP_ERROR, err.message());
            }
        }
        self.imp().task.take();
    }

    /// Video o audio listo para reproducir (sin empezar). La duración se
    /// añade cuando GStreamer termina de prepararlo.
    fn load_media(&self, file: &gio::File, video: bool) {
        let imp = self.imp();
        let stream = imp.media.open(file, video);
        imp.stack.set_visible_child_name(PAGE_MEDIA);
        stream.connect_prepared_notify(glib::clone!(
            #[weak(rename_to = pane)]
            self,
            move |stream| {
                let media = &pane.imp().media;
                if stream.is_prepared()
                    && media.is_current(stream)
                    && let Some(text) = format_duration(stream.duration())
                {
                    pane.add_row(strings::PROP_DURATION, &text);
                }
            }
        ));
        stream.connect_error_notify(glib::clone!(
            #[weak(rename_to = pane)]
            self,
            move |stream| {
                if let Some(err) = stream.error()
                    && pane.imp().media.is_current(stream)
                {
                    tracing::debug!("vista previa multimedia: {err}");
                    pane.imp().stack.set_visible_child_name(PAGE_ICON);
                    pane.add_row(strings::PROP_ERROR, err.message());
                }
            }
        ));
    }

    /// Primera página del PDF y fila con el número de páginas. Si no se
    /// puede abrir (dañado, con contraseña), se queda el ícono.
    async fn load_pdf(&self, path: PathBuf) {
        let imp = self.imp();
        match imp.pdf.open(path).await {
            Ok(pages) => {
                imp.stack.set_visible_child_name(PAGE_PDF);
                self.add_row(strings::PROP_PAGES, &pages.to_string());
            }
            Err(err) => {
                tracing::debug!("vista previa de PDF: {err}");
                self.add_row(strings::PROP_ERROR, &err);
            }
        }
    }

    /// Cuenta los elementos y mide la carpeta, actualizando las filas.
    async fn measure_folder(&self, dir: &gio::File) {
        let entries = self.add_row(strings::PROP_ITEMS, strings::PROP_CALCULATING);
        let total = self.add_row(strings::PROP_SIZE, strings::PROP_CALCULATING);
        let cancellable = gio::Cancellable::new();
        self.imp().measuring.replace(Some(cancellable.clone()));
        let (live_entries, live_total) = (entries.clone(), total.clone());
        let result = size::measure(
            std::slice::from_ref(dir),
            &cancellable,
            move |m: &Measure| {
                live_entries.set_text(&strings::items_count(m.entries as usize));
                live_total.set_text(&strings::prop_measuring(m.bytes, m.files));
            },
        )
        .await;
        self.imp().measuring.take();
        match result {
            Ok(m) => {
                entries.set_text(&strings::items_count(m.entries as usize));
                total.set_text(&strings::prop_measured(m.bytes, m.files, m.dirs));
            }
            Err(err) if err.matches(gio::IOErrorEnum::Cancelled) => {}
            Err(err) => {
                entries.set_text("—");
                total.set_text(err.message());
            }
        }
    }

    /// Límite de bytes de texto a leer (`[preview] max_text_bytes`).
    pub fn set_max_text_bytes(&self, bytes: u64) {
        self.imp().max_text_bytes.set(bytes.max(1));
    }

    /// Lee el principio del archivo en un hilo de trabajo y lo muestra; si
    /// parece binario se queda el ícono.
    async fn load_text(&self, path: PathBuf, name: &str, content_type: Option<&str>) {
        let max = self.imp().max_text_bytes.get();
        let read = gio::spawn_blocking(move || {
            read_head(&path, max).map(|(bytes, truncated)| {
                text_excerpt(&bytes, truncated).map(|text| (text, truncated))
            })
        })
        .await;
        let (text, truncated) = match read {
            Ok(Ok(Some(excerpt))) => excerpt,
            Ok(Ok(None)) => return,
            Ok(Err(err)) => {
                self.add_row(strings::PROP_ERROR, &err.to_string());
                return;
            }
            Err(_) => return,
        };
        let imp = self.imp();
        imp.text
            .set_text(&text, name, content_type, truncated.then_some(max));
        imp.stack.set_visible_child_name(PAGE_TEXT);
    }

    /// Decodifica en un hilo de trabajo y muestra la imagen. Si se aborta
    /// la tarea, el resultado del hilo se descarta.
    async fn load_image(&self, path: std::path::PathBuf) {
        let decoded = match gio::spawn_blocking(move || decode_image(&path)).await {
            Ok(Ok(decoded)) => decoded,
            Ok(Err(message)) => {
                tracing::debug!("vista previa de imagen: {message}");
                return;
            }
            Err(_) => return,
        };
        let format = if decoded.has_alpha {
            gdk::MemoryFormat::R8g8b8a8
        } else {
            gdk::MemoryFormat::R8g8b8
        };
        let texture = gdk::MemoryTexture::new(
            decoded.width,
            decoded.height,
            format,
            &decoded.bytes,
            decoded.stride,
        );
        let imp = self.imp();
        imp.image.set_texture(Some(texture.upcast_ref()));
        imp.stack.set_visible_child_name(PAGE_IMAGE);
        let (width, height) = decoded.original;
        self.add_row(
            strings::PROP_DIMENSIONS,
            &strings::prop_dimensions(width, height),
        );
    }

    fn show_file(&self, file: &gio::File, info: &gio::FileInfo) {
        let entry = entry_from_info(info);
        self.clear();
        let imp = self.imp();
        set_category_icon(
            &imp.icon,
            classify(
                entry.content_type.as_deref(),
                &entry.name,
                entry.is_dir,
                entry.is_executable,
            ),
        );
        imp.title.set_text(&display_name(file));

        self.add_row(strings::PROP_TYPE, &type_text(&entry));
        if !entry.is_dir {
            self.add_row(strings::PROP_SIZE, &strings::prop_size(entry.size));
        }
        if let Some(modified) = entry.modified {
            self.add_row(strings::PROP_MODIFIED, &date_text(modified));
            let age = Age::between(modified, SystemTime::now());
            self.add_row(strings::PROP_AGE, &strings::age(&age));
        }
        if let Some(created) = entry.created {
            self.add_row(strings::PROP_CREATED, &date_text(created));
        }
        if info.has_attribute(gio::FILE_ATTRIBUTE_UNIX_MODE) {
            let mode = info.attribute_uint32(gio::FILE_ATTRIBUTE_UNIX_MODE);
            let text = format!("{} ({})", perms::mode_string(mode), perms::mode_octal(mode));
            self.add_row(strings::PROP_PERMISSIONS, &text);
        }
        let owner = info.attribute_string(gio::FILE_ATTRIBUTE_OWNER_USER);
        let group = info.attribute_string(gio::FILE_ATTRIBUTE_OWNER_GROUP);
        if let (Some(owner), Some(group)) = (owner, group) {
            self.add_row(strings::PROP_OWNER, &format!("{owner}:{group}"));
        }
    }

    fn show_many(&self, summary: SelectionSummary) {
        self.clear();
        let imp = self.imp();
        let category = if summary.files == 0 {
            FileCategory::Folder
        } else {
            FileCategory::Other
        };
        set_category_icon(&imp.icon, category);
        imp.title
            .set_text(&strings::items_count(summary.count() as usize));
        if summary.dirs > 0 {
            self.add_row(strings::PROP_FOLDERS, &summary.dirs.to_string());
        }
        if summary.files > 0 {
            self.add_row(strings::PROP_FILES, &summary.files.to_string());
            self.add_row(strings::PROP_SIZE, &strings::prop_size(summary.bytes));
        }
    }

    fn clear(&self) {
        let imp = self.imp();
        while let Some(child) = imp.details.first_child() {
            imp.details.remove(&child);
        }
        imp.rows.set(0);
        imp.stack.set_visible_child_name(PAGE_ICON);
        imp.image.set_texture(None);
        imp.text.clear();
        imp.pdf.clear();
        imp.media.stop();
        imp.icon.set_icon_name(None);
        imp.title.set_text("");
    }

    fn add_row(&self, key: &str, value: &str) -> gtk::Label {
        let imp = self.imp();
        let row = imp.rows.get();
        let key = gtk::Label::builder()
            .label(key)
            .xalign(1.0)
            .yalign(0.0)
            .css_classes(["tl-detail-key"])
            .build();
        let value = gtk::Label::builder()
            .label(value)
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(pango::WrapMode::WordChar)
            .selectable(true)
            // Seleccionable con el ratón sin quitarle el foco a la lista.
            .focusable(false)
            .hexpand(true)
            .css_classes(["tl-detail-value"])
            .build();
        imp.details.attach(&key, 0, row, 1, 1);
        imp.details.attach(&value, 1, row, 1, 1);
        imp.rows.set(row + 1);
        value
    }
}

fn type_text(entry: &FileEntry) -> String {
    match &entry.content_type {
        Some(mime) => format!("{} ({mime})", gio::content_type_get_description(mime)),
        None => String::new(),
    }
}

fn date_text(time: SystemTime) -> String {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .and_then(|secs| glib::DateTime::from_unix_local(secs).ok())
        .and_then(|dt| dt.format("%Y-%m-%d %H:%M").ok())
        .map(|text| text.to_string())
        .unwrap_or_default()
}

/// Píxeles de una imagen decodificada (sin `gdk`: `MemoryTexture` solo se
/// crea en el hilo de GTK).
struct Decoded {
    width: i32,
    height: i32,
    has_alpha: bool,
    stride: usize,
    bytes: glib::Bytes,
    /// Dimensiones del archivo.
    original: (i32, i32),
}

/// En un hilo de trabajo: lee, escala y orienta (EXIF) la imagen.
fn decode_image(path: &Path) -> Result<Decoded, String> {
    let (format, width, height) = gdk_pixbuf::Pixbuf::file_info(path)
        .ok_or_else(|| format!("formato no reconocido: {}", path.display()))?;
    // Los vectoriales (SVG) se rasterizan grandes para que se vean nítidos.
    let (w, h) = if format.is_scalable() {
        scale_to_side(width, height, VECTOR_SIDE)
    } else {
        decode_size(width, height, MAX_DECODE_SIDE)
    };
    let pixbuf =
        gdk_pixbuf::Pixbuf::from_file_at_scale(path, w, h, true).map_err(|err| err.to_string())?;
    let pixbuf = pixbuf.apply_embedded_orientation().unwrap_or(pixbuf);
    Ok(Decoded {
        width: pixbuf.width(),
        height: pixbuf.height(),
        has_alpha: pixbuf.has_alpha(),
        stride: usize::try_from(pixbuf.rowstride()).map_err(|err| err.to_string())?,
        bytes: pixbuf.read_pixel_bytes(),
        original: (width, height),
    })
}

/// En un hilo de trabajo: hasta `max` bytes del principio del archivo y si
/// queda más.
fn read_head(path: &Path, max: u64) -> std::io::Result<(Vec<u8>, bool)> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(max.saturating_add(1)).read_to_end(&mut bytes)?;
    let truncated = bytes.len() as u64 > max;
    if truncated {
        bytes.truncate(usize::try_from(max).unwrap_or(usize::MAX));
    }
    Ok((bytes, truncated))
}
