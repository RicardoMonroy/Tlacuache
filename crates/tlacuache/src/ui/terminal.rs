//! Terminal de un panel: envuelve `vte4::Terminal` con el shell del usuario,
//! la fuente de la config y la paleta Nord. Emite `exited` cuando el shell
//! termina (el panel la oculta y la recrea en el siguiente F4).

use std::cell::OnceCell;
use std::path::Path;
use std::sync::OnceLock;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::{gdk, gio, glib, pango};
use tlacuache_core::config::Config;
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
        /// Ruta del shell lanzado (para elegir el escapado en 5.3).
        pub shell: OnceCell<String>,
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

        self.set_child(Some(&terminal));
        let _ = self.imp().terminal.set(terminal);
    }

    fn spawn(&self, config: &Config, dir: Option<&gio::File>) {
        let Some(terminal) = self.terminal() else {
            return;
        };
        let shell = resolve_shell(&config.terminal.shell);
        let _ = self.imp().shell.set(shell.clone());
        // Solo carpetas locales: VTE necesita una ruta del sistema.
        let cwd = dir
            .and_then(gio::File::path)
            .unwrap_or_else(glib::home_dir)
            .to_string_lossy()
            .into_owned();
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
