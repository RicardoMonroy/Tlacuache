//! Terminal de un panel: envuelve `vte4::Terminal` con el shell del usuario,
//! la fuente de la config y la paleta Nord. Emite `exited` cuando el shell
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
use tlacuache_core::config::Config;
use tlacuache_core::shell::{ShellKind, cd_command};
use tlacuache_core::theme;
use vte::prelude::*;

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
            SIGNALS.get_or_init(|| vec![Signal::builder("exited").build()])
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
        terminal.set_font(Some(&pango::FontDescription::from_string(
            &config.terminal.font,
        )));
        apply_palette(&terminal);
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
        self.set_child(Some(&overlay));
        let _ = self.imp().terminal.set(terminal);
    }

    /// Pastilla «Carpeta desincronizada · Sincronizar» (arriba a la derecha).
    fn build_desync_indicator(&self) -> gtk::Revealer {
        let label = gtk::Label::new(Some(strings::TERMINAL_DESYNC));
        let sync = gtk::Button::with_label(strings::TERMINAL_SYNC);
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
        pill.add_css_class("terminal-desync");
        pill.append(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
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
            imp.desync.set_reveal_child(false);
            return;
        }
        imp.pending_dir.replace(Some(dir.clone()));
        if !self.apply_pending() {
            imp.desync.set_reveal_child(true);
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
        imp.desync.set_reveal_child(false);
        true
    }

    fn spawn(&self, config: &Config, dir: Option<&gio::File>) {
        let Some(terminal) = self.terminal() else {
            return;
        };
        let shell = resolve_shell(&config.terminal.shell);
        let _ = self.imp().shell.set(shell.clone());
        // Solo carpetas locales: VTE necesita una ruta del sistema.
        let cwd_path = dir.and_then(gio::File::path).unwrap_or_else(glib::home_dir);
        self.imp()
            .current_dir
            .replace(Some(gio::File::for_path(&cwd_path)));
        let cwd = cwd_path.to_string_lossy().into_owned();
        terminal.spawn_async(
            vte::PtyFlags::DEFAULT,
            Some(&cwd),
            &[&shell],
            &[],
            glib::SpawnFlags::DEFAULT,
            || {},
            -1,
            None::<&gio::Cancellable>,
            glib::clone!(
                #[weak(rename_to = view)]
                self,
                move |result| {
                    if let Ok(pid) = &result {
                        view.imp().shell_pid.set(Some(pid.0));
                    }
                    if let Err(err) = result {
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

fn rgba(hex: &str) -> gdk::RGBA {
    gdk::RGBA::parse(hex).unwrap_or(gdk::RGBA::BLACK)
}

fn apply_palette(terminal: &vte::Terminal) {
    let palette: Vec<gdk::RGBA> = theme::TERMINAL_PALETTE.iter().map(|c| rgba(c)).collect();
    let refs: Vec<&gdk::RGBA> = palette.iter().collect();
    terminal.set_colors(
        Some(&rgba(theme::TERMINAL_FOREGROUND)),
        Some(&rgba(theme::TERMINAL_BACKGROUND)),
        &refs,
    );
}
