//! Representación de una entrada de directorio independiente de GTK.
//!
//! La app construye `FileEntry` a partir de `gio::FileInfo` (ver `fs/listing.rs`).

use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Nombre visible (sin ruta).
    pub name: String,
    /// Carpeta, o enlace simbólico que apunta a una carpeta.
    pub is_dir: bool,
    pub is_symlink: bool,
    pub is_hidden: bool,
    /// Tamaño en bytes; 0 para carpetas.
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    /// Tipo MIME (`text/plain`, `inode/directory`, …) si se conoce.
    pub content_type: Option<String>,
}

impl FileEntry {
    /// Entrada mínima; `is_hidden` se deduce del nombre (prefijo `.`).
    pub fn new(name: impl Into<String>, is_dir: bool) -> Self {
        let name = name.into();
        Self {
            is_hidden: name.starts_with('.'),
            name,
            is_dir,
            is_symlink: false,
            size: 0,
            modified: None,
            created: None,
            content_type: None,
        }
    }

    /// Extensión sin el punto. `None` para carpetas, archivos sin punto y
    /// dotfiles sin extensión (`.bashrc`).
    pub fn extension(&self) -> Option<&str> {
        if self.is_dir {
            return None;
        }
        let stem_start = usize::from(self.name.starts_with('.'));
        let dot = self.name[stem_start..].rfind('.')? + stem_start;
        let ext = &self.name[dot + 1..];
        (!ext.is_empty()).then_some(ext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_is_derived_from_name() {
        assert!(FileEntry::new(".git", true).is_hidden);
        assert!(!FileEntry::new("src", true).is_hidden);
    }

    #[test]
    fn extension_cases() {
        let ext = |name: &str| FileEntry::new(name, false).extension().map(str::to_owned);
        assert_eq!(ext("foto.JPG").as_deref(), Some("JPG"));
        assert_eq!(ext("archivo.tar.gz").as_deref(), Some("gz"));
        assert_eq!(ext(".bashrc"), None);
        assert_eq!(ext(".config.toml").as_deref(), Some("toml"));
        assert_eq!(ext("Makefile"), None);
        assert_eq!(ext("raro."), None);
        assert_eq!(ext("canción.ñ").as_deref(), Some("ñ"));
    }

    #[test]
    fn directories_have_no_extension() {
        assert_eq!(FileEntry::new("v1.2", true).extension(), None);
    }
}
