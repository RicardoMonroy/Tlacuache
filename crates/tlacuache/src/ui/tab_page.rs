//! Contenido de una pestaña: barra de navegación (‹ › ↑ + ruta) y la vista
//! de la carpeta. Es dueña del historial y de los atajos de navegación.

use std::cell::{OnceCell, RefCell};

use adw::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use tlacuache_core::history::History;

use crate::strings;
use crate::ui::list_view::FileListView;
use crate::ui::path_bar::PathBar;

/// Botones laterales del ratón (atrás/adelante).
const MOUSE_BUTTON_BACK: u32 = 8;
const MOUSE_BUTTON_FORWARD: u32 = 9;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TabPage {
        pub view: OnceCell<FileListView>,
        pub path_bar: PathBar,
        /// URIs visitadas (gio admite rutas no locales vía GVfs).
        pub history: RefCell<Option<History<String>>>,
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

            // Solo actúan con el foco dentro de la pestaña (no en la
            // terminal). El entry de la ruta consume Backspace al editar.
            let alt = gdk::ModifierType::ALT_MASK;
            klass.add_binding_action(gdk::Key::BackSpace, gdk::ModifierType::empty(), "nav.up");
            klass.add_binding_action(gdk::Key::Up, alt, "nav.up");
            klass.add_binding_action(gdk::Key::Left, alt, "nav.back");
            klass.add_binding_action(gdk::Key::Right, alt, "nav.forward");
            klass.add_binding_action(
                gdk::Key::l,
                gdk::ModifierType::CONTROL_MASK,
                "nav.edit-path",
            );
        }
    }

    impl ObjectImpl for TabPage {}
    impl WidgetImpl for TabPage {}
    impl BoxImpl for TabPage {}
}

glib::wrapper! {
    pub struct TabPage(ObjectSubclass<imp::TabPage>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl TabPage {
    pub fn new(dir: &gio::File, show_hidden: bool) -> Self {
        let page: Self = glib::Object::new();
        page.build(dir, show_hidden);
        page
    }

    fn build(&self, dir: &gio::File, show_hidden: bool) {
        let imp = self.imp();
        self.set_orientation(gtk::Orientation::Vertical);

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

        let view = FileListView::new(dir, show_hidden);
        view.connect_directory_activated(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_, dir| page.navigate_to(dir)
        ));
        imp.path_bar.connect_navigate(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_, dir| page.navigate_to(dir)
        ));
        imp.path_bar.connect_editing_done(glib::clone!(
            #[weak]
            view,
            move |_| view.focus_list()
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

        self.append(&nav);
        self.append(&view);

        imp.history
            .replace(Some(History::new(dir.uri().to_string())));
        imp.path_bar.set_directory(dir);
        let _ = imp.view.set(view);
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

    fn current_directory(&self) -> Option<gio::File> {
        self.imp().view.get().and_then(|v| v.directory())
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
        let imp = self.imp();
        if let Some(view) = imp.view.get() {
            view.show_directory(dir);
        }
        imp.path_bar.set_directory(dir);
        self.update_nav_actions();
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
