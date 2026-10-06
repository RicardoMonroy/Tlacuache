//! Vista de columnas Miller: una columna por nivel desde `/` hasta la
//! carpeta actual, más una columna de vista previa con el contenido de la
//! carpeta seleccionada.
//!
//! La «carpeta actual» es la de la columna con foco (ver `directory`). ↑/↓
//! solo cambian la vista previa; → entra y ← sube emitiendo
//! `directory-activated`, para que `TabPage` lo registre en el historial.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::{gdk, gio, glib, pango};
use tlacuache_core::filter::{self, FilterKey};
use tlacuache_core::summary::{SelectionSummary, ViewStatus};

use crate::fs::file_item::{FileItem, SelectedInfo};
use crate::fs::launch;
use crate::fs::listing::DirectoryModel;
use crate::strings;
use crate::ui::file_name_cell::FileNameCell;
use crate::ui::filter_indicator::{FilterIndicator, filter_key};
use crate::ui::{context_menu, dnd};
use crate::window;

const COLUMN_WIDTH: i32 = 240;
/// Espera antes de listar la carpeta seleccionada, para no listar cada
/// carpeta por la que se pasa al mantener ↑/↓.
const PREVIEW_DELAY: Duration = Duration::from_millis(80);

/// Una columna: carpeta, modelo, lista y el widget que la contiene.
#[derive(Clone)]
struct Column {
    dir: gio::File,
    model: DirectoryModel,
    selection: gtk::SingleSelection,
    list: gtk::ListView,
    widget: gtk::Box,
    /// Entrada a seleccionar en cuanto termine de cargar.
    pending_select: Rc<RefCell<Option<gio::File>>>,
}

impl Column {
    fn selected_item(&self) -> Option<FileItem> {
        self.selection.selected_item().and_downcast()
    }

    fn selected_dir(&self) -> Option<gio::File> {
        self.selected_item()
            .filter(|item| item.entry().is_dir)
            .and_then(|item| item.file())
    }
}

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::MillerView)]
    pub struct MillerView {
        /// Carpeta actual: la de la columna activa.
        #[property(get, nullable)]
        pub directory: RefCell<Option<gio::File>>,
        pub(super) columns: RefCell<Vec<Column>>,
        /// Índice de la columna activa (con foco).
        pub active: Cell<usize>,
        pub row: gtk::Box,
        pub scroll: gtk::ScrolledWindow,
        pub show_hidden: Cell<bool>,
        /// Filtro rápido de la columna activa.
        pub query: RefCell<String>,
        pub indicator: FilterIndicator,
        pub preview_timer: RefCell<Option<glib::SourceId>>,
        /// Mientras se reestructuran las columnas se ignoran los cambios de
        /// foco y selección que provoca GTK.
        pub updating: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MillerView {
        const NAME: &'static str = "TlacuacheMillerView";
        type Type = super::MillerView;
        type ParentType = adw::Bin;
    }

    #[glib::derived_properties]
    impl ObjectImpl for MillerView {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // El usuario quiere ir a esta carpeta (→, ←, Enter,
                    // doble clic o clic en otra columna).
                    Signal::builder("directory-activated")
                        .param_types([gio::File::static_type()])
                        .build(),
                    // Cambiaron los elementos o la selección de la columna
                    // activa.
                    Signal::builder("status-changed").build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }

        fn dispose(&self) {
            if let Some(id) = self.preview_timer.take() {
                id.remove();
            }
        }
    }

    impl WidgetImpl for MillerView {}
    impl BinImpl for MillerView {}
}

