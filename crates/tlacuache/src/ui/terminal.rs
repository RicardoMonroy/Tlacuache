//! Terminal de un panel: envuelve `vte4::Terminal` con el shell del usuario,
//! los colores y la fuente del tema activo (se actualizan al cambiarlo; el
//! tamaño y una fuente propia salen de `[terminal]`). Emite `exited` cuando el shell
//! termina (el panel la oculta y la recrea en el siguiente F4).
//!
//! Panel → terminal: `change_directory` manda `cd` solo si el shell está en
//! primer plano (grupo en primer plano del PTY = PID del shell). Si hay un
//! programa corriendo o la carpeta no es local, muestra «Carpeta
//! desincronizada» con un botón para sincronizar.
//!
//! Dentro de Flatpak (ADR-016) el shell se lanza en el sistema con
//! `flatpak-spawn --host` y, como `tcgetpgrp` no ve sus procesos, la
//! integración del shell avisa en un archivo de estado si espera órdenes.

use std::cell::{Cell, OnceCell, RefCell};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::{gdk, gio, glib, pango};
use tlacuache_core::APP_ID;
use tlacuache_core::config::{self, Config};
use tlacuache_core::shell::{self, ShellKind, ShellState, cd_command, paste_paths};
use tlacuache_core::theme::Theme;
use vte::prelude::*;

use crate::ui::{rgba, theme_manager};
use crate::{strings, window};

/// Respaldo si ni la config ni `$SHELL` indican un shell.
const FALLBACK_SHELL: &str = "/bin/bash";
/// Líneas de historial.
const SCROLLBACK_LINES: i64 = 10_000;
/// Variables que VTE define para su hijo y que hay que pasar a mano al
/// shell del sistema (flatpak-spawn no reenvía el entorno del sandbox).
const HOST_TERM_ENV: [&str; 2] = ["TERM=xterm-256color", "COLORTERM=truecolor"];

