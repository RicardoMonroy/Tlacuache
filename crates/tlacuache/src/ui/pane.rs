//! Panel: pestañas (`TabStrip` propia + `adw::TabView`) con un `TabPage` cada
//! una, vista previa (Espacio) y terminal (F4), cada una en un `gtk::Paned`
//! vertical: pestañas / vista previa / terminal.
//!
//! `adw::TabView` ya trae Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+RePág/AvPág y
//! Alt+1…9; aquí se añaden Ctrl+T y Ctrl+W, con alcance de panel para que en
//! el modo dual cada lado tenga los suyos.

use std::cell::OnceCell;
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use tlacuache_core::config::{Config, PreviewPosition, ViewMode};
use tlacuache_core::keymap::Action;
use tlacuache_core::preview;
use tlacuache_core::session::{PaneSession, TabSession};

use crate::fs::display::{display_name, display_path};
use crate::strings;
use crate::ui::Accel;
use crate::ui::preview::PreviewPane;
use crate::ui::tab_page::TabPage;
use crate::ui::tab_strip::TabStrip;
use crate::ui::terminal::TerminalView;
use crate::ui::theme_manager;

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Pane)]
    pub struct Pane {
        /// Terminal visible. Propiedad para exponerla como acción
        /// conmutable (`pane.terminal`) al botón de la barra.
        #[property(get, set = Self::set_terminal_visible)]
        pub terminal_visible: std::cell::Cell<bool>,
        /// Vista previa visible (acción `pane.preview`).
        #[property(get, set = Self::set_preview_visible)]
        pub preview_visible: std::cell::Cell<bool>,
        pub tab_view: adw::TabView,
        pub tab_bar: TabStrip,
        pub config: OnceCell<Rc<Config>>,
        /// (Pestañas | vista previa) | terminal.
        pub split: gtk::Paned,
        /// Pestañas | vista previa.
        pub preview_split: gtk::Paned,
        pub preview: PreviewPane,
        /// Alto de las pestañas recordado al ocultar la vista previa.
        pub preview_position: std::cell::Cell<Option<i32>>,
        /// Terminal del panel: se crea en el primer F4.
        pub terminal: std::cell::RefCell<Option<TerminalView>>,
        /// Es el panel activo de la ventana.
        pub active: std::cell::Cell<bool>,
        /// La terminal tiene el foco.
        pub terminal_focused: std::cell::Cell<bool>,
        /// Atajos configurables: alternar y salir de la terminal.
        pub accels: OnceCell<(Accel, Accel)>,
        /// Altura de la zona de pestañas recordada al ocultar la terminal.
        pub split_position: std::cell::Cell<Option<i32>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Pane {
        const NAME: &'static str = "TlacuachePane";
        type Type = super::Pane;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.install_action("pane.new-tab", None, |pane, _, _| pane.new_tab());
            klass.install_action("pane.close-tab", None, |pane, _, _| pane.close_tab());
            klass.install_property_action("pane.terminal", "terminal-visible");
            klass.install_property_action("pane.preview", "preview-visible");

            let ctrl = gdk::ModifierType::CONTROL_MASK;
            klass.add_binding_action(gdk::Key::t, ctrl, "pane.new-tab");
            klass.add_binding_action(gdk::Key::w, ctrl, "pane.close-tab");
        }
    }

    impl Pane {
        fn set_preview_visible(&self, visible: bool) {
            if self.preview_visible.get() != visible {
                self.obj().toggle_preview();
            }
        }

        fn set_terminal_visible(&self, visible: bool) {
            if self.terminal_visible.get() != visible {
                self.obj().toggle_terminal();
            }
        }
    }

    #[glib::derived_properties]
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
        self.add_css_class("tl-pane");

        let new_tab = gtk::Button::from_icon_name("tab-new-symbolic");
        new_tab.add_css_class("flat");
        new_tab.set_action_name(Some("pane.new-tab"));
        new_tab.set_tooltip_text(Some(strings::TAB_NEW));

        imp.tab_bar.set_view(&imp.tab_view);
        imp.tab_bar.set_end_widget(&new_tab);

        imp.tab_view.set_vexpand(true);
        // Señales de cada pestaña: se conectan al entrar en este panel (al
        // crearla o al arrastrarla desde el otro) y se sueltan al salir.
        imp.tab_view.connect_page_attached(glib::clone!(
            #[weak(rename_to = pane)]
            self,
            move |_, tab, _| pane.attach_tab(tab)
        ));
        imp.tab_view.connect_page_detached(|_, tab, _| {
            if let Ok(page) = tab.child().downcast::<TabPage>() {
                page.disconnect_pane_handlers();
            }
        });
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
                pane.sync_terminal();
                pane.update_preview();
            }
        ));

        // Pestañas y vista previa (oculta hasta Espacio), debajo o a la
        // derecha según la config.
        imp.preview_split.set_start_child(Some(&imp.tab_view));
        imp.preview_split.set_end_child(Some(&imp.preview));
        imp.preview_split.set_resize_end_child(false);
        imp.preview_split.set_shrink_start_child(false);
        imp.preview_split.set_shrink_end_child(false);
        imp.preview.set_visible(false);

        // Y la terminal al fondo (oculta hasta el primer F4).
        imp.split.set_orientation(gtk::Orientation::Vertical);
        imp.split.set_start_child(Some(&imp.preview_split));
        imp.split.set_resize_end_child(false);
        imp.split.set_shrink_start_child(false);
        imp.split.set_shrink_end_child(false);
        imp.split.set_vexpand(true);

        if let Some(config) = imp.config.get() {
            imp.preview
                .set_max_text_bytes(config.preview.max_text_bytes);
            self.set_preview_position(config.preview.position);
            let _ = imp.accels.set((
                Accel::for_action(Action::ToggleTerminal, &config.keys),
                Accel::for_action(Action::LeaveTerminal, &config.keys),
            ));
        }

        // En captura: llegan antes que a VTE, así funcionan con el foco en
        // la terminal. Son las únicas teclas que la app le quita a la
        // terminal: alternarla (F4) y salir de ella (Ctrl+Shift+Tab).
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = pane)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, mods| pane.on_key(key, mods)
        ));
        self.add_controller(keys);

        self.append(&imp.tab_bar);
        self.append(&imp.split);

        // Con `bracket_titles` cambian los títulos de las pestañas.
        if let Some(themes) = theme_manager::get() {
            themes.connect_theme_changed(glib::clone!(
                #[weak(rename_to = pane)]
                self,
                move |_| pane.refresh_tab_titles()
            ));
        }
    }

    fn on_key(&self, key: gdk::Key, mods: gdk::ModifierType) -> glib::Propagation {
        let imp = self.imp();
        let Some((toggle, leave)) = imp.accels.get() else {
            return glib::Propagation::Proceed;
        };
        if toggle.matches(key, mods) {
            self.toggle_terminal();
            return glib::Propagation::Stop;
        }
        if imp.terminal_focused.get() && leave.matches(key, mods) {
            self.focus_current();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    }

    /// Lo marca la ventana: los atajos de pestañas (Ctrl+Tab, Alt+1…9…)
    /// solo valen en el panel activo.
    pub fn set_active(&self, active: bool) {
        self.imp().active.set(active);
        self.update_tab_shortcuts();
    }

    /// Los atajos de `adw::TabView` son de toda la ventana: con dos paneles
    /// competirían, y dentro de la terminal le quitarían teclas a los
    /// programas (tmux, nvim…). Solo en el panel activo y fuera de la
    /// terminal.
    fn update_tab_shortcuts(&self) {
        let imp = self.imp();
        let enabled = imp.active.get() && !imp.terminal_focused.get();
        imp.tab_view.set_shortcuts(if enabled {
            adw::TabViewShortcuts::ALL_SHORTCUTS
        } else {
            adw::TabViewShortcuts::NONE
        });
    }

    /// F4: muestra la terminal (creándola la primera vez, en la carpeta
    /// actual) u oculta la que hay, devolviendo el foco a la vista.
    pub fn toggle_terminal(&self) {
        let visible = self.imp().split.end_child().is_some_and(|c| c.is_visible());
        if visible {
            self.hide_terminal();
        } else {
            self.show_terminal(true);
        }
    }

    /// Muestra la terminal (creándola si hace falta, en la carpeta actual).
    fn show_terminal(&self, take_focus: bool) {
        let imp = self.imp();
        if imp.split.end_child().is_some_and(|c| c.is_visible()) {
            return;
        }
        let existing = imp.terminal.borrow().clone();
        let terminal = match existing {
            Some(terminal) => terminal,
            None => {
                let Some(config) = imp.config.get() else {
                    return;
                };
                let terminal = TerminalView::new(config, self.current_directory().as_ref());
                terminal.connect_exited(glib::clone!(
                    #[weak(rename_to = pane)]
                    self,
                    move |_| pane.on_terminal_exited()
                ));
                terminal.connect_directory_changed(glib::clone!(
                    #[weak(rename_to = pane)]
                    self,
                    move |_, dir| pane.follow_terminal(dir)
                ));
                let focus = gtk::EventControllerFocus::new();
                focus.connect_contains_focus_notify(glib::clone!(
                    #[weak(rename_to = pane)]
                    self,
                    move |focus| {
                        pane.imp().terminal_focused.set(focus.contains_focus());
                        pane.update_tab_shortcuts();
                    }
                ));
                terminal.add_controller(focus);
                imp.split.set_end_child(Some(&terminal));
                imp.terminal.replace(Some(terminal.clone()));
                terminal
            }
        };
        terminal.set_visible(true);
        self.set_terminal_state(true);
        // Posición recordada o dos tercios para las pestañas.
        match imp.split_position.get() {
            Some(position) => imp.split.set_position(position.max(1)),
            None => place_at_two_thirds(&imp.split),
        }
        if take_focus {
            terminal.grab_focus();
        }
    }

    fn refresh_tab_titles(&self) {
        let view = &self.imp().tab_view;
        for i in 0..view.n_pages() {
            let tab = view.nth_page(i);
            if let Some(dir) = tab
                .child()
                .downcast::<TabPage>()
                .ok()
                .and_then(|p| p.directory())
            {
                update_tab_title(&tab, &dir);
            }
        }
    }

    /// Vista previa debajo de las pestañas o a su derecha.
    pub fn set_preview_position(&self, position: PreviewPosition) {
        let imp = self.imp();
        let orientation = match position {
            PreviewPosition::Bottom => gtk::Orientation::Vertical,
            PreviewPosition::Right => gtk::Orientation::Horizontal,
        };
        imp.preview.set_position(position);
        if imp.preview_split.orientation() == orientation {
            return;
        }
        imp.preview_split.set_orientation(orientation);
        // Un alto recordado no sirve como ancho.
        imp.preview_position.set(None);
        if imp.preview.is_visible() {
            place_at_two_thirds(&imp.preview_split);
        }
    }

    /// Espacio: muestra u oculta la vista previa, recordando su altura.
    pub fn toggle_preview(&self) {
        let imp = self.imp();
        let visible = !imp.preview.is_visible();
        if visible {
            imp.preview.set_visible(true);
            match imp.preview_position.get() {
                Some(position) => imp.preview_split.set_position(position.max(1)),
                None => place_at_two_thirds(&imp.preview_split),
            }
        } else {
            imp.preview_position.set(Some(imp.preview_split.position()));
            imp.preview.cancel();
            imp.preview.set_visible(false);
        }
        if imp.preview_visible.replace(visible) != visible {
            self.notify_preview_visible();
        }
        self.update_preview();
    }

    /// Pide a la vista previa (si está visible) la selección actual: un
    /// elemento, el resumen de varios o la carpeta si no hay selección.
    fn update_preview(&self) {
        let imp = self.imp();
        if !imp.preview.is_visible() {
            return;
        }
        let Some(page) = self.current_page() else {
            return;
        };
        let target = preview::target(page.selected_infos(), page.status().selection);
        imp.preview.request(target, page.directory());
    }

    /// Panel → terminal: lleva la terminal a la carpeta de la pestaña
    /// actual (si existe y está activada la opción en la config).
    fn sync_terminal(&self) {
        let imp = self.imp();
        let enabled = imp
            .config
            .get()
            .is_some_and(|c| c.terminal.sync_panel_to_terminal);
        if !enabled {
            return;
        }
        let terminal = imp.terminal.borrow().clone();
        if let (Some(terminal), Some(dir)) = (terminal, self.current_directory()) {
            terminal.change_directory(&dir);
        }
    }

    /// Terminal → panel: la pestaña actual navega a donde hizo `cd` el
    /// shell. Al navegar, `sync_terminal` ve que la terminal ya está ahí y
    /// no envía otro `cd` (sin bucles).
    fn follow_terminal(&self, dir: &gio::File) {
        let enabled = self
            .imp()
            .config
            .get()
            .is_some_and(|c| c.terminal.sync_terminal_to_panel);
        if enabled && let Some(page) = self.current_page() {
            page.navigate_to(dir);
        }
    }

    fn hide_terminal(&self) {
        let imp = self.imp();
        imp.split_position.set(Some(imp.split.position()));
        if let Some(terminal) = imp.split.end_child() {
            terminal.set_visible(false);
        }
        self.set_terminal_state(false);
        self.focus_current();
    }

    /// Actualiza la propiedad (y el botón) sin volver a alternar.
    fn set_terminal_state(&self, visible: bool) {
        if self.imp().terminal_visible.replace(visible) != visible {
            self.notify_terminal_visible();
        }
    }

    /// El shell terminó (`exit`): se oculta y se descarta; el próximo F4
    /// lanza uno nuevo.
    fn on_terminal_exited(&self) {
        let imp = self.imp();
        let had_focus = imp
            .terminal
            .borrow()
            .as_ref()
            .is_some_and(|t| t.has_focus() || t.focus_child().is_some());
        if imp.split.end_child().is_some_and(|c| c.is_visible()) {
            imp.split_position.set(Some(imp.split.position()));
        }
        imp.split.set_end_child(None::<&gtk::Widget>);
        imp.terminal.replace(None);
        imp.terminal_focused.set(false);
        self.update_tab_shortcuts();
        self.set_terminal_state(false);
        if had_focus {
            self.focus_current();
        }
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
        let tab = imp.tab_view.append(&page);
        imp.tab_view.set_selected_page(&tab);
    }

    /// Conecta las señales de una pestaña que entra en este panel.
    fn attach_tab(&self, tab: &adw::TabPage) {
        let Ok(page) = tab.child().downcast::<TabPage>() else {
            return;
        };
        if let Some(dir) = page.directory() {
            update_tab_title(tab, &dir);
        }
        let status = page.connect_status_changed(glib::clone!(
            #[weak(rename_to = pane)]
            self,
            move |page| {
                if pane.current_page().as_ref() == Some(page) {
                    pane.emit_by_name::<()>("selection-changed", &[]);
                    pane.update_preview();
                }
            }
        ));
        let directory = page.connect_directory_notify(glib::clone!(
            #[weak(rename_to = pane)]
            self,
            #[weak]
            tab,
            move |page| {
                if let Some(dir) = page.directory() {
                    update_tab_title(&tab, &dir);
                }
                if pane.current_page().as_ref() == Some(page) {
                    pane.sync_terminal();
                }
            }
        ));
        page.set_pane_handlers(vec![status, directory]);
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
        let imp = self.imp();
        let right = imp.preview_split.orientation() == gtk::Orientation::Horizontal;
        if session.preview_position.is_some() && session.preview_right == right {
            imp.preview_position.set(session.preview_position);
        }
        if session.preview && !imp.preview.is_visible() {
            self.toggle_preview();
        }
        if session.terminal_position.is_some() {
            imp.split_position.set(session.terminal_position);
        }
        // Un shell nuevo en la carpeta del panel; el foco se queda en la vista.
        if session.terminal {
            self.show_terminal(false);
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
        let imp = self.imp();
        let preview = imp.preview.is_visible();
        let preview_position = if preview {
            Some(imp.preview_split.position())
        } else {
            imp.preview_position.get()
        };
        let terminal = imp.split.end_child().is_some_and(|c| c.is_visible());
        let terminal_position = if terminal {
            Some(imp.split.position())
        } else {
            imp.split_position.get()
        };
        PaneSession {
            selected,
            tabs,
            preview,
            preview_position,
            terminal,
            terminal_position,
            preview_right: imp.preview_split.orientation() == gtk::Orientation::Horizontal,
        }
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

/// Dos tercios para las pestañas (del alto o del ancho, según la
/// orientación). Si el `Paned` aún no tiene tamaño (al restaurar la
/// sesión), espera al primer cuadro con tamaño.
fn place_at_two_thirds(paned: &gtk::Paned) {
    fn length(paned: &gtk::Paned) -> i32 {
        match paned.orientation() {
            gtk::Orientation::Horizontal => paned.width(),
            _ => paned.height(),
        }
    }
    if length(paned) > 0 {
        paned.set_position(length(paned) * 2 / 3);
        return;
    }
    paned.add_tick_callback(|paned, _| {
        let size = length(paned);
        if size == 0 {
            return glib::ControlFlow::Continue;
        }
        paned.set_position(size * 2 / 3);
        glib::ControlFlow::Break
    });
}

/// Título (con corchetes si el tema usa `bracket_titles`) y ruta completa
/// en el tooltip.
fn update_tab_title(tab: &adw::TabPage, dir: &gio::File) {
    let name = display_name(dir);
    let title = match theme_manager::get() {
        Some(themes) => themes.theme().pane_title(&name),
        None => name,
    };
    tab.set_title(&title);
    tab.set_tooltip(&display_path(dir));
}
