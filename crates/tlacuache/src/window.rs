//! Ventana principal: header bar + (más adelante) sidebar y área de paneles.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::strings;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TlacuacheWindow;

    #[glib::object_subclass]
    impl ObjectSubclass for TlacuacheWindow {
        const NAME: &'static str = "TlacuacheWindow";
        type Type = super::TlacuacheWindow;
        type ParentType = adw::ApplicationWindow;
    }

    impl ObjectImpl for TlacuacheWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.set_title(Some(strings::APP_NAME));
            obj.set_default_size(1200, 800);

            let header = adw::HeaderBar::new();
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);

            let toolbar = adw::ToolbarView::new();
            toolbar.add_top_bar(&header);
            toolbar.set_content(Some(&content));

            obj.set_content(Some(&toolbar));
        }
    }

    impl WidgetImpl for TlacuacheWindow {}
    impl WindowImpl for TlacuacheWindow {}
    impl ApplicationWindowImpl for TlacuacheWindow {}
    impl AdwApplicationWindowImpl for TlacuacheWindow {}
}

glib::wrapper! {
    pub struct TlacuacheWindow(ObjectSubclass<imp::TlacuacheWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
            gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl TlacuacheWindow {
    pub fn new(app: &adw::Application) -> Self {
        glib::Object::builder().property("application", app).build()
    }
}
