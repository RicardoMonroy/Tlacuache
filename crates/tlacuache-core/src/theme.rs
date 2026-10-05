//! Temas (`docs/THEMES.md`): un TOML con colores semánticos, paleta de la
//! terminal y flags de estilo. De aquí salen las variables CSS `--tl-*` y
//! los colores de VTE; ningún color vive en el código de la UI (regla 9).
//!
//! El parseo es estricto (`deny_unknown_fields`) para detectar erratas en
//! temas de usuario, y `validate` aplica las reglas de contraste WCAG de
//! `scripts/check-theme-contrast.py`.

use std::fmt;
use std::fmt::Write as _;

use serde::{Deserialize, Deserializer};

/// Temas incluidos: (id, TOML). `nord` es el predeterminado.
const BUILTIN: [(&str, &str); 5] = [
    ("nord", include_str!("../themes/nord.toml")),
    ("consola", include_str!("../themes/consola.toml")),
    ("claro", include_str!("../themes/claro.toml")),
    ("cyberpunk", include_str!("../themes/cyberpunk.toml")),
    ("talavera", include_str!("../themes/talavera.toml")),
];

pub const DEFAULT_THEME: &str = "nord";

/// Ruta en el gresource de las decoraciones (`[decor] pattern`).
const DECOR_RESOURCE: &str = "resource:///io/github/rmonroy/Tlacuache/decor";

/// Color `#RRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#')?;
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
        })
    }

    /// Luminancia relativa WCAG 2.x.
    pub fn luminance(self) -> f64 {
        let channel = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| {
            serde::de::Error::custom(format!("color inválido «{text}» (se espera #RRGGBB)"))
        })
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Relación de contraste WCAG entre dos colores (1–21).
pub fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    let (light, dark) = if la >= lb { (la, lb) } else { (lb, la) };
    (light + 0.05) / (dark + 0.05)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconStyle {
    /// Íconos simbólicos teñidos con `[filetypes]`.
    Color,
    /// Todos en `fg_muted`.
    Symbolic,
    /// Glifo de texto (`[glyphs]`) y nombre coloreado.
    Glyph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectionStyle {
    Fill,
    Invert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgeStyle {
    Chip,
    Text,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub variant: Variant,
    pub credits: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Style {
    /// Vacío = fuente del sistema.
    pub font_ui: String,
    pub font_mono: String,
    pub mono_ui: bool,
    pub radius: u32,
    pub border_width: u32,
    pub row_height: u32,
    pub flat: bool,
    pub glow: bool,
    pub uppercase_headers: bool,
    pub bracket_titles: bool,
    pub icon_style: IconStyle,
    pub selection_style: SelectionStyle,
    pub age_style: AgeStyle,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    pub window_bg: Color,
    pub sidebar_bg: Color,
    pub sidebar_fg: Color,
    pub header_bg: Color,
    pub header_fg: Color,
    pub pane_bg: Color,
    pub pane_alt_row: Color,
    pub fg: Color,
    pub fg_muted: Color,
    pub border: Color,
    pub border_strong: Color,
    pub hover_bg: Color,
    pub accent: Color,
    pub accent_fg: Color,
    pub focus_ring: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub selection_inactive_bg: Color,
    pub pane_active_border: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    pub link: Color,
}

/// Colores de los buckets de edad (`core::age`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgeColors {
    pub chip_fg: Color,
    pub hour: Color,
    pub day: Color,
    pub week: Color,
    pub month: Color,
    pub year: Color,
}

/// Colores por categoría de archivo (`core::filetype`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileTypeColors {
    pub folder: Color,
    pub code: Color,
    pub image: Color,
    pub video: Color,
    pub audio: Color,
    pub archive: Color,
    pub document: Color,
    pub pdf: Color,
    pub spreadsheet: Color,
    pub text: Color,
    pub executable: Color,
    pub other: Color,
}

/// Qué glifo le toca a una entrada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphKind {
    Folder,
    File,
    Symlink,
    Executable,
}

