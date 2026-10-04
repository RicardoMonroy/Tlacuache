//! Franja central entre los dos paneles con las acciones hacia el otro
//! panel (Copiar → / Mover →). Las flechas apuntan del panel activo al
//! otro; los botones usan las acciones de la ventana, que se deshabilitan
//! cuando no hay selección.

use adw::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;

use crate::strings;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PaneActions {
        /// Flechas de dirección de cada botón.
        pub arrows: std::cell::RefCell<Vec<gtk::Image>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PaneActions {
        const NAME: &'static str = "TlacuachePaneActions";
        type Type = super::PaneActions;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for PaneActions {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }
    }

    impl WidgetImpl for PaneActions {}
    impl BoxImpl for PaneActions {}
}

glib::wrapper! {
    pub struct PaneActions(ObjectSubclass<imp::PaneActions>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for PaneActions {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PaneActions {
    fn setup(&self) {
        self.set_orientation(gtk::Orientation::Vertical);
        self.set_spacing(8);
        self.set_valign(gtk::Align::Center);
        self.add_css_class("pane-actions");
        // Toda la franja sirve para redimensionar los paneles (ver ventana).
        self.set_cursor_from_name(Some("col-resize"));

        let mut arrows = Vec::new();
        for (icon, action, tooltip) in [
            (
                "edit-copy-symbolic",
                "panes.copy-to-other",
                strings::COPY_TO_OTHER,
            ),
            (
                "edit-cut-symbolic",
                "panes.move-to-other",
                strings::MOVE_TO_OTHER,
            ),
        ] {
            let arrow = gtk::Image::from_icon_name("go-next-symbolic");
            let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
            content.append(&gtk::Image::from_icon_name(icon));
            content.append(&arrow);
            let button = gtk::Button::builder()
                .child(&content)
                .action_name(action)
                .tooltip_text(tooltip)
                .focusable(false)
                .build();
            button.add_css_class("flat");
            button.set_cursor_from_name(Some("default"));
            self.append(&button);
            arrows.push(arrow);
        }
        self.imp().arrows.replace(arrows);
    }

    /// `true` si el activo es el panel izquierdo (las flechas van →).
    pub fn set_left_active(&self, left: bool) {
        let icon = if left {
            "go-next-symbolic"
        } else {
            "go-previous-symbolic"
        };
        for arrow in self.imp().arrows.borrow().iter() {
            arrow.set_icon_name(Some(icon));
        }
    }
}
