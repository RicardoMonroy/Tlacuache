mod app;
mod strings;
mod window;

use adw::prelude::*;
use gtk::glib;
use tracing_subscriber::EnvFilter;

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("tlacuache=info")),
        )
        .init();

    app::build().run()
}
