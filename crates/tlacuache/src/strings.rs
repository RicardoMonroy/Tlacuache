//! Textos visibles de la UI.
//!
//! Punto único para todos los textos en español; se migrará a `gettext`.

use std::path::Path;

use gtk::glib;
use tlacuache_core::age::{Age, AgeUnit};
use tlacuache_core::ops::OpKind;
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
pub const OP_CREATING_FOLDER: &str = "Creando carpeta";
pub const OP_CREATING_FILE: &str = "Creando archivo";
pub const OP_RENAMING: &str = "Renombrando";
pub const NEW_FOLDER_NAME: &str = "Nueva carpeta";
pub const NEW_FILE_NAME: &str = "Nuevo archivo";
pub const NEW_FOLDER_TITLE: &str = "Nueva carpeta";
pub const NEW_FILE_TITLE: &str = "Nuevo archivo";
pub const RENAME_TITLE: &str = "Renombrar";
pub const RENAME_SINGLE_ONLY: &str =
    "Selecciona un solo elemento para renombrar (el renombrado masivo llegará en la v0.2)";
pub const CONFLICT_FILES: &str = "Ya hay un archivo con ese nombre en el destino.";
pub const CONFLICT_FOLDERS: &str = "Ya hay una carpeta con ese nombre. «Combinar» copia el contenido dentro y pregunta por cada archivo que coincida.";
pub const CONFLICT_TYPES: &str =
    "En el destino hay un elemento de otro tipo con ese nombre; no se puede reemplazar.";
pub const CONFLICT_SAME: &str =
    "El origen y el destino son el mismo elemento. «Conservar ambos» crea una copia.";
pub const CONFLICT_EXISTING: &str = "Existente";
pub const CONFLICT_INCOMING: &str = "Nueva";
pub const CONFLICT_APPLY_ALL: &str = "Aplicar a todos los conflictos de esta operación";
pub const ACTION_SKIP: &str = "Omitir";
pub const ACTION_KEEP_BOTH: &str = "Conservar ambos";
pub const ACTION_REPLACE: &str = "Reemplazar";
pub const ACTION_MERGE: &str = "Combinar";
pub const OP_RESTORING: &str = "Restaurando";
pub const OP_RESTORED: &str = "Restaurados";
pub const OP_CREATED: &str = "Creados";
pub const OP_RENAMED: &str = "Renombrados";
pub const OP_QUEUED: &str = "En cola";
pub const OP_CANCELLING: &str = "Cancelando…";
pub const OP_DONE: &str = "Terminado";
pub const OPS_PANEL: &str = "Operaciones";
pub const OPS_CLEAR: &str = "Limpiar";
pub const ACTION_UNDO: &str = "Deshacer";
pub const UNDO_NOTHING: &str = "No hay nada que deshacer";
pub const OPEN_WITH: &str = "Abrir con";
pub const OPEN_WITH_OTHER: &str = "Otra aplicación…";
pub const ACTION_CUT: &str = "Cortar";
pub const ACTION_COPY: &str = "Copiar";
pub const ACTION_PASTE: &str = "Pegar";
pub const ACTION_RENAME_ELLIPSIS: &str = "Renombrar…";
pub const ACTION_DELETE_ELLIPSIS: &str = "Eliminar permanentemente…";
pub const ADD_TO_FAVORITES: &str = "Añadir a favoritos";
pub const COPY_PATH: &str = "Copiar ruta";
pub const PROPERTIES: &str = "Propiedades";
pub const FOLDER_PROPERTIES: &str = "Propiedades de la carpeta";
pub const NEW_FOLDER_ELLIPSIS: &str = "Nueva carpeta…";
pub const NEW_FILE_ELLIPSIS: &str = "Nuevo archivo…";
pub const TOGGLE_HIDDEN: &str = "Mostrar u ocultar archivos ocultos";
pub const PATH_COPIED: &str = "Ruta copiada al portapapeles";
pub const OPEN_FAILED_APP: &str = "No se pudo abrir con esa aplicación";
pub const PROP_NAME: &str = "Nombre";
pub const PROP_TYPE: &str = "Tipo";
pub const PROP_LOCATION: &str = "Ubicación";
pub const PROP_SIZE: &str = "Tamaño";
pub const PROP_TOTAL_SIZE: &str = "Tamaño total";
pub const PROP_ITEMS: &str = "Elementos";
pub const PROP_MODIFIED: &str = "Modificado";
pub const PROP_CREATED: &str = "Creado";
pub const PROP_ACCESSED: &str = "Último acceso";
pub const PROP_PERMISSIONS: &str = "Permisos";
pub const PROP_OWNER: &str = "Propietario:grupo";
pub const PROP_ERROR: &str = "Error";
pub const PROP_AGE: &str = "Edad";
pub const PROP_DIMENSIONS: &str = "Dimensiones";
pub const PROP_PAGES: &str = "Páginas";
pub const PROP_DURATION: &str = "Duración";
pub const MARKDOWN_RENDERED: &str = "Vista";
pub const MARKDOWN_SOURCE: &str = "Código";
pub const PDF_PREVIOUS: &str = "Página anterior";
pub const PDF_NEXT: &str = "Página siguiente";
pub const PREVIEW_IMAGE_HINT: &str = "Rueda: zoom · Arrastrar: mover · Doble clic: encajar";
pub const PROP_FILES: &str = "Archivos";
pub const PROP_FOLDERS: &str = "Carpetas";
pub const PROP_CALCULATING: &str = "Calculando…";
pub const TOGGLE_TERMINAL: &str = "Terminal (F4)";
pub const TOGGLE_PREVIEW: &str = "Vista previa (Espacio)";
pub const MAIN_MENU: &str = "Menú principal";
pub const MENU_THEME: &str = "Tema";
pub const MENU_PREFERENCES: &str = "Preferencias";
pub const PREFERENCES: &str = "Preferencias";
pub const PREF_APPEARANCE: &str = "Apariencia";
pub const PREF_THEME: &str = "Tema";
pub const PREF_THEMES: &str = "Temas";
pub const PREF_FOLLOW_SYSTEM: &str = "Seguir el modo del sistema";
pub const PREF_FOLLOW_SYSTEM_HINT: &str =
    "Usa un tema claro u oscuro según la preferencia del sistema";
