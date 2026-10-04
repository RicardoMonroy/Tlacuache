//! Textos visibles de la UI.
//!
//! Punto único para todos los textos en español; se migrará a `gettext`.

use std::path::Path;

use gtk::glib;

pub const APP_NAME: &str = "Tlacuache";

pub const COLUMN_NAME: &str = "Nombre";
pub const COLUMN_EXTENSION: &str = "Ext";
pub const COLUMN_SIZE: &str = "Tamaño";
pub const COLUMN_MODIFIED: &str = "Modificado";

pub const NAV_BACK: &str = "Atrás (Alt+←)";
pub const NAV_FORWARD: &str = "Adelante (Alt+→)";
pub const NAV_UP: &str = "Subir (Alt+↑)";

pub fn config_load_failed(path: &Path) -> String {
    format!(
        "No se pudo cargar {}; se usan valores por defecto",
        path.display()
    )
}

pub fn listing_failed(err: &glib::Error) -> String {
    format!("No se pudo abrir la carpeta: {}", err.message())
}

pub fn path_not_found(text: &str) -> String {
    format!("No existe la ruta: {text}")
}
