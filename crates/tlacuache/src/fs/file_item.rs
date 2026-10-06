//! `FileItem`: GObject por fila que cachea el `FileEntry` de core junto al
//! `gio::FileInfo` original, para no reconstruirlo en cada comparación.

use std::cell::{Ref, RefCell};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::entry::FileEntry;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FileItem {
        pub entry: RefCell<FileEntry>,
        pub info: RefCell<Option<gio::FileInfo>>,
        /// Archivo, si el elemento no viene del listado de una carpeta.
        pub file: RefCell<Option<gio::File>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileItem {
        const NAME: &'static str = "TlacuacheFileItem";
        type Type = super::FileItem;
    }

    impl ObjectImpl for FileItem {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                // Se actualizó en el sitio (la carpeta se resincronizó): las
                // celdas que lo muestran se vuelven a pintar.
                vec![glib::subclass::Signal::builder("changed").build()]
            })
        }
    }
}

glib::wrapper! {
    pub struct FileItem(ObjectSubclass<imp::FileItem>);
}

impl FileItem {
    pub fn from_info(info: gio::FileInfo) -> Self {
        let item: Self = glib::Object::new();
        item.imp().entry.replace(entry_from_info(&info));
        item.imp().info.replace(Some(info));
        item
    }

    /// Elemento armado fuera de un listado (resultados de búsqueda).
    pub fn from_entry(entry: FileEntry, file: gio::File) -> Self {
        let item: Self = glib::Object::new();
        item.imp().entry.replace(entry);
        item.imp().file.replace(Some(file));
        item
    }

    /// Reemplaza sus datos por los de `info` (mismo archivo, leído de
    /// nuevo) y avisa con `changed`.
    pub fn update(&self, info: gio::FileInfo) {
        self.imp().entry.replace(entry_from_info(&info));
        self.imp().info.replace(Some(info));
        self.emit_by_name::<()>("changed", &[]);
    }

    pub fn connect_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "changed",
            false,
            glib::closure_local!(move |item: &Self| f(item)),
        )
    }

    /// Nombre real del archivo (clave única dentro de su carpeta).
    pub fn key(&self) -> Option<PathBuf> {
        self.imp().info.borrow().as_ref().map(|info| info.name())
    }

    pub fn entry(&self) -> Ref<'_, FileEntry> {
        self.imp().entry.borrow()
    }

    pub fn info(&self) -> Option<gio::FileInfo> {
        self.imp().info.borrow().clone()
    }

    /// Archivo de la entrada; el listado lo guarda en `standard::file`.
    pub fn file(&self) -> Option<gio::File> {
        if let Some(file) = self.imp().file.borrow().clone() {
            return Some(file);
        }
        self.imp()
            .info
            .borrow()
            .as_ref()?
            .attribute_object("standard::file")
            .and_downcast()
    }
}

/// Un elemento seleccionado con lo que hace falta para su menú contextual.
#[derive(Clone, Debug)]
pub struct SelectedInfo {
    pub file: gio::File,
    pub is_dir: bool,
    pub content_type: Option<String>,
}

impl SelectedInfo {
    pub fn from_item(item: &FileItem) -> Option<Self> {
        let entry = item.entry();
        Some(Self {
            file: item.file()?,
            is_dir: entry.is_dir,
            content_type: entry.content_type.clone(),
        })
    }
}

pub(crate) fn entry_from_info(info: &gio::FileInfo) -> FileEntry {
    let name = info.display_name().to_string();
    let mut entry = FileEntry::new(name, info.file_type() == gio::FileType::Directory);
    entry.is_hidden |= info.is_hidden();
    entry.is_symlink = info.is_symlink();
    entry.is_executable = info.boolean(gio::FILE_ATTRIBUTE_ACCESS_CAN_EXECUTE);
    entry.size = if entry.is_dir {
        0
    } else {
        u64::try_from(info.size()).unwrap_or(0)
    };
    entry.modified = to_system_time(info.modification_date_time());
    entry.created = created_time(info);
    entry.content_type = info.content_type().map(|t| t.to_string());
    entry
}

/// `time::created` directo: `FileInfo::creation_date_time` exige gio >= 2.70
/// como feature de la crate.
fn created_time(info: &gio::FileInfo) -> Option<SystemTime> {
    if !info.has_attribute(gio::FILE_ATTRIBUTE_TIME_CREATED) {
        return None;
    }
    let secs = info.attribute_uint64(gio::FILE_ATTRIBUTE_TIME_CREATED);
    let usec = info.attribute_uint32(gio::FILE_ATTRIBUTE_TIME_CREATED_USEC);
    SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_secs(secs))?
        .checked_add(Duration::from_micros(u64::from(usec)))
}

fn to_system_time(dt: Option<glib::DateTime>) -> Option<SystemTime> {
    let dt = dt?;
    let secs = dt.to_unix();
    let base = if secs >= 0 {
        SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(secs.unsigned_abs()))
    } else {
        SystemTime::UNIX_EPOCH.checked_sub(Duration::from_secs(secs.unsigned_abs()))
    }?;
    let micros = u64::try_from(dt.microsecond()).unwrap_or(0);
    base.checked_add(Duration::from_micros(micros))
}
