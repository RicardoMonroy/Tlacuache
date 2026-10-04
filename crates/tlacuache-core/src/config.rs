//! Configuración de usuario en TOML (`~/.config/tlacuache/config.toml`).
//!
//! Todas las secciones y campos tienen valor por defecto, así que un archivo
//! parcial se combina con los defaults. Las claves desconocidas se ignoran.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Nombre del archivo dentro del directorio de configuración de la app.
pub const FILE_NAME: &str = "config.toml";

/// Plantilla que se escribe la primera vez. Debe equivaler a `Config::default()`.
pub const DEFAULT_TEMPLATE: &str = include_str!("config_default.toml");

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub preview: Preview,
    pub terminal: Terminal,
    pub theme: Theme,
    pub favorites: Vec<FavoriteGroup>,
    /// Atajos remapeados: nombre de acción -> acelerador (p. ej. `"F4"`).
    pub keys: BTreeMap<String, String>,
}

/// Vista de una pestaña. Al añadir una (p. ej. iconos), el compilador
/// señala cada lugar que debe manejarla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    /// Columnas Miller.
    #[default]
    Columns,
    /// Lista detallada.
    Details,
}

impl ViewMode {
    pub const ALL: [Self; 2] = [Self::Columns, Self::Details];

    /// Identificador estable (el mismo que en el TOML).
    pub fn id(self) -> &'static str {
        match self {
            Self::Columns => "columns",
            Self::Details => "details",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.id() == id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub show_hidden: bool,
    pub dual_pane: bool,
    pub confirm_trash: bool,
    /// Vista con la que se abren las pestañas nuevas.
    pub default_view: ViewMode,
}

impl Default for General {
    fn default() -> Self {
        Self {
            show_hidden: false,
            dual_pane: true,
            confirm_trash: false,
            default_view: ViewMode::Columns,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewPosition {
    #[default]
    Bottom,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preview {
    pub position: PreviewPosition,
    pub max_text_bytes: u64,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            position: PreviewPosition::Bottom,
            max_text_bytes: 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Terminal {
    /// Vacío = usar `$SHELL` (con `/bin/bash` como respaldo).
    pub shell: String,
    pub font: String,
    pub sync_panel_to_terminal: bool,
    pub sync_terminal_to_panel: bool,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            shell: String::new(),
            font: "JetBrains Mono 11".to_owned(),
            sync_panel_to_terminal: true,
            sync_terminal_to_panel: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    #[default]
    Dark,
    Light,
    /// Seguir la preferencia del sistema.
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    pub color_scheme: ColorScheme,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "nord".to_owned(),
            color_scheme: ColorScheme::Dark,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FavoriteGroup {
    pub group: String,
    /// Rutas tal como las escribió el usuario (pueden empezar con `~`).
    pub paths: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("no se pudo leer o escribir {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("error de sintaxis en {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

impl Config {
    /// Interpreta el contenido TOML de un archivo de configuración.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Lee `path`; si no existe, crea los directorios y escribe la plantilla
    /// por defecto. Nunca sobrescribe un archivo existente.
    pub fn load_or_create(path: &Path) -> Result<Self, ConfigError> {
        let io_err = |source| ConfigError::Io {
            path: path.to_owned(),
            source,
        };

        match fs::read_to_string(path) {
            Ok(text) => Self::from_toml(&text).map_err(|source| ConfigError::Parse {
                path: path.to_owned(),
                source,
            }),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                write_template(path).map_err(io_err)?;
                Ok(Self::default())
            }
            Err(err) => Err(io_err(err)),
        }
    }
}

fn write_template(path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // `create_new` evita pisar un archivo creado entre la lectura y la escritura.
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => file.write_all(DEFAULT_TEMPLATE.as_bytes()),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let c = Config::default();
        assert!(!c.general.show_hidden);
        assert!(c.general.dual_pane);
        assert!(!c.general.confirm_trash);
        assert_eq!(c.general.default_view, ViewMode::Columns);
        assert_eq!(c.preview.position, PreviewPosition::Bottom);
        assert_eq!(c.preview.max_text_bytes, 1_048_576);
        assert_eq!(c.terminal.shell, "");
        assert_eq!(c.terminal.font, "JetBrains Mono 11");
        assert!(c.terminal.sync_panel_to_terminal);
        assert!(c.terminal.sync_terminal_to_panel);
        assert_eq!(c.theme.name, "nord");
        assert_eq!(c.theme.color_scheme, ColorScheme::Dark);
        assert!(c.favorites.is_empty());
        assert!(c.keys.is_empty());
    }

    #[test]
    fn template_equals_defaults() {
        assert_eq!(
            Config::from_toml(DEFAULT_TEMPLATE).unwrap(),
            Config::default()
        );
    }

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(Config::from_toml("").unwrap(), Config::default());
    }

    #[test]
    fn partial_section_keeps_other_defaults() {
        let c = Config::from_toml("[general]\nshow_hidden = true\n").unwrap();
        assert!(c.general.show_hidden);
        assert!(c.general.dual_pane);
        assert_eq!(c.terminal, Terminal::default());
    }

    #[test]
    fn parses_full_example() {
        let text = r#"
            [general]
            show_hidden = true
            dual_pane = false
            confirm_trash = true
            default_view = "details"

            [preview]
            position = "right"
            max_text_bytes = 2048

            [terminal]
            shell = "/usr/bin/zsh"
            font = "Iosevka 12"
            sync_panel_to_terminal = false
            sync_terminal_to_panel = false

            [theme]
            name = "nord"
            color_scheme = "system"

            [[favorites]]
            group = "Proyectos"
            paths = ["~/dev", "~/Documentos/clases"]

            [[favorites]]
            group = "Vacío"

            [keys]
            toggle_terminal = "F4"
        "#;
        let c = Config::from_toml(text).unwrap();
        assert!(!c.general.dual_pane);
        assert_eq!(c.general.default_view, ViewMode::Details);
        assert_eq!(c.preview.position, PreviewPosition::Right);
        assert_eq!(c.preview.max_text_bytes, 2048);
        assert_eq!(c.terminal.shell, "/usr/bin/zsh");
        assert_eq!(c.theme.color_scheme, ColorScheme::System);
        assert_eq!(c.favorites.len(), 2);
        assert_eq!(c.favorites[0].group, "Proyectos");
        assert_eq!(c.favorites[0].paths, ["~/dev", "~/Documentos/clases"]);
        assert!(c.favorites[1].paths.is_empty());
        assert_eq!(
            c.keys.get("toggle_terminal").map(String::as_str),
            Some("F4")
        );
    }

    #[test]
    fn view_mode_ids_round_trip() {
        for mode in ViewMode::ALL {
            assert_eq!(ViewMode::from_id(mode.id()), Some(mode));
        }
        assert_eq!(ViewMode::from_id("icons"), None);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let c = Config::from_toml("future_option = 1\n[general]\nnew_flag = true\n").unwrap();
        assert_eq!(c, Config::default());
    }

    #[test]
    fn invalid_enum_value_is_an_error() {
        assert!(Config::from_toml("[preview]\nposition = \"left\"\n").is_err());
    }

    #[test]
    fn wrong_type_is_an_error() {
        assert!(Config::from_toml("[general]\nshow_hidden = \"yes\"\n").is_err());
    }

    #[test]
    fn load_creates_missing_file_with_template() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tlacuache").join(FILE_NAME);

        let c = Config::load_or_create(&path).unwrap();

        assert_eq!(c, Config::default());
        assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_TEMPLATE);
    }

    #[test]
    fn load_reads_existing_file_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let text = "[general]\ndual_pane = false\n";
        fs::write(&path, text).unwrap();

        let c = Config::load_or_create(&path).unwrap();

        assert!(!c.general.dual_pane);
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn load_reports_parse_error_with_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        fs::write(&path, "[general\n").unwrap();

        let err = Config::load_or_create(&path).unwrap_err();

        assert!(matches!(err, ConfigError::Parse { .. }));
        assert!(err.to_string().contains(FILE_NAME));
    }
}
