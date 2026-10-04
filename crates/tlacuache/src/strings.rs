//! Textos visibles de la UI.
//!
//! Punto único para todos los textos en español; se migrará a `gettext`.

use std::path::Path;

use gtk::glib;
use tlacuache_core::age::{Age, AgeUnit};

pub const APP_NAME: &str = "Tlacuache";

pub const COLUMN_NAME: &str = "Nombre";
pub const COLUMN_EXTENSION: &str = "Ext";
pub const COLUMN_SIZE: &str = "Tamaño";
pub const COLUMN_MODIFIED: &str = "Modificado";
pub const COLUMN_AGE: &str = "Edad";

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

pub fn open_failed(name: &str, err: &glib::Error) -> String {
    format!("No se pudo abrir «{name}»: {}", err.message())
}

pub fn path_not_found(text: &str) -> String {
    format!("No existe la ruta: {text}")
}

/// Edad relativa compacta: "ahora", "5 min", "3 h", "2 d", "1 mes", "4 años".
pub fn age(age: &Age) -> String {
    let n = age.amount;
    match age.unit {
        AgeUnit::Now => "ahora".to_owned(),
        AgeUnit::Minutes => format!("{n} min"),
        AgeUnit::Hours => format!("{n} h"),
        AgeUnit::Days => format!("{n} d"),
        AgeUnit::Months if n == 1 => "1 mes".to_owned(),
        AgeUnit::Months => format!("{n} meses"),
        AgeUnit::Years if n == 1 => "1 año".to_owned(),
        AgeUnit::Years => format!("{n} años"),
    }
}

/// Indicador del filtro rápido: «texto» · N elementos.
pub fn filter_indicator(query: &str, count: u32) -> String {
    let items = if count == 1 { "elemento" } else { "elementos" };
    format!("«{query}» · {count} {items}")
}