pub const PREF_LIGHT_THEME: &str = "Tema claro";
pub const PREF_DARK_THEME: &str = "Tema oscuro";
pub const TERMINAL_DESYNC: &str = "Carpeta desincronizada";
pub const TERMINAL_SYNC: &str = "Sincronizar";
pub const TERMINAL_BUSY: &str = "La terminal está ocupada con otro programa";
pub const CLIPBOARD_FAILED: &str = "No se pudo usar el portapapeles";
pub const CLIPBOARD_EMPTY: &str = "El portapapeles no contiene archivos";
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
pub const TAB_CLOSE: &str = "Cerrar pestaña";
pub const TAB_CLOSE_OTHERS: &str = "Cerrar las demás";
pub const TAB_DUPLICATE: &str = "Duplicar pestaña";

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

/// Edad relativa compacta (`docs/THEMES.md` §3): «ahora», «12min», «2h»,
/// «3d», «3M» (meses), «1a».
pub fn age(age: &Age) -> String {
    let n = age.amount;
    match age.unit {
        AgeUnit::Now => "ahora".to_owned(),
        AgeUnit::Minutes => format!("{n}min"),
        AgeUnit::Hours => format!("{n}h"),
        AgeUnit::Days => format!("{n}d"),
        AgeUnit::Months => format!("{n}M"),
        AgeUnit::Years => format!("{n}a"),
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

pub fn theme_load_failed(path: &std::path::Path, detail: &str) -> String {
    let file = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    format!("No se pudo cargar el tema {file}: {detail}")
}

pub fn theme_missing(id: &str) -> String {
    format!("El tema «{id}» no existe; se usa Nord")
}

/// Pares de contraste que no cumple un tema de usuario (los tres primeros).
pub fn theme_contrast(name: &str, issues: &[tlacuache_core::theme::ContrastIssue]) -> String {
    const SHOWN: usize = 3;
    let list = issues
        .iter()
        .take(SHOWN)
        .map(|i| format!("{} {:.1} de {}", i.pair, i.ratio, i.minimum))
        .collect::<Vec<_>>()
        .join(", ");
    let more = issues.len().saturating_sub(SHOWN);
    let more = if more > 0 {
        format!(" y {more} más")
    } else {
        String::new()
    };
    format!("El tema «{name}» tiene poco contraste: {list}{more}")
}

pub fn theme_save_failed(detail: &str) -> String {
    format!("No se pudo guardar el tema en la configuración: {detail}")
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

/// «1.2 MB · 2026-10-01 12:00 · más reciente» (sin tamaño en carpetas).
pub fn conflict_version(entry: &tlacuache_core::entry::FileEntry, newer: bool) -> String {
    let mut parts = Vec::new();
    if !entry.is_dir {
        parts.push(glib::format_size(entry.size).to_string());
    }
    if let Some(date) = entry
        .modified
        .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .and_then(|secs| glib::DateTime::from_unix_local(secs).ok())
        .and_then(|dt| dt.format("%Y-%m-%d %H:%M").ok())
    {
        parts.push(date.to_string());
    }
    if newer {
        parts.push("más reciente".to_owned());
    }
    parts.join(" · ")
}

pub fn conflict_title(name: &str) -> String {
    format!("«{name}» ya existe")
}

pub fn untrash_missing(name: &str) -> String {
    format!("«{name}» ya no está en la papelera")
}

pub fn items_count(count: usize) -> String {
    let items = if count == 1 { "elemento" } else { "elementos" };
    format!("{count} {items}")
}

/// Título de una operación en el panel: `what` es «nombre» o «N elementos».
pub fn op_title(kind: OpKind, what: &str, dest: Option<&str>) -> String {
    let arrow = |verb: &str| match dest {
        Some(dest) => format!("{verb} {what} → {dest}"),
        None => format!("{verb} {what}"),
    };
    match kind {
        OpKind::Copy => arrow("Copiar"),
        OpKind::Move => arrow("Mover"),
        OpKind::Rename => match dest {
            Some(dest) => format!("Renombrar {what} → «{dest}»"),
            None => format!("Renombrar {what}"),
        },
        OpKind::Trash => format!("Papelera: {what}"),
        OpKind::Delete => format!("Eliminar {what}"),
        OpKind::Mkdir => format!("Crear carpeta {what}"),
        OpKind::CreateFile => format!("Crear archivo {what}"),
        OpKind::Restore => format!("Restaurar {what}"),
        OpKind::Untrash => format!("Sacar de la papelera {what}"),
    }
}

pub fn ops_finished(count: usize) -> String {
    if count == 1 {
        "1 terminada".to_owned()
    } else {
        format!("{count} terminadas")
    }
}

pub fn clipboard_copied(count: usize, cut: bool) -> String {
    let action = if cut { "cortados" } else { "copiados" };
    format!("{} {action} al portapapeles", items_count(count))
}

pub fn prop_symlink(target: &str) -> String {
    format!("Enlace simbólico → {target}")
}

/// «1.2 MB (1 234 567 bytes)».
/// Nota bajo la vista previa de texto recortada.
pub fn preview_truncated(bytes: u64) -> String {
    format!(
        "Vista previa recortada: se muestran los primeros {}",
        glib::format_size(bytes)
    )
}

pub fn pdf_page(page: i32, pages: i32) -> String {
    format!("Página {page} de {pages}")
}

pub fn prop_dimensions(width: i32, height: i32) -> String {
    format!("{width} × {height} px")
}

pub fn prop_size(bytes: u64) -> String {
    format!("{} ({bytes} bytes)", glib::format_size(bytes))
}

pub fn prop_measuring(bytes: u64, files: u64) -> String {
    format!(
        "Calculando… {} en {files} archivos",
        glib::format_size(bytes)
    )
}

pub fn prop_measured(bytes: u64, files: u64, dirs: u64) -> String {
    let files_text = if files == 1 { "archivo" } else { "archivos" };
    let dirs_text = if dirs == 1 { "carpeta" } else { "carpetas" };
    format!(
        "{} · {files} {files_text}, {dirs} {dirs_text}",
        prop_size(bytes)
    )
}

pub fn terminal_spawn_failed(detail: &str) -> String {
    format!("No se pudo iniciar la terminal: {detail}")
}

#[cfg(test)]
mod tests {
    use tlacuache_core::theme::ContrastIssue;

    use super::*;

    #[test]
    fn theme_contrast_lists_three_pairs() {
        let issue = |pair: &str| ContrastIssue {
            pair: pair.into(),
            ratio: 3.08,
            minimum: 4.5,
        };
        let text = theme_contrast("Mío", &[issue("a"), issue("b"), issue("c"), issue("d")]);
        assert_eq!(
            text,
            "El tema «Mío» tiene poco contraste: a 3.1 de 4.5, b 3.1 de 4.5, c 3.1 de 4.5 y 1 más"
        );
        assert!(!theme_contrast("X", &[issue("a")]).contains("más"));
    }
}
