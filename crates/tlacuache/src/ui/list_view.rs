//! Vista de lista detallada (`gtk::ColumnView`) de una carpeta.

use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, pango};
use tlacuache_core::sort::{SortDirection, SortKey, SortSpec};

use crate::fs::file_item::FileItem;
use crate::fs::listing::DirectoryModel;
use crate::strings;
use crate::window::TlacuacheWindow;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FileListView {
        pub model: RefCell<Option<DirectoryModel>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileListView {
        const NAME: &'static str = "TlacuacheFileListView";
        type Type = super::FileListView;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for FileListView {}
    impl WidgetImpl for FileListView {}
    impl BinImpl for FileListView {}
}

glib::wrapper! {
    pub struct FileListView(ObjectSubclass<imp::FileListView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl FileListView {
    pub fn new(dir: &gio::File, show_hidden: bool) -> Self {
        let view: Self = glib::Object::new();
        view.load(dir, show_hidden);
        view
    }

    fn load(&self, dir: &gio::File, show_hidden: bool) {
        let model = DirectoryModel::new(dir, show_hidden);

        let column_view = gtk::ColumnView::new(Some(model.selection().clone()));
        column_view.add_css_class("file-list");
        column_view.add_css_class("data-table");

        let name = column(ColumnId::Name);
        name.set_expand(true);
        column_view.append_column(&name);
        column_view.append_column(&column(ColumnId::Extension));
        column_view.append_column(&column(ColumnId::Size));
        column_view.append_column(&column(ColumnId::Modified));

        // Los encabezados solo indican qué columna/dirección eligió el
        // usuario; el orden real lo aplica core::sort (carpetas primero).
        if let Some(sorter) = column_view.sorter().and_downcast::<gtk::ColumnViewSorter>() {
            sorter.connect_changed(glib::clone!(
                #[weak(rename_to = view)]
                self,
                move |sorter, _| view.apply_header_sort(sorter)
            ));
        }
        column_view.sort_by_column(Some(&name), gtk::SortType::Ascending);

        model.directory_list().connect_error_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |list| {
                if let Some(err) = list.error() {
                    tracing::warn!("error al listar: {err}");
                    if let Some(window) = view.root().and_downcast::<TlacuacheWindow>() {
                        window.show_toast(&strings::listing_failed(&err));
                    }
                }
            }
        ));

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&column_view)
            .build();
        self.set_child(Some(&scrolled));
        self.imp().model.replace(Some(model));
    }

    fn apply_header_sort(&self, sorter: &gtk::ColumnViewSorter) {
        let key = sorter
            .primary_sort_column()
            .and_then(|c| c.id())
            .and_then(|id| ColumnId::from_id(&id))
            .map_or(SortKey::Name, ColumnId::sort_key);
        let direction = match sorter.primary_sort_order() {
            gtk::SortType::Descending => SortDirection::Descending,
            _ => SortDirection::Ascending,
        };
        if let Some(model) = self.imp().model.borrow().as_ref() {
            model.set_sort(SortSpec { key, direction });
        }
    }
}

#[derive(Clone, Copy)]
enum ColumnId {
    Name,
    Extension,
    Size,
    Modified,
}

impl ColumnId {
    const ALL: [Self; 4] = [Self::Name, Self::Extension, Self::Size, Self::Modified];

    fn id(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Extension => "extension",
            Self::Size => "size",
            Self::Modified => "modified",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }

    fn title(self) -> &'static str {
        match self {
            Self::Name => strings::COLUMN_NAME,
            Self::Extension => strings::COLUMN_EXTENSION,
            Self::Size => strings::COLUMN_SIZE,
            Self::Modified => strings::COLUMN_MODIFIED,
        }
    }

    fn sort_key(self) -> SortKey {
        match self {
            Self::Name => SortKey::Name,
            Self::Extension => SortKey::Extension,
            Self::Size => SortKey::Size,
            Self::Modified => SortKey::Modified,
        }
    }
}

fn column(id: ColumnId) -> gtk::ColumnViewColumn {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, obj| {
        if let Some(list_item) = obj.downcast_ref::<gtk::ListItem>() {
            list_item.set_child(Some(&cell_widget(id)));
        }
    });
    factory.connect_bind(move |_, obj| {
        let Some(list_item) = obj.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        if let (Some(item), Some(child)) = (
            list_item.item().and_downcast::<FileItem>(),
            list_item.child(),
        ) {
            bind_cell(id, &item, &child);
        }
    });

    let column = gtk::ColumnViewColumn::new(Some(id.title()), Some(factory));
    column.set_id(Some(id.id()));
    column.set_resizable(true);
    // Sorter simbólico: hace clicables los encabezados (ver apply_header_sort).
    column.set_sorter(Some(&gtk::CustomSorter::new(|_, _| gtk::Ordering::Equal)));
    column
}

fn cell_widget(id: ColumnId) -> gtk::Widget {
    match id {
        ColumnId::Name => {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let icon = gtk::Image::new();
            icon.set_pixel_size(16);
            let label = gtk::Label::builder()
                .xalign(0.0)
                .ellipsize(pango::EllipsizeMode::Middle)
                .build();
            row.append(&icon);
            row.append(&label);
            row.upcast()
        }
        ColumnId::Size => {
            let label = gtk::Label::builder().xalign(1.0).build();
            label.add_css_class("numeric");
            label.upcast()
        }
        ColumnId::Extension | ColumnId::Modified => {
            let label = gtk::Label::builder().xalign(0.0).build();
            if matches!(id, ColumnId::Modified) {
                label.add_css_class("numeric");
            }
            label.upcast()
        }
    }
}

fn bind_cell(id: ColumnId, item: &FileItem, child: &gtk::Widget) {
    let entry = item.entry();
    match id {
        ColumnId::Name => {
            let icon = child.first_child().and_downcast::<gtk::Image>();
            let label = child.last_child().and_downcast::<gtk::Label>();
            if let Some(icon) = icon {
                match item.info().and_then(|i| i.icon()) {
                    Some(gicon) => icon.set_from_gicon(&gicon),
                    None => icon.clear(),
                }
            }
            if let Some(label) = label {
                label.set_text(&entry.name);
            }
        }
        ColumnId::Extension => set_label(child, entry.extension().unwrap_or("")),
        ColumnId::Size => {
            let text = if entry.is_dir {
                glib::GString::default()
            } else {
                glib::format_size(entry.size)
            };
            set_label(child, &text);
        }
        ColumnId::Modified => {
            let text = item
                .info()
                .and_then(|i| i.modification_date_time())
                .and_then(|dt| dt.to_local().ok())
                .and_then(|dt| dt.format("%Y-%m-%d %H:%M").ok())
                .unwrap_or_default();
            set_label(child, &text);
        }
    }
}

fn set_label(child: &gtk::Widget, text: &str) {
    if let Some(label) = child.downcast_ref::<gtk::Label>() {
        label.set_text(text);
    }
}
