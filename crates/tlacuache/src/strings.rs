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

pub const OP_COPYING: &str = "Copiando";
pub const OP_MOVING: &str = "Moviendo";
pub const OP_COPIED: &str = "Copiados";
pub const OP_MOVED: &str = "Movidos";
pub const OP_TRASHING: &str = "Enviando a la papelera";
pub const OP_DELETING: &str = "Eliminando";
pub const OP_TRASHED: &str = "Enviados a la papelera";
pub const OP_DELETED: &str = "Eliminados";
pub const TRASH_TITLE: &str = "¿Mover a la papelera?";
pub const DELETE_TITLE: &str = "¿Eliminar permanentemente?";
pub const ACTION_TRASH: &str = "Mover a la papelera";
pub const ACTION_DELETE: &str = "Eliminar";
pub const OP_PREPARING: &str = "Preparando…";
pub const OP_CANCEL: &str = "Cancelar operación";
pub const OP_CANCELLED: &str = "Operación cancelada";
pub const OP_INTERRUPTED: &str = "La operación se interrumpió";
pub const OP_NOT_SUPPORTED: &str = "Operación no disponible todavía";
pub const OP_NO_NAME: &str = "El origen no tiene nombre de archivo";
pub const OP_INTO_ITSELF: &str = "No se puede copiar ni mover una carpeta dentro de sí misma";
pub const COPY_TO_OTHER: &str = "Copiar al otro panel (F5)";
pub const MOVE_TO_OTHER: &str = "Mover al otro panel (F6)";
pub const DUAL_PANE_REQUIRED: &str = "Activa el doble panel (F3) para copiar o mover entre paneles";
pub const OP_NOTHING_SELECTED: &str = "No hay nada seleccionado";

pub const SESSION_RESTORE_FAILED: &str = "No se pudo restaurar la sesión anterior";

pub const SIDEBAR_PLACES: &str = "Lugares";
pub const PLACE_HOME: &str = "Inicio";
pub const PLACE_DESKTOP: &str = "Escritorio";
pub const PLACE_DOCUMENTS: &str = "Documentos";
pub const PLACE_DOWNLOADS: &str = "Descargas";
pub const PLACE_MUSIC: &str = "Música";
pub const PLACE_PICTURES: &str = "Imágenes";
pub const PLACE_VIDEOS: &str = "Vídeos";
pub const PLACE_TRASH: &str = "Papelera";

pub const SIDEBAR_DRIVES: &str = "Unidades";
pub const SIDEBAR_FAVORITES: &str = "Favoritos";
pub const FAVORITES_DEFAULT_GROUP: &str = "Favoritos";
pub const FAVORITES_EMPTY: &str = "Arrastra carpetas aquí o pulsa Ctrl+D";
pub const FAVORITES_ONLY_FOLDERS: &str = "Solo se pueden añadir carpetas a favoritos";
pub const FAVORITE_REMOVE: &str = "Quitar de favoritos";
pub const GROUP_NEW: &str = "Nuevo grupo…";
pub const GROUP_RENAME: &str = "Renombrar grupo…";
pub const GROUP_REMOVE: &str = "Eliminar grupo";
pub const GROUP_NEW_TITLE: &str = "Nuevo grupo de favoritos";
pub const GROUP_RENAME_TITLE: &str = "Renombrar grupo";
pub const GROUP_NAME_PLACEHOLDER: &str = "Nombre del grupo";
pub const ACTION_OPEN: &str = "Abrir";
pub const ACTION_CANCEL: &str = "Cancelar";
pub const ACTION_CREATE: &str = "Crear";
pub const ACTION_RENAME: &str = "Renombrar";
pub const ACTION_REMOVE: &str = "Eliminar";
pub const DRIVE_SYSTEM: &str = "Sistema";
pub const DRIVE_NOT_MOUNTED: &str = "Sin montar";
pub const DRIVE_OPEN: &str = "Abrir";
pub const DRIVE_MOUNT: &str = "Montar";
pub const DRIVE_UNMOUNT: &str = "Desmontar";
pub const DRIVE_EJECT: &str = "Expulsar";
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

pub fn drive_mount_failed(name: &str, err: &glib::Error) -> String {
    format!("No se pudo montar «{name}»: {}", err.message())
}

pub fn drive_unmount_failed(name: &str, err: &glib::Error) -> String {
    format!("No se pudo desmontar «{name}»: {}", err.message())
}

pub fn group_remove_confirm(name: &str, count: usize) -> String {
    let favorites = if count == 1 { "favorito" } else { "favoritos" };
    format!("¿Eliminar el grupo «{name}»?\n\nTiene {count} {favorites}. Las carpetas no se borran.")
}

pub fn favorites_save_failed(detail: &str) -> String {
    format!("No se guardaron los favoritos: {detail}")
}

pub fn op_exists(name: &str) -> String {
    format!("«{name}» ya existe en el destino")
}

/// «Copiando 3 de 10 · archivo.txt».
pub fn op_progress(verb: &str, done: u64, total: Option<u64>, current: Option<&str>) -> String {
    let count = match total {
        Some(total) => format!("{verb} {done} de {total}"),
        None => format!("{verb}…"),
    };
    match current {
        Some(name) => format!("{count} · {name}"),
        None => count,
    }
}

pub fn op_done(verb_past: &str, count: u64) -> String {
    let items = if count == 1 { "elemento" } else { "elementos" };
    format!("{verb_past} {count} {items}")
}

pub fn op_failed(message: &str) -> String {
    format!("La operación falló: {message}")
}

/// Nombres para un diálogo: hasta tres y «y N más».
fn names_summary(names: &[String]) -> String {
    const SHOWN: usize = 3;
    let mut text = names
        .iter()
        .take(SHOWN)
        .map(|n| format!("«{n}»"))
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > SHOWN {
        text.push_str(&format!(" y {} más", names.len() - SHOWN));
    }
    text
}

pub fn trash_confirm(names: &[String]) -> String {
    format!("Se moverán a la papelera: {}.", names_summary(names))
}

pub fn delete_confirm(names: &[String]) -> String {
    format!(
        "Se eliminarán {}. Esta acción no se puede deshacer.",
        names_summary(names)
    )
}
