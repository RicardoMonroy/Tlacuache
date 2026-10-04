//! `FileItem`: GObject por fila que cachea el `FileEntry` de core junto al
//! `gio::FileInfo` original, para no reconstruirlo en cada comparación.

use std::cell::{Ref, RefCell};
use std::time::{Duration, SystemTime};

use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::entry::FileEntry;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FileItem {
        pub entry: RefCell<FileEntry>,
        pub info: RefCell<Option<gio::FileInfo>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileItem {
        const NAME: &'static str = "TlacuacheFileItem";
        type Type = super::FileItem;
    }

    impl ObjectImpl for FileItem {}
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

    pub fn entry(&self) -> Ref<'_, FileEntry> {
        self.imp().entry.borrow()
    }

    pub fn info(&self) -> Option<gio::FileInfo> {
        self.imp().info.borrow().clone()
    }
}

fn entry_from_info(info: &gio::FileInfo) -> FileEntry {
    let name = info.display_name().to_string();
    let mut entry = FileEntry::new(name, info.file_type() == gio::FileType::Directory);
    entry.is_hidden |= info.is_hidden();
    entry.is_symlink = info.is_symlink();
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
