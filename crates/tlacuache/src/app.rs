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
use tlacuache_core::theme_catalog::{self, ThemeCatalog};

use crate::strings;
use crate::ui::preferences_dialog::PreferencesDialog;
use crate::ui::theme_manager;
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

/// `app.preferences` (Ctrl+,) y `app.theme` (radio con el id del tema, para
/// el menú principal; sigue al tema aplicado).
fn install_app_actions(app: &adw::Application) {
    let preferences = gio::ActionEntry::builder("preferences")
        .activate(|app: &adw::Application, _, _| {
            let dialog = PreferencesDialog::default();
            dialog.present(app.active_window().as_ref());
        })
        .build();
    app.add_action_entries([preferences]);
    app.set_accels_for_action("app.preferences", &["<Control>comma"]);

    let Some(themes) = theme_manager::get() else {
        return;
    };
    let theme = gio::SimpleAction::new_stateful(
        "theme",
        Some(glib::VariantTy::STRING),
        &themes.theme().meta.id.to_variant(),
    );
    // Elegir un tema en el menú lo fija (deja de seguir al sistema).
    theme.connect_change_state(|_, value| {
        let (Some(id), Some(themes)) =
            (value.and_then(|v| v.get::<String>()), theme_manager::get())
        else {
            return;
        };
        let mut settings = themes.settings();
        settings.name = id;
        settings.follow_system = false;
        themes.set_settings(settings);
    });
    themes.connect_theme_changed(glib::clone!(
        #[weak]
        theme,
        move |themes| theme.set_state(&themes.theme().meta.id.to_variant())
    ));
    app.add_action(&theme);
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
    // Temas de usuario: se leen aquí, antes del bucle de GTK, como la config.
    let catalog = ThemeCatalog::load(
        &glib::user_config_dir()
            .join("tlacuache")
            .join(theme_catalog::USER_DIR),
    );
    app.connect_startup(move |app| {
        // Ícono de las ventanas (incluido en el gresource: funciona aunque
        // la app no esté instalada).
        gtk::Window::set_default_icon_name(APP_ID);
        sourceview5::init();
        match gtk::gdk::Display::default() {
            Some(display) => {
                crate::ui::theme_manager::init(theme_settings.clone(), catalog.clone(), &display);
            }
            None => tracing::warn!("sin display: no se aplica el tema"),
        }
        install_app_actions(app);
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
        if let Some(themes) = theme_manager::get() {
            for notice in themes.take_pending_notices() {
                window.show_toast(&notice);
            }
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
