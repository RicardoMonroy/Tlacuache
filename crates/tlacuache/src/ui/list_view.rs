//! Vista de lista detallada (`gtk::ColumnView`) de una carpeta.
//!
//! Solo muestra; la navegación (historial, atajos) la decide `TabPage`, que
//! escucha `directory-activated`.

use std::cell::{OnceCell, RefCell};
use std::sync::OnceLock;
use std::time::SystemTime;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::{gdk, gio, glib, pango};
use tlacuache_core::age::{Age, AgeBucket};
use tlacuache_core::filter;
use tlacuache_core::sort::{SortDirection, SortKey, SortSpec};
use tlacuache_core::summary::{SelectionSummary, ViewStatus};

use crate::fs::file_item::{FileItem, SelectedInfo};
use crate::fs::launch;
use crate::fs::listing::DirectoryModel;
use crate::strings;
use crate::ui::filter_indicator::{FilterIndicator, filter_key};
use crate::ui::{context_menu, dnd};
use crate::window;

/// Ancho inicial de la columna "Nombre" (px); el usuario puede cambiarlo.
const NAME_COLUMN_WIDTH: i32 = 360;

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::FileListView)]
    pub struct FileListView {
        /// Carpeta mostrada.
        #[property(get, nullable)]
        pub directory: RefCell<Option<gio::File>>,
        pub model: OnceCell<DirectoryModel>,
        pub selection: OnceCell<gtk::MultiSelection>,
        pub column_view: OnceCell<gtk::ColumnView>,
        /// Al terminar de cargar, enfocar esta entrada (la carpeta de la que
        /// venimos al subir o retroceder).
        pub pending_focus: RefCell<Option<gio::File>>,
        /// Seleccionar este archivo en cuanto aparezca (recién creado o
        /// renombrado; el monitor de la carpeta lo añade después).
        pub pending_select: RefCell<Option<gio::File>>,
        /// Texto del filtro rápido (vacío = inactivo).
        pub query: RefCell<String>,
        pub indicator: FilterIndicator,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileListView {
        const NAME: &'static str = "TlacuacheFileListView";
        type Type = super::FileListView;
        type ParentType = adw::Bin;
    }

    #[glib::derived_properties]
    impl ObjectImpl for FileListView {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // Enter o doble clic sobre una carpeta (los archivos se
                    // abren aquí mismo con su app predeterminada).
                    Signal::builder("directory-activated")
                        .param_types([gio::File::static_type()])
                        .build(),
                    // Cambiaron los elementos visibles o la selección.
                    Signal::builder("status-changed").build(),
                ]
            })
        }
    }

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

    pub fn connect_directory_activated<F: Fn(&Self, &gio::File) + 'static>(&self, f: F) {
        self.connect_closure(
            "directory-activated",
            false,
            glib::closure_local!(move |view: &Self, dir: &gio::File| f(view, dir)),
        );
    }

    fn build(&self, dir: &gio::File, show_hidden: bool) {
        let imp = self.imp();
        let model = DirectoryModel::new(dir, show_hidden);

        let selection = gtk::MultiSelection::new(Some(model.model().clone()));
        selection.connect_selection_changed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, _, _| view.emit_by_name::<()>("status-changed", &[])
        ));
        let column_view = gtk::ColumnView::new(Some(selection.clone()));
        column_view.add_css_class("file-list");
        column_view.add_css_class("data-table");
        // Selección por rectángulo arrastrando con el ratón.
        column_view.set_enable_rubberband(true);

        // Solo la última columna se expande: si se expandiera "Nombre", GTK
        // le seguiría dando el espacio sobrante al redimensionarla y los
        // bordes intermedios se moverían al revés de lo que se arrastra.
        let name = column(ColumnId::Name, selection.upcast_ref());
        name.set_fixed_width(NAME_COLUMN_WIDTH);
        column_view.append_column(&name);
        column_view.append_column(&column(ColumnId::Extension, selection.upcast_ref()));
        column_view.append_column(&column(ColumnId::Size, selection.upcast_ref()));
        column_view.append_column(&column(ColumnId::Modified, selection.upcast_ref()));
        let age = column(ColumnId::Age, selection.upcast_ref());
        age.set_expand(true);
        column_view.append_column(&age);

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

        let dir_list = model.directory_list();
        dir_list.connect_error_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |list| {
                if let Some(err) = list.error() {
                    tracing::warn!("error al listar: {err}");
                    window::show_toast_from(&view, &strings::listing_failed(&err));
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

        // Scroll horizontal automático: columnas anchas no deben impedir
        // achicar la ventana.
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .child(&column_view)
            .build();

        // Clic derecho en la zona vacía: sin selección y menú de carpeta.
        context_menu::attach_background_menu(
            &scrolled,
            glib::clone!(
                #[weak]
                selection,
                move || {
                    selection.unselect_all();
                }
            ),
        );

        // Soltar en la zona vacía: a la carpeta mostrada.
        dnd::attach_dir_drop(
            &scrolled,
            glib::clone!(
                #[weak(rename_to = view)]
                self,
                #[upgrade_or]
                None,
                move || view.directory()
            ),
        );

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&scrolled));
        overlay.add_overlay(&imp.indicator);
        self.set_child(Some(&overlay));

        // Captura: las teclas de texto llegan aquí antes que a las filas, así
        // el foco se queda en la lista y las flechas/Enter siguen funcionando.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, mods| view.on_key(key, mods)
        ));
        self.add_controller(keys);

        model.model().connect_items_changed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, _, _, _| {
                view.update_filter_indicator();
                view.try_select_pending();
                view.emit_by_name::<()>("status-changed", &[]);
            }
        ));

        imp.directory.replace(Some(dir.clone()));
        let _ = imp.model.set(model);
        let _ = imp.selection.set(selection);
        let _ = imp.column_view.set(column_view);
    }

    pub fn connect_status_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_closure(
            "status-changed",
            false,
            glib::closure_local!(move |view: &Self| f(view)),
        );
    }

    fn selected_items(&self) -> Vec<FileItem> {
        let imp = self.imp();
        let (Some(model), Some(selection)) = (imp.model.get(), imp.selection.get()) else {
            return Vec::new();
        };
        let bitset = selection.selection();
        gtk::BitsetIter::init_first(&bitset)
            .map(|(iter, first)| std::iter::once(first).chain(iter))
            .into_iter()
            .flatten()
            .filter_map(|position| model.item(position))
            .collect()
    }

    /// Seleccionados con su tipo: (archivo, es carpeta, tipo MIME).
    pub fn selected_infos(&self) -> Vec<SelectedInfo> {
        self.selected_items()
            .iter()
            .filter_map(SelectedInfo::from_item)
            .collect()
    }

    /// Archivos seleccionados (para copiar, mover, etc.).
    pub fn selected_files(&self) -> Vec<gio::File> {
        self.selected_items()
            .iter()
            .filter_map(FileItem::file)
            .collect()
    }

    /// Elementos visibles y resumen de la selección.
    pub fn status(&self) -> ViewStatus {
        let items = self.selected_items();
        let entries: Vec<_> = items.iter().map(FileItem::entry).collect();
        ViewStatus {
            items: self.imp().model.get().map_or(0, |m| m.model().n_items()),
            selection: SelectionSummary::from_entries(entries.iter().map(|e| &**e)),
        }
    }

    /// Cambia la carpeta mostrada. Al terminar de cargar se enfoca la
    /// carpeta anterior si está en la nueva (al subir o retroceder).
    pub fn show_directory(&self, dir: &gio::File) {
        let imp = self.imp();
        let Some(model) = imp.model.get() else {
            return;
        };
        tracing::debug!("carpeta: {}", dir.uri());
        let previous = imp.directory.replace(Some(dir.clone()));
        imp.pending_focus.replace(previous);
        // El filtro es por carpeta: al cambiar se limpia.
        if !imp.query.borrow().is_empty() {
            imp.query.replace(String::new());
            model.set_query("");
            self.update_filter_indicator();
        }
        model.set_directory(dir);
        self.notify_directory();
    }

    /// Selecciona `file` ahora o en cuanto aparezca en la lista.
    pub fn select_when_present(&self, file: &gio::File) {
        self.imp().pending_select.replace(Some(file.clone()));
        self.try_select_pending();
    }

    fn try_select_pending(&self) {
        let imp = self.imp();
        let (Some(model), Some(column_view)) = (imp.model.get(), imp.column_view.get()) else {
            return;
        };
        let position = imp
            .pending_select
            .borrow()
            .as_ref()
            .and_then(|f| model.position_of(f));
        if let Some(position) = position {
            imp.pending_select.replace(None);
            let flags = gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT;
            column_view.scroll_to(position, None, flags, None);
        }
    }

    /// Muestra u oculta los archivos ocultos.
    pub fn set_show_hidden(&self, show: bool) {
        if let Some(model) = self.imp().model.get() {
            model.set_show_hidden(show);
        }
    }

    fn on_key(&self, key: gdk::Key, mods: gdk::ModifierType) -> glib::Propagation {
        let Some(filter_key) = filter_key(key, mods) else {
            return glib::Propagation::Proceed;
        };
        let current = self.imp().query.borrow().clone();
        match filter::apply_key(&current, filter_key) {
            Some(query) => {
                self.set_query(query);
                glib::Propagation::Stop
            }
            None => glib::Propagation::Proceed,
        }
    }

    fn set_query(&self, query: String) {
        let imp = self.imp();
        let Some(model) = imp.model.get() else {
            return;
        };
        model.set_query(&query);
        imp.query.replace(query);
        self.update_filter_indicator();
        // Seleccionar la primera coincidencia para poder abrirla con Enter.
        if let Some(column_view) = imp.column_view.get()
            && model.model().n_items() > 0
        {
            let flags = gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT;
            column_view.scroll_to(0, None, flags, None);
        }
    }

    fn update_filter_indicator(&self) {
        let imp = self.imp();
        let count = imp.model.get().map_or(0, |m| m.model().n_items());
        imp.indicator.update(&imp.query.borrow(), count);
    }

    /// Lleva el foco del teclado a la fila actual de la lista.
    pub fn focus_list(&self) {
        if let Some(column_view) = self.imp().column_view.get() {
            column_view.grab_focus();
        }
    }

    fn activate_position(&self, position: u32) {
        let Some(item) = self.imp().model.get().and_then(|m| m.item(position)) else {
            return;
        };
        let Some(file) = item.file() else {
            return;
        };
        if item.entry().is_dir {
            self.emit_by_name::<()>("directory-activated", &[&file]);
        } else {
            launch::open_file(self, file, item.entry().name.clone());
        }
    }

    /// Selecciona la carpeta de la que venimos o, si no está, la primera fila.
    fn focus_after_load(&self) {
        let imp = self.imp();
        let (Some(model), Some(column_view)) = (imp.model.get(), imp.column_view.get()) else {
            return;
        };
        if model.model().n_items() == 0 {
            return;
        }
        let position = imp
            .pending_focus
            .take()
            .and_then(|f| model.position_of(&f))
            .unwrap_or(0);
        // Sin quitar el foco a otro widget (p. ej. la terminal tras un `cd`).
        let flags = if crate::ui::has_focus_within(self) {
            gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT
        } else {
            gtk::ListScrollFlags::SELECT
        };
        column_view.scroll_to(position, None, flags, None);
    }

    fn apply_header_sort(&self, sorter: &gtk::ColumnViewSorter) {
        let column = sorter
            .primary_sort_column()
            .and_then(|c| c.id())
            .and_then(|id| ColumnId::from_id(&id))
            .unwrap_or(ColumnId::Name);
        let ascending = sorter.primary_sort_order() != gtk::SortType::Descending;
        // Edad ascendente = lo más reciente primero = fecha descendente.
        let ascending = ascending != matches!(column, ColumnId::Age);
        let key = column.sort_key();
        let direction = if ascending {
            SortDirection::Ascending
        } else {
            SortDirection::Descending
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
    Age,
}

impl ColumnId {
    const ALL: [Self; 5] = [
        Self::Name,
        Self::Extension,
        Self::Size,
        Self::Modified,
        Self::Age,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Extension => "extension",
            Self::Size => "size",
            Self::Modified => "modified",
            Self::Age => "age",
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
            Self::Age => strings::COLUMN_AGE,
        }
    }

    fn sort_key(self) -> SortKey {
        match self {
            Self::Name => SortKey::Name,
            Self::Extension => SortKey::Extension,
            Self::Size => SortKey::Size,
            Self::Modified | Self::Age => SortKey::Modified,
        }
    }
}

