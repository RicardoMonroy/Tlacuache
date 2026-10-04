//! Vista de lista detallada (`gtk::ColumnView`) de una carpeta, con
//! navegación por teclado/ratón e historial atrás/adelante.

use std::cell::{OnceCell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib, pango};
use tlacuache_core::history::History;
use tlacuache_core::sort::{SortDirection, SortKey, SortSpec};

use crate::fs::file_item::FileItem;
use crate::fs::listing::DirectoryModel;
use crate::strings;
use crate::window::TlacuacheWindow;

/// Botones laterales del ratón (atrás/adelante) según X11/Wayland.
const MOUSE_BUTTON_BACK: u32 = 8;
const MOUSE_BUTTON_FORWARD: u32 = 9;

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::FileListView)]
    pub struct FileListView {
        /// Carpeta mostrada.
        #[property(get, nullable)]
        pub directory: RefCell<Option<gio::File>>,
        pub model: OnceCell<DirectoryModel>,
        pub column_view: OnceCell<gtk::ColumnView>,
        /// URIs visitadas (gio admite rutas no locales vía GVfs).
        pub history: RefCell<Option<History<String>>>,
        /// Al terminar de cargar, enfocar esta entrada (la carpeta de la que
        /// venimos al subir o retroceder).
        pub pending_focus: RefCell<Option<gio::File>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileListView {
        const NAME: &'static str = "TlacuacheFileListView";
        type Type = super::FileListView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.install_action("nav.up", None, |view, _, _| view.go_up());
            klass.install_action("nav.back", None, |view, _, _| view.go_back());
            klass.install_action("nav.forward", None, |view, _, _| view.go_forward());

            // Solo actúan con el foco dentro de la lista (no en la terminal).
            let alt = gdk::ModifierType::ALT_MASK;
            klass.add_binding_action(gdk::Key::BackSpace, gdk::ModifierType::empty(), "nav.up");
            klass.add_binding_action(gdk::Key::Up, alt, "nav.up");
            klass.add_binding_action(gdk::Key::Left, alt, "nav.back");
            klass.add_binding_action(gdk::Key::Right, alt, "nav.forward");
        }
    }

    #[glib::derived_properties]
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
        view.build(dir, show_hidden);
        view
    }

    fn build(&self, dir: &gio::File, show_hidden: bool) {
        let imp = self.imp();
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

        // Enter y doble clic.
        column_view.connect_activate(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, position| view.activate_position(position)
        ));

        let mouse_nav = gtk::GestureClick::new();
        mouse_nav.set_button(0);
        mouse_nav.connect_pressed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |gesture, _, _, _| match gesture.current_button() {
                MOUSE_BUTTON_BACK => view.go_back(),
                MOUSE_BUTTON_FORWARD => view.go_forward(),
                _ => {}
            }
        ));
        column_view.add_controller(mouse_nav);

        let dir_list = model.directory_list();
        dir_list.connect_error_notify(glib::clone!(
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
        dir_list.connect_loading_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |list| {
                if !list.is_loading() {
                    view.focus_after_load();
                }
            }
        ));

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&column_view)
            .build();
        self.set_child(Some(&scrolled));

        imp.history
            .replace(Some(History::new(dir.uri().to_string())));
        imp.directory.replace(Some(dir.clone()));
        let _ = imp.model.set(model);
        let _ = imp.column_view.set(column_view);
        self.update_nav_actions();
    }

    /// Navega a `dir` registrándolo en el historial.
    pub fn navigate_to(&self, dir: &gio::File) {
        let moved = self
            .imp()
            .history
            .borrow_mut()
            .as_mut()
            .is_some_and(|h| h.navigate(dir.uri().to_string()));
        if moved {
            self.show_directory(dir);
        }
    }

    fn go_up(&self) {
        if let Some(parent) = self.directory().and_then(|d| d.parent()) {
            self.navigate_to(&parent);
        }
    }

    fn go_back(&self) {
        let target = self
            .imp()
            .history
            .borrow_mut()
            .as_mut()
            .and_then(|h| h.back().cloned());
        if let Some(uri) = target {
            self.show_directory(&gio::File::for_uri(&uri));
        }
    }

    fn go_forward(&self) {
        let target = self
            .imp()
            .history
            .borrow_mut()
            .as_mut()
            .and_then(|h| h.forward().cloned());
        if let Some(uri) = target {
            self.show_directory(&gio::File::for_uri(&uri));
        }
    }

    /// Cambia la carpeta mostrada sin tocar el historial.
    fn show_directory(&self, dir: &gio::File) {
        let imp = self.imp();
        let Some(model) = imp.model.get() else {
            return;
        };
        tracing::debug!("carpeta: {}", dir.uri());
        let previous = imp.directory.replace(Some(dir.clone()));
        imp.pending_focus.replace(previous);
        model.set_directory(dir);
        self.notify_directory();
        self.update_nav_actions();
    }

    fn activate_position(&self, position: u32) {
        let Some(item) = self
            .imp()
            .model
            .get()
            .and_then(|m| m.selection().item(position))
            .and_downcast::<FileItem>()
        else {
            return;
        };
        let is_dir = item.entry().is_dir;
        if is_dir && let Some(dir) = item.file() {
            self.navigate_to(&dir);
        }
        // Abrir archivos con su app predeterminada: tarea 1.6.
    }

    /// Selecciona la carpeta de la que venimos o, si no está, la primera fila.
    fn focus_after_load(&self) {
        let imp = self.imp();
        let (Some(model), Some(column_view)) = (imp.model.get(), imp.column_view.get()) else {
            return;
        };
        if model.selection().n_items() == 0 {
            return;
        }
        let position = imp
            .pending_focus
            .take()
            .and_then(|f| model.position_of(&f))
            .unwrap_or(0);
        let flags = gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT;
        column_view.scroll_to(position, None, flags, None);
    }

    fn update_nav_actions(&self) {
        let (back, forward) = self
            .imp()
            .history
            .borrow()
            .as_ref()
            .map_or((false, false), |h| (h.can_go_back(), h.can_go_forward()));
        let has_parent = self.directory().and_then(|d| d.parent()).is_some();
        self.action_set_enabled("nav.back", back);
        self.action_set_enabled("nav.forward", forward);
        self.action_set_enabled("nav.up", has_parent);
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
        if let Some(model) = self.imp().model.get() {
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
