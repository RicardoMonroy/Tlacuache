//! Contenido de una pestaña: barra de navegación (‹ › ↑ + ruta + selector de
//! vista) y la vista de la carpeta. Es dueña del historial, de los atajos de
//! navegación y del estado que debe sobrevivir al cambiar de vista.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use tlacuache_core::config::ViewMode;
use tlacuache_core::history::History;
use tlacuache_core::summary::ViewStatus;

use crate::fs::file_item::SelectedInfo;
use crate::strings;
use crate::ui::list_view::FileListView;
use crate::ui::miller_view::MillerView;
use crate::ui::path_bar::PathBar;
use crate::ui::status_bar::StatusBar;

/// Botones laterales del ratón (atrás/adelante).
const MOUSE_BUTTON_BACK: u32 = 8;
const MOUSE_BUTTON_FORWARD: u32 = 9;

/// La vista activa de la pestaña. Cada variante expone la misma interfaz;
/// añadir una vista nueva obliga a completar cada `match`.
#[derive(Clone)]
enum PageView {
    Columns(MillerView),
    Details(FileListView),
}

impl PageView {
    fn new(mode: ViewMode, dir: &gio::File, show_hidden: bool) -> Self {
        match mode {
            ViewMode::Columns => Self::Columns(MillerView::new(dir, show_hidden)),
            ViewMode::Details => Self::Details(FileListView::new(dir, show_hidden)),
        }
    }

    fn widget(&self) -> gtk::Widget {
        match self {
            Self::Columns(view) => view.clone().upcast(),
            Self::Details(view) => view.clone().upcast(),
        }
    }

    fn directory(&self) -> Option<gio::File> {
        match self {
            Self::Columns(view) => view.directory(),
            Self::Details(view) => view.directory(),
        }
    }

    fn show_directory(&self, dir: &gio::File) {
        match self {
            Self::Columns(view) => view.show_directory(dir),
            Self::Details(view) => view.show_directory(dir),
        }
    }

    fn focus(&self) {
        match self {
            Self::Columns(view) => view.focus_list(),
            Self::Details(view) => view.focus_list(),
        }
    }

    fn set_show_hidden(&self, show: bool) {
        match self {
            Self::Columns(view) => view.set_show_hidden(show),
            Self::Details(view) => view.set_show_hidden(show),
        }
    }

    fn select_when_present(&self, file: &gio::File) {
        match self {
            Self::Columns(view) => view.select_when_present(file),
            Self::Details(view) => view.select_when_present(file),
        }
    }

    fn selected_infos(&self) -> Vec<SelectedInfo> {
        match self {
            Self::Columns(view) => view.selected_infos(),
            Self::Details(view) => view.selected_infos(),
        }
    }

    fn selected_files(&self) -> Vec<gio::File> {
        match self {
            Self::Columns(view) => view.selected_files(),
            Self::Details(view) => view.selected_files(),
        }
    }

    fn status(&self) -> ViewStatus {
        match self {
            Self::Columns(view) => view.status(),
            Self::Details(view) => view.status(),
        }
    }

