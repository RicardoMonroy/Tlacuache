//! Temas disponibles: los incluidos más los del usuario en
//! `~/.config/tlacuache/themes/*.toml` (`docs/THEMES.md` §4 y §6).
//!
//! Un tema de usuario con el mismo `id` que uno incluido lo reemplaza (así
//! se puede retocar Nord). Los archivos que no se pueden leer o parsear no
//! entran en la lista y quedan en `errors` para avisar al usuario.

use std::fs;
use std::path::{Path, PathBuf};

use crate::theme::{Theme, ThemeError};

/// Nombre de la carpeta de temas de usuario dentro de la config.
pub const USER_DIR: &str = "themes";

/// Un archivo de tema que no se pudo cargar.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadError {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ThemeCatalog {
    themes: Vec<Theme>,
    /// Ids que vienen de archivos del usuario.
    user_ids: Vec<String>,
    errors: Vec<LoadError>,
}

impl Default for ThemeCatalog {
    fn default() -> Self {
        Self::builtin()
    }
}

impl ThemeCatalog {
    /// Solo los temas incluidos.
    pub fn builtin() -> Self {
        Self {
            themes: Theme::builtin_ids().filter_map(Theme::builtin).collect(),
            user_ids: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Incluidos más los `*.toml` de `dir` (en orden alfabético). Si la
    /// carpeta no existe, solo los incluidos.
    pub fn load(dir: &Path) -> Self {
        let mut catalog = Self::builtin();
        let Ok(entries) = fs::read_dir(dir) else {
            return catalog;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "toml") && p.is_file())
            .collect();
        paths.sort();
        for path in paths {
            let loaded = fs::read_to_string(&path)
                .map_err(|err| err.to_string())
                .and_then(|text| Theme::from_toml(&text).map_err(|err| summary(&text, &err)));
            match loaded {
                Ok(theme) => catalog.add_user(theme),
                Err(message) => catalog.errors.push(LoadError { path, message }),
            }
        }
        catalog
    }

    fn add_user(&mut self, theme: Theme) {
        let id = theme.meta.id.clone();
        match self.themes.iter_mut().find(|t| t.meta.id == id) {
            Some(existing) => *existing = theme,
            None => self.themes.push(theme),
        }
        if !self.user_ids.contains(&id) {
            self.user_ids.push(id);
        }
    }

    /// Todos los temas en orden de presentación (incluidos primero).
    pub fn themes(&self) -> &[Theme] {
        &self.themes
    }

    pub fn get(&self, id: &str) -> Option<&Theme> {
        self.themes.iter().find(|t| t.meta.id == id)
    }

    /// El tema viene de un archivo del usuario (hay que validarlo).
    pub fn is_user(&self, id: &str) -> bool {
        self.user_ids.iter().any(|u| u == id)
    }

    /// Archivos de tema que no se pudieron cargar.
    pub fn errors(&self) -> &[LoadError] {
        &self.errors
    }
}

/// Error en una línea (para un toast): el de TOML trae varias líneas con
/// un dibujo de la columna.
fn summary(text: &str, err: &ThemeError) -> String {
    match err {
        ThemeError::Parse(err) => match err.span() {
            Some(span) => {
                let line = text[..span.start.min(text.len())].matches('\n').count() + 1;
                format!("{} (línea {line})", err.message())
            }
            None => err.message().to_owned(),
        },
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NORD: &str = include_str!("../themes/nord.toml");

    fn with_id(id: &str, name: &str) -> String {
        NORD.replacen("id = \"nord\"", &format!("id = \"{id}\""), 1)
            .replacen("name = \"Nord\"", &format!("name = \"{name}\""), 1)
    }

    #[test]
    fn builtin_only_without_user_dir() {
        let catalog = ThemeCatalog::load(Path::new("/no/existe/tlacuache-themes"));
        let ids: Vec<_> = catalog
            .themes()
            .iter()
            .map(|t| t.meta.id.as_str())
            .collect();
        assert_eq!(ids, ["nord", "consola", "claro", "cyberpunk", "talavera"]);
        assert!(catalog.errors().is_empty());
        assert!(!catalog.is_user("nord"));
    }

    #[test]
    fn loads_user_themes_overrides_and_reports_errors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("mio.toml"), with_id("mio", "Mío")).unwrap();
        fs::write(
            dir.path().join("nord.toml"),
            with_id("nord", "Nord retocado"),
        )
        .unwrap();
        fs::write(dir.path().join("roto.toml"), "[meta\nid = ").unwrap();
        fs::write(dir.path().join("notas.txt"), "no es un tema").unwrap();
        fs::create_dir(dir.path().join("carpeta.toml")).unwrap();

        let catalog = ThemeCatalog::load(dir.path());
        let ids: Vec<_> = catalog
            .themes()
            .iter()
            .map(|t| t.meta.id.as_str())
            .collect();
        assert_eq!(
            ids,
            ["nord", "consola", "claro", "cyberpunk", "talavera", "mio"]
        );
        assert_eq!(catalog.get("nord").unwrap().meta.name, "Nord retocado");
        assert_eq!(catalog.get("mio").unwrap().meta.name, "Mío");
        assert!(catalog.is_user("mio") && catalog.is_user("nord"));
        assert!(!catalog.is_user("claro"));
        // Se carga aunque no pase la validación (la app avisa).
        let low = NORD
            .replacen("id = \"nord\"", "id = \"bajo\"", 1)
            .replace("fg_muted = \"#A7B0C0\"", "fg_muted = \"#3B4252\"");
        fs::write(dir.path().join("bajo.toml"), low).unwrap();
        let catalog = ThemeCatalog::load(dir.path());
        let bajo = catalog.get("bajo").unwrap();
        assert!(bajo.validate().iter().any(|i| i.pair.starts_with("muted")));

        assert_eq!(catalog.errors().len(), 1);
        let error = &catalog.errors()[0];
        assert!(error.path.ends_with("roto.toml"));
        assert!(!error.message.contains('\n'), "{}", error.message);
        assert!(error.message.ends_with("(línea 1)"), "{}", error.message);
    }
}