glib::wrapper! {
    pub struct MillerView(ObjectSubclass<imp::MillerView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl MillerView {
    pub fn new(dir: &gio::File, show_hidden: bool) -> Self {
        let view: Self = glib::Object::new();
        view.imp().show_hidden.set(show_hidden);
        view.show_directory(dir);
        view
    }

    pub fn connect_directory_activated<F: Fn(&Self, &gio::File) + 'static>(&self, f: F) {
        self.connect_closure(
            "directory-activated",
            false,
            glib::closure_local!(move |view: &Self, dir: &gio::File| f(view, dir)),
        );
    }

    fn setup(&self) {
        let imp = self.imp();
        imp.row.set_orientation(gtk::Orientation::Horizontal);
        imp.scroll
            .set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
        imp.scroll.set_vexpand(true);
        imp.scroll.set_child(Some(&imp.row));
        // La columna activa y la de vista previa son siempre las últimas:
        // basta con mantener visible el final. Se difiere a un idle porque
        // `changed` llega antes de terminar la asignación.
        imp.scroll.hadjustment().connect_changed(|adj| {
            glib::idle_add_local_once(glib::clone!(
                #[weak]
                adj,
                move || adj.set_value(adj.upper() - adj.page_size())
            ));
        });

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&imp.scroll));
        overlay.add_overlay(&imp.indicator);
        self.set_child(Some(&overlay));

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
    }

    /// Muestra `dir` como carpeta actual. Reutiliza las columnas que ya
    /// coinciden con su ruta (incluida la de vista previa al entrar) y
    /// recrea el resto.
    pub fn show_directory(&self, dir: &gio::File) {
        // Solo conservar el foco si ya lo tenía: si el cambio viene de la
        // terminal (OSC 7) no hay que quitárselo mientras se escribe.
        let had_focus = crate::ui::has_focus_within(self);
        tracing::debug!("carpeta: {}", dir.uri());
        let imp = self.imp();
        self.cancel_preview();
        self.clear_query();

        let chain = ancestors(dir);
        imp.updating.set(true);
        let keep = imp
            .columns
            .borrow()
            .iter()
            .zip(&chain)
            .take_while(|(column, file)| column.dir.equal(*file))
            .count();
        self.truncate(keep);
        // En las columnas ancestro, seleccionar el siguiente nivel de la ruta.
        for (i, child) in chain
            .iter()
            .enumerate()
            .skip(1)
            .take(keep.saturating_sub(1))
        {
            self.select_in_column(i - 1, child);
        }
        for (i, file) in chain.iter().enumerate().skip(keep) {
            self.push_column(file, chain.get(i + 1).cloned());
        }
        imp.active.set(chain.len().saturating_sub(1));
        imp.directory.replace(Some(dir.clone()));
        imp.updating.set(false);

        self.notify_directory();
        self.focus_active(had_focus);
        self.update_preview();
        self.emit_status_changed();
    }

    pub fn connect_status_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_closure(
            "status-changed",
            false,
            glib::closure_local!(move |view: &Self| f(view)),
        );
    }

    /// Selecciona `file` en la columna activa ahora o en cuanto aparezca.
    pub fn select_when_present(&self, file: &gio::File) {
        if let Some(column) = self.active_column() {
            column.pending_select.replace(Some(file.clone()));
        }
        self.try_select_pending();
    }

    fn try_select_pending(&self) {
        let Some(column) = self.active_column() else {
            return;
        };
        let position = column
            .pending_select
            .borrow()
            .as_ref()
            .and_then(|f| column.model.position_of(f));
        if let Some(position) = position {
            column.pending_select.replace(None);
            let flags = gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT;
            column.list.scroll_to(position, flags, None);
            column.list.grab_focus();
        }
    }

    /// Seleccionado en la columna activa, con su tipo.
    pub fn selected_infos(&self) -> Vec<SelectedInfo> {
        self.active_column()
            .and_then(|c| c.selected_item())
            .and_then(|item| SelectedInfo::from_item(&item))
            .into_iter()
            .collect()
    }

    /// Archivo seleccionado en la columna activa.
    pub fn selected_files(&self) -> Vec<gio::File> {
        self.active_column()
            .and_then(|c| c.selected_item())
            .and_then(|item| item.file())
            .into_iter()
            .collect()
    }

    /// Elementos y selección de la columna activa.
    pub fn status(&self) -> ViewStatus {
        let Some(column) = self.active_column() else {
            return ViewStatus::default();
        };
        let selected = column.selected_item();
        let entry = selected.as_ref().map(FileItem::entry);
        ViewStatus {
            items: column.model.model().n_items(),
            selection: SelectionSummary::from_entries(entry.as_deref()),
        }
    }

    fn emit_status_changed(&self) {
        self.emit_by_name::<()>("status-changed", &[]);
    }

    /// Lleva el foco del teclado a la columna activa.
    pub fn focus_list(&self) {
        self.focus_active(true);
    }

    /// Muestra u oculta los archivos ocultos en todas las columnas.
    pub fn set_show_hidden(&self, show: bool) {
        self.imp().show_hidden.set(show);
        for column in self.columns_snapshot() {
            column.model.set_show_hidden(show);
        }
    }

    /// Vuelve a leer las carpetas de todas las columnas conservando
    /// selección y desplazamiento.
    pub fn reload(&self) {
        for column in self.columns_snapshot() {
            column.model.reload();
        }
    }

    fn columns_snapshot(&self) -> Vec<Column> {
        self.imp().columns.borrow().clone()
    }

    /// Copia de la columna `index`; se clona para no mantener el préstamo
    /// mientras GTK emite señales que vuelven a esta vista.
    fn column(&self, index: usize) -> Option<Column> {
        self.imp().columns.borrow().get(index).cloned()
    }

    fn index_of(&self, list: &gtk::ListView) -> Option<usize> {
        self.imp()
            .columns
            .borrow()
            .iter()
            .position(|column| column.list == *list)
    }

    fn active_column(&self) -> Option<Column> {
        self.column(self.imp().active.get())
    }

    /// Quita las columnas desde `keep` en adelante.
    fn truncate(&self, keep: usize) {
        let imp = self.imp();
        let removed: Vec<Column> = {
            let mut columns = imp.columns.borrow_mut();
            let keep = keep.min(columns.len());
            columns.drain(keep..).collect()
        };
        for column in removed {
            imp.row.remove(&column.widget);
        }
    }

    fn push_column(&self, dir: &gio::File, pending_select: Option<gio::File>) {
        let imp = self.imp();
        let model = DirectoryModel::new(dir, imp.show_hidden.get());
        let selection = gtk::SingleSelection::new(Some(model.model().clone()));
        selection.set_autoselect(false);
        selection.set_can_unselect(true);

        let list = gtk::ListView::new(
            Some(selection.clone()),
            Some(row_factory(selection.upcast_ref())),
        );
        list.add_css_class("miller-column");

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .width_request(COLUMN_WIDTH)
            .child(&list)
            .build();
        // Clic derecho en la zona vacía: menú de la carpeta de la columna
        // (al tomar el foco, esa columna pasa a ser la actual).
        context_menu::attach_background_menu(&scrolled, || {});

        // Soltar en la zona vacía de la columna: a su carpeta.
        let column_dir = dir.clone();
        dnd::attach_dir_drop(&scrolled, move || Some(column_dir.clone()));
        let widget = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        widget.append(&scrolled);
        widget.append(&gtk::Separator::new(gtk::Orientation::Vertical));

        let column = Column {
            dir: dir.clone(),
            model,
            selection,
            list,
            widget,
            pending_select: Rc::new(RefCell::new(pending_select)),
        };
        self.connect_column(&column);
        imp.row.append(&column.widget);
        imp.columns.borrow_mut().push(column);
    }

    fn connect_column(&self, column: &Column) {
        let list = &column.list;

        column.selection.connect_selected_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            #[weak]
            list,
            move |_| {
                if view.index_of(&list) != Some(view.imp().active.get()) {
                    return;
                }
                view.emit_status_changed();
                if !view.imp().updating.get() {
                    view.schedule_preview();
                }
            }
        ));

        list.connect_activate(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |list, position| view.activate(list, position)
        ));

        // Clic o Tab hacia otra columna: esa carpeta pasa a ser la actual.
        let focus = gtk::EventControllerFocus::new();
        focus.connect_enter(glib::clone!(
            #[weak(rename_to = view)]
            self,
            #[weak]
            list,
            move |_| {
                let imp = view.imp();
                if imp.updating.get() {
                    return;
                }
                if let Some(index) = view.index_of(&list)
                    && index != imp.active.get()
                    && let Some(column) = view.column(index)
                {
                    view.emit_by_name::<()>("directory-activated", &[&column.dir]);
                }
            }
        ));
        list.add_controller(focus);

        let source = column.model.source();
        crate::ui::track_visibility(list, source);
        source.connect_loading_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            #[weak]
            list,
            move |source| {
                if !source.loading() {
                    view.on_column_loaded(&list);
                }
            }
        ));
        source.connect_error_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |source| {
                if let Some(err) = source.error() {
                    tracing::warn!("error al listar: {err}");
                    window::show_toast_from(&view, &strings::listing_failed(&err));
                }
            }
        ));

        column.model.model().connect_items_changed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            #[weak]
            list,
            move |_, _, _, _| {
                if view.index_of(&list) == Some(view.imp().active.get()) {
                    view.update_filter_indicator();
                    view.try_select_pending();
                    view.emit_status_changed();
                }
            }
        ));
    }

    fn on_column_loaded(&self, list: &gtk::ListView) {
        let Some(index) = self.index_of(list) else {
            return;
        };
        let Some(column) = self.column(index) else {
            return;
        };
        let is_active = index == self.imp().active.get();
        let pending = column.pending_select.take();
        if let Some(file) = pending {
            self.imp().updating.set(true);
            self.select_in_column(index, &file);
            self.imp().updating.set(false);
        }
        if is_active {
            self.focus_active(crate::ui::has_focus_within(self));
            self.update_preview();
        }
    }

    /// Selecciona `file` en la columna `index`; si aún no cargó, lo deja
    /// pendiente.
    fn select_in_column(&self, index: usize, file: &gio::File) {
        let Some(column) = self.column(index) else {
            return;
        };
        match column.model.position_of(file) {
            Some(position) => {
                column.selection.set_selected(position);
                column
                    .list
                    .scroll_to(position, gtk::ListScrollFlags::NONE, None);
            }
            None => {
                column.pending_select.replace(Some(file.clone()));
            }
        }
    }

    /// Enfoca la fila seleccionada de la columna activa (o la primera).
    /// Selecciona la fila actual de la columna activa y, con `take_focus`,
    /// le da el foco de teclado.
    fn focus_active(&self, take_focus: bool) {
        let Some(column) = self.active_column() else {
            return;
        };
        if column.model.model().n_items() == 0 {
            if take_focus {
                column.list.grab_focus();
            }
            return;
        }
        let position = match column.selection.selected() {
            gtk::INVALID_LIST_POSITION => 0,
            selected => selected,
        };
        if !take_focus {
            column
                .list
                .scroll_to(position, gtk::ListScrollFlags::SELECT, None);
            return;
        }
        let flags = gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT;
        column.list.scroll_to(position, flags, None);
        // `FOCUS` solo mueve la fila enfocada dentro de la lista; si el foco
        // de teclado estaba en otra columna (p. ej. tras un clic) se queda
        // ahí. Pedirlo explícitamente lo lleva a esa fila.
        column.list.grab_focus();
    }

    fn schedule_preview(&self) {
        self.cancel_preview();
        let id = glib::timeout_add_local_once(
            PREVIEW_DELAY,
            glib::clone!(
                #[weak(rename_to = view)]
                self,
                move || {
                    view.imp().preview_timer.take();
                    view.update_preview();
                }
            ),
        );
        self.imp().preview_timer.replace(Some(id));
    }

    fn cancel_preview(&self) {
        if let Some(id) = self.imp().preview_timer.take() {
            id.remove();
        }
    }

    /// Ajusta la columna de vista previa a la selección de la activa: si es
    /// una carpeta, muestra su contenido; si no, quita la columna.
    fn update_preview(&self) {
        let imp = self.imp();
        let active = imp.active.get();
        let Some(column) = self.column(active) else {
            return;
        };
        let selected = column.selected_dir();
        let current_preview = self.column(active + 1).map(|c| c.dir);
        let unchanged = match (&selected, &current_preview) {
            (Some(sel), Some(prev)) => sel.equal(prev),
            (None, None) => true,
            _ => false,
        };
        if unchanged {
            return;
        }
        imp.updating.set(true);
        self.truncate(active + 1);
        if let Some(dir) = selected {
            self.push_column(&dir, None);
        }
        imp.updating.set(false);
    }

    fn activate(&self, list: &gtk::ListView, position: u32) {
        let Some(column) = self.index_of(list).and_then(|i| self.column(i)) else {
            return;
        };
        let Some(item) = column.model.item(position) else {
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

    fn on_key(&self, key: gdk::Key, mods: gdk::ModifierType) -> glib::Propagation {
        let modifiers = gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::ALT_MASK
            | gdk::ModifierType::SUPER_MASK
            | gdk::ModifierType::SHIFT_MASK;
        if !mods.intersects(modifiers) {
            match key {
                gdk::Key::Right | gdk::Key::KP_Right => {
                    if let Some(dir) = self.active_column().and_then(|c| c.selected_dir()) {
                        self.emit_by_name::<()>("directory-activated", &[&dir]);
                    }
                    return glib::Propagation::Stop;
                }
                gdk::Key::Left | gdk::Key::KP_Left => {
                    let active = self.imp().active.get();
                    if active > 0
                        && let Some(parent) = self.column(active - 1)
                    {
                        self.emit_by_name::<()>("directory-activated", &[&parent.dir]);
                    }
                    return glib::Propagation::Stop;
                }
                _ => {}
            }
        }

        let Some(filter_key) = filter_key(key, mods) else {
            return glib::Propagation::Proceed;
        };
        let current = self.imp().query.borrow().clone();
        match filter::apply_key(&current, filter_key) {
            Some(query) => {
                self.set_query(query);
                glib::Propagation::Stop
            }
            // Espacio sin filtro: alterna la vista previa (ADR-006).
            None if filter_key == FilterKey::Char(' ') => {
                let _ = self.activate_action("pane.preview", None);
                glib::Propagation::Stop
            }
            None => glib::Propagation::Proceed,
        }
    }

    fn set_query(&self, query: String) {
        let Some(column) = self.active_column() else {
            return;
        };
        column.model.set_query(&query);
        self.imp().query.replace(query);
        self.update_filter_indicator();
        if column.model.model().n_items() > 0 {
            let flags = gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT;
            column.list.scroll_to(0, flags, None);
        }
    }

    /// Limpia el filtro de la columna activa (antes de cambiar de carpeta).
    fn clear_query(&self) {
        let imp = self.imp();
        if imp.query.borrow().is_empty() {
            return;
        }
        if let Some(column) = self.active_column() {
            column.model.set_query("");
        }
        imp.query.replace(String::new());
        self.update_filter_indicator();
    }

    fn update_filter_indicator(&self) {
        let imp = self.imp();
        let count = self
            .active_column()
            .map_or(0, |c| c.model.model().n_items());
        imp.indicator.update(&imp.query.borrow(), count);
    }
}

/// Ruta desde la raíz hasta `dir`, ambos incluidos.
fn ancestors(dir: &gio::File) -> Vec<gio::File> {
    let mut chain = vec![dir.clone()];
    while let Some(parent) = chain.last().and_then(|f| f.parent()) {
        chain.push(parent);
    }
    chain.reverse();
    chain
}

fn row_factory(selection: &gtk::SelectionModel) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    let selection = selection.clone();
    factory.connect_setup(move |_, obj| {
        let Some(list_item) = obj.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let cell = FileNameCell::new(pango::EllipsizeMode::End);
        cell.set_hexpand(true);
        let chevron = gtk::Image::from_icon_name("go-next-symbolic");
        chevron.set_pixel_size(12);
        chevron.add_css_class("tl-chevron");
        row.append(&cell);
        row.append(&chevron);
        dnd::attach_file_drag(&row, list_item, &selection);
        dnd::attach_row_drop(&row, list_item);
        context_menu::attach_row_menu(&row, list_item, &selection);
        list_item.set_child(Some(&row));
        crate::ui::refresh_on_item_change(list_item, bind_row);
    });
    factory.connect_bind(|_, obj| {
        if let Some(list_item) = obj.downcast_ref::<gtk::ListItem>() {
            bind_row(list_item);
        }
    });
    factory
}

/// Pinta la fila con su `FileItem` (nombre, ícono y flecha de carpeta).
fn bind_row(list_item: &gtk::ListItem) {
    let (Some(item), Some(row)) = (
        list_item.item().and_downcast::<FileItem>(),
        list_item.child(),
    ) else {
        return;
    };
    let cell = row.first_child().and_downcast::<FileNameCell>();
    let chevron = row.last_child();
    let entry = item.entry();
    if let Some(cell) = cell {
        cell.set_entry(&entry, item.file().map(|f| f.uri().to_string()));
    }
    if let Some(chevron) = chevron {
        chevron.set_visible(entry.is_dir);
    }
}
