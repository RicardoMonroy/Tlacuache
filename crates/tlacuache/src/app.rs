//! Construcción de la `adw::Application` y acciones globales.

use adw::prelude::*;
use tlacuache_core::APP_ID;

use crate::window::TlacuacheWindow;

pub fn build() -> adw::Application {
    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_startup(|_| {
        // Oscuro por defecto; seguir al sistema será opción de configuración.
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    });

    app.connect_activate(|app| {
        // Una sola ventana por ahora: reactivar la app la trae al frente.
        let window = app
            .active_window()
            .unwrap_or_else(|| TlacuacheWindow::new(app).upcast());
        window.present();
    });

    app
}
