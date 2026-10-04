//! Textos visibles de la UI.
//!
//! Punto único para todos los textos en español; se migrará a `gettext`.

use std::path::Path;

pub const APP_NAME: &str = "Tlacuache";

pub fn config_load_failed(path: &Path) -> String {
    format!(
        "No se pudo cargar {}; se usan valores por defecto",
        path.display()
    )
}
