//! Textos visibles de la UI.
//!
//! Punto único para todos los textos en español; se migrará a `gettext`.

use std::path::Path;

use gtk::glib;
use tlacuache_core::age::{Age, AgeUnit};
use tlacuache_core::summary::ViewStatus;

pub const APP_NAME: &str = "Tlacuache";

pub const COLUMN_NAME: &str = "Nombre";
pub const COLUMN_EXTENSION: &str = "Ext";
pub const COLUMN_SIZE: &str = "Tamaño";
pub const COLUMN_MODIFIED: &str = "Modificado";
pub const COLUMN_AGE: &str = "Edad";

pub const NAV_BACK: &str = "Atrás (Alt+←)";
pub const NAV_FORWARD: &str = "Adelante (Alt+→)";
pub const NAV_UP: &str = "Subir (Alt+↑)";

pub const DUAL_PANE: &str = "Doble panel (F3)";
pub const SHOW_SIDEBAR: &str = "Barra lateral (F9)";

pub const SIDEBAR_PLACES: &str = "Lugares";
pub const PLACE_HOME: &str = "Inicio";
pub const PLACE_DESKTOP: &str = "Escritorio";
pub const PLACE_DOCUMENTS: &str = "Documentos";
pub const PLACE_DOWNLOADS: &str = "Descargas";
pub const PLACE_MUSIC: &str = "Música";
pub const PLACE_PICTURES: &str = "Imágenes";
pub const PLACE_VIDEOS: &str = "Vídeos";
pub const PLACE_ROOT: &str = "Raíz";
pub const PLACE_TRASH: &str = "Papelera";
pub const TAB_NEW: &str = "Nueva pestaña (Ctrl+T)";

pub const VIEW_COLUMNS: &str = "Columnas (Ctrl+1)";
pub const VIEW_DETAILS: &str = "Detalles (Ctrl+2)";

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

fn count(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// «24 elementos · 3 seleccionados (1.2 MB)». El tamaño solo suma archivos.
pub fn status_items(status: &ViewStatus) -> String {
    let items = count(status.items, "elemento", "elementos");
    let selection = &status.selection;
    if selection.is_empty() {
        return items;
    }
    let selected = count(selection.count(), "seleccionado", "seleccionados");
    if selection.files > 0 {
        format!(
            "{items} · {selected} ({})",
            glib::format_size(selection.bytes)
        )
    } else {
        format!("{items} · {selected}")
    }
}

/// «45.3 GB libres de 500 GB».
pub fn free_space(free: u64, size: u64) -> String {
    format!(
        "{} libres de {}",
        glib::format_size(free),
        glib::format_size(size)
    )
}
