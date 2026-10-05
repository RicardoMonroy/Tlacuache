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

/// Vista de una pestaña. Al añadir una, el compilador señala cada lugar
/// que debe manejarla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    /// Columnas Miller.
    #[default]
    Columns,
    /// Lista detallada.
    Details,
    /// Cuadrícula de íconos.
    Icons,
}

impl ViewMode {
    pub const ALL: [Self; 3] = [Self::Columns, Self::Details, Self::Icons];

    /// Identificador estable (el mismo que en el TOML).
    pub fn id(self) -> &'static str {
        match self {
            Self::Columns => "columns",
            Self::Details => "details",
            Self::Icons => "icons",
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
    /// Familia (o descripción de Pango) de la fuente. Vacío = `font_mono`
    /// del tema activo.
    pub font: String,
    /// Tamaño en puntos; manda sobre un tamaño escrito en `font`.
    pub font_size: f64,
    pub sync_panel_to_terminal: bool,
    pub sync_terminal_to_panel: bool,
    /// Integración automática del shell (OSC 7) para seguir los `cd` de la
    /// terminal: bash y zsh; fish lo hace solo.
    pub shell_integration: bool,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            shell: String::new(),
            font: String::new(),
            font_size: DEFAULT_FONT_SIZE,
            sync_panel_to_terminal: true,
            sync_terminal_to_panel: true,
            shell_integration: true,
        }
    }
}

const DEFAULT_FONT_SIZE: f64 = 11.0;
const FONT_SIZE_RANGE: std::ops::RangeInclusive<f64> = 6.0..=72.0;

impl Terminal {
    /// Fuente a usar: la de la config o, si está vacía, la del tema.
    pub fn font_family<'a>(&'a self, theme_mono: &'a str) -> &'a str {
        match self.font.trim() {
            "" => theme_mono,
            font => font,
        }
    }

    /// Tamaño en puntos dentro de un rango legible (un valor absurdo o no
    /// finito vuelve al predeterminado).
    pub fn font_size(&self) -> f64 {
        if self.font_size.is_finite() {
            self.font_size
                .clamp(*FONT_SIZE_RANGE.start(), *FONT_SIZE_RANGE.end())
        } else {
            DEFAULT_FONT_SIZE
        }
    }
}

/// `[theme]` (`docs/THEMES.md` §5.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    /// Tema fijo (id), si no se sigue al sistema.
    pub name: String,
    /// Usar `light` o `dark` según la preferencia clara/oscura del sistema.
    pub follow_system: bool,
    pub light: String,
    pub dark: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "nord".to_owned(),
            follow_system: false,
            light: "claro".to_owned(),
            dark: "nord".to_owned(),
        }
    }
}

