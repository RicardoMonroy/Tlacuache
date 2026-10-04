//! Panel: pestañas (`adw::TabBar` + `adw::TabView`) con un `TabPage` cada
//! una. Más adelante también tendrá vista previa y terminal.
//!
//! `adw::TabView` ya trae Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+RePág/AvPág y
//! Alt+1…9; aquí se añaden Ctrl+T y Ctrl+W, con alcance de panel para que en
//! el modo dual cada lado tenga los suyos.

use std::cell::OnceCell;
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use tlacuache_core::config::Config;

use crate::fs::display::{display_name, display_path};
use crate::strings;
use crate::ui::tab_page::TabPage;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Pane {
        pub tab_view: adw::TabView,
        pub tab_bar: adw::TabBar,
        pub config: OnceCell<Rc<Config>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Pane {
        const NAME: &'static str = "TlacuachePane";
        type Type = super::Pane;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.install_action("pane.new-tab", None, |pane, _, _| pane.new_tab());
            klass.install_action("pane.close-tab", None, |pane, _, _| pane.close_tab());

            let ctrl = gdk::ModifierType::CONTROL_MASK;
            klass.add_binding_action(gdk::Key::t, ctrl, "pane.new-tab");
            klass.add_binding_action(gdk::Key::w, ctrl, "pane.close-tab");
        }
    }

    impl ObjectImpl for Pane {}
    impl WidgetImpl for Pane {}
    impl BoxImpl for Pane {}
}

glib::wrapper! {
    pub struct Pane(ObjectSubclass<imp::Pane>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Pane {
    pub fn new(dir: &gio::File, config: Rc<Config>) -> Self {
        let pane: Self = glib::Object::new();
        let _ = pane.imp().config.set(config);
        pane.build();
        pane.add_tab(dir);
        pane
    }

    fn build(&self) {
        let imp = self.imp();
        self.set_orientation(gtk::Orientation::Vertical);

        let new_tab = gtk::Button::from_icon_name("tab-new-symbolic");
        new_tab.add_css_class("flat");
        new_tab.set_action_name(Some("pane.new-tab"));
        new_tab.set_tooltip_text(Some(strings::TAB_NEW));

        imp.tab_bar.set_view(Some(&imp.tab_view));
        // Siempre visible, también con una sola pestaña (como en la spec).
        imp.tab_bar.set_autohide(false);
        imp.tab_bar.set_end_action_widget(Some(&new_tab));

        imp.tab_view.set_vexpand(true);
        // La última pestaña no se cierra: el panel nunca queda vacío.
        imp.tab_view.connect_close_page(|view, page| {
            view.close_page_finish(page, view.n_pages() > 1);
            glib::Propagation::Stop
        });
        imp.tab_view.connect_selected_page_notify(|view| {
            if let Some(page) = view
                .selected_page()
                .map(|p| p.child())
                .and_downcast::<TabPage>()
            {
                page.focus_view();
            }
        });

        self.append(&imp.tab_bar);
        self.append(&imp.tab_view);
    }

    /// Abre una pestaña en `dir` y la selecciona.
    pub fn add_tab(&self, dir: &gio::File) {
        let imp = self.imp();
        let Some(config) = imp.config.get() else {
            return;
        };
        let page = TabPage::new(dir, config.general.show_hidden, config.general.default_view);
        let tab = imp.tab_view.append(&page);
        update_tab_title(&tab, dir);
        page.connect_directory_notify(glib::clone!(
            #[weak]
            tab,
            move |page| {
                if let Some(dir) = page.directory() {
                    update_tab_title(&tab, &dir);
                }
            }
        ));
        imp.tab_view.set_selected_page(&tab);
    }

    fn current_page(&self) -> Option<TabPage> {
        self.imp()
            .tab_view
            .selected_page()
            .map(|p| p.child())
            .and_downcast()
    }

    /// Nueva pestaña en la carpeta de la actual.
    fn new_tab(&self) {
        let dir = self
            .current_page()
            .and_then(|p| p.directory())
            .unwrap_or_else(|| gio::File::for_path(glib::home_dir()));
        self.add_tab(&dir);
    }

    fn close_tab(&self) {
        let view = &self.imp().tab_view;
        if let Some(page) = view.selected_page() {
            view.close_page(&page);
        }
    }
}

fn update_tab_title(tab: &adw::TabPage, dir: &gio::File) {
    tab.set_title(&display_name(dir));
    tab.set_tooltip(&display_path(dir));
}
