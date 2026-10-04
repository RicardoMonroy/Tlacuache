mod app;
mod fs;
mod strings;
mod ui;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};
use tracing_subscriber::EnvFilter;

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("tlacuache=info")),
        )
        .init();

    // Sin recursos la app funciona con el tema por defecto de libadwaita.
    if let Err(err) = gio::resources_register_include!("tlacuache.gresource") {
        tracing::error!("no se pudieron registrar los recursos: {err}");
    }

    app::build().run()
}