impl GlyphKind {
    /// El enlace manda sobre el tipo; luego carpeta y ejecutable.
    pub fn of(is_dir: bool, is_symlink: bool, is_executable: bool) -> Self {
        match (is_symlink, is_dir, is_executable) {
            (true, _, _) => Self::Symlink,
            (false, true, _) => Self::Folder,
            (false, false, true) => Self::Executable,
            _ => Self::File,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Glyphs {
    pub folder: String,
    pub file: String,
    pub symlink: String,
    pub executable: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decor {
    /// Nombre de un SVG de `resources/decor/` (sin extensión).
    pub pattern: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalColors {
    pub background: Color,
    pub foreground: Color,
    pub cursor: Color,
    pub cursor_text: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    /// ANSI 0–15.
    pub palette: [Color; 16],
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub meta: Meta,
    pub style: Style,
    pub colors: Colors,
    pub age: AgeColors,
    pub filetypes: FileTypeColors,
    #[serde(default)]
    pub glyphs: Option<Glyphs>,
    #[serde(default)]
    pub decor: Option<Decor>,
    pub terminal: TerminalColors,
}

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("tema con errores: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("el tema «{0}» usa icon_style = \"glyph\" pero no define [glyphs]")]
    MissingGlyphs(String),
    #[error("el tema no tiene id")]
    EmptyId,
}

/// Un par de colores que no alcanza el contraste mínimo.
#[derive(Debug, Clone, PartialEq)]
pub struct ContrastIssue {
    pub pair: String,
    pub ratio: f64,
    pub minimum: f64,
}

impl fmt::Display for ContrastIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {:.2} (mínimo {})",
            self.pair, self.ratio, self.minimum
        )
    }
}

impl Theme {
    pub fn from_toml(text: &str) -> Result<Self, ThemeError> {
        let theme: Self = toml::from_str(text)?;
        if theme.meta.id.trim().is_empty() {
            return Err(ThemeError::EmptyId);
        }
        if theme.style.icon_style == IconStyle::Glyph && theme.glyphs.is_none() {
            return Err(ThemeError::MissingGlyphs(theme.meta.id));
        }
        Ok(theme)
    }

    /// Tema incluido por `id`.
    pub fn builtin(id: &str) -> Option<Self> {
        let (_, text) = BUILTIN.iter().find(|(builtin, _)| *builtin == id)?;
        Self::from_toml(text).ok()
    }

    /// El tema predeterminado (`nord`). Es un archivo incluido en el
    /// binario y validado por las pruebas
    /// (`builtin_themes_parse_and_pass_contrast`); fallar aquí sería un
    /// error de compilación del proyecto, no de datos del usuario.
    pub fn default_theme() -> Self {
        match Self::from_toml(BUILTIN[0].1) {
            Ok(theme) => theme,
            Err(err) => panic!("el tema predeterminado incluido no es válido: {err}"),
        }
    }

