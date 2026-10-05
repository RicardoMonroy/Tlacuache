//! Vista de íconos (`gtk::GridView`) de una carpeta (8.4). Mismo
//! comportamiento que la lista detallada: selección múltiple y por
//! rectángulo, Enter/doble clic, filtro rápido, Espacio, arrastrar y soltar
//! y menú contextual. El orden es el del modelo (carpetas primero, nombre).

use std::cell::{OnceCell, RefCell};
use std::sync::OnceLock;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::{gdk, gio, glib};
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

/// Columnas como máximo (el ancho del panel decide cuántas caben).
const MAX_COLUMNS: u32 = 24;

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::IconView)]
    pub struct IconView {
        /// Carpeta mostrada.
        #[property(get, nullable)]
        pub directory: RefCell<Option<gio::File>>,
        pub model: OnceCell<DirectoryModel>,
        pub selection: OnceCell<gtk::MultiSelection>,
        pub grid: OnceCell<gtk::GridView>,
        /// Al terminar de cargar, enfocar esta entrada (al subir o volver).
        pub pending_focus: RefCell<Option<gio::File>>,
        /// Seleccionar este archivo en cuanto aparezca.
        pub pending_select: RefCell<Option<gio::File>>,
        /// Texto del filtro rápido (vacío = inactivo).
        pub query: RefCell<String>,
        pub indicator: FilterIndicator,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for IconView {
        const NAME: &'static str = "TlacuacheIconView";
        type Type = super::IconView;
        type ParentType = adw::Bin;
    }

    #[glib::derived_properties]
    impl ObjectImpl for IconView {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("directory-activated")
                        .param_types([gio::File::static_type()])
                        .build(),
                    Signal::builder("status-changed").build(),
                ]
            })
        }
    }

    impl WidgetImpl for IconView {}
    impl BinImpl for IconView {}
}

glib::wrapper! {
    pub struct IconView(ObjectSubclass<imp::IconView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl IconView {
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

    pub fn connect_status_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_closure(
            "status-changed",
            false,
            glib::closure_local!(move |view: &Self| f(view)),
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
        let grid = gtk::GridView::new(
            Some(selection.clone()),
            Some(tile_factory(selection.upcast_ref())),
        );
        grid.add_css_class("tl-icon-grid");
        grid.set_max_columns(MAX_COLUMNS);
        grid.set_enable_rubberband(true);
        grid.connect_activate(glib::clone!(
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

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&grid)
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

        // Captura: el texto va al filtro rápido antes que a los mosaicos.
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
        let _ = imp.grid.set(grid);
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

    pub fn selected_infos(&self) -> Vec<SelectedInfo> {
        self.selected_items()
            .iter()
            .filter_map(SelectedInfo::from_item)
            .collect()
    }

    pub fn selected_files(&self) -> Vec<gio::File> {
        self.selected_items()
            .iter()
            .filter_map(FileItem::file)
            .collect()
    }

    pub fn status(&self) -> ViewStatus {
        let items = self.selected_items();
        let entries: Vec<_> = items.iter().map(FileItem::entry).collect();
        ViewStatus {
            items: self.imp().model.get().map_or(0, |m| m.model().n_items()),
            selection: SelectionSummary::from_entries(entries.iter().map(|e| &**e)),
        }
    }

    /// Cambia la carpeta mostrada; al cargar se enfoca la anterior si está.
    pub fn show_directory(&self, dir: &gio::File) {
        let imp = self.imp();
        let Some(model) = imp.model.get() else {
            return;
        };
        let previous = imp.directory.replace(Some(dir.clone()));
        imp.pending_focus.replace(previous);
        if !imp.query.borrow().is_empty() {
            imp.query.replace(String::new());
            model.set_query("");
            self.update_filter_indicator();
        }
        model.set_directory(dir);
        self.notify_directory();
    }

    pub fn select_when_present(&self, file: &gio::File) {
        self.imp().pending_select.replace(Some(file.clone()));
        self.try_select_pending();
    }

    fn try_select_pending(&self) {
        let imp = self.imp();
        let Some(model) = imp.model.get() else {
            return;
        };
        let position = imp
            .pending_select
            .borrow()
            .as_ref()
            .and_then(|f| model.position_of(f));
        if let Some(position) = position {
            imp.pending_select.replace(None);
            self.scroll_to(position, true);
        }
    }

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
            // Espacio sin filtro: alterna la vista previa (ADR-006).
            None if filter_key == FilterKey::Char(' ') => {
                let _ = self.activate_action("pane.preview", None);
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
        if model.model().n_items() > 0 {
            self.scroll_to(0, true);
        }
    }

    fn update_filter_indicator(&self) {
        let imp = self.imp();
        let count = imp.model.get().map_or(0, |m| m.model().n_items());
        imp.indicator.update(&imp.query.borrow(), count);
    }

    /// Lleva el foco del teclado al mosaico actual.
    pub fn focus_list(&self) {
        if let Some(grid) = self.imp().grid.get() {
            grid.grab_focus();
        }
    }

    /// Selecciona (y enfoca si `focus`) la posición y la muestra.
    fn scroll_to(&self, position: u32, focus: bool) {
        if let Some(grid) = self.imp().grid.get() {
            let flags = if focus {
                gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT
            } else {
                gtk::ListScrollFlags::SELECT
            };
            grid.scroll_to(position, flags, None);
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

    /// Selecciona la carpeta de la que venimos o, si no está, la primera.
    fn focus_after_load(&self) {
        let imp = self.imp();
        let Some(model) = imp.model.get() else {
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
        self.scroll_to(position, crate::ui::has_focus_within(self));
    }
}

/// Mosaicos: ícono (o glifo) grande y nombre, con arrastre, destino de
/// soltar y menú contextual como las filas de las otras vistas.
fn tile_factory(selection: &gtk::SelectionModel) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    let selection = selection.clone();
    factory.connect_setup(move |_, obj| {
        let Some(list_item) = obj.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let tile = FileNameCell::new_tile();
        dnd::attach_file_drag(&tile, list_item, &selection);
        dnd::attach_row_drop(&tile, list_item);
        context_menu::attach_row_menu(&tile, list_item, &selection);
        list_item.set_child(Some(&tile));
    });
    factory.connect_bind(|_, obj| {
        let Some(list_item) = obj.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        if let (Some(item), Some(tile)) = (
            list_item.item().and_downcast::<FileItem>(),
            list_item.child().and_downcast::<FileNameCell>(),
        ) {
            tile.set_entry(&item.entry(), item.file().map(|f| f.uri().to_string()));
        }
    });
    factory
}