fn column(id: ColumnId, selection: &gtk::SelectionModel) -> gtk::ColumnViewColumn {
    let factory = gtk::SignalListItemFactory::new();
    let selection = selection.clone();
    factory.connect_setup(move |_, obj| {
        if let Some(list_item) = obj.downcast_ref::<gtk::ListItem>() {
            let cell = cell_widget(id);
            dnd::attach_file_drag(&cell, list_item, &selection);
            dnd::attach_row_drop(&cell, list_item);
            context_menu::attach_row_menu(&cell, list_item, &selection);
            list_item.set_child(Some(&cell));
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
        ColumnId::Age => {
            let chip = gtk::Label::builder().halign(gtk::Align::Start).build();
            chip.add_css_class("age-chip");
            chip.add_css_class("numeric");
            chip.upcast()
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
        ColumnId::Age => {
            if let Some(chip) = child.downcast_ref::<gtk::Label>() {
                bind_age_chip(chip, entry.modified);
            }
        }
    }
}

const AGE_CLASSES: [&str; 5] = ["age-day", "age-week", "age-month", "age-year", "age-older"];

fn bind_age_chip(chip: &gtk::Label, modified: Option<SystemTime>) {
    for class in AGE_CLASSES {
        chip.remove_css_class(class);
    }
    let Some(modified) = modified else {
        chip.set_visible(false);
        return;
    };
    let age = Age::between(modified, SystemTime::now());
    chip.add_css_class(match age.bucket {
        AgeBucket::Day => "age-day",
        AgeBucket::Week => "age-week",
        AgeBucket::Month => "age-month",
        AgeBucket::Year => "age-year",
        AgeBucket::Older => "age-older",
    });
    chip.set_text(&strings::age(&age));
    chip.set_visible(true);
}

fn set_label(child: &gtk::Widget, text: &str) {
    if let Some(label) = child.downcast_ref::<gtk::Label>() {
        label.set_text(text);
    }
}
