//! Vista previa por panel (`docs/ARCHITECTURE.md` §6): qué se muestra según
//! la selección y qué renderer le toca a cada archivo.

use crate::filetype::{FileCategory, classify};
use crate::summary::SelectionSummary;

/// Espera tras el último cambio de selección antes de cargar la vista
/// previa (al recorrer la lista con las flechas no se carga cada fila).
pub const DEBOUNCE_MS: u64 = 120;

/// Qué muestra la vista previa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target<T> {
    /// Sin selección: la carpeta actual.
    Directory,
    /// Un elemento seleccionado.
    Single(T),
    /// Varios: solo el resumen.
    Many(SelectionSummary),
}

/// Objetivo según los elementos seleccionados (`summary` es su resumen).
pub fn target<T>(mut selected: Vec<T>, summary: SelectionSummary) -> Target<T> {
    match selected.len() {
        0 => Target::Directory,
        1 => selected.pop().map_or(Target::Directory, Target::Single),
        _ => Target::Many(summary),
    }
}

/// Renderer de la vista previa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Image,
    /// Texto o código (sourceview).
    Text,
    Folder,
    /// Ícono grande y detalles.
    Other,
}

/// Renderer para un archivo, con la misma clasificación que los íconos.
pub fn kind(content_type: Option<&str>, name: &str, is_dir: bool) -> PreviewKind {
    match classify(content_type, name, is_dir, false) {
        FileCategory::Folder => PreviewKind::Folder,
        FileCategory::Image => PreviewKind::Image,
        FileCategory::Code | FileCategory::Text => PreviewKind::Text,
        _ => PreviewKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_depends_on_selection_size() {
        let none = SelectionSummary::default();
        assert_eq!(target(Vec::<&str>::new(), none), Target::Directory);
        assert_eq!(target(vec!["a"], none), Target::Single("a"));
        let two = SelectionSummary {
            files: 1,
            dirs: 1,
            bytes: 10,
        };
        assert_eq!(target(vec!["a", "b"], two), Target::Many(two));
    }

    #[test]
    fn kind_by_type() {
        let cases = [
            (Some("image/png"), "a.png", false, PreviewKind::Image),
            (Some("image/svg+xml"), "a.svg", false, PreviewKind::Image),
            (Some("text/plain"), "notas.txt", false, PreviewKind::Text),
            (Some("text/markdown"), "README.md", false, PreviewKind::Text),
            (Some("text/rust"), "main.rs", false, PreviewKind::Text),
            (Some("application/json"), "a.json", false, PreviewKind::Text),
            (
                Some("application/x-shellscript"),
                "a.sh",
                false,
                PreviewKind::Text,
            ),
            (None, "Cargo.toml", false, PreviewKind::Text),
            (Some("inode/directory"), "src", false, PreviewKind::Folder),
            (None, "src", true, PreviewKind::Folder),
            (Some("application/pdf"), "a.pdf", false, PreviewKind::Other),
            (Some("application/zip"), "a.zip", false, PreviewKind::Other),
            (Some("video/mp4"), "a.mp4", false, PreviewKind::Other),
            (
                Some("application/octet-stream"),
                "a.bin",
                false,
                PreviewKind::Other,
            ),
        ];
        for (mime, name, is_dir, expected) in cases {
            assert_eq!(kind(mime, name, is_dir), expected, "{mime:?} {name}");
        }
    }
}