/// ¿El shell espera órdenes?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Foreground {
    Shell,
    Busy,
    /// Dentro de Flatpak sin integración: no se puede saber.
    Unknown,
}

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
        /// El shell corre en el sistema (Flatpak, ADR-016).
        pub host: Cell<bool>,
        /// Archivo de estado de la integración (solo con `host`).
        pub state_file: RefCell<Option<PathBuf>>,
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
            move |_, _status| {
                view.remove_state_file();
                view.emit_by_name::<()>("exited", &[]);
            }
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
                glib::spawn_future_local(async move {
                    view.apply_pending(false).await;
                });
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
                glib::spawn_future_local(async move {
                    if !view.apply_pending(true).await {
                        window::show_toast_from(&view, strings::TERMINAL_BUSY);
                    }
                });
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

    /// ¿El shell está esperando órdenes? Dentro de Flatpak lo dice el
    /// archivo de estado de la integración; fuera, el grupo en primer plano
    /// del PTY.
    async fn foreground(&self) -> Foreground {
        let imp = self.imp();
        if imp.host.get() {
            let Some(path) = imp.state_file.borrow().clone() else {
                return Foreground::Unknown;
            };
            // Sin archivo todavía: el shell aún no muestra su primer prompt.
            return match gio::File::for_path(path).load_contents_future().await {
                Ok((bytes, _)) => match ShellState::parse(&String::from_utf8_lossy(&bytes)) {
                    Some(ShellState::Prompt) => Foreground::Shell,
                    _ => Foreground::Busy,
                },
                Err(_) => Foreground::Busy,
            };
        }
        if self.is_shell_in_foreground() {
            Foreground::Shell
        } else {
            Foreground::Busy
        }
    }

    /// Fuera de Flatpak: no hay otro programa en primer plano en la
    /// terminal.
    fn is_shell_in_foreground(&self) -> bool {
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
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                // Otra llamada pudo sincronizar mientras tanto.
                if !view.apply_pending(false).await && view.imp().pending_dir.borrow().is_some() {
                    view.set_desync(true);
                }
            }
        ));
    }

    /// Envía el `cd` pendiente si se puede. `requested`: lo pidió el usuario
    /// (botón Sincronizar), así que se envía también si no se puede saber
    /// si el shell está libre. Devuelve `true` si quedó sincronizada.
    async fn apply_pending(&self, requested: bool) -> bool {
        if self.imp().pending_dir.borrow().is_none() {
            return true;
        }
        let ready = match self.foreground().await {
            Foreground::Shell => true,
            Foreground::Unknown => requested,
            Foreground::Busy => false,
        };
        // La carpeta pendiente se lee después de esperar: pudo cambiar.
        let imp = self.imp();
        let Some(dir) = imp.pending_dir.borrow().clone() else {
            return true;
        };
        let (Some(path), Some(terminal)) = (dir.path(), imp.terminal.get()) else {
            return false;
        };
        if !ready {
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
        // Solo carpetas locales: VTE necesita una ruta del sistema.
        let cwd_path = dir.and_then(gio::File::path).unwrap_or_else(glib::home_dir);
        self.imp()
            .current_dir
            .replace(Some(gio::File::for_path(&cwd_path)));
        let cwd = cwd_path.to_string_lossy().into_owned();
        let host = in_flatpak();
        self.imp().host.set(host);
        let configured = config.terminal.shell.trim().to_owned();
        let with_integration = config.terminal.shell_integration;
        let dir = integration_dir(host);

        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                // Dentro de Flatpak `$SHELL` es el `/bin/sh` del sandbox: el
                // shell de inicio de sesión se pregunta al sistema.
                let shell = if host && configured.is_empty() {
                    host_login_shell().await
                } else {
                    resolve_shell(&configured)
                };
                let _ = view.imp().shell.set(shell.clone());
                let integration = if with_integration {
                    shell_name(&shell, host)
                } else {
                    None
                };
                if host && integration.is_some() {
                    view.imp().state_file.replace(Some(new_state_file(&dir)));
                }
                let mut argv = vec![shell.clone()];
                let mut env = Vec::new();
                // Los archivos de integración se escriben fuera del hilo de
                // GTK. Si falla, se lanza el shell normal (sin seguir sus `cd`).
                let written = match integration {
                    Some(name) => {
                        let writer = move || {
                            if host {
                                remove_stale_state_files(&dir);
                            }
                            write_integration(name, &dir)
                        };
                        match gio::spawn_blocking(writer).await {
                            Ok(Ok(path)) => Some((name, path)),
                            Ok(Err(err)) => {
                                tracing::warn!("sin integración de {name}: {err}");
                                None
                            }
                            Err(_) => {
                                tracing::warn!("sin integración de {name}: falló el hilo");
                                None
                            }
                        }
                    }
                    None => None,
                };
                match written {
                    Some(("bash", rc)) => argv.extend(["--rcfile".to_owned(), rc]),
                    Some(("zsh", dir)) => {
                        // El ZDOTDIR del usuario se restaura en nuestro .zshenv.
                        let user = std::env::var("ZDOTDIR").unwrap_or_default();
                        env.push(format!("{}={user}", shell::ZSH_USER_ZDOTDIR_VAR));
                        env.push(format!("ZDOTDIR={dir}"));
                    }
                    Some(("fish", file)) => argv.extend([
                        "--init-command".to_owned(),
                        format!("source {}", shell::quote(&file, ShellKind::Fish)),
                    ]),
                    _ => {
                        // Sin integración no hay quién escriba el estado.
                        view.imp().state_file.replace(None);
                    }
                }
                if host {
                    if let Some(state) = view.imp().state_file.borrow().as_ref() {
                        env.push(format!("{}={}", shell::STATE_FILE_VAR, state.display()));
                    }
                    env.extend(HOST_TERM_ENV.map(str::to_owned));
                    let host_dir = host_visible_dir(&cwd_path);
                    argv = shell::host_command(&argv, &host_dir.to_string_lossy(), &env);
                    env.clear();
                }
                view.launch(&cwd, &argv, &env, host);
            }
        ));
    }

    /// `env`: variables que se añaden al entorno heredado. `host`: `argv` es
    /// un `flatpak-spawn --host`; la PTY no se vuelve su terminal de control
    /// para que la tome el shell del sistema.
    fn launch(&self, cwd: &str, argv: &[String], env: &[String], host: bool) {
        let Some(terminal) = self.terminal() else {
            return;
        };
        let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
        let env: Vec<&str> = env.iter().map(String::as_str).collect();
        let flags = if host {
            vte::PtyFlags::NO_CTTY
        } else {
            vte::PtyFlags::DEFAULT
        };
        terminal.spawn_async(
            flags,
            Some(cwd),
            &argv,
            &env,
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

    /// Borra el archivo de estado del shell que terminó (sin esperar).
    fn remove_state_file(&self) {
        if let Some(path) = self.imp().state_file.take() {
            glib::spawn_future_local(async move {
                let _ = gio::File::for_path(path)
                    .delete_future(glib::Priority::LOW)
                    .await;
            });
        }
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

/// `bash` o `zsh` si el shell tiene integración propia. fish emite OSC 7
/// por sí mismo; dentro de Flatpak también la necesita para el archivo de
/// estado.
fn shell_name(shell: &str, host: bool) -> Option<&'static str> {
    match Path::new(shell).file_name().and_then(|n| n.to_str()) {
        Some("bash") => Some("bash"),
        Some("zsh") => Some("zsh"),
        Some("fish") if host => Some("fish"),
        _ => None,
    }
}

/// Shell de inicio de sesión del usuario en el sistema (dentro de Flatpak),
/// con `getent passwd`. Se pregunta una vez; si falla, `/bin/bash`.
async fn host_login_shell() -> String {
    static SHELL: OnceLock<String> = OnceLock::new();
    if let Some(shell) = SHELL.get() {
        return shell.clone();
    }
    let shell = query_host_shell().await.unwrap_or_else(|| {
        tracing::warn!("no se pudo saber el shell del sistema; se usa {FALLBACK_SHELL}");
        FALLBACK_SHELL.to_owned()
    });
    SHELL.get_or_init(|| shell).clone()
}

async fn query_host_shell() -> Option<String> {
    let user = glib::user_name();
    let argv: [&std::ffi::OsStr; 5] = [
        "flatpak-spawn".as_ref(),
        "--host".as_ref(),
        "getent".as_ref(),
        "passwd".as_ref(),
        user.as_os_str(),
    ];
    let process = gio::Subprocess::newv(&argv, gio::SubprocessFlags::STDOUT_PIPE).ok()?;
    let (stdout, _) = process.communicate_utf8_future(None).await.ok()?;
    shell::login_shell(stdout?.as_str()).map(str::to_owned)
}

/// ¿Corre dentro de Flatpak? Se consulta una vez.
fn in_flatpak() -> bool {
    static IN_FLATPAK: OnceLock<bool> = OnceLock::new();
    *IN_FLATPAK.get_or_init(|| Path::new("/.flatpak-info").exists())
}

/// Carpeta de los archivos de integración y de estado. Dentro de Flatpak,
/// `$XDG_RUNTIME_DIR/app/<app-id>/`, que el sistema ve en la misma ruta.
fn integration_dir(host: bool) -> PathBuf {
    let runtime = glib::user_runtime_dir();
    if host {
        let id = std::env::var("FLATPAK_ID").unwrap_or_else(|_| APP_ID.to_owned());
        runtime.join("app").join(id).join("tlacuache")
    } else {
        runtime.join("tlacuache")
    }
}

/// Prefijo de los archivos de estado de esta ejecución. No se usa el PID:
/// dentro del sandbox siempre es el mismo (Flatpak tiene su propio espacio
/// de PIDs).
fn state_prefix() -> &'static str {
    static PREFIX: OnceLock<String> = OnceLock::new();
    PREFIX.get_or_init(|| format!("state-{}-", glib::uuid_string_random()))
}

/// Ruta de un archivo de estado nuevo (uno por terminal).
fn new_state_file(dir: &Path) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("{}{n}", state_prefix()))
}

