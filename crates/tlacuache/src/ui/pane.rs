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
use tlacuache_core::config::{Config, ViewMode};
use tlacuache_core::session::{PaneSession, TabSession};

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

    impl ObjectImpl for Pane {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
                std::sync::OnceLock::new();
            SIGNALS.get_or_init(|| {
                // Cambió la selección de la pestaña actual (o la pestaña).
                vec![glib::subclass::Signal::builder("selection-changed").build()]
            })
        }
    }
    impl WidgetImpl for Pane {}
    impl BoxImpl for Pane {}
}

glib::wrapper! {
    pub struct Pane(ObjectSubclass<imp::Pane>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Pane {
    /// Panel sin pestañas: añadirlas con `add_tab` o `restore`.
    pub fn new(config: Rc<Config>) -> Self {
        let pane: Self = glib::Object::new();
        let _ = pane.imp().config.set(config);
        pane.build();
        pane
    }

    fn build(&self) {
        let imp = self.imp();
        self.set_orientation(gtk::Orientation::Vertical);
        self.add_css_class("pane");

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
        imp.tab_view.connect_selected_page_notify(glib::clone!(
            #[weak(rename_to = pane)]
            self,
            move |view| {
                if let Some(page) = view
                    .selected_page()
                    .map(|p| p.child())
                    .and_downcast::<TabPage>()
                {
                    page.focus_view();
                }
                pane.emit_by_name::<()>("selection-changed", &[]);
            }
        ));

        self.append(&imp.tab_bar);
        self.append(&imp.tab_view);
    }

    /// Abre una pestaña en `dir` con la vista y ocultos de la config, y la
    /// selecciona.
    pub fn add_tab(&self, dir: &gio::File) {
        let Some(config) = self.imp().config.get() else {
            return;
        };
        let (mode, show_hidden) = (config.general.default_view, config.general.show_hidden);
        self.add_tab_with(dir, mode, show_hidden);
    }

    fn add_tab_with(&self, dir: &gio::File, mode: ViewMode, show_hidden: bool) {
        let imp = self.imp();
        let page = TabPage::new(dir, show_hidden, mode);
        page.connect_status_changed(glib::clone!(
            #[weak(rename_to = pane)]
            self,
            move |page| {
                if pane.current_page().as_ref() == Some(page) {
                    pane.emit_by_name::<()>("selection-changed", &[]);
                }
            }
        ));
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

    /// Restaura las pestañas de una sesión. Devuelve `false` si no había
    /// ninguna (el llamador abre una por defecto).
    pub fn restore(&self, session: &PaneSession) -> bool {
        for tab in &session.tabs {
            self.add_tab_with(&gio::File::for_uri(&tab.uri), tab.view, tab.show_hidden);
        }
        let view = &self.imp().tab_view;
        if let Ok(selected) = i32::try_from(session.selected)
            && selected < view.n_pages()
        {
            view.set_selected_page(&view.nth_page(selected));
        }
        !session.tabs.is_empty()
    }

    /// Estado actual de las pestañas para guardar la sesión.
    pub fn session(&self) -> PaneSession {
        let view = &self.imp().tab_view;
        let tabs = (0..view.n_pages())
            .filter_map(|i| view.nth_page(i).child().downcast::<TabPage>().ok())
            .filter_map(|page| {
                Some(TabSession {
                    uri: page.directory()?.uri().to_string(),
                    view: page.mode(),
                    show_hidden: page.show_hidden(),
                })
            })
            .collect();
        let selected = view
            .selected_page()
            .and_then(|page| usize::try_from(view.page_position(&page)).ok())
            .unwrap_or(0);
        PaneSession { selected, tabs }
    }

    /// Navega la pestaña seleccionada a `dir` y le da el foco.
    pub fn navigate(&self, dir: &gio::File) {
        if let Some(page) = self.current_page() {
            page.navigate_to(dir);
            page.focus_view();
        }
    }

    /// Lleva el foco a la vista de la pestaña seleccionada.
    pub fn focus_current(&self) {
        if let Some(page) = self.current_page() {
            page.focus_view();
        }
    }

    pub fn connect_selection_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_closure(
            "selection-changed",
            false,
            glib::closure_local!(move |pane: &Self| f(pane)),
        );
    }

    /// Selecciona `file` en la pestaña actual en cuanto aparezca.
    pub fn select_when_present(&self, file: &gio::File) {
        if let Some(page) = self.current_page() {
            page.select_when_present(file);
        }
    }

    /// Seleccionados con su tipo en la pestaña actual.
    pub fn selected_infos(&self) -> Vec<crate::fs::file_item::SelectedInfo> {
        self.current_page()
            .map(|p| p.selected_infos())
            .unwrap_or_default()
    }

    /// Archivos seleccionados en la pestaña actual.
    pub fn selected_files(&self) -> Vec<gio::File> {
        self.current_page()
            .map(|p| p.selected_files())
            .unwrap_or_default()
    }

    /// Carpeta de la pestaña seleccionada.
    pub fn current_directory(&self) -> Option<gio::File> {
        self.current_page().and_then(|p| p.directory())
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
