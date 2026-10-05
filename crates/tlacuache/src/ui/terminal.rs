//! Terminal de un panel: envuelve `vte4::Terminal` con el shell del usuario,
//! los colores y la fuente del tema activo (se actualizan al cambiarlo; el
//! tamaño y una fuente propia salen de `[terminal]`). Emite `exited` cuando el shell
//! termina (el panel la oculta y la recrea en el siguiente F4).
//!
//! Panel → terminal: `change_directory` manda `cd` solo si el shell está en
//! primer plano (grupo en primer plano del PTY = PID del shell). Si hay un
//! programa corriendo o la carpeta no es local, muestra «Carpeta
//! desincronizada» con un botón para sincronizar.

use std::cell::{Cell, OnceCell, RefCell};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::OnceLock;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::{gdk, gio, glib, pango};
use tlacuache_core::config::{self, Config};
use tlacuache_core::shell::{self, ShellKind, cd_command, paste_paths};
use tlacuache_core::theme::{Color, Theme};
use vte::prelude::*;

use crate::ui::theme_manager;
use crate::{strings, window};

/// Respaldo si ni la config ni `$SHELL` indican un shell.
const FALLBACK_SHELL: &str = "/bin/bash";
/// Líneas de historial.
const SCROLLBACK_LINES: i64 = 10_000;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TerminalView {
        pub terminal: OnceCell<vte::Terminal>,
        /// Ruta del shell lanzado (elige el escapado).
        pub shell: OnceCell<String>,
        pub shell_pid: Cell<Option<i32>>,
        /// Carpeta en la que está la terminal según lo último enviado.
        pub current_dir: RefCell<Option<gio::File>>,
        /// Carpeta del panel que falta aplicar (programa corriendo).
        pub pending_dir: RefCell<Option<gio::File>>,
        pub desync: gtk::Revealer,
        /// `[terminal]` de la config (fuente al cambiar de tema).
        pub settings: OnceCell<config::Terminal>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TerminalView {
        const NAME: &'static str = "TlacuacheTerminalView";
        type Type = super::TerminalView;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for TerminalView {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("exited").build(),
                    // El shell cambió de carpeta (OSC 7): terminal → panel.
                    Signal::builder("directory-changed")
                        .param_types([gio::File::static_type()])
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for TerminalView {
        fn grab_focus(&self) -> bool {
            self.terminal.get().is_some_and(|t| t.grab_focus())
        }
    }
    impl BinImpl for TerminalView {}
}

glib::wrapper! {
    pub struct TerminalView(ObjectSubclass<imp::TerminalView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TerminalView {
    /// Crea la terminal y lanza el shell en `dir` (si es local).
    pub fn new(config: &Config, dir: Option<&gio::File>) -> Self {
        let view: Self = glib::Object::new();
        view.build(config);
        view.spawn(config, dir);
        view
    }

    pub fn connect_exited<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_closure(
            "exited",
            false,
            glib::closure_local!(move |view: &Self| f(view)),
        );
    }

    pub fn terminal(&self) -> Option<&vte::Terminal> {
        self.imp().terminal.get()
    }

    /// Ruta del shell en uso.
    pub fn shell(&self) -> &str {
        self.imp()
            .shell
            .get()
            .map_or(FALLBACK_SHELL, String::as_str)
    }

    fn build(&self, config: &Config) {
        let terminal = vte::Terminal::new();
        terminal.set_vexpand(true);
        terminal.set_scrollback_lines(SCROLLBACK_LINES);
        terminal.set_mouse_autohide(true);
        let _ = self.imp().settings.set(config.terminal.clone());
        match theme_manager::get() {
            Some(themes) => {
                apply_theme(&terminal, &themes.theme(), &config.terminal);
                themes.connect_theme_changed(glib::clone!(
                    #[weak]
                    terminal,
                    #[weak(rename_to = view)]
                    self,
                    move |themes| {
                        if let Some(settings) = view.imp().settings.get() {
                            apply_theme(&terminal, &themes.theme(), settings);
                        }
                    }
                ));
            }
            None => apply_theme(&terminal, &Theme::default_theme(), &config.terminal),
        }
        terminal.connect_child_exited(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, _status| view.emit_by_name::<()>("exited", &[])
        ));

        // Copiar/pegar como en las terminales: Ctrl+Shift+C / Ctrl+Shift+V.
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak]
            terminal,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, mods| {
                let ctrl_shift = gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK;
                if !mods.contains(ctrl_shift) {
                    return glib::Propagation::Proceed;
                }
                match key {
                    gdk::Key::C | gdk::Key::c => {
                        terminal.copy_clipboard_format(vte::Format::Text);
                        glib::Propagation::Stop
                    }
                    gdk::Key::V | gdk::Key::v => {
                        terminal.paste_clipboard();
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            }
        ));
        terminal.add_controller(keys);

        // Terminal → panel. `current_directory_uri` está obsoleta desde VTE
        // 0.78, pero su reemplazo (`ref_termprop_uri`) no está expuesto en
        // vte4 0.10; sigue funcionando.
        #[allow(deprecated)]
        terminal.connect_current_directory_uri_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |terminal| {
                #[allow(deprecated)]
                if let Some(uri) = terminal.current_directory_uri() {
                    view.on_directory_reported(&uri);
                }
            }
        ));

        // Soltar archivos: escribe sus rutas escapadas (sin ejecutar). Solo
        // COPY: aceptar MOVE haría que la app de origen borrara los archivos.
        let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        drop.connect_drop(glib::clone!(
            #[weak(rename_to = view)]
            self,
            #[upgrade_or]
            false,
            move |_, value, _, _| view.paste_dropped(value)
        ));
        terminal.add_controller(drop);

        // Al volver a la terminal, aplicar la carpeta pendiente si el shell
        // ya está libre.
        let focus = gtk::EventControllerFocus::new();
        focus.connect_enter(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| {
                view.apply_pending();
            }
        ));
        terminal.add_controller(focus);

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&terminal));
        overlay.add_overlay(&self.build_desync_indicator());
        self.add_css_class("tl-terminal-frame");
        self.set_child(Some(&overlay));
        let _ = self.imp().terminal.set(terminal);
    }

    /// Muestra u oculta la pastilla y marca el marco de la terminal.
    fn set_desync(&self, desync: bool) {
        self.imp().desync.set_reveal_child(desync);
        if desync {
            self.add_css_class("desync");
        } else {
            self.remove_css_class("desync");
        }
    }

    /// Pastilla «Carpeta desincronizada · Sincronizar» (arriba a la derecha).
    fn build_desync_indicator(&self) -> gtk::Revealer {
        let label = gtk::Label::new(Some(strings::TERMINAL_DESYNC));
        let sync = gtk::Button::builder()
            .child(
                &adw::ButtonContent::builder()
                    .icon_name("tl-terminal-sync-symbolic")
                    .label(strings::TERMINAL_SYNC)
                    .build(),
            )
            .build();
        sync.add_css_class("flat");
        sync.set_focusable(false);
        sync.connect_clicked(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| {
                if !view.apply_pending() {
                    window::show_toast_from(&view, strings::TERMINAL_BUSY);
                }
            }
        ));
        let pill = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        pill.add_css_class("tl-desync-pill");
        pill.append(&gtk::Image::from_icon_name("tl-terminal-desync-symbolic"));
        pill.append(&label);
        pill.append(&sync);

        let revealer = &self.imp().desync;
        revealer.set_child(Some(&pill));
        revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
        revealer.set_halign(gtk::Align::End);
        revealer.set_valign(gtk::Align::Start);
        revealer.clone()
    }

    /// El shell está esperando órdenes (no hay otro programa en primer
    /// plano en la terminal).
    pub fn is_shell_in_foreground(&self) -> bool {
        let imp = self.imp();
        let (Some(terminal), Some(pid)) = (imp.terminal.get(), imp.shell_pid.get()) else {
            return false;
        };
        let Some(pty) = terminal.pty() else {
            return false;
        };
        let fd = pty.fd();
        // SAFETY: `tcgetpgrp` solo consulta el grupo de procesos en primer
        // plano de un descriptor válido; `pty` lo mantiene abierto durante
        // la llamada y no se toca memoria.
        let foreground = unsafe { libc::tcgetpgrp(fd.as_raw_fd()) };
        foreground > 0 && foreground == pid
    }

    /// Panel → terminal: cambia a `dir` si el shell está libre; si no,
    /// queda pendiente y se muestra el aviso de desincronización.
    pub fn change_directory(&self, dir: &gio::File) {
        let imp = self.imp();
        if imp
            .current_dir
            .borrow()
            .as_ref()
            .is_some_and(|d| d.equal(dir))
        {
            imp.pending_dir.replace(None);
            self.set_desync(false);
            return;
        }
        imp.pending_dir.replace(Some(dir.clone()));
        if !self.apply_pending() {
            self.set_desync(true);
        }
    }

    /// Envía el `cd` pendiente si se puede. Devuelve `true` si quedó
    /// sincronizada.
    fn apply_pending(&self) -> bool {
        let imp = self.imp();
        let Some(dir) = imp.pending_dir.borrow().clone() else {
            return true;
        };
        let (Some(path), Some(terminal)) = (dir.path(), imp.terminal.get()) else {
            return false;
        };
        if !self.is_shell_in_foreground() {
            return false;
        }
        // Ctrl+E y Ctrl+U limpian lo escrito a medias; el espacio inicial
        // evita que el `cd` quede en el historial (HISTCONTROL=ignorespace).
        let shell = ShellKind::from_shell_path(self.shell());
        let line = format!("\x05\x15 {}", cd_command(&path.to_string_lossy(), shell));
        terminal.feed_child(line.as_bytes());
        imp.current_dir.replace(Some(dir));
        imp.pending_dir.replace(None);
        self.set_desync(false);
        true
    }

    fn spawn(&self, config: &Config, dir: Option<&gio::File>) {
        let shell = resolve_shell(&config.terminal.shell);
        let _ = self.imp().shell.set(shell.clone());
        // Solo carpetas locales: VTE necesita una ruta del sistema.
        let cwd_path = dir.and_then(gio::File::path).unwrap_or_else(glib::home_dir);
        self.imp()
            .current_dir
            .replace(Some(gio::File::for_path(&cwd_path)));
        let cwd = cwd_path.to_string_lossy().into_owned();
        let integrate = config.terminal.shell_integration && is_bash(&shell);

        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                let mut argv = vec![shell.clone()];
                if integrate {
                    // El rcfile se escribe fuera del hilo de GTK. Si falla, se
                    // lanza bash normal (sin seguir sus `cd`).
                    match gio::spawn_blocking(write_bash_rc).await {
                        Ok(Ok(rc)) => argv.extend(["--rcfile".to_owned(), rc]),
                        Ok(Err(err)) => tracing::warn!("sin integración de bash: {err}"),
                        Err(_) => tracing::warn!("sin integración de bash: falló el hilo"),
                    }
                }
                view.launch(&cwd, &argv);
            }
        ));
    }

    fn launch(&self, cwd: &str, argv: &[String]) {
        let Some(terminal) = self.terminal() else {
            return;
        };
        let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
        terminal.spawn_async(
            vte::PtyFlags::DEFAULT,
            Some(cwd),
            &argv,
            &[],
            glib::SpawnFlags::DEFAULT,
            || {},
            -1,
            None::<&gio::Cancellable>,
            glib::clone!(
                #[weak(rename_to = view)]
                self,
                move |result| match result {
                    Ok(pid) => view.imp().shell_pid.set(Some(pid.0)),
                    Err(err) => {
                        tracing::warn!("no se pudo iniciar el shell: {err}");
                        window::show_toast_from(
                            &view,
                            &strings::terminal_spawn_failed(err.message()),
                        );
                        view.emit_by_name::<()>("exited", &[]);
                    }
                }
            ),
        );
    }

    /// Terminal → panel: el shell informó su carpeta (OSC 7). Si es la que
    /// ya se conocía (p. ej. tras un `cd` enviado por el panel) no se avisa,
    /// así no hay bucles.
    fn on_directory_reported(&self, uri: &str) {
        let imp = self.imp();
        let dir = gio::File::for_uri(uri);
        if imp
            .current_dir
            .borrow()
            .as_ref()
            .is_some_and(|d| d.equal(&dir))
        {
            return;
        }
        imp.current_dir.replace(Some(dir.clone()));
        imp.pending_dir.replace(None);
        self.set_desync(false);
        self.emit_by_name::<()>("directory-changed", &[&dir]);
    }

    /// Escribe en la línea de comandos las rutas de los archivos soltados.
    fn paste_dropped(&self, value: &glib::Value) -> bool {
        let (Ok(files), Some(terminal)) = (value.get::<gdk::FileList>(), self.terminal()) else {
            return false;
        };
        let paths: Vec<String> = files
            .files()
            .iter()
            .map(|f| {
                f.path()
                    .map_or_else(|| f.uri().to_string(), |p| p.to_string_lossy().into_owned())
            })
            .collect();
        if paths.is_empty() {
            return false;
        }
        let shell = ShellKind::from_shell_path(self.shell());
        let text = paste_paths(paths.iter().map(String::as_str), shell);
        terminal.feed_child(text.as_bytes());
        terminal.grab_focus();
        true
    }

    pub fn connect_directory_changed<F: Fn(&Self, &gio::File) + 'static>(&self, f: F) {
        self.connect_closure(
            "directory-changed",
            false,
            glib::closure_local!(move |view: &Self, dir: &gio::File| f(view, dir)),
        );
    }
}