/// Borra los archivos de estado de ejecuciones anteriores (la app terminó
/// sin que sus shells terminaran antes). E/S bloqueante: hilo de trabajo.
fn remove_stale_state_files(dir: &Path) {
    let ours = state_prefix();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("state-") && !name.starts_with(ours) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Carpeta inicial del shell del sistema: las rutas que solo existen en el
/// sandbox (`/app`, el portal de documentos) se cambian por la carpeta
/// personal.
fn host_visible_dir(dir: &Path) -> PathBuf {
    let doc_portal = glib::user_runtime_dir().join("doc");
    if dir.starts_with("/app") || dir.starts_with(&doc_portal) {
        glib::home_dir()
    } else {
        dir.to_owned()
    }
}

/// Escribe el archivo de integración de `shell` en `dir` y devuelve lo que
/// se le pasa al shell: el rcfile (bash), la carpeta para `ZDOTDIR` (zsh) o
/// el archivo a cargar (fish). E/S bloqueante: se llama desde un hilo de
/// trabajo.
fn write_integration(shell: &str, dir: &Path) -> std::io::Result<String> {
    let path = match shell {
        "bash" => {
            std::fs::create_dir_all(dir)?;
            let path = dir.join("bash-integration.rc");
            std::fs::write(&path, shell::BASH_INTEGRATION_RC)?;
            path
        }
        "zsh" => {
            let zdotdir = dir.join("zsh");
            std::fs::create_dir_all(&zdotdir)?;
            std::fs::write(zdotdir.join(".zshenv"), shell::ZSH_INTEGRATION_ZSHENV)?;
            zdotdir
        }
        _ => {
            std::fs::create_dir_all(dir)?;
            let path = dir.join("fish-integration.fish");
            std::fs::write(&path, shell::FISH_INTEGRATION)?;
            path
        }
    };
    Ok(path.to_string_lossy().into_owned())
}

/// Shell de la config, si no `$SHELL`, si no `/bin/bash`.
fn resolve_shell(configured: &str) -> String {
    if !configured.is_empty() {
        return configured.to_owned();
    }
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty() && Path::new(s).is_absolute())
        .unwrap_or_else(|| FALLBACK_SHELL.to_owned())
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
