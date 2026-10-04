//! Barra de estado de una pestaña: elementos y selección a la izquierda,
//! espacio libre de la unidad a la derecha.

use adw::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{glib, pango};
use tlacuache_core::summary::ViewStatus;

use crate::strings;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct StatusBar {
        pub items: gtk::Label,
        pub free_space: gtk::Label,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StatusBar {
        const NAME: &'static str = "TlacuacheStatusBar";
        type Type = super::StatusBar;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for StatusBar {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_orientation(gtk::Orientation::Horizontal);
            obj.set_spacing(12);
            obj.add_css_class("status-bar");

            self.items.set_xalign(0.0);
            self.items.set_hexpand(true);
            self.items.set_ellipsize(pango::EllipsizeMode::End);
            self.free_space.set_xalign(1.0);
            for label in [&self.items, &self.free_space] {
                label.add_css_class("numeric");
                obj.append(label);
            }
        }
    }

    impl WidgetImpl for StatusBar {}
    impl BoxImpl for StatusBar {}
}

glib::wrapper! {
    pub struct StatusBar(ObjectSubclass<imp::StatusBar>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for StatusBar {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl StatusBar {
    pub fn set_status(&self, status: &ViewStatus) {
        self.imp().items.set_text(&strings::status_items(status));
    }

    /// `(libre, total)` en bytes, o `None` si el sistema de archivos no lo
    /// informa (p. ej. algunos montajes remotos).
    pub fn set_free_space(&self, space: Option<(u64, u64)>) {
        let text = space.map_or_else(String::new, |(free, size)| strings::free_space(free, size));
        self.imp().free_space.set_text(&text);
    }
}
