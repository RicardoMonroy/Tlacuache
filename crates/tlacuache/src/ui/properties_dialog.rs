//! Diálogo de propiedades de uno o varios elementos. Los datos se consultan
//! de forma asíncrona; el tamaño de las carpetas se calcula en segundo plano
//! (con avance en vivo) y se cancela al cerrar el diálogo.

use std::cell::OnceCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::perms;

use crate::fs::display::{display_name, display_path};
use crate::fs::size::{self, Measure};
use crate::strings;

const ATTRS: &str = "standard::*,time::*,unix::mode,owner::user,owner::group";
const WIDTH: i32 = 480;
const MAX_HEIGHT: i32 = 640;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PropertiesDialog {
        pub group: adw::PreferencesGroup,
        pub cancellable: gio::Cancellable,
        pub size_row: OnceCell<adw::ActionRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PropertiesDialog {
        const NAME: &'static str = "TlacuachePropertiesDialog";
        type Type = super::PropertiesDialog;
        type ParentType = adw::Dialog;
    }

    impl ObjectImpl for PropertiesDialog {}
    impl WidgetImpl for PropertiesDialog {}
    impl AdwDialogImpl for PropertiesDialog {
        fn closed(&self) {
            // Detener el cálculo de tamaño si sigue en curso.
            self.cancellable.cancel();
            self.parent_closed();
        }
    }
}

glib::wrapper! {
    pub struct PropertiesDialog(ObjectSubclass<imp::PropertiesDialog>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl PropertiesDialog {
    pub fn new(files: Vec<gio::File>) -> Self {
        let dialog: Self = glib::Object::new();
        dialog.build(files);
        dialog
    }

    fn build(&self, files: Vec<gio::File>) {
        let imp = self.imp();
        let title = match files.as_slice() {
            [one] => display_name(one),
            many => strings::items_count(many.len()),
        };
        self.set_title(&title);

        // El diálogo toma el tamaño del contenido (`PreferencesPage` no
        // propaga su altura y lo dejaba diminuto); solo hay scroll si no
        // cabe en `MAX_HEIGHT`.
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.set_margin_top(12);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(&imp.group);
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(MAX_HEIGHT)
            .width_request(WIDTH)
            .child(&content)
            .build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&scrolled));
        self.set_follows_content_size(true);
        self.set_child(Some(&toolbar));

        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                match files.as_slice() {
                    [one] => dialog.fill_single(one).await,
                    _ => dialog.fill_many(&files).await,
                }
            }
        ));
    }

    fn row(&self, title: &str, value: &str) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(value)
            .subtitle_selectable(true)
            .build();
        row.add_css_class("property");
        self.imp().group.add(&row);
        row
    }

    async fn fill_single(&self, file: &gio::File) {
        let info = match file
            .query_info_future(
                ATTRS,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                glib::Priority::DEFAULT,
            )
            .await
        {
            Ok(info) => info,
            Err(err) => {
                self.row(strings::PROP_ERROR, err.message());
                return;
            }
        };
        let is_dir = info.file_type() == gio::FileType::Directory;

        self.row(strings::PROP_NAME, &info.display_name());
        let kind = if info.is_symlink() {
            let target = info
                .symlink_target()
                .map(|t| t.display().to_string())
                .unwrap_or_default();
            strings::prop_symlink(&target)
        } else {
            info.content_type()
                .map(|t| format!("{} ({t})", gio::content_type_get_description(&t)))
                .unwrap_or_default()
        };
        self.row(strings::PROP_TYPE, &kind);
        if let Some(parent) = file.parent() {
            self.row(strings::PROP_LOCATION, &display_path(&parent));
        }

        if is_dir {
            let row = self.row(strings::PROP_SIZE, strings::PROP_CALCULATING);
            let _ = self.imp().size_row.set(row);
            self.measure(std::slice::from_ref(file)).await;
        } else {
            let bytes = u64::try_from(info.size()).unwrap_or(0);
            self.row(strings::PROP_SIZE, &strings::prop_size(bytes));
        }

        let date = |secs: Option<u64>| {
            secs.and_then(|s| i64::try_from(s).ok())
                .and_then(|s| glib::DateTime::from_unix_local(s).ok())
                .and_then(|dt| dt.format("%Y-%m-%d %H:%M:%S").ok())
                .map(|text| text.to_string())
        };
        let attr_time = |name: &str| {
            info.has_attribute(name)
                .then(|| info.attribute_uint64(name))
        };
        for (title, secs) in [
            (
                strings::PROP_MODIFIED,
                attr_time(gio::FILE_ATTRIBUTE_TIME_MODIFIED),
            ),
            (
                strings::PROP_CREATED,
                attr_time(gio::FILE_ATTRIBUTE_TIME_CREATED),
            ),
            (
                strings::PROP_ACCESSED,
                attr_time(gio::FILE_ATTRIBUTE_TIME_ACCESS),
            ),
        ] {
            if let Some(text) = date(secs) {
                self.row(title, &text);
            }
        }

        if info.has_attribute(gio::FILE_ATTRIBUTE_UNIX_MODE) {
            let mode = info.attribute_uint32(gio::FILE_ATTRIBUTE_UNIX_MODE);
            let text = format!("{} ({})", perms::mode_string(mode), perms::mode_octal(mode));
            self.row(strings::PROP_PERMISSIONS, &text);
        }
        let owner = info.attribute_string(gio::FILE_ATTRIBUTE_OWNER_USER);
        let group = info.attribute_string(gio::FILE_ATTRIBUTE_OWNER_GROUP);
        if let (Some(owner), Some(group)) = (owner, group) {
            self.row(strings::PROP_OWNER, &format!("{owner}:{group}"));
        }
    }

    async fn fill_many(&self, files: &[gio::File]) {
        self.row(strings::PROP_ITEMS, &strings::items_count(files.len()));
        if let Some(parent) = files.first().and_then(|f| f.parent()) {
            self.row(strings::PROP_LOCATION, &display_path(&parent));
        }
        let row = self.row(strings::PROP_TOTAL_SIZE, strings::PROP_CALCULATING);
        let _ = self.imp().size_row.set(row);
        self.measure(files).await;
    }

    /// Calcula el tamaño en segundo plano actualizando la fila en vivo.
    async fn measure(&self, files: &[gio::File]) {
        let imp = self.imp();
        let Some(row) = imp.size_row.get().cloned() else {
            return;
        };
        let live = row.clone();
        let result = size::measure(files, &imp.cancellable, move |partial: &Measure| {
            live.set_subtitle(&strings::prop_measuring(partial.bytes, partial.files));
        })
        .await;
        match result {
            Ok(total) => row.set_subtitle(&strings::prop_measured(
                total.bytes,
                total.files,
                total.dirs,
            )),
            Err(err) if err.matches(gio::IOErrorEnum::Cancelled) => {}
            Err(err) => row.set_subtitle(err.message()),
        }
    }
}
