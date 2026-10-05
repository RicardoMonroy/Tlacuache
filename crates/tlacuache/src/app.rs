//! Construcción de la `adw::Application`, carga de config y sesión, y
//! acciones globales.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::APP_ID;
use tlacuache_core::config::{self, Config};
use tlacuache_core::session::{self, Session};

use crate::strings;
use crate::window::TlacuacheWindow;

/// Ruta de `config.toml` según XDG (`~/.config/tlacuache/config.toml`).
pub(crate) fn config_path() -> PathBuf {
    glib::user_config_dir()
        .join("tlacuache")
        .join(config::FILE_NAME)
}

/// Ruta de la sesión según XDG (`~/.local/state/tlacuache/session.toml`).
pub(crate) fn session_path() -> PathBuf {
    glib::user_state_dir()
        .join("tlacuache")
        .join(session::FILE_NAME)
}

/// Carga la config antes de arrancar el bucle de GTK. Si falla, usa los
/// defaults y devuelve el aviso que se mostrará al usuario.
fn load_config() -> (Config, Option<String>) {
    let path = config_path();
    match Config::load_or_create(&path) {
        Ok(config) => {
            tracing::debug!("config cargada desde {}", path.display());
            (config, None)
        }
        Err(err) => {
            tracing::error!("{err}");
            (Config::default(), Some(strings::config_load_failed(&path)))
        }
    }
}

/// Carga la sesión anterior (antes del bucle de GTK). Si está dañada se
/// ignora: la app arranca como la primera vez.
fn load_session() -> (Option<Session>, Option<String>) {
    match Session::load(&session_path()) {
        Ok(session) => (session, None),
        Err(err) => {
            tracing::warn!("{err}");
            (None, Some(strings::SESSION_RESTORE_FAILED.to_owned()))
        }
    }
}

pub fn build() -> adw::Application {
    let (config, config_warning) = load_config();
    for name in tlacuache_core::keymap::unknown_actions(&config.keys) {
        tracing::warn!("acción desconocida en [keys]: {name}");
    }
    let config = Rc::new(config);
    let (session, session_warning) = load_session();
    // La sesión se usa una sola vez, en la primera ventana.
    let session = Rc::new(RefCell::new(session));
    let warnings: Rc<Vec<String>> =
        Rc::new(config_warning.into_iter().chain(session_warning).collect());

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    let theme_settings = config.theme.clone();
    app.connect_startup(move |_| {
        // Ícono de las ventanas (incluido en el gresource: funciona aunque
        // la app no esté instalada).
        gtk::Window::set_default_icon_name(APP_ID);
        match gtk::gdk::Display::default() {
            Some(display) => {
                crate::ui::theme_manager::init(theme_settings.clone(), &display);
            }
            None => tracing::warn!("sin display: no se aplica el tema"),
        }
    });

    let present = move |app: &adw::Application, dir: Option<&gio::File>| {
        // Una sola ventana: si ya existe, la carpeta pedida se abre en una
        // pestaña nueva del panel activo.
        if let Some(window) = app.active_window().and_downcast::<TlacuacheWindow>() {
            if let Some(dir) = dir {
                window.open_in_new_tab(dir);
            }
            window.present();
            return;
        }
        let session = session.borrow_mut().take();
        let window = TlacuacheWindow::new(app, config.clone(), session, dir);
        window.present();
        for warning in warnings.iter() {
            window.show_toast(warning);
        }
    };
    let present = Rc::new(present);

    app.connect_activate(glib::clone!(
        #[strong]
        present,
        move |app| present(app, None)
    ));
    // `tlacuache <carpeta>` abre esa carpeta.
    app.connect_open(move |app, files, _hint| present(app, files.first()));

    app
}
