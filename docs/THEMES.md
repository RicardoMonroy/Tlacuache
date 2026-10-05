# Sistema de temas — Tlacuache Browser

Vista previa de los cinco temas: `branding/brand-board.html` §5 (generada desde los TOML reales).

## 1. Objetivo

Un tema es **un solo archivo TOML** que define colores semánticos, la paleta de la terminal y unos cuantos flags de estilo. La app genera las variables CSS y configura VTE a partir de ese archivo. Ningún color se escribe directamente en el código Rust ni en `base.css`.

## 2. Temas incluidos

| id | Nombre | Variante | Carácter |
|---|---|---|---|
| `nord` | Nord | dark | **Predeterminado.** Ártico y sereno. |
| `consola` | Consola | dark | Aspecto TUI: todo monoespaciado, radio 0, sin sombras, selección invertida, glifos en lugar de íconos, edad como texto, títulos `┤ Panel ├`. |
| `claro` | Claro | light | Claro frío y de alto contraste. |
| `cyberpunk` | Cyberpunk | dark | Neón cian/magenta/amarillo sobre violeta noche, brillo en el panel activo y encabezados en mayúsculas. |
| `talavera` | Talavera | light | Azul cobalto sobre crema, inspirado en la talavera poblana. Encabezados de panel en cobalto y franja de azulejo opcional en la sidebar. |

Ubicación: `crates/tlacuache-core/themes/*.toml`, embebidos con `include_str!` o gresource. Los temas del usuario van en `~/.config/tlacuache/themes/*.toml`; si un tema del usuario tiene el mismo `id` que uno incluido, lo reemplaza.

## 3. Esquema del archivo

Ver cualquier archivo en `crates/tlacuache-core/themes/` como referencia completa. Todas las secciones son obligatorias excepto `[glyphs]` (solo si `icon_style = "glyph"`) y `[decor]`.

| Sección | Claves |
|---|---|
| `[meta]` | `id`, `name`, `description`, `variant` (`dark`/`light`), `credits` |
| `[style]` | `font_ui` (vacío = sistema), `font_mono`, `mono_ui`, `radius`, `border_width`, `row_height`, `flat`, `glow`, `uppercase_headers`, `bracket_titles`, `icon_style` (`color`/`symbolic`/`glyph`), `selection_style` (`fill`/`invert`), `age_style` (`chip`/`text`) |
| `[colors]` | `window_bg`, `sidebar_bg`, `sidebar_fg`, `header_bg`, `header_fg`, `pane_bg`, `pane_alt_row`, `fg`, `fg_muted`, `border`, `border_strong`, `hover_bg`, `accent`, `accent_fg`, `focus_ring`, `selection_bg`, `selection_fg`, `selection_inactive_bg`, `pane_active_border`, `success`, `warning`, `error`, `info`, `link` |
| `[age]` | `chip_fg`, `hour`, `day`, `week`, `month`, `year` |
| `[filetypes]` | `folder`, `code`, `image`, `video`, `audio`, `archive`, `document`, `pdf`, `spreadsheet`, `text`, `executable`, `other` |
| `[glyphs]` | `folder`, `file`, `symlink`, `executable` |
| `[decor]` | `pattern` (nombre de un SVG en `crates/tlacuache/resources/decor/`) |
| `[terminal]` | `background`, `foreground`, `cursor`, `cursor_text`, `selection_bg`, `selection_fg`, `palette` (exactamente 16 colores, orden ANSI 0–15) |

Buckets de edad (`core::age`), que se muestran como `2h`, `3d`, `12d`, `3M`, `1a`:

| Clave | Rango |
|---|---|
| `hour` | menos de 24 h |
| `day` | 1–6 días |
| `week` | 7–29 días |
| `month` | 1–11 meses |
| `year` | 1 año o más |

Esto **sustituye** la tabla de colores de edad de `UI_SPEC.md` §2.

`icon_style`:

- `color`: íconos simbólicos teñidos con `[filetypes]`.
- `symbolic`: todos los íconos en `fg_muted`.
- `glyph`: en lugar de ícono, un `gtk::Label` monoespaciado con el glifo de `[glyphs]`, y el **nombre** del archivo coloreado según `[filetypes]` (al estilo `LS_COLORS`). Las carpetas llevan `/` al final.

## 4. Reglas de contraste (se validan en pruebas)

`scripts/check-theme-contrast.py crates/tlacuache-core/themes` implementa estas reglas; `core::theme` debe portarlas a Rust como prueba unitaria que recorra todos los temas incluidos.

| Par | Mínimo WCAG |
|---|---|
| `fg` sobre `pane_bg` y `pane_alt_row` | 7:1 |
| `fg_muted`, `sidebar_fg`, `header_fg`, `link`, `success`/`warning`/`error`/`info` sobre su fondo | 4.5:1 |
| `accent_fg`/`accent`, `selection_fg`/`selection_bg`, `fg`/`selection_inactive_bg` | 4.5:1 |
| `chip_fg` sobre cada color de `[age]` (con `chip`); color de edad sobre `pane_bg` (con `text`) | 4.5:1 |
| `[filetypes]` sobre `pane_bg`, `focus_ring`, `pane_active_border`, cursor de terminal | 3:1 |
| Terminal: `foreground`/`background` | 7:1 |
| Terminal: colores ANSI 1–6 y 9–14 sobre `background` | 3:1 |

