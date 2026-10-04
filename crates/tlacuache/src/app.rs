//! Construcción de la `adw::Application`, carga de config y acciones globales.

use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::APP_ID;
use tlacuache_core::config::{self, ColorScheme, Config};

use crate::strings;
use crate::window::TlacuacheWindow;

/// Ruta de `config.toml` según XDG (`~/.config/tlacuache/config.toml`).
pub(crate) fn config_path() -> PathBuf {
    glib::user_config_dir()
        .join("tlacuache")
        .join(config::FILE_NAME)
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

fn adw_color_scheme(scheme: ColorScheme) -> adw::ColorScheme {
    match scheme {
        ColorScheme::Dark => adw::ColorScheme::ForceDark,
        ColorScheme::Light => adw::ColorScheme::ForceLight,
        ColorScheme::System => adw::ColorScheme::Default,
    }
}

pub fn build() -> adw::Application {
    let (config, config_warning) = load_config();
    let config = Rc::new(config);

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    let scheme = adw_color_scheme(config.theme.color_scheme);
    app.connect_startup(move |_| {
        adw::StyleManager::default().set_color_scheme(scheme);
    });

    let config_warning = Rc::new(config_warning);
    let present = move |app: &adw::Application, start_dir: &gio::File| {
        // Una sola ventana por ahora: reactivar la app la trae al frente.
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let window = TlacuacheWindow::new(app, config.clone(), start_dir);
        window.present();
        if let Some(warning) = config_warning.as_ref() {
            window.show_toast(warning);
        }
    };
    let present = Rc::new(present);

    app.connect_activate(glib::clone!(
        #[strong]
        present,
        move |app| present(app, &gio::File::for_path(glib::home_dir()))
    ));
    // `tlacuache <carpeta>` abre esa carpeta.
    app.connect_open(move |app, files, _hint| {
        let home = gio::File::for_path(glib::home_dir());
        present(app, files.first().unwrap_or(&home));
    });

    app
}
