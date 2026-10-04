//! Nombres legibles de archivos y carpetas para la UI.

use gtk::gio;
use gtk::prelude::*;

/// Ruta local si existe; si no (GVfs), la URI.
pub fn display_path(file: &gio::File) -> String {
    match file.path() {
        Some(path) => path.display().to_string(),
        None => file.uri().to_string(),
    }
}

/// Nombre corto: el último componente, `/` para la raíz local o la URI para
/// la raíz de un recurso remoto.
pub fn display_name(file: &gio::File) -> String {
    if file.parent().is_none() {
        return if file.has_uri_scheme("file") {
            "/".to_owned()
        } else {
            file.uri().to_string()
        };
    }
    file.basename()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.uri().to_string())
}
