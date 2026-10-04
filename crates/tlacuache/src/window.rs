//! Ventana principal: header bar y área de paneles (uno o dos, F3). Lleva
//! la cuenta del panel activo, que Tab alterna y que las operaciones entre
//! paneles (F5/F6) usarán como origen.

use std::cell::{Cell, OnceCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use tlacuache_core::config::Config;

use crate::strings;
use crate::ui::pane::Pane;

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::TlacuacheWindow)]
    pub struct TlacuacheWindow {
        pub toast_overlay: adw::ToastOverlay,
        pub toolbar: adw::ToolbarView,
        pub paned: gtk::Paned,
        /// Izquierdo y derecho.
        pub panes: OnceCell<[Pane; 2]>,
        /// Índice del panel activo (0 izquierdo, 1 derecho).
        pub active: Cell<usize>,
        /// Modo doble panel. Es propiedad para exponerla como acción
        /// conmutable (`panes.dual`) al botón de la barra.
        #[property(get, set = Self::set_dual_pane)]
        pub dual_pane: Cell<bool>,
    }

    impl TlacuacheWindow {
        fn set_dual_pane(&self, dual: bool) {
            if self.dual_pane.replace(dual) == dual {
                return;
            }
            let obj = self.obj();
            obj.apply_dual_pane();
            obj.notify_dual_pane();
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TlacuacheWindow {
        const NAME: &'static str = "TlacuacheWindow";
        type Type = super::TlacuacheWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.install_property_action("panes.dual", "dual-pane");
            klass.add_binding(gdk::Key::F3, gdk::ModifierType::empty(), |window| {
                window.set_dual_pane(!window.dual_pane());
                glib::Propagation::Stop
            });
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for TlacuacheWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.set_title(Some(strings::APP_NAME));
            obj.set_default_size(1200, 800);

            let dual = gtk::ToggleButton::builder()
                .icon_name("view-dual-symbolic")
                .tooltip_text(strings::DUAL_PANE)
                .action_name("panes.dual")
                .focusable(false)
                .build();
            let header = adw::HeaderBar::new();
            header.pack_end(&dual);

            self.toolbar.add_top_bar(&header);
            self.toast_overlay.set_child(Some(&self.toolbar));
            obj.set_content(Some(&self.toast_overlay));

            // Captura: Tab cambia de panel antes de que GTK lo use para
            // mover el foco entre widgets.
            let keys = gtk::EventControllerKey::new();
            keys.set_propagation_phase(gtk::PropagationPhase::Capture);
            keys.connect_key_pressed(glib::clone!(
                #[weak(rename_to = window)]
                obj,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |_, key, _, mods| window.on_key(key, mods)
            ));
            obj.add_controller(keys);
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
        window.build_panes(&config, start_dir);
        window
    }

    fn build_panes(&self, config: &Rc<Config>, start_dir: &gio::File) {
        let imp = self.imp();
        let panes = [
            Pane::new(start_dir, config.clone()),
            Pane::new(start_dir, config.clone()),
        ];
        for (index, pane) in panes.iter().enumerate() {
            let focus = gtk::EventControllerFocus::new();
            focus.connect_enter(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_| window.set_active_pane(index)
            ));
            pane.add_controller(focus);
        }

        let paned = &imp.paned;
        paned.set_orientation(gtk::Orientation::Horizontal);
        paned.set_start_child(Some(&panes[0]));
        paned.set_end_child(Some(&panes[1]));
        paned.set_shrink_start_child(false);
        paned.set_shrink_end_child(false);
        // Mitad y mitad la primera vez que se muestra.
        paned.connect_map(|paned| {
            glib::idle_add_local_once(glib::clone!(
                #[weak]
                paned,
                move || {
                    if paned.position() == 0 || paned.width() > 0 {
                        paned.set_position(paned.width() / 2);
                    }
                }
            ));
        });
        imp.toolbar.set_content(Some(paned));

        let _ = imp.panes.set(panes);
        imp.dual_pane.set(config.general.dual_pane);
        self.apply_dual_pane();
        self.set_active_pane(0);
    }

    fn pane(&self, index: usize) -> Option<&Pane> {
        self.imp().panes.get().and_then(|panes| panes.get(index))
    }

    /// Muestra u oculta el panel derecho (conserva sus pestañas).
    fn apply_dual_pane(&self) {
        let dual = self.dual_pane();
        if let Some(right) = self.pane(1) {
            right.set_visible(dual);
        }
        if !dual && self.imp().active.get() == 1 {
            if let Some(left) = self.pane(0) {
                left.focus_current();
            }
            self.set_active_pane(0);
        }
        self.update_active_style();
    }

    fn set_active_pane(&self, index: usize) {
        self.imp().active.set(index);
        self.update_active_style();
    }

    /// Acento en el panel activo, solo en modo dual (con uno no aporta).
    fn update_active_style(&self) {
        let active = self.imp().active.get();
        let dual = self.dual_pane();
        for index in 0..2 {
            if let Some(pane) = self.pane(index) {
                if dual && index == active {
                    pane.add_css_class("active-pane");
                } else {
                    pane.remove_css_class("active-pane");
                }
            }
        }
    }

    fn switch_active_pane(&self) {
        let other = 1 - self.imp().active.get();
        if let Some(pane) = self.pane(other) {
            // El foco dispara `set_active_pane` vía el controlador de foco.
            pane.focus_current();
        }
    }

    fn on_key(&self, key: gdk::Key, mods: gdk::ModifierType) -> glib::Propagation {
        let modifiers = gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::ALT_MASK
            | gdk::ModifierType::SHIFT_MASK
            | gdk::ModifierType::SUPER_MASK;
        if key != gdk::Key::Tab || mods.intersects(modifiers) || !self.dual_pane() {
            return glib::Propagation::Proceed;
        }
        // Escribiendo texto (p. ej. la ruta), Tab autocompleta.
        if gtk::prelude::RootExt::focus(self)
            .is_some_and(|w| w.is::<gtk::Text>() || w.ancestor(gtk::Entry::static_type()).is_some())
        {
            return glib::Propagation::Proceed;
        }
        self.switch_active_pane();
        glib::Propagation::Stop
    }

    /// Muestra un aviso no bloqueante en la parte inferior de la ventana.
    pub fn show_toast(&self, text: &str) {
        let toast = adw::Toast::new(text);
        toast.set_timeout(10);
        self.imp().toast_overlay.add_toast(toast);
    }
}

/// Muestra un toast en la ventana que contiene `widget`, si la hay.
pub fn show_toast_from(widget: &impl IsA<gtk::Widget>, text: &str) {
    if let Some(window) = widget.root().and_downcast::<TlacuacheWindow>() {
        window.show_toast(text);
    }
}