    fn connect_status_changed<F: Fn() + 'static>(&self, f: F) {
        match self {
            Self::Columns(view) => view.connect_status_changed(move |_| f()),
            Self::Details(view) => view.connect_status_changed(move |_| f()),
        }
    }

    fn connect_directory_activated<F: Fn(&gio::File) + 'static>(&self, f: F) {
        match self {
            Self::Columns(view) => view.connect_directory_activated(move |_, dir| f(dir)),
            Self::Details(view) => view.connect_directory_activated(move |_, dir| f(dir)),
        }
    }
}

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::TabPage)]
    pub struct TabPage {
        /// Id de la vista (`ViewMode::id`). Es string para poder exponerla
        /// como acción de propiedad (`view.mode`) a los botones conmutados.
        #[property(get, set = Self::set_view_mode)]
        pub view_mode: RefCell<String>,
        /// Carpeta actual (para el título de la pestaña).
        #[property(get, nullable)]
        pub directory: RefCell<Option<gio::File>>,
        pub(super) view: RefCell<Option<PageView>>,
        pub view_slot: adw::Bin,
        /// Menú contextual (se crea al primer uso).
        pub context_menu: std::cell::OnceCell<gtk::PopoverMenu>,
        pub path_bar: PathBar,
        pub status_bar: StatusBar,
        /// Consulta en curso del espacio libre.
        pub free_space_query: RefCell<Option<glib::JoinHandle<()>>>,
        /// URIs visitadas (gio admite rutas no locales vía GVfs).
        pub history: RefCell<Option<History<String>>>,
        /// Ocultos visibles en esta pestaña; sobrevive al cambiar de vista.
        pub show_hidden: Cell<bool>,
    }

    impl TabPage {
        fn set_view_mode(&self, id: String) {
            let Some(mode) = ViewMode::from_id(&id) else {
                tracing::warn!("vista desconocida: {id}");
                return;
            };
            if *self.view_mode.borrow() == id {
                return;
            }
            self.view_mode.replace(id);
            let obj = self.obj();
            obj.switch_view(mode);
            obj.notify_view_mode();
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TabPage {
        const NAME: &'static str = "TlacuacheTabPage";
        type Type = super::TabPage;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.install_action("nav.up", None, |page, _, _| page.go_up());
            klass.install_action("nav.back", None, |page, _, _| page.go_back());
            klass.install_action("nav.forward", None, |page, _, _| page.go_forward());
            klass.install_action("nav.edit-path", None, |page, _, _| {
                page.imp().path_bar.start_editing();
            });
            // (x, y en coordenadas de la ventana, ¿sobre elementos?)
            let menu_param = <(f64, f64, bool)>::static_variant_type();
            klass.install_action("tab.context-menu", Some(&menu_param), |page, _, param| {
                if let Some((x, y, on_item)) = param.and_then(|p| p.get::<(f64, f64, bool)>()) {
                    page.show_context_menu(x, y, on_item);
                }
            });
            klass.install_action("view.toggle-hidden", None, |page, _, _| {
                page.toggle_show_hidden();
            });
            // Estado = propiedad `view-mode`; los botones usan action-target.
            klass.install_property_action("view.mode", "view-mode");

            // Solo actúan con el foco dentro de la pestaña (no en la
            // terminal). El entry de la ruta consume Backspace al editar.
            let alt = gdk::ModifierType::ALT_MASK;
            let ctrl = gdk::ModifierType::CONTROL_MASK;
            klass.add_binding_action(gdk::Key::BackSpace, gdk::ModifierType::empty(), "nav.up");
            klass.add_binding_action(gdk::Key::Up, alt, "nav.up");
            klass.add_binding_action(gdk::Key::Left, alt, "nav.back");
            klass.add_binding_action(gdk::Key::Right, alt, "nav.forward");
            klass.add_binding_action(gdk::Key::l, ctrl, "nav.edit-path");
            klass.add_binding_action(gdk::Key::h, ctrl, "view.toggle-hidden");
            klass.add_binding(gdk::Key::_1, ctrl, |page| {
                page.set_view_mode(ViewMode::Columns.id());
                glib::Propagation::Stop
            });
            klass.add_binding(gdk::Key::_2, ctrl, |page| {
                page.set_view_mode(ViewMode::Details.id());
                glib::Propagation::Stop
            });
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for TabPage {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
                std::sync::OnceLock::new();
            SIGNALS.get_or_init(|| {
                // Cambiaron los elementos o la selección de la vista.
                vec![glib::subclass::Signal::builder("status-changed").build()]
            })
        }

        fn dispose(&self) {
            if let Some(menu) = self.context_menu.get() {
                menu.unparent();
            }
        }
    }
    impl WidgetImpl for TabPage {}
    impl BoxImpl for TabPage {}
}

glib::wrapper! {
    pub struct TabPage(ObjectSubclass<imp::TabPage>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl TabPage {
    pub fn new(dir: &gio::File, show_hidden: bool, mode: ViewMode) -> Self {
        let page: Self = glib::Object::new();
        page.build(dir, show_hidden, mode);
        page
    }

    fn build(&self, dir: &gio::File, show_hidden: bool, mode: ViewMode) {
        let imp = self.imp();
        self.set_orientation(gtk::Orientation::Vertical);
        imp.show_hidden.set(show_hidden);

        let nav = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        nav.add_css_class("nav-bar");
        for (icon, action, tooltip) in [
            ("go-previous-symbolic", "nav.back", strings::NAV_BACK),
            ("go-next-symbolic", "nav.forward", strings::NAV_FORWARD),
            ("go-up-symbolic", "nav.up", strings::NAV_UP),
        ] {
            let button = gtk::Button::from_icon_name(icon);
            button.add_css_class("flat");
            button.set_action_name(Some(action));
            button.set_tooltip_text(Some(tooltip));
            button.set_focusable(false);
            nav.append(&button);
        }
        imp.path_bar.set_hexpand(true);
        nav.append(&imp.path_bar);
        nav.append(&view_switcher());
        // Alterna la terminal del panel (acción del `Pane`, como F4).
        let terminal = gtk::ToggleButton::builder()
            .icon_name("utilities-terminal-symbolic")
            .tooltip_text(strings::TOGGLE_TERMINAL)
            .action_name("pane.terminal")
            .focusable(false)
            .build();
        terminal.add_css_class("flat");
        nav.append(&terminal);

        imp.path_bar.connect_navigate(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_, dir| page.navigate_to(dir)
        ));
        imp.path_bar.connect_editing_done(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_| page.focus_view()
        ));

        let mouse_nav = gtk::GestureClick::new();
        mouse_nav.set_button(0);
        mouse_nav.connect_pressed(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |gesture, _, _, _| match gesture.current_button() {
                MOUSE_BUTTON_BACK => page.go_back(),
                MOUSE_BUTTON_FORWARD => page.go_forward(),
                _ => {}
            }
        ));
        self.add_controller(mouse_nav);

        imp.view_slot.set_vexpand(true);
        self.append(&nav);
        self.append(&imp.view_slot);
        self.append(&imp.status_bar);

        imp.history
            .replace(Some(History::new(dir.uri().to_string())));
        imp.directory.replace(Some(dir.clone()));
        imp.path_bar.set_directory(dir);
        imp.view_mode.replace(mode.id().to_owned());
        self.install_view(PageView::new(mode, dir, show_hidden));
        self.update_nav_actions();
        self.refresh_free_space(dir);
    }

    fn view(&self) -> Option<PageView> {
        self.imp().view.borrow().clone()
    }

    fn install_view(&self, view: PageView) {
        view.connect_directory_activated(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |dir| page.navigate_to(dir)
        ));
        view.connect_status_changed(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move || page.update_status()
        ));
        self.imp().view_slot.set_child(Some(&view.widget()));
        self.imp().view.replace(Some(view));
        self.update_status();
    }

    fn update_status(&self) {
        let status = self.view().map(|v| v.status()).unwrap_or_default();
        self.imp().status_bar.set_status(&status);
        self.emit_by_name::<()>("status-changed", &[]);
    }

    pub fn connect_status_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_closure(
            "status-changed",
            false,
            glib::closure_local!(move |page: &Self| f(page)),
        );
    }

    /// Consulta en segundo plano el espacio libre de la unidad de `dir`.
    fn refresh_free_space(&self, dir: &gio::File) {
        let imp = self.imp();
        if let Some(handle) = imp.free_space_query.take() {
            handle.abort();
        }
        let dir = dir.clone();
        let handle = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = page)]
            self,
            async move {
                let attrs = format!(
                    "{},{}",
                    gio::FILE_ATTRIBUTE_FILESYSTEM_FREE,
                    gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE
                );
                let space = dir
                    .query_filesystem_info_future(&attrs, glib::Priority::DEFAULT)
                    .await
                    .ok()
                    .filter(|info| info.has_attribute(gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE))
                    .map(|info| {
                        (
                            info.attribute_uint64(gio::FILE_ATTRIBUTE_FILESYSTEM_FREE),
                            info.attribute_uint64(gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE),
                        )
                    });
                page.imp().free_space_query.take();
                page.imp().status_bar.set_free_space(space);
            }
        ));
        imp.free_space_query.replace(Some(handle));
    }

    /// Reemplaza la vista por otra en la misma carpeta (sin tocar historial).
    fn switch_view(&self, mode: ViewMode) {
        let Some(dir) = self.current_directory() else {
            return;
        };
        self.install_view(PageView::new(mode, &dir, self.imp().show_hidden.get()));
        self.focus_view();
    }

    /// Selecciona `file` en cuanto aparezca en la vista.
    pub fn select_when_present(&self, file: &gio::File) {
        if let Some(view) = self.view() {
            view.select_when_present(file);
        }
    }

    /// Seleccionados con su tipo (menú contextual, abrir).
    pub fn selected_infos(&self) -> Vec<SelectedInfo> {
        self.view().map(|v| v.selected_infos()).unwrap_or_default()
    }

    /// Menú contextual en (x, y) de la ventana: de los elementos
    /// seleccionados o, si no, de la carpeta. Usa las acciones de la
    /// ventana, así que respeta qué está habilitado.
    fn show_context_menu(&self, x: f64, y: f64, on_item: bool) {
        let infos = if on_item {
            self.selected_infos()
        } else {
            Vec::new()
        };
        let menu = if infos.is_empty() {
            folder_menu()
        } else {
            items_menu(&infos)
        };
        let popover = self.imp().context_menu.get_or_init(|| {
            let popover = gtk::PopoverMenu::from_model(None::<&gio::MenuModel>);
            popover.set_parent(self);
            popover.set_has_arrow(false);
            popover.set_halign(gtk::Align::Start);
            popover
        });
        popover.set_menu_model(Some(&menu));
        let point = self
            .root()
            .and_then(|root| {
                root.compute_point(self, &gtk::graphene::Point::new(x as f32, y as f32))
            })
            .unwrap_or_else(|| gtk::graphene::Point::new(x as f32, y as f32));
        // Coordenadas a píxeles enteros para anclar el menú.
        let rect = gtk::gdk::Rectangle::new(point.x() as i32, point.y() as i32, 1, 1);
        popover.set_pointing_to(Some(&rect));
        popover.popup();
    }

    /// Archivos seleccionados en la vista.
    pub fn selected_files(&self) -> Vec<gio::File> {
        self.view().map(|v| v.selected_files()).unwrap_or_default()
    }

    /// Ocultos visibles en esta pestaña.
    pub fn show_hidden(&self) -> bool {
        self.imp().show_hidden.get()
    }

    /// Vista actual de la pestaña.
    pub fn mode(&self) -> ViewMode {
        ViewMode::from_id(&self.view_mode()).unwrap_or_default()
    }

    /// Lleva el foco del teclado a la vista de la carpeta.
    pub fn focus_view(&self) {
        if let Some(view) = self.view() {
            view.focus();
        }
    }

    fn toggle_show_hidden(&self) {
        let show = !self.imp().show_hidden.get();
        self.imp().show_hidden.set(show);
        if let Some(view) = self.view() {
            view.set_show_hidden(show);
        }
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

    fn current_directory(&self) -> Option<gio::File> {
        self.view().and_then(|v| v.directory())
    }

    fn go_up(&self) {
        if let Some(parent) = self.current_directory().and_then(|d| d.parent()) {
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
        if let Some(view) = self.view() {
            view.show_directory(dir);
        }
        self.imp().path_bar.set_directory(dir);
        self.imp().directory.replace(Some(dir.clone()));
        self.notify_directory();
        self.update_nav_actions();
        self.refresh_free_space(dir);
    }

    fn update_nav_actions(&self) {
        let (back, forward) = self
            .imp()
            .history
            .borrow()
            .as_ref()
            .map_or((false, false), |h| (h.can_go_back(), h.can_go_forward()));
        let has_parent = self.current_directory().and_then(|d| d.parent()).is_some();
        self.action_set_enabled("nav.back", back);
        self.action_set_enabled("nav.forward", forward);
        self.action_set_enabled("nav.up", has_parent);
    }
}

/// Botones conmutados de vista, ligados a la acción `view.mode`.
fn view_switcher() -> gtk::Box {
    let switcher = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    switcher.add_css_class("linked");
    for mode in ViewMode::ALL {
        let (icon, tooltip) = match mode {
            ViewMode::Columns => ("view-columns-symbolic", strings::VIEW_COLUMNS),
            ViewMode::Details => ("view-list-symbolic", strings::VIEW_DETAILS),
        };
        let button = gtk::ToggleButton::builder()
            .icon_name(icon)
            .tooltip_text(tooltip)
            .action_name("view.mode")
            .action_target(&mode.id().to_variant())
            .focusable(false)
            .build();
        button.add_css_class("flat");
        switcher.append(&button);
    }
    switcher
}

fn section(items: &[(&str, &str)]) -> gio::Menu {
    let menu = gio::Menu::new();
    for (label, action) in items {
        menu.append(Some(label), Some(action));
    }
    menu
}

/// Máximo de apps en «Abrir con».
const OPEN_WITH_LIMIT: usize = 8;

fn items_menu(infos: &[SelectedInfo]) -> gio::Menu {
    let menu = gio::Menu::new();

    let open = section(&[(strings::ACTION_OPEN, "files.open")]);
    let content_type = infos
        .first()
        .and_then(|i| i.content_type.clone())
        .unwrap_or_else(|| "application/octet-stream".to_owned());
    let open_with = gio::Menu::new();
    for app in gio::AppInfo::recommended_for_type(&content_type)
        .into_iter()
        .take(OPEN_WITH_LIMIT)
    {
        let Some(id) = app.id() else {
            continue;
        };
        let item = gio::MenuItem::new(Some(&app.display_name()), None);
        item.set_action_and_target_value(Some("files.open-with"), Some(&id.to_variant()));
        open_with.append_item(&item);
    }
    open_with.append(
        Some(strings::OPEN_WITH_OTHER),
        Some("files.open-with-other"),
    );
    open.append_submenu(Some(strings::OPEN_WITH), &open_with);
    menu.append_section(None, &open);

    menu.append_section(
        None,
        &section(&[
            (strings::ACTION_CUT, "clipboard.cut"),
            (strings::ACTION_COPY, "clipboard.copy"),
            (strings::ACTION_PASTE, "clipboard.paste"),
        ]),
    );
    menu.append_section(
        None,
        &section(&[
            (strings::ACTION_RENAME_ELLIPSIS, "files.rename"),
            (strings::ACTION_TRASH, "files.trash"),
            (strings::ACTION_DELETE_ELLIPSIS, "files.delete"),
        ]),
    );
    let extra = gio::Menu::new();
    if infos.iter().any(|i| i.is_dir) {
        extra.append(Some(strings::ADD_TO_FAVORITES), Some("files.add-favorite"));
    }
    extra.append(Some(strings::COPY_PATH), Some("files.copy-path"));
    menu.append_section(None, &extra);
    menu.append_section(None, &section(&[(strings::PROPERTIES, "files.properties")]));
    menu
}

fn folder_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append_section(
        None,
        &section(&[(strings::ACTION_PASTE, "clipboard.paste")]),
    );
    menu.append_section(
        None,
        &section(&[
            (strings::NEW_FOLDER_ELLIPSIS, "files.new-folder"),
            (strings::NEW_FILE_ELLIPSIS, "files.new-file"),
        ]),
    );
    menu.append_section(
        None,
        &section(&[(strings::TOGGLE_HIDDEN, "view.toggle-hidden")]),
    );
    menu.append_section(
        None,
        &section(&[(strings::FOLDER_PROPERTIES, "files.folder-properties")]),
    );
    menu
}
