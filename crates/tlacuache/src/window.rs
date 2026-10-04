//! Ventana principal: header bar + (más adelante) sidebar y área de paneles.

use std::cell::OnceCell;
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use tlacuache_core::config::Config;

use crate::strings;
use crate::ui::list_view::FileListView;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TlacuacheWindow {
        pub toast_overlay: OnceCell<adw::ToastOverlay>,
        pub toolbar: OnceCell<adw::ToolbarView>,
        pub title: adw::WindowTitle,
    }

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
            self.title.set_title(strings::APP_NAME);
            obj.set_default_size(1200, 800);

            let header = adw::HeaderBar::new();
            header.set_title_widget(Some(&self.title));
            let toolbar = adw::ToolbarView::new();
            toolbar.add_top_bar(&header);

            let toast_overlay = adw::ToastOverlay::new();
            toast_overlay.set_child(Some(&toolbar));
            obj.set_content(Some(&toast_overlay));
            let _ = self.toast_overlay.set(toast_overlay);
            let _ = self.toolbar.set(toolbar);
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
    pub fn new(app: &adw::Application, config: Rc<Config>, start_dir: &gio::File) -> Self {
        let window: Self = glib::Object::builder().property("application", app).build();
        // Por ahora una sola lista; paneles y pestañas llegan en hitos 2 y 3.
        let list = FileListView::new(start_dir, config.general.show_hidden);
        // Subtítulo con la ruta actual hasta que exista la barra de ruta (1.4).
        list.connect_directory_notify(glib::clone!(
            #[weak]
            window,
            move |list| window.update_subtitle(list.directory().as_ref())
        ));
        window.update_subtitle(Some(start_dir));
        if let Some(toolbar) = window.imp().toolbar.get() {
            toolbar.set_content(Some(&list));
        }
        window
    }

    fn update_subtitle(&self, dir: Option<&gio::File>) {
        let text = dir.map_or_else(String::new, |d| match d.path() {
            Some(path) => path.display().to_string(),
            None => d.uri().to_string(),
        });
        self.imp().title.set_subtitle(&text);
    }

    /// Muestra un aviso no bloqueante en la parte inferior de la ventana.
    pub fn show_toast(&self, text: &str) {
        if let Some(overlay) = self.imp().toast_overlay.get() {
            let toast = adw::Toast::new(text);
            toast.set_timeout(10);
            overlay.add_toast(toast);
        }
    }
}
