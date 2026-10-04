//! Ventana principal: header bar, barra lateral (F9) y área de paneles
//! (uno o dos, F3). Lleva la cuenta del panel activo, que Tab alterna y que
//! las operaciones entre paneles (F5/F6) usarán como origen. Restaura la
//! sesión al abrir y la guarda al cerrar.

use std::cell::{Cell, OnceCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use tlacuache_core::config::Config;
use tlacuache_core::names;
use tlacuache_core::ops::{ConflictAction, OpId, OpKind, OpState};
use tlacuache_core::session::{Session, WindowSession};

use crate::fs::ops_runner::{Conflict, ConflictDecision, OpsManager, Resolver};
use crate::strings;
use crate::ui::ops_indicator::OpsIndicator;
use crate::ui::pane::Pane;
use crate::ui::pane_actions::PaneActions;
use crate::ui::sidebar::Sidebar;

/// Ancho mínimo de la barra lateral (px); el inicial viene de la sesión.
const SIDEBAR_MIN_WIDTH: i32 = 140;

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::TlacuacheWindow)]
    pub struct TlacuacheWindow {
        pub toast_overlay: adw::ToastOverlay,
        pub toolbar: adw::ToolbarView,
        /// Barra lateral | paneles.
        pub outer: gtk::Paned,
        pub sidebar: Sidebar,
        /// Panel izquierdo | panel derecho.
        pub paned: gtk::Paned,
        /// Izquierdo y derecho.
        pub panes: OnceCell<[Pane; 2]>,
        /// Índice del panel activo (0 izquierdo, 1 derecho).
        pub active: Cell<usize>,
        /// Modo doble panel. Es propiedad para exponerla como acción
        /// conmutable (`panes.dual`) al botón de la barra.
        #[property(get, set = Self::set_dual_pane)]
        pub dual_pane: Cell<bool>,
        /// Barra lateral visible (F9).
        #[property(get, set = Self::set_show_sidebar)]
        pub show_sidebar: Cell<bool>,
        /// Franja central + panel derecho (se ocultan juntos con F3).
        pub right_side: gtk::Box,
        pub pane_actions: PaneActions,
        pub config: OnceCell<Rc<Config>>,
        /// Cola de operaciones de archivo de la ventana.
        pub ops: OpsManager,
        /// Ya se guardó la sesión: el siguiente cierre es definitivo.
        pub session_saved: Cell<bool>,
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

        fn set_show_sidebar(&self, show: bool) {
            if self.show_sidebar.replace(show) == show {
                return;
            }
            self.sidebar.set_visible(show);
            self.obj().notify_show_sidebar();
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TlacuacheWindow {
        const NAME: &'static str = "TlacuacheWindow";
        type Type = super::TlacuacheWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.install_property_action("panes.dual", "dual-pane");
            klass.install_property_action("sidebar.show", "show-sidebar");
            klass.add_binding(gdk::Key::d, gdk::ModifierType::CONTROL_MASK, |window| {
                window.add_current_to_favorites();
                glib::Propagation::Stop
            });
            // Copiar/mover la selección del panel activo al otro.
            klass.install_action("panes.copy-to-other", None, |window, _, _| {
                window.transfer_selection(OpKind::Copy);
            });
            klass.install_action("panes.move-to-other", None, |window, _, _| {
                window.transfer_selection(OpKind::Move);
            });
            let none = gdk::ModifierType::empty();
            klass.add_binding_action(gdk::Key::F5, none, "panes.copy-to-other");
            klass.add_binding_action(gdk::Key::F6, none, "panes.move-to-other");

            // Papelera (Supr) y borrado permanente (Shift+Supr, confirmado).
            // El entry de la ruta consume Supr al editar.
            klass.install_action("files.trash", None, |window, _, _| window.trash_selection());
            klass.install_action("files.delete", None, |window, _, _| {
                window.delete_selection();
            });
            klass.add_binding_action(gdk::Key::Delete, none, "files.trash");
            klass.add_binding_action(gdk::Key::KP_Delete, none, "files.trash");
            let shift = gdk::ModifierType::SHIFT_MASK;
            klass.add_binding_action(gdk::Key::Delete, shift, "files.delete");
            klass.add_binding_action(gdk::Key::KP_Delete, shift, "files.delete");

            // Renombrar (F2), nueva carpeta (F7 / Ctrl+Shift+N) y nuevo
            // archivo (Ctrl+Alt+N).
            klass.install_action("files.rename", None, |window, _, _| {
                window.rename_selection()
            });
            klass.install_action("files.new-folder", None, |window, _, _| {
                window.create_item(true);
            });
            klass.install_action("files.new-file", None, |window, _, _| {
                window.create_item(false)
            });
            klass.add_binding_action(gdk::Key::F2, none, "files.rename");
            // Portapapeles de archivos. En el entry de la ruta, Ctrl+C/X/V
            // actúan sobre el texto (lo consume antes).
            klass.install_action("clipboard.copy", None, |window, _, _| {
                window.copy_to_clipboard(false);
            });
            klass.install_action("clipboard.cut", None, |window, _, _| {
                window.copy_to_clipboard(true);
            });
            klass.install_action("clipboard.paste", None, |window, _, _| window.paste());
            let ctrl = gdk::ModifierType::CONTROL_MASK;
            klass.add_binding_action(gdk::Key::c, ctrl, "clipboard.copy");
            klass.add_binding_action(gdk::Key::x, ctrl, "clipboard.cut");
            klass.add_binding_action(gdk::Key::v, ctrl, "clipboard.paste");

            // Menú contextual: abrir, abrir con, favoritos, ruta, propiedades.
            klass.install_action("files.open", None, |window, _, _| window.open_selection());
            klass.install_action(
                "files.open-with",
                Some(&String::static_variant_type()),
                |window, _, param| {
                    if let Some(app_id) = param.and_then(|p| p.get::<String>()) {
                        window.open_selection_with(&app_id);
                    }
                },
            );
            klass.install_action("files.open-with-other", None, |window, _, _| {
                window.open_with_other();
            });
            klass.install_action("files.add-favorite", None, |window, _, _| {
                window.add_selection_to_favorites();
            });
            klass.install_action("files.copy-path", None, |window, _, _| {
                window.copy_selection_paths();
            });
            klass.install_action("files.properties", None, |window, _, _| {
                window.show_properties(false);
            });
            klass.install_action("files.folder-properties", None, |window, _, _| {
                window.show_properties(true);
            });

            // Ctrl+Z: deshacer la última operación (el entry de la ruta
            // usa su propio Ctrl+Z al editar).
            klass.add_binding(gdk::Key::z, gdk::ModifierType::CONTROL_MASK, |window| {
                window.undo_last();
                glib::Propagation::Stop
            });
            klass.add_binding_action(gdk::Key::F7, none, "files.new-folder");
            let ctrl = gdk::ModifierType::CONTROL_MASK;
            klass.add_binding_action(gdk::Key::N, ctrl | shift, "files.new-folder");
            klass.add_binding_action(
                gdk::Key::n,
                ctrl | gdk::ModifierType::ALT_MASK,
                "files.new-file",
            );
            klass.add_binding(gdk::Key::F9, gdk::ModifierType::empty(), |window| {
                window.set_show_sidebar(!window.show_sidebar());
                glib::Propagation::Stop
            });
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

            let dual = gtk::ToggleButton::builder()
                .icon_name("view-dual-symbolic")
                .tooltip_text(strings::DUAL_PANE)
                .action_name("panes.dual")
                .focusable(false)
                .build();
            let sidebar_toggle = gtk::ToggleButton::builder()
                .icon_name("sidebar-show-symbolic")
                .tooltip_text(strings::SHOW_SIDEBAR)
                .action_name("sidebar.show")
                .focusable(false)
                .build();
            let header = adw::HeaderBar::new();
            header.pack_start(&sidebar_toggle);
            header.pack_end(&dual);
            header.pack_end(&OpsIndicator::new(&self.ops));
            obj.install_conflict_resolver();
            self.ops.connect_finished(glib::clone!(
                #[weak]
                obj,
                move |_, id| obj.on_operation_finished(id)
            ));

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
    impl WindowImpl for TlacuacheWindow {
        /// Aplaza el cierre hasta guardar la sesión en un hilo de trabajo.
        fn close_request(&self) -> glib::Propagation {
            if self.session_saved.get() {
                return self.parent_close_request();
            }
            self.obj().save_session_and_close();
            glib::Propagation::Stop
        }
    }
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
    /// Crea la ventana restaurando `session` si la hay. `open_dir` (de
    /// `tlacuache <carpeta>`) se abre en una pestaña nueva del panel
    /// izquierdo, o como su única pestaña si no hay sesión.
    pub fn new(
        app: &adw::Application,
        config: Rc<Config>,
        session: Option<Session>,
        open_dir: Option<&gio::File>,
    ) -> Self {
        let window: Self = glib::Object::builder().property("application", app).build();
        let window_session = session
            .as_ref()
            .map(|s| s.window.clone())
            .unwrap_or_else(|| WindowSession {
                dual_pane: config.general.dual_pane,
                ..WindowSession::default()
            });
        window.set_default_size(window_session.width, window_session.height);
        if window_session.maximized {
            window.maximize();
        }
        window.build_panes(&config, &window_session, session.as_ref(), open_dir);
        window
    }

    fn build_panes(
        &self,
        config: &Rc<Config>,
        state: &WindowSession,
        session: Option<&Session>,
        open_dir: Option<&gio::File>,
    ) {
        let imp = self.imp();
        let home = gio::File::for_path(glib::home_dir());
        let start = open_dir.unwrap_or(&home);
        let panes = [Pane::new(config.clone()), Pane::new(config.clone())];
        for (index, pane) in panes.iter().enumerate() {
            let restored = session
                .and_then(|s| s.panes.get(index))
                .is_some_and(|p| pane.restore(p));
            if !restored {
                pane.add_tab(start);
            } else if index == 0
                && let Some(dir) = open_dir
            {
                pane.add_tab(dir);
            }

            let focus = gtk::EventControllerFocus::new();
            focus.connect_enter(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_| window.set_active_pane(index)
            ));
            pane.add_controller(focus);

            // Un clic en cualquier parte del panel (también en zonas vacías,
            // que no toman el foco) lo vuelve activo. En captura y sin
            // reclamar el evento: el clic sigue funcionando normal.
            let click = gtk::GestureClick::new();
            click.set_button(0);
            click.set_propagation_phase(gtk::PropagationPhase::Capture);
            click.connect_pressed(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[weak]
                pane,
                move |_, _, _, _| {
                    window.set_active_pane(index);
                    // Tras procesar el clic: si el foco no quedó dentro del
                    // panel (zona vacía), dárselo a su vista. Así no se le
                    // quita a la barra de ruta al editarla.
                    glib::idle_add_local_once(glib::clone!(
                        #[weak]
                        window,
                        #[weak]
                        pane,
                        move || {
                            let inside = gtk::prelude::RootExt::focus(&window).is_some_and(|w| {
                                w == *pane.upcast_ref::<gtk::Widget>() || w.is_ancestor(&pane)
                            });
                            if !inside {
                                pane.focus_current();
                            }
                        }
                    ));
                }
            ));
            pane.add_controller(click);
            pane.connect_selection_changed(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_| window.update_transfer_actions()
            ));
        }

        let paned = &imp.paned;
        paned.set_orientation(gtk::Orientation::Horizontal);
        paned.set_start_child(Some(&panes[0]));
        imp.right_side.append(&imp.pane_actions);
        imp.right_side
            .append(&gtk::Separator::new(gtk::Orientation::Vertical));
        imp.right_side.append(&panes[1]);
        self.make_strip_resize_panes();
        panes[1].set_hexpand(true);
        paned.set_end_child(Some(&imp.right_side));
        paned.set_shrink_start_child(false);
        paned.set_shrink_end_child(false);
        // Al mostrarse: posición guardada o mitad y mitad; y foco al panel
        // activo.
        let saved_position = state.panes_position;
        let active = state.active_pane;
        paned.connect_map(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |paned| {
                glib::idle_add_local_once(glib::clone!(
                    #[weak]
                    window,
                    #[weak]
                    paned,
                    move || {
                        paned.set_position(saved_position.unwrap_or(paned.width() / 2));
                        if let Some(pane) = window.pane(active) {
                            pane.focus_current();
                        }
                    }
                ));
            }
        ));

        imp.sidebar.set_width_request(SIDEBAR_MIN_WIDTH);
        imp.sidebar.connect_place_activated(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, file| window.navigate_active(file)
        ));
        imp.show_sidebar.set(state.sidebar_visible);
        imp.sidebar.set_visible(state.sidebar_visible);
        imp.sidebar
            .set_favorites(config.favorites.clone(), crate::app::config_path());

        let outer = &imp.outer;
        outer.set_orientation(gtk::Orientation::Horizontal);
        outer.set_start_child(Some(&imp.sidebar));
        outer.set_end_child(Some(paned));
        outer.set_resize_start_child(false);
        outer.set_shrink_start_child(false);
        outer.set_shrink_end_child(false);
        outer.set_position(state.sidebar_width);
        imp.toolbar.set_content(Some(outer));

        let _ = imp.config.set(config.clone());
        let _ = imp.panes.set(panes);
        imp.dual_pane.set(state.dual_pane);
        self.apply_dual_pane();
        self.set_active_pane(if state.dual_pane { active } else { 0 });
    }

    /// Copia o mueve la selección del panel activo a la carpeta del otro.
    fn transfer_selection(&self, kind: OpKind) {
        if !self.dual_pane() {
            self.show_toast(strings::DUAL_PANE_REQUIRED);
            return;
        }
        let active = self.imp().active.get();
        let sources = self
            .pane(active)
            .map(Pane::selected_files)
            .unwrap_or_default();
        if sources.is_empty() {
            self.show_toast(strings::OP_NOTHING_SELECTED);
            return;
        }
        let Some(dest) = self.pane(1 - active).and_then(Pane::current_directory) else {
            return;
        };
        self.imp().ops.enqueue(kind, &sources, Some(&dest));
    }

    fn on_operation_finished(&self, id: OpId) {
        let Some(op) = self.imp().ops.operation(id) else {
            return;
        };
        let undoable = op.undo.is_some();
        let text = match &op.state {
            OpState::Done => strings::op_done(done_verb(op.kind), op.sources.len() as u64),
            OpState::Failed(message) => strings::op_failed(message),
            OpState::Cancelled => strings::OP_CANCELLED.to_owned(),
            _ => return,
        };
        if undoable {
            self.show_undo_toast(&text, id);
        } else if !(op.state == OpState::Done && is_quick(op.kind)) {
            // Crear/renombrar sin deshacer ya se ven en la lista.
            self.show_toast(&text);
        }
    }

    /// Toast con «Deshacer» para una operación reversible.
    fn show_undo_toast(&self, text: &str, id: OpId) {
        let toast = adw::Toast::new(text);
        toast.set_timeout(10);
        toast.set_button_label(Some(strings::ACTION_UNDO));
        toast.connect_button_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.imp().ops.undo(id);
            }
        ));
        self.imp().toast_overlay.add_toast(toast);
    }

    /// Ctrl+Z: deshace la última operación reversible.
    fn undo_last(&self) {
        if self.imp().ops.undo_last().is_none() {
            self.show_toast(strings::UNDO_NOTHING);
        }
    }

    /// Las operaciones preguntan por los conflictos con un diálogo.
    fn install_conflict_resolver(&self) {
        let window = self.downgrade();
        let resolver: Resolver = Rc::new(move |conflict| {
            let window = window.clone();
            Box::pin(async move {
                match window.upgrade() {
                    Some(window) => window.ask_conflict(conflict).await,
                    None => ConflictDecision {
                        action: None,
                        apply_to_all: false,
                    },
                }
            })
        });
        self.imp().ops.set_conflict_resolver(resolver);
    }

    /// Diálogo de conflicto: Cancelar, Omitir, Conservar ambos y
    /// Reemplazar/Combinar (deshabilitado si no aplica), más «aplicar a
    /// todos». Por defecto, la opción que no destruye nada.
    async fn ask_conflict(&self, conflict: Conflict) -> ConflictDecision {
        let name = crate::fs::display::display_name(&conflict.dest);
        let folders = conflict.source_is_dir && conflict.dest_is_dir;
        let body = if conflict.source.equal(&conflict.dest) {
            strings::CONFLICT_SAME
        } else if conflict.source_is_dir != conflict.dest_is_dir {
            strings::CONFLICT_TYPES
        } else if folders {
            strings::CONFLICT_FOLDERS
        } else {
            strings::CONFLICT_FILES
        };
        let replace_label = if folders {
            strings::ACTION_MERGE
        } else {
            strings::ACTION_REPLACE
        };

        let dialog = adw::AlertDialog::new(Some(&strings::conflict_title(&name)), Some(body));
        dialog.add_responses(&[
            ("cancel", strings::ACTION_CANCEL),
            ("skip", strings::ACTION_SKIP),
            ("keep", strings::ACTION_KEEP_BOTH),
            ("replace", replace_label),
        ]);
        let appearance = if folders {
            adw::ResponseAppearance::Suggested
        } else {
            adw::ResponseAppearance::Destructive
        };
        dialog.set_response_appearance("replace", appearance);
        dialog.set_response_enabled("replace", conflict.replace_allowed);
        dialog.set_default_response(Some("keep"));
        dialog.set_close_response("cancel");
        let apply_all = gtk::CheckButton::with_label(strings::CONFLICT_APPLY_ALL);
        dialog.set_extra_child(Some(&apply_all));

        let action = match dialog.choose_future(Some(self)).await.as_str() {
            "skip" => Some(ConflictAction::Skip),
            "keep" => Some(ConflictAction::KeepBoth),
            "replace" => Some(ConflictAction::Replace),
            _ => None,
        };
        ConflictDecision {
            action,
            apply_to_all: apply_all.is_active(),
        }
    }

    /// Abre `dir` en una pestaña nueva del panel activo.
    pub fn open_in_new_tab(&self, dir: &gio::File) {
        if let Some(pane) = self.pane(self.imp().active.get()) {
            pane.add_tab(dir);
            pane.focus_current();
        }
    }

    /// Estado actual para restaurarlo la próxima vez.
    fn current_session(&self) -> Session {
        let imp = self.imp();
        let (width, height) = self.default_size();
        Session {
            window: WindowSession {
                width,
                height,
                maximized: self.is_maximized(),
                sidebar_visible: self.show_sidebar(),
                sidebar_width: imp.outer.position(),
                dual_pane: self.dual_pane(),
                panes_position: Some(imp.paned.position()).filter(|p| *p > 0),
                active_pane: imp.active.get(),
            },
            panes: (0..2)
                .filter_map(|i| self.pane(i))
                .map(Pane::session)
                .collect(),
        }
        .sanitized()
    }

    /// Guarda la sesión fuera del hilo de GTK y luego cierra la ventana.
    /// Si falla, se registra y se cierra igual: no debe impedir salir.
    fn save_session_and_close(&self) {
        let session = self.current_session();
        let path = crate::app::session_path();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                match gio::spawn_blocking(move || session.save(&path)).await {
                    Ok(Ok(())) => tracing::debug!("sesión guardada"),
                    Ok(Err(err)) => tracing::warn!("{err}"),
                    Err(_) => tracing::warn!("falló el hilo que guarda la sesión"),
                }
                window.imp().session_saved.set(true);
                window.close();
            }
        ));
    }

    /// Navega el panel activo a `dir` (barra lateral, favoritos…).
    fn navigate_active(&self, dir: &gio::File) {
        if let Some(pane) = self.pane(self.imp().active.get()) {
            pane.navigate(dir);
        }
    }

    /// Ctrl+D: añade la carpeta actual del panel activo a favoritos.
    fn add_current_to_favorites(&self) {
        let dir = self
            .pane(self.imp().active.get())
            .and_then(Pane::current_directory);
        if let Some(dir) = dir {
            self.imp().sidebar.add_favorite(&dir);
        }
    }

    fn pane(&self, index: usize) -> Option<&Pane> {
        self.imp().panes.get().and_then(|panes| panes.get(index))
    }

    /// Muestra u oculta el panel derecho (conserva sus pestañas).
    fn apply_dual_pane(&self) {
        let dual = self.dual_pane();
        self.imp().right_side.set_visible(dual);
        if !dual && self.imp().active.get() == 1 {
            if let Some(left) = self.pane(0) {
                left.focus_current();
            }
            self.set_active_pane(0);
        }
        self.update_active_style();
        self.update_transfer_actions();
    }

    fn set_active_pane(&self, index: usize) {
        self.imp().active.set(index);
        self.imp().pane_actions.set_left_active(index == 0);
        self.update_active_style();
        self.update_transfer_actions();
    }

    /// Arrastrar en la franja central mueve el divisor entre paneles. Se
    /// calcula con la posición del puntero en el `Paned` (no con el
    /// desplazamiento del gesto) porque la franja se mueve al arrastrar.
    fn make_strip_resize_panes(&self) {
        let imp = self.imp();
        // (x del puntero dentro de la franja, distancia divisor→franja).
        let grab = Rc::new(Cell::new((0.0_f64, 0.0_f64)));
        let drag = gtk::GestureDrag::new();
        drag.connect_drag_begin(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[strong]
            grab,
            move |_, x, _| {
                let imp = window.imp();
                let strip_x = imp
                    .pane_actions
                    .compute_point(&imp.paned, &gtk::graphene::Point::new(0.0, 0.0))
                    .map_or(0.0, |p| f64::from(p.x()));
                grab.set((x, strip_x - f64::from(imp.paned.position())));
            }
        ));
        drag.connect_drag_update(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |gesture, dx, _| {
                let imp = window.imp();
                let Some((start_x, start_y)) = gesture.start_point() else {
                    return;
                };
                let pointer = gtk::graphene::Point::new((start_x + dx) as f32, start_y as f32);
                let Some(in_paned) = imp.pane_actions.compute_point(&imp.paned, &pointer) else {
                    return;
                };
                let (grab_x, handle) = grab.get();
                // Píxeles enteros para la posición del divisor.
                let position = (f64::from(in_paned.x()) - grab_x - handle).round() as i32;
                imp.paned.set_position(position.max(0));
            }
        ));
        imp.pane_actions.add_controller(drag);
    }

    /// Habilita las acciones sobre la selección: F5/F6 y la franja central
    /// solo en modo dual; papelera y borrar siempre que haya selección.
    fn update_transfer_actions(&self) {
        let has_selection = self
            .pane(self.imp().active.get())
            .is_some_and(|pane| !pane.selected_files().is_empty());
        let transfer = has_selection && self.dual_pane();
        self.action_set_enabled("panes.copy-to-other", transfer);
        self.action_set_enabled("panes.move-to-other", transfer);
        self.action_set_enabled("files.trash", has_selection);
        self.action_set_enabled("files.delete", has_selection);
        self.action_set_enabled("files.rename", has_selection);
        self.action_set_enabled("clipboard.copy", has_selection);
        self.action_set_enabled("clipboard.cut", has_selection);
        for action in [
            "files.open",
            "files.open-with",
            "files.open-with-other",
            "files.add-favorite",
            "files.copy-path",
            "files.properties",
        ] {
            self.action_set_enabled(action, has_selection);
        }
    }

    fn active_pane(&self) -> Option<&Pane> {
        self.pane(self.imp().active.get())
    }

    /// Abrir: una carpeta navega; si hay varias, cada una en una pestaña
    /// nueva; los archivos, con su app predeterminada.
    fn open_selection(&self) {
        let Some(pane) = self.active_pane() else {
            return;
        };
        let infos = pane.selected_infos();
        if let [only] = infos.as_slice()
            && only.is_dir
        {
            pane.navigate(&only.file);
            return;
        }
        for info in infos {
            if info.is_dir {
                pane.add_tab(&info.file);
            } else {
                let name = crate::fs::display::display_name(&info.file);
                crate::fs::launch::open_file(self, info.file, name);
            }
        }
    }

    /// Abrir con una app recomendada (por id) para el tipo del primero.
    fn open_selection_with(&self, app_id: &str) {
        let infos = self
            .active_pane()
            .map(Pane::selected_infos)
            .unwrap_or_default();
        let Some(content_type) = infos.first().and_then(|i| i.content_type.clone()) else {
            return;
        };
        let app = gio::AppInfo::recommended_for_type(&content_type)
            .into_iter()
            .find(|app| app.id().as_deref() == Some(app_id));
        let Some(app) = app else {
            return;
        };
        let uris: Vec<String> = infos.iter().map(|i| i.file.uri().to_string()).collect();
        let context = WidgetExt::display(self).app_launch_context();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                let refs: Vec<&str> = uris.iter().map(String::as_str).collect();
                if let Err(err) = app.launch_uris_future(&refs, Some(&context)).await {
                    tracing::warn!("no se pudo abrir con {}: {err}", app.name());
                    window.show_toast(strings::OPEN_FAILED_APP);
                }
            }
        ));
    }

    /// «Otra aplicación…»: el selector del sistema (portal) para el primero.
    fn open_with_other(&self) {
        let Some(file) = self.active_selection().0.into_iter().next() else {
            return;
        };
        let launcher = gtk::FileLauncher::new(Some(&file));
        launcher.set_always_ask(true);
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                if let Err(err) = launcher.launch_future(Some(&window)).await {
                    // Cerrar el selector sin elegir no es un error.
                    if !err.matches(gtk::DialogError::Dismissed) {
                        tracing::warn!("abrir con otra aplicación: {err}");
                        window.show_toast(strings::OPEN_FAILED_APP);
                    }
                }
            }
        ));
    }

    fn add_selection_to_favorites(&self) {
        let infos = self
            .active_pane()
            .map(Pane::selected_infos)
            .unwrap_or_default();
        for info in infos.iter().filter(|i| i.is_dir) {
            self.imp().sidebar.add_favorite(&info.file);
        }
    }

    /// Copiar ruta: rutas (o URIs si no son locales), una por línea.
    fn copy_selection_paths(&self) {
        let (files, _) = self.active_selection();
        let text = files
            .iter()
            .map(crate::fs::display::display_path)
            .collect::<Vec<_>>()
            .join("\n");
        WidgetExt::display(self).clipboard().set_text(&text);
        self.show_toast(strings::PATH_COPIED);
    }

    /// Propiedades de la selección o, con `folder`, de la carpeta actual.
    fn show_properties(&self, folder: bool) {
        let (files, dir) = self.active_selection();
        let files = if folder {
            dir.into_iter().collect()
        } else {
            files
        };
        if files.is_empty() {
            return;
        }
        crate::ui::properties_dialog::PropertiesDialog::new(files).present(Some(self));
    }

    /// Ctrl+C / Ctrl+X: la selección del panel activo al portapapeles.
    fn copy_to_clipboard(&self, cut: bool) {
        let (files, _) = self.active_selection();
        if files.is_empty() {
            return;
        }
        match crate::fs::clipboard::set_files(&WidgetExt::display(self), &files, cut) {
            Ok(()) => self.show_toast(&strings::clipboard_copied(files.len(), cut)),
            Err(err) => {
                tracing::warn!("no se pudo usar el portapapeles: {err}");
                self.show_toast(strings::CLIPBOARD_FAILED);
            }
        }
    }

    /// Ctrl+V: copia (o mueve, si se cortó) lo del portapapeles a la
    /// carpeta del panel activo, por la cola de operaciones.
    fn paste(&self) {
        let (_, dir) = self.active_selection();
        let Some(dir) = dir else {
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                let display = WidgetExt::display(&window);
                let files = match crate::fs::clipboard::read_files(&display).await {
                    Ok(Some(files)) => files,
                    Ok(None) => {
                        window.show_toast(strings::CLIPBOARD_EMPTY);
                        return;
                    }
                    Err(err) => {
                        tracing::warn!("no se pudo leer el portapapeles: {err}");
                        window.show_toast(strings::CLIPBOARD_EMPTY);
                        return;
                    }
                };
                let sources: Vec<gio::File> =
                    files.uris.iter().map(|u| gio::File::for_uri(u)).collect();
                let kind = if files.cut {
                    OpKind::Move
                } else {
                    OpKind::Copy
                };
                window.imp().ops.enqueue(kind, &sources, Some(&dir));
                // Lo cortado ya se movió: no se puede volver a pegar.
                if files.cut {
                    crate::fs::clipboard::clear(&display);
                }
            }
        ));
    }

    /// F2: renombra el elemento seleccionado (uno solo).
    fn rename_selection(&self) {
        let (files, _) = self.active_selection();
        let [file] = files.as_slice() else {
            if !files.is_empty() {
                self.show_toast(strings::RENAME_SINGLE_ONLY);
            }
            return;
        };
        let (Some(parent), Some(name)) = (file.parent(), file.basename()) else {
            return;
        };
        let file = file.clone();
        let current = name.to_string_lossy().into_owned();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                let is_dir = file
                    .query_info_future(
                        gio::FILE_ATTRIBUTE_STANDARD_TYPE,
                        gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                        glib::Priority::DEFAULT,
                    )
                    .await
                    .is_ok_and(|info| info.file_type() == gio::FileType::Directory);
                let stem = names::stem_len(&current, is_dir);
                let new_name = window
                    .ask_name(
                        strings::RENAME_TITLE,
                        strings::ACTION_RENAME,
                        &current,
                        stem,
                    )
                    .await;
                let Some(new_name) = new_name.filter(|n| *n != current) else {
                    return;
                };
                let dest = parent.child(&new_name);
                window
                    .imp()
                    .ops
                    .enqueue(OpKind::Rename, &[file], Some(&dest));
                window.select_in_active(&dest);
            }
        ));
    }

    /// F7 / Ctrl+Alt+N: crea una carpeta o un archivo vacío en la carpeta
    /// del panel activo, proponiendo un nombre libre.
    fn create_item(&self, folder: bool) {
        let Some(dir) = self
            .pane(self.imp().active.get())
            .and_then(Pane::current_directory)
        else {
            return;
        };
        let (base, title) = if folder {
            (strings::NEW_FOLDER_NAME, strings::NEW_FOLDER_TITLE)
        } else {
            (strings::NEW_FILE_NAME, strings::NEW_FILE_TITLE)
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                let initial = free_name(&dir, base).await;
                let all = initial.chars().count();
                let Some(name) = window
                    .ask_name(title, strings::ACTION_CREATE, &initial, all)
                    .await
                else {
                    return;
                };
                let target = dir.child(&name);
                let kind = if folder {
                    OpKind::Mkdir
                } else {
                    OpKind::CreateFile
                };
                window
                    .imp()
                    .ops
                    .enqueue(kind, std::slice::from_ref(&target), None);
                window.select_in_active(&target);
            }
        ));
    }

    fn select_in_active(&self, file: &gio::File) {
        if let Some(pane) = self.pane(self.imp().active.get()) {
            pane.select_when_present(file);
        }
    }

    /// Pide un nombre validándolo mientras se escribe (el botón de aceptar
    /// se deshabilita si no es válido). Selecciona los primeros `select`
    /// caracteres (p. ej. el nombre sin extensión al renombrar).
    async fn ask_name(
        &self,
        title: &str,
        accept: &str,
        initial: &str,
        select: usize,
    ) -> Option<String> {
        let entry = gtk::Entry::builder()
            .text(initial)
            .activates_default(true)
            .build();
        let error = gtk::Label::builder().xalign(0.0).wrap(true).build();
        error.add_css_class("error");
        error.add_css_class("caption");
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.append(&entry);
        content.append(&error);

        let dialog = adw::AlertDialog::new(Some(title), None);
        dialog.add_responses(&[("cancel", strings::ACTION_CANCEL), ("accept", accept)]);
        dialog.set_response_appearance("accept", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("accept"));
        dialog.set_close_response("cancel");
        dialog.set_extra_child(Some(&content));

        let validate = glib::clone!(
            #[weak]
            dialog,
            #[weak]
            error,
            move |entry: &gtk::Entry| {
                let result = names::validate_name(&entry.text());
                dialog.set_response_enabled("accept", result.is_ok());
                error.set_text(&result.err().map(|e| e.to_string()).unwrap_or_default());
                error.set_visible(result.is_err());
            }
        );
        validate(&entry);
        entry.connect_changed(validate);
        // Al mostrarse: foco y selección (p. ej. sin la extensión).
        let select = i32::try_from(select).unwrap_or(-1);
        entry.connect_map(move |entry| {
            entry.grab_focus();
            entry.select_region(0, select);
        });

        let response = dialog.choose_future(Some(self)).await;
        (response == "accept").then(|| entry.text().to_string())
    }

    /// Selección del panel activo y su carpeta.
    fn active_selection(&self) -> (Vec<gio::File>, Option<gio::File>) {
        let pane = self.pane(self.imp().active.get());
        (
            pane.map(Pane::selected_files).unwrap_or_default(),
            pane.and_then(Pane::current_directory),
        )
    }

    /// Supr: a la papelera (confirmando si `general.confirm_trash`). Dentro
    /// de la papelera no se puede volver a enviar: se borra con confirmación.
    fn trash_selection(&self) {
        let (files, dir) = self.active_selection();
        if files.is_empty() {
            return;
        }
        if dir.is_some_and(|d| d.has_uri_scheme("trash")) {
            self.delete_selection();
            return;
        }
        let confirm = self
            .imp()
            .config
            .get()
            .is_some_and(|c| c.general.confirm_trash);
        if !confirm {
            self.imp().ops.enqueue(OpKind::Trash, &files, None);
            return;
        }
        let body = strings::trash_confirm(&display_names(&files));
        self.confirm_then(
            strings::TRASH_TITLE,
            &body,
            strings::ACTION_TRASH,
            false,
            move |window| {
                window.imp().ops.enqueue(OpKind::Trash, &files, None);
            },
        );
    }

    /// Shift+Supr: borrado permanente, siempre con confirmación.
    fn delete_selection(&self) {
        let (files, _) = self.active_selection();
        if files.is_empty() {
            return;
        }
        let body = strings::delete_confirm(&display_names(&files));
        self.confirm_then(
            strings::DELETE_TITLE,
            &body,
            strings::ACTION_DELETE,
            true,
            move |window| {
                window.imp().ops.enqueue(OpKind::Delete, &files, None);
            },
        );
    }

    /// Diálogo de confirmación; ejecuta `then` solo si se acepta. Por
    /// defecto (Enter/Esc) se cancela.
    fn confirm_then<F>(&self, title: &str, body: &str, accept: &str, destructive: bool, then: F)
    where
        F: FnOnce(&Self) + 'static,
    {
        let dialog = adw::AlertDialog::new(Some(title), Some(body));
        dialog.add_responses(&[("cancel", strings::ACTION_CANCEL), ("accept", accept)]);
        let appearance = if destructive {
            adw::ResponseAppearance::Destructive
        } else {
            adw::ResponseAppearance::Suggested
        };
        dialog.set_response_appearance("accept", appearance);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                if dialog.choose_future(Some(&window)).await == "accept" {
                    then(&window);
                }
            }
        ));
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
        // Escribiendo texto (la ruta) o en la terminal, Tab es del widget:
        // autocompletar ruta o del shell.
        if gtk::prelude::RootExt::focus(self).is_some_and(|w| {
            w.is::<gtk::Text>()
                || w.ancestor(gtk::Entry::static_type()).is_some()
                || w.is::<vte::Terminal>()
                || w.ancestor(vte::Terminal::static_type()).is_some()
        }) {
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

/// Encola una operación en la ventana que contiene `widget` (arrastrar y
/// soltar). Devuelve `false` si no hay ventana.
pub fn enqueue_from(
    widget: &impl IsA<gtk::Widget>,
    kind: OpKind,
    sources: &[gio::File],
    dest: &gio::File,
) -> bool {
    let Some(window) = widget.root().and_downcast::<TlacuacheWindow>() else {
        return false;
    };
    window.imp().ops.enqueue(kind, sources, Some(dest));
    true
}

fn display_names(files: &[gio::File]) -> Vec<String> {
    files.iter().map(crate::fs::display::display_name).collect()
}

/// Primer nombre libre en `dir`: `base`, `base (2)`, `base (3)`…
async fn free_name(dir: &gio::File, base: &str) -> String {
    for candidate in names::numbered_names(base).take(1000) {
        let taken = dir
            .child(&candidate)
            .query_info_future(
                gio::FILE_ATTRIBUTE_STANDARD_TYPE,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                glib::Priority::DEFAULT,
            )
            .await
            .is_ok();
        if !taken {
            return candidate;
        }
    }
    base.to_owned()
}

/// Operaciones inmediatas cuyo resultado ya se ve en la lista.
fn is_quick(kind: OpKind) -> bool {
    matches!(kind, OpKind::Mkdir | OpKind::CreateFile | OpKind::Rename)
}

fn done_verb(kind: OpKind) -> &'static str {
    match kind {
        OpKind::Copy => strings::OP_COPIED,
        OpKind::Move => strings::OP_MOVED,
        OpKind::Trash => strings::OP_TRASHED,
        OpKind::Delete => strings::OP_DELETED,
        OpKind::Mkdir | OpKind::CreateFile => strings::OP_CREATED,
        OpKind::Rename => strings::OP_RENAMED,
        OpKind::Restore | OpKind::Untrash => strings::OP_RESTORED,
    }
}