Un tema de usuario que no pase la validación **se carga de todos modos** y se avisa con un `adw::Toast` que liste los pares fallidos. Un tema que no se pueda parsear provoca un respaldo a `nord` y un toast con el error.

## 5. Implementación

### 5.1 Core (`tlacuache-core::theme`)

- `Theme` es un struct con `serde::Deserialize` y `#[serde(deny_unknown_fields)]` en cada sección; los colores son un newtype `Color` que valida `#RRGGBB`.
- `fn contrast(a: Color, b: Color) -> f64` y `fn validate(&Theme) -> Vec<ContrastIssue>`.
- `fn css_variables(&Theme) -> String` genera el bloque `:root { --tl-...: ...; }`. Pruebas con snapshot del CSS de `nord`.
- `ThemeRegistry` combina los temas incluidos con los del usuario y resuelve por `id`.

### 5.2 Variables CSS generadas

Convención de nombres: `--tl-<clave con guiones>`.

- `[colors]`: `window_bg` → `--tl-window-bg`
- `[age]`: `hour` → `--tl-age-hour`, `chip_fg` → `--tl-age-chip-fg`
- `[filetypes]`: `code` → `--tl-ft-code`
- `[style]`: `--tl-radius: 8px`, `--tl-border-width: 1px`, `--tl-row-height: 26px`, `--tl-font-mono: "JetBrains Mono"`
- `[decor]`: `--tl-decor-image: url("resource:///io/github/rmonroy/Tlacuache/decor/talavera-strip.svg")`. Si el tema no tiene `[decor]`, el valor es `none`.

Además, para que los widgets nativos de libadwaita sigan el tema, se sobrescriben sus variables CSS: ventana, vista, headerbar, sidebar, card, popover, diálogo, acento y colores de estado. **Antes de escribir los nombres, verifica la lista exacta en la página "CSS Variables" de la documentación de libadwaita de la versión instalada** (libadwaita 1.6 o posterior); no los inventes.

### 5.3 App (`ui/theme_manager.rs`)

1. Al iniciar: cargar `base.css` (estático) en un `CssProvider` con prioridad `APPLICATION`.
2. Aplicar el tema: generar las variables y cargarlas en un **segundo** `CssProvider` con `load_from_string`, con prioridad mayor que la de `base.css`. Para cambiar de tema solo se recarga este provider.
3. Llamar `adw::StyleManager::default().set_color_scheme(ForceDark | ForceLight)` según `variant`.
4. Poner o quitar en la ventana las clases de los flags: `tl-mono`, `tl-flat`, `tl-glow`, `tl-uppercase`, `tl-select-invert`, `tl-age-text`.
5. Lo que CSS no puede hacer se hace en Rust:
   - `bracket_titles`: formatear el texto del encabezado como `┤ {título} ├`.
   - `icon_style = "glyph"`: usar un `Label` en lugar de una `Image` en la factory de filas.
   - `uppercase_headers`: si la versión de GTK no soporta `text-transform`, convertir el texto a mayúsculas.
6. Terminal: emitir la señal `theme-changed`. Cada `ui/terminal.rs` aplica:
   - `set_colors(fg, bg, &palette)` con `gdk::RGBA`
   - `set_color_cursor`, `set_color_cursor_foreground`
   - `set_color_highlight`, `set_color_highlight_foreground`
   - fuente con `pango::FontDescription::from_string(font_mono + tamaño de config)`

   Verifica las firmas en docs.rs de la versión de `vte4` fijada.
7. Cambio en vivo sin reiniciar. El tema se elige en Preferencias (lista con miniatura de cada tema) y en el menú principal.

### 5.4 Configuración

```toml
[theme]
name = "nord"            # tema fijo
follow_system = false    # true: usa light/dark según la preferencia del sistema
light = "claro"
dark = "nord"
```

### 5.5 Íconos

- Los íconos van en el gresource bajo `/io/github/rmonroy/Tlacuache/icons/scalable/{actions,mimetypes}/`. `gtk::Application` añade automáticamente `<resource_base_path>/icons` al tema de íconos; verifica que el `resource_base_path` coincida con el app-id.
- En la factory de filas, al ícono se le añade la clase `tl-ft-<categoría>`. El color sale del tema.

## 6. Añadir un tema nuevo (guía para usuarios y comunidad)

1. Copiar `crates/tlacuache-core/themes/nord.toml` a `~/.config/tlacuache/themes/mitema.toml` y cambiar `id` y `name`.
2. Ajustar los colores y ejecutar `python scripts/check-theme-contrast.py ~/.config/tlacuache/themes`.
3. Seleccionarlo en Preferencias; se aplica al instante.

## 7. Ideas futuras (fuera del MVP)

- **Tema "Sistema (Omarchy)":** leer los colores del tema actual de Omarchy y mapearlos a tokens. Antes de diseñarlo, verifica en la máquina dónde guarda Omarchy el tema activo y en qué formato (posiblemente bajo `~/.config/omarchy/current/theme/`). Puede necesitar un ADR propio.
- Importar esquemas base16 como temas.
- Miniaturas de temas generadas automáticamente para el selector.