fn is_bash(shell: &str) -> bool {
    Path::new(shell).file_name().and_then(|n| n.to_str()) == Some("bash")
}

/// Escribe el rcfile de integración en `$XDG_RUNTIME_DIR/tlacuache/` y
/// devuelve su ruta. E/S bloqueante: se llama desde un hilo de trabajo.
fn write_bash_rc() -> std::io::Result<String> {
    let dir = glib::user_runtime_dir().join("tlacuache");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("bash-integration.rc");
    std::fs::write(&path, shell::BASH_INTEGRATION_RC)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Shell de la config, si no `$SHELL`, si no `/bin/bash`.
fn resolve_shell(configured: &str) -> String {
    let configured = configured.trim();
    if !configured.is_empty() {
        return configured.to_owned();
    }
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty() && Path::new(s).is_absolute())
        .unwrap_or_else(|| FALLBACK_SHELL.to_owned())
}

fn rgba(color: Color) -> gdk::RGBA {
    gdk::RGBA::new(
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        1.0,
    )
}

/// Colores, cursor, selección y fuente de `theme` (`docs/THEMES.md` §5.3).
fn apply_theme(terminal: &vte::Terminal, theme: &Theme, settings: &config::Terminal) {
    let colors = &theme.terminal;
    let palette: Vec<gdk::RGBA> = colors.palette.iter().map(|c| rgba(*c)).collect();
    let refs: Vec<&gdk::RGBA> = palette.iter().collect();
    terminal.set_colors(
        Some(&rgba(colors.foreground)),
        Some(&rgba(colors.background)),
        &refs,
    );
    terminal.set_color_cursor(Some(&rgba(colors.cursor)));
    terminal.set_color_cursor_foreground(Some(&rgba(colors.cursor_text)));
    terminal.set_color_highlight(Some(&rgba(colors.selection_bg)));
    terminal.set_color_highlight_foreground(Some(&rgba(colors.selection_fg)));

    // `from_string` acepta familia o descripción completa («Iosevka 12»);
    // el tamaño de la config manda.
    let mut font =
        pango::FontDescription::from_string(settings.font_family(&theme.style.font_mono));
    font.set_size((settings.font_size() * f64::from(pango::SCALE)).round() as i32);
    terminal.set_font(Some(&font));
}