impl Theme {
    /// Id del tema a usar según la preferencia del sistema (`system_dark`)
    /// si `follow_system` está activo; si no, el tema fijo.
    pub fn resolve(&self, system_dark: bool) -> &str {
        match (self.follow_system, system_dark) {
            (false, _) => &self.name,
            (true, true) => &self.dark,
            (true, false) => &self.light,
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

/// Reescribe solo `[[favorites]]` en el texto de config.toml, conservando
/// comentarios y formato del resto. Falla si el texto no es TOML válido
/// (en ese caso no hay que tocar el archivo).
pub fn update_favorites_toml(
    text: &str,
    groups: &[FavoriteGroup],
) -> Result<String, toml_edit::TomlError> {
    let mut doc: toml_edit::DocumentMut = text.parse()?;

    // Los comentarios al final del archivo (p. ej. ejemplos bajo `[keys]`)
    // son texto final del documento: quedarían debajo de `[[favorites]]` y
    // descomentarlos los metería en un favorito. Se llevan delante del
    // primer favorito, junto con lo que ya hubiera ahí.
    let mut carried = String::new();
    if let Some(toml_edit::Item::ArrayOfTables(old)) = doc.remove("favorites")
        && let Some(prefix) = old.get(0).and_then(|t| t.decor().prefix()?.as_str())
    {
        carried.push_str(prefix);
    }
    carried.push_str(doc.trailing().as_str().unwrap_or(""));
    doc.set_trailing("");

    if groups.is_empty() {
        doc.set_trailing(carried.trim_start_matches('\n'));
        return Ok(doc.to_string());
    }

    let mut tables = toml_edit::ArrayOfTables::new();
    for group in groups {
        let mut table = toml_edit::Table::new();
        table["group"] = toml_edit::value(group.group.as_str());
        let paths: toml_edit::Array = group.paths.iter().map(String::as_str).collect();
        table["paths"] = toml_edit::value(paths);
        tables.push(table);
    }
    let mut prefix = carried.trim_matches('\n').to_owned();
    prefix.insert(0, '\n');
    if prefix.len() > 1 {
        prefix.push_str("\n\n");
    }
    if let Some(first) = tables.get_mut(0) {
        first.decor_mut().set_prefix(prefix);
    }
    doc.insert("favorites", toml_edit::Item::ArrayOfTables(tables));
    Ok(doc.to_string())
}

/// Reescribe las claves de `[theme]` en el texto de config.toml (Preferencias),
/// conservando comentarios, formato y el resto del archivo. Falla si el
/// texto no es TOML válido (en ese caso no hay que tocar el archivo).
pub fn update_theme_toml(text: &str, theme: &Theme) -> Result<String, toml_edit::TomlError> {
    let mut doc: toml_edit::DocumentMut = text.parse()?;
    if !doc.contains_table("theme") {
        doc.insert("theme", toml_edit::Item::Table(toml_edit::Table::new()));
    }
    let table = &mut doc["theme"];
    // Una config vieja puede traer `color_scheme` (antes de ADR-009).
    if let Some(table) = table.as_table_mut() {
        table.remove("color_scheme");
    }
    for (key, value) in [
        ("name", toml_edit::value(theme.name.as_str())),
        ("follow_system", toml_edit::value(theme.follow_system)),
        ("light", toml_edit::value(theme.light.as_str())),
        ("dark", toml_edit::value(theme.dark.as_str())),
    ] {
        match table.get_mut(key).and_then(|item| item.as_value_mut()) {
            // Conserva el comentario al final de la línea.
            Some(old) => {
                let decor = old.decor().clone();
                let mut new = value
                    .into_value()
                    .unwrap_or_else(|_| toml_edit::Value::from(""));
                *new.decor_mut() = decor;
                *old = new;
            }
            None => table[key] = value,
        }
    }
    Ok(doc.to_string())
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
        assert_eq!(c.terminal.font, "");
        assert_eq!(c.terminal.font_size(), 11.0);
        assert!(c.terminal.sync_panel_to_terminal);
        assert!(c.terminal.sync_terminal_to_panel);
        assert!(c.terminal.shell_integration);
        assert_eq!(c.theme.name, "nord");
        assert!(!c.theme.follow_system);
        assert_eq!(
            (c.theme.light.as_str(), c.theme.dark.as_str()),
            ("claro", "nord")
        );
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
            follow_system = true
            light = "talavera"

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
        assert_eq!(c.terminal.font_family("JetBrains Mono"), "Iosevka 12");
        assert!(c.theme.follow_system);
        assert_eq!(c.theme.resolve(false), "talavera");
        assert_eq!(c.theme.resolve(true), "nord");
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
        assert_eq!(ViewMode::from_id("icons"), Some(ViewMode::Icons));
        assert_eq!(ViewMode::from_id("mosaico"), None);
        let c = Config::from_toml("[general]\ndefault_view = \"icons\"\n").unwrap();
        assert_eq!(c.general.default_view, ViewMode::Icons);
    }

    #[test]
    fn terminal_font_comes_from_theme_unless_configured() {
        let mut t = Terminal::default();
        assert_eq!(t.font_family("JetBrains Mono"), "JetBrains Mono");
        t.font = "  ".to_owned();
        assert_eq!(t.font_family("Fira Code"), "Fira Code");
        t.font = "Iosevka".to_owned();
        assert_eq!(t.font_family("Fira Code"), "Iosevka");

        for (size, expected) in [(13.5, 13.5), (2.0, 6.0), (500.0, 72.0), (f64::NAN, 11.0)] {
            t.font_size = size;
            assert_eq!(t.font_size(), expected);
        }
        let c = Config::from_toml("[terminal]\nfont_size = 14\n").unwrap();
        assert_eq!(c.terminal.font_size(), 14.0);
    }

    #[test]
    fn update_theme_keeps_comments_and_other_sections() {
        let text = "# mi config\n[general]\nshow_hidden = true\n\n[theme]\nname = \"nord\"              # comentario\ncolor_scheme = \"dark\"\n\n[keys]\n";
        let theme = Theme {
            name: "claro".into(),
            follow_system: true,
            light: "talavera".into(),
            dark: "consola".into(),
        };
        let updated = update_theme_toml(text, &theme).unwrap();
        assert!(updated.starts_with("# mi config\n[general]\nshow_hidden = true\n"));
        assert!(updated.contains("name = \"claro\"              # comentario\n"));
        assert!(!updated.contains("color_scheme"));
        let config = Config::from_toml(&updated).unwrap();
        assert_eq!(config.theme, theme);
        assert!(config.general.show_hidden);

        // Sin sección [theme] (o archivo vacío) se crea.
        let created = update_theme_toml("", &theme).unwrap();
        assert_eq!(Config::from_toml(&created).unwrap().theme, theme);
        // La plantilla por defecto conserva sus comentarios.
        let template = update_theme_toml(DEFAULT_TEMPLATE, &theme).unwrap();
        assert!(template.contains("# nord | consola | claro | cyberpunk | talavera"));
        assert_eq!(Config::from_toml(&template).unwrap().theme, theme);

        assert!(update_theme_toml("[theme", &theme).is_err());
    }

    #[test]
    fn theme_resolution() {
        let fixed = Theme {
            name: "consola".into(),
            ..Theme::default()
        };
        assert_eq!(fixed.resolve(true), "consola");
        assert_eq!(fixed.resolve(false), "consola");
        // Una config vieja con `color_scheme` se ignora sin error.
        let old = Config::from_toml("[theme]\nname = \"nord\"\ncolor_scheme = \"dark\"\n").unwrap();
        assert_eq!(old.theme, Theme::default());
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

    fn group(name: &str, paths: &[&str]) -> FavoriteGroup {
        FavoriteGroup {
            group: name.to_owned(),
            paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        }
    }

    #[test]
    fn update_favorites_keeps_comments_and_other_sections() {
        let groups = [
            group("Proyectos", &["~/dev", "/srv/x"]),
            group("Vacío", &[]),
        ];
        let text = update_favorites_toml(DEFAULT_TEMPLATE, &groups).unwrap();

        assert!(text.contains("# Configuración de Tlacuache Browser."));
        assert!(text.contains("position = \"bottom\"        # bottom | right"));
        let config = Config::from_toml(&text).unwrap();
        assert_eq!(config.favorites, groups);
        assert_eq!(
            Config {
                favorites: Vec::new(),
                ..config
            },
            Config::default()
        );
    }

    #[test]
    fn update_favorites_replaces_previous_ones() {
        let first = update_favorites_toml(DEFAULT_TEMPLATE, &[group("A", &["~/a"])]).unwrap();
        let second = update_favorites_toml(&first, &[group("B", &["~/b"])]).unwrap();
        assert_eq!(
            Config::from_toml(&second).unwrap().favorites,
            [group("B", &["~/b"])]
        );
        assert!(!second.contains("\"A\""));
    }

    #[test]
    fn update_favorites_with_none_removes_section() {
        let first = update_favorites_toml(DEFAULT_TEMPLATE, &[group("A", &["~/a"])]).unwrap();
        let cleared = update_favorites_toml(&first, &[]).unwrap();
        assert!(Config::from_toml(&cleared).unwrap().favorites.is_empty());
        assert!(!cleared.contains("[[favorites]]\ngroup"));
    }

    #[test]
    fn trailing_comments_stay_above_favorites_across_rewrites() {
        let groups = [group("A", &["~/a"])];
        let first = update_favorites_toml(DEFAULT_TEMPLATE, &groups).unwrap();
        let second = update_favorites_toml(&first, &groups).unwrap();
        for text in [&first, &second] {
            let comment = text.find("# toggle_terminal").unwrap();
            let favorites = text.find("[[favorites]]\ngroup").unwrap();
            assert!(comment < favorites, "{text}");
        }
        assert_eq!(first, second);
        // Al quitar los favoritos el comentario vuelve al final.
        let cleared = update_favorites_toml(&second, &[]).unwrap();
        assert!(
            cleared
                .trim_end()
                .ends_with("# leave_terminal = \"<Control><Shift>Tab\"")
        );
    }

    #[test]
    fn update_favorites_rejects_invalid_toml() {
        assert!(update_favorites_toml("[general", &[]).is_err());
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