    /// Ids de los temas incluidos, en orden de presentación.
    pub fn builtin_ids() -> impl Iterator<Item = &'static str> {
        BUILTIN.iter().map(|(id, _)| *id)
    }

    /// Pares de colores por debajo del contraste mínimo (vacío = cumple).
    /// Mismas reglas que `scripts/check-theme-contrast.py`.
    pub fn validate(&self) -> Vec<ContrastIssue> {
        let c = &self.colors;
        let t = &self.terminal;
        let mut checks: Vec<(String, Color, Color, f64)> = vec![
            ("fg/pane".into(), c.fg, c.pane_bg, 7.0),
            ("fg/alt".into(), c.fg, c.pane_alt_row, 7.0),
            ("muted/pane".into(), c.fg_muted, c.pane_bg, 4.5),
            ("sidebar".into(), c.sidebar_fg, c.sidebar_bg, 4.5),
            ("muted/sidebar".into(), c.fg_muted, c.sidebar_bg, 4.5),
            ("header".into(), c.header_fg, c.header_bg, 4.5),
            ("accent_fg/accent".into(), c.accent_fg, c.accent, 4.5),
            ("sel".into(), c.selection_fg, c.selection_bg, 4.5),
            ("fg/sel_inact".into(), c.fg, c.selection_inactive_bg, 4.5),
            ("link/pane".into(), c.link, c.pane_bg, 4.5),
            ("focus/pane".into(), c.focus_ring, c.pane_bg, 3.0),
            (
                "active_border/pane".into(),
                c.pane_active_border,
                c.pane_bg,
                3.0,
            ),
            ("border_strong/pane".into(), c.border_strong, c.pane_bg, 1.6),
            ("term fg/bg".into(), t.foreground, t.background, 7.0),
            ("term sel".into(), t.selection_fg, t.selection_bg, 4.5),
            ("cursor/bg".into(), t.cursor, t.background, 3.0),
        ];
        for (name, color) in [
            ("success", c.success),
            ("warning", c.warning),
            ("error", c.error),
            ("info", c.info),
        ] {
            checks.push((format!("{name}/pane"), color, c.pane_bg, 4.5));
        }
        for (name, color) in self.age_colors() {
            match self.style.age_style {
                AgeStyle::Chip => {
                    checks.push((format!("age {name}"), self.age.chip_fg, color, 4.5))
                }
                AgeStyle::Text => checks.push((format!("age txt {name}"), color, c.pane_bg, 4.5)),
            }
        }
        for (name, color) in self.filetype_colors() {
            checks.push((format!("ft {name}"), color, c.pane_bg, 3.0));
        }
        for i in (1..=6).chain(9..=14) {
            checks.push((format!("pal{i}"), t.palette[i], t.background, 3.0));
        }

        checks
            .into_iter()
            .filter_map(|(pair, a, b, minimum)| {
                let ratio = contrast(a, b);
                (ratio < minimum).then_some(ContrastIssue {
                    pair,
                    ratio,
                    minimum,
                })
            })
            .collect()
    }

    fn age_colors(&self) -> [(&'static str, Color); 5] {
        let a = &self.age;
        [
            ("hour", a.hour),
            ("day", a.day),
            ("week", a.week),
            ("month", a.month),
            ("year", a.year),
        ]
    }

    fn filetype_colors(&self) -> [(&'static str, Color); 12] {
        let f = &self.filetypes;
        [
            ("folder", f.folder),
            ("code", f.code),
            ("image", f.image),
            ("video", f.video),
            ("audio", f.audio),
            ("archive", f.archive),
            ("document", f.document),
            ("pdf", f.pdf),
            ("spreadsheet", f.spreadsheet),
            ("text", f.text),
            ("executable", f.executable),
            ("other", f.other),
        ]
    }

    /// Título de una pestaña: `┤ nombre ├` con `bracket_titles`.
    pub fn pane_title(&self, name: &str) -> String {
        if self.style.bracket_titles {
            format!("┤ {name} ├")
        } else {
            name.to_owned()
        }
    }

    /// Glifo en lugar del ícono (`icon_style = "glyph"`); `None` si el tema
    /// usa íconos.
    pub fn glyph(&self, kind: GlyphKind) -> Option<&str> {
        if self.style.icon_style != IconStyle::Glyph {
            return None;
        }
        let glyphs = self.glyphs.as_ref()?;
        Some(match kind {
            GlyphKind::Folder => &glyphs.folder,
            GlyphKind::File => &glyphs.file,
            GlyphKind::Symlink => &glyphs.symlink,
            GlyphKind::Executable => &glyphs.executable,
        })
    }

    /// Nombre a mostrar en las filas: con glifos, las carpetas llevan `/`
    /// al final (como `ls -F`).
    pub fn entry_name<'a>(&self, name: &'a str, is_dir: bool) -> std::borrow::Cow<'a, str> {
        if is_dir && self.style.icon_style == IconStyle::Glyph {
            format!("{name}/").into()
        } else {
            name.into()
        }
    }

    /// Id del esquema de GtkSourceView generado para este tema.
    pub fn source_scheme_id(&self) -> String {
        format!("tlacuache-{}", self.meta.id)
    }

    /// Esquema de GtkSourceView (XML) para la vista previa de código
    /// (ADR-011): fondo, texto y selección de `[colors]`; la sintaxis, de la
    /// paleta ANSI de `[terminal]`, como la ven los programas en la terminal.
    pub fn source_scheme(&self) -> String {
        let c = &self.colors;
        let t = &self.terminal;
        let p = &t.palette;
        let kind = match self.meta.variant {
            Variant::Dark => "dark",
            Variant::Light => "light",
        };
        // (estilo, frente, fondo, extra)
        let styles: [(&str, Option<Color>, Option<Color>, &str); 26] = [
            ("text", Some(t.foreground), Some(t.background), ""),
            ("selection", Some(c.selection_fg), Some(c.selection_bg), ""),
            (
                "selection-unfocused",
                None,
                Some(c.selection_inactive_bg),
                "",
            ),
            ("cursor", Some(t.cursor), None, ""),
            ("current-line", None, Some(c.pane_alt_row), ""),
            ("line-numbers", Some(c.fg_muted), Some(t.background), ""),
            ("bracket-match", Some(c.accent_fg), Some(c.accent), ""),
            (
                "search-match",
                Some(c.selection_fg),
                Some(c.selection_bg),
                "",
            ),
            ("def:comment", Some(c.fg_muted), None, r#" italic="true""#),
            ("def:shebang", Some(c.fg_muted), None, r#" bold="true""#),
            ("def:keyword", Some(p[4]), None, r#" bold="true""#),
            ("def:statement", Some(p[4]), None, ""),
            ("def:type", Some(p[14]), None, ""),
            ("def:function", Some(p[6]), None, ""),
            ("def:identifier", Some(t.foreground), None, ""),
            ("def:string", Some(p[2]), None, ""),
            ("def:special-char", Some(p[3]), None, ""),
            ("def:constant", Some(p[5]), None, ""),
            ("def:number", Some(p[5]), None, ""),
            ("def:preprocessor", Some(p[3]), None, ""),
            ("def:builtin", Some(p[12]), None, ""),
            ("def:error", Some(p[1]), None, r#" underline="error""#),
            ("def:heading", Some(p[4]), None, r#" bold="true""#),
            ("def:link-text", Some(c.link), None, ""),
            ("def:inserted", Some(p[2]), None, ""),
            ("def:deleted", Some(p[1]), None, ""),
        ];
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!-- Generado por Tlacuache desde el tema «{id}»; no editar. -->\n\
             <style-scheme id=\"{scheme}\" name=\"{name}\" version=\"1.0\" kind=\"{kind}\">\n",
            id = xml_escape(&self.meta.id),
            scheme = xml_escape(&self.source_scheme_id()),
            name = xml_escape(&format!("Tlacuache {}", self.meta.name)),
        );
        for (name, fg, bg, extra) in styles {
            xml.push_str(&format!("  <style name=\"{name}\""));
            if let Some(fg) = fg {
                xml.push_str(&format!(" foreground=\"{fg}\""));
            }
            if let Some(bg) = bg {
                xml.push_str(&format!(" background=\"{bg}\""));
            }
            xml.push_str(extra);
            xml.push_str("/>\n");
        }
        xml.push_str("</style-scheme>\n");
        xml
    }

    /// Bloque `:root { --tl-*: …; }` que carga el `ThemeManager`.
    /// Convención: `--tl-<clave con guiones>` (`docs/THEMES.md` §5.2).
    pub fn css_variables(&self) -> String {
        let c = &self.colors;
        let mut vars: Vec<(String, String)> = [
            ("window-bg", c.window_bg),
            ("sidebar-bg", c.sidebar_bg),
            ("sidebar-fg", c.sidebar_fg),
            ("header-bg", c.header_bg),
            ("header-fg", c.header_fg),
            ("pane-bg", c.pane_bg),
            ("pane-alt-row", c.pane_alt_row),
            ("fg", c.fg),
            ("fg-muted", c.fg_muted),
            ("border", c.border),
            ("border-strong", c.border_strong),
            ("hover-bg", c.hover_bg),
            ("accent", c.accent),
            ("accent-fg", c.accent_fg),
            ("focus-ring", c.focus_ring),
            ("selection-bg", c.selection_bg),
            ("selection-fg", c.selection_fg),
            ("selection-inactive-bg", c.selection_inactive_bg),
            ("pane-active-border", c.pane_active_border),
            ("success", c.success),
            ("warning", c.warning),
            ("error", c.error),
            ("info", c.info),
            ("link", c.link),
        ]
        .into_iter()
        .map(|(name, color)| (name.to_owned(), color.to_string()))
        .collect();

        vars.push(("age-chip-fg".into(), self.age.chip_fg.to_string()));
        for (name, color) in self.age_colors() {
            vars.push((format!("age-{name}"), color.to_string()));
        }
        for (name, color) in self.filetype_colors() {
            vars.push((format!("ft-{name}"), color.to_string()));
        }

        let s = &self.style;
        vars.push(("radius".into(), format!("{}px", s.radius)));
        vars.push(("border-width".into(), format!("{}px", s.border_width)));
        vars.push(("row-height".into(), format!("{}px", s.row_height)));
        vars.push(("font-mono".into(), font_family(&s.font_mono)));
        vars.push(("font-ui".into(), font_family(&s.font_ui)));
        let decor = self.decor.as_ref().map_or_else(
            || "none".to_owned(),
            |d| format!("url(\"{DECOR_RESOURCE}/{}.svg\")", d.pattern),
        );
        vars.push(("decor-image".into(), decor));

        let mut css = String::from(":root {\n");
        for (name, value) in vars {
            // Escribir en un String no falla.
            let _ = writeln!(css, "  --tl-{name}: {value};");
        }
        css.push_str("}\n");
        css
    }
}

/// Familia CSS: el nombre entre comillas con respaldo genérico; vacío =
/// fuente del sistema (`inherit`).
fn font_family(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        "inherit".to_owned()
    } else {
        format!("\"{}\", monospace", name.replace('"', ""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_parsing() {
        assert_eq!(
            Color::parse("#2E3440"),
            Some(Color {
                r: 0x2e,
                g: 0x34,
                b: 0x40
            })
        );
        assert_eq!(
            Color::parse("#2e3440").map(|c| c.to_string()).as_deref(),
            Some("#2e3440")
        );
        for bad in ["2E3440", "#2E344", "#2E34400", "#GG0000", "", "#"] {
            assert_eq!(Color::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn contrast_matches_wcag_references() {
        let black = Color::parse("#000000").unwrap();
        let white = Color::parse("#FFFFFF").unwrap();
        assert!((contrast(black, white) - 21.0).abs() < 1e-9);
        assert!((contrast(white, white) - 1.0).abs() < 1e-9);
        assert_eq!(contrast(black, white), contrast(white, black));
        // Referencia conocida: #767676 sobre blanco ≈ 4.54.
        let gray = Color::parse("#767676").unwrap();
        assert!((contrast(gray, white) - 4.54).abs() < 0.01);
    }

    /// Criterio de aceptación de la tarea 7.3.
    #[test]
    fn consola_flags_in_rust() {
        let consola = Theme::builtin("consola").unwrap();
        assert_eq!(consola.pane_title("Descargas"), "┤ Descargas ├");
        assert_eq!(consola.glyph(GlyphKind::Folder), Some("▸"));
        assert_eq!(consola.glyph(GlyphKind::File), Some("·"));
        assert_eq!(consola.glyph(GlyphKind::Symlink), Some("↪"));
        assert_eq!(consola.glyph(GlyphKind::Executable), Some("*"));
        assert_eq!(consola.entry_name("src", true), "src/");
        assert_eq!(consola.entry_name("main.rs", false), "main.rs");

        let nord = Theme::default_theme();
        assert_eq!(nord.pane_title("Descargas"), "Descargas");
        assert_eq!(nord.glyph(GlyphKind::Folder), None);
        assert_eq!(nord.entry_name("src", true), "src");
    }

    #[test]
    fn glyph_kind_precedence() {
        assert_eq!(GlyphKind::of(true, true, false), GlyphKind::Symlink);
        assert_eq!(GlyphKind::of(true, false, true), GlyphKind::Folder);
        assert_eq!(GlyphKind::of(false, false, true), GlyphKind::Executable);
        assert_eq!(GlyphKind::of(false, false, false), GlyphKind::File);
    }

    #[test]
    fn source_scheme_uses_theme_colors_and_escapes() {
        let mut theme = Theme::default_theme();
        let xml = theme.source_scheme();
        assert!(xml.contains(r#"<style-scheme id="tlacuache-nord" name="Tlacuache Nord""#));
        assert!(xml.contains(r#"kind="dark""#));
        assert!(
            xml.contains(r##"<style name="text" foreground="#d8dee9" background="#2e3440"/>"##)
        );
        assert!(xml.contains(r##"<style name="def:string" foreground="#a3be8c"/>"##));
        assert!(xml.contains(r#"italic="true""#));
        assert_eq!(xml.matches("<style ").count(), 26);
        assert!(xml.trim_end().ends_with("</style-scheme>"));

        theme.meta.name = "A & <B>".into();
        assert!(
            theme
                .source_scheme()
                .contains(r#"name="Tlacuache A &amp; &lt;B&gt;""#)
        );
    }

    #[test]
    fn syntax_colors_are_readable_in_every_builtin_theme() {
        for id in Theme::builtin_ids() {
            let theme = Theme::builtin(id).unwrap();
            let t = &theme.terminal;
            let mut colors: Vec<Color> = [1, 2, 3, 4, 5, 6, 12, 14].map(|i| t.palette[i]).into();
            colors.extend([theme.colors.fg_muted, theme.colors.link]);
            for color in colors {
                let ratio = contrast(color, t.background);
                assert!(
                    ratio >= 3.0,
                    "{id}: {color} sobre {} = {ratio:.2}",
                    t.background
                );
            }
        }
    }

    #[test]
    fn builtin_themes_parse_and_pass_contrast() {
        let ids: Vec<_> = Theme::builtin_ids().collect();
        assert_eq!(ids, ["nord", "consola", "claro", "cyberpunk", "talavera"]);
        for id in ids {
            let (_, text) = BUILTIN.iter().find(|(b, _)| *b == id).unwrap();
            let theme = Theme::from_toml(text).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(theme.meta.id, id, "el id del archivo coincide");
            let issues = theme.validate();
            assert!(issues.is_empty(), "{id}: {issues:?}");
        }
    }

    #[test]
    fn builtin_flags_match_their_character() {
        let consola = Theme::builtin("consola").unwrap();
        assert!(consola.style.mono_ui && consola.style.bracket_titles);
        assert_eq!(consola.style.icon_style, IconStyle::Glyph);
        assert_eq!(consola.style.age_style, AgeStyle::Text);
        assert!(consola.glyphs.is_some());
        assert!(Theme::builtin("talavera").unwrap().decor.is_some());
        assert_eq!(
            Theme::builtin("claro").unwrap().meta.variant,
            Variant::Light
        );
        assert!(Theme::builtin("cyberpunk").unwrap().style.glow);
        assert_eq!(Theme::default_theme().meta.id, "nord");
        assert!(Theme::builtin("no-existe").is_none());
    }

    fn nord_text() -> &'static str {
        BUILTIN[0].1
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let typo = nord_text().replace("pane_bg = ", "pane_gb = ");
        assert!(Theme::from_toml(&typo).is_err());
        let extra = format!("{}\n[extra]\nx = 1\n", nord_text());
        assert!(Theme::from_toml(&extra).is_err());
    }

    #[test]
    fn invalid_color_is_reported_with_value() {
        let bad = nord_text().replace("fg = \"#ECEFF4\"", "fg = \"azul\"");
        let err = Theme::from_toml(&bad).unwrap_err().to_string();
        assert!(err.contains("azul"), "{err}");
    }

    #[test]
    fn glyph_style_requires_glyphs_section() {
        let text = nord_text().replace("icon_style = \"color\"", "icon_style = \"glyph\"");
        assert!(matches!(
            Theme::from_toml(&text),
            Err(ThemeError::MissingGlyphs(_))
        ));
    }

    #[test]
    fn palette_needs_exactly_16_colors() {
        let short = nord_text().replace("\"#8FBCBB\", \"#ECEFF4\"]", "\"#8FBCBB\"]");
        assert_ne!(short, nord_text());
        assert!(Theme::from_toml(&short).is_err());
    }

    #[test]
    fn validate_reports_low_contrast_pairs() {
        let mut theme = Theme::default_theme();
        theme.colors.fg = theme.colors.pane_bg;
        let issues = theme.validate();
        assert!(issues.iter().any(|i| i.pair == "fg/pane" && i.ratio < 1.01));
        assert!(issues.iter().all(|i| i.ratio < i.minimum));
    }

    #[test]
    fn age_text_style_checks_colors_against_pane() {
        let consola = Theme::builtin("consola").unwrap();
        let mut theme = consola.clone();
        theme.age.hour = theme.colors.pane_bg;
        assert!(theme.validate().iter().any(|i| i.pair == "age txt hour"));
    }

    /// Prueba de snapshot: el CSS de `nord` es exactamente el guardado.
    /// Si cambia a propósito, actualizar `tests/data/nord-variables.css`.
    #[test]
    fn nord_css_variables_snapshot() {
        let expected = include_str!("../tests/data/nord-variables.css");
        assert_eq!(Theme::default_theme().css_variables(), expected);
    }

    #[test]
    fn css_variables_cover_decor_and_fonts() {
        let talavera = Theme::builtin("talavera").unwrap().css_variables();
        assert!(talavera.contains(
            "--tl-decor-image: url(\"resource:///io/github/rmonroy/Tlacuache/decor/talavera-strip.svg\");"
        ));
        let nord = Theme::default_theme().css_variables();
        assert!(nord.contains("--tl-decor-image: none;"));
        assert!(nord.contains("--tl-font-ui: inherit;"));
        assert!(nord.contains("--tl-font-mono: \"JetBrains Mono\", monospace;"));
    }
}
