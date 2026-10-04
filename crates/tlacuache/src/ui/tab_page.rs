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
