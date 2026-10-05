# Decisiones de arquitectura (ADR)

Formato: contexto → decisión → consecuencias. Añadir nuevas al final con número consecutivo.

## ADR-001 — Solo Linux, Rust + GTK4 + libadwaita
- **Contexto**: el objetivo es un gestor de archivos para Arch Linux; el autor no necesita Windows/macOS para esta app.
- **Decisión**: Rust con gtk4-rs y libadwaita. Sin webview.
- **Consecuencias**: app nativa, ligera y con integración directa a `gio` (montajes, papelera, monitoreo, GVfs). Se pierde la portabilidad; se acepta.

## ADR-002 — VTE para la terminal integrada
- **Contexto**: la terminal por panel es una función central.
- **Decisión**: `vte4` (el widget de GNOME Terminal) en lugar de un emulador propio o xterm.js.
- **Consecuencias**: soporte completo de secuencias de escape, OSC 7, selección, scrollback y rendimiento probado. Dependencia del paquete del sistema `vte4`.

## ADR-003 — `gtk::DirectoryList` para el listado
- **Contexto**: carpetas grandes no deben congelar la UI y deben refrescarse ante cambios externos.
- **Decisión**: usar `gtk::DirectoryList` (asíncrono e incremental, con monitoreo) encadenado a modelos de filtro y orden, en lugar de `std::fs` en hilos propios.
- **Consecuencias**: menos código de concurrencia. La lógica de orden y filtro se mantiene en core para poder probarla.

## ADR-004 — Configuración en TOML, no GSettings
- **Contexto**: se quiere configuración editable a mano y versionable en dotfiles.
- **Decisión**: TOML en `~/.config/tlacuache/config.toml` con `serde`.
- **Consecuencias**: no se requiere instalar esquemas GSettings; la recarga en vivo se implementa a mano.

## ADR-005 — UI construida en código Rust (sin Blueprint por ahora)
- **Contexto**: reducir dependencias de herramientas (`blueprint-compiler`) al inicio.
- **Decisión**: widgets como subclases GObject construidos en Rust.
- **Consecuencias**: más verboso; puede reconsiderarse Blueprint o Relm4 si la UI crece. Requiere un ADR nuevo.

## ADR-006 — Backspace y Espacio editan el filtro rápido cuando está activo
- **Contexto**: el filtro rápido se activa al escribir directamente, pero Backspace (subir nivel) y Espacio (vista previa) también son atajos del panel.
- **Decisión**: si el filtro tiene texto, Backspace y Espacio editan el filtro; si está vacío, ejecutan su acción normal. Esc siempre limpia el filtro.
- **Consecuencias**: el filtro admite espacios y se puede corregir sin salir de él. El despacho de teclas del panel consulta primero el estado del filtro; la regla vive en `core::filter::apply_key`, con pruebas.

## ADR-007 — Orden natural propio con plegado de acentos
- **Contexto**: los nombres deben ordenarse de forma natural (`img2` < `img10`), y el autor usa nombres en español con acentos y `ñ`. Comparar por código Unicode deja `Árbol` después de `zeta`.
- **Decisión**: implementar `core::sort::natural_cmp` a mano, sin crates: tramos numéricos por valor (sin desbordes), texto sin distinguir mayúsculas ni acentos latinos comunes, `ñ` entre `n` y `o`. Empates resueltos por ceros a la izquierda y luego por bytes para un orden total. Carpetas siempre primero.
- **Consecuencias**: sin dependencias y con orden razonable para español. No es una collation completa por locale (p. ej. no maneja ligaduras ni alfabetos no latinos de forma especial); si hiciera falta, evaluar `icu_collator` con un ADR nuevo.

## ADR-008 — Integración del shell (OSC 7) automática y propia
- **Contexto**: para que el panel siga los `cd` de la terminal, el shell debe emitir OSC 7. Bash no lo hace por defecto. `/etc/profile.d/vte.sh` lo resuelve, pero si `PROMPT_COMMAND` es una cadena lo **sobrescribe** (rompe ganchos como mise o zoxide) y además modifica `PS1`. Pedir al usuario que edite su `.bashrc` afecta a todas sus terminales.
- **Decisión**: Tlacuache lanza bash con `--rcfile` apuntando a un archivo propio (`core::shell::BASH_INTEGRATION_RC`, escrito en `$XDG_RUNTIME_DIR/tlacuache/`) que carga `/etc/bash.bashrc` y `~/.bashrc` y **antepone** a `PROMPT_COMMAND` (cadena o arreglo) un gancho que emite OSC 7 con la ruta codificada en bash puro, conservando `$?`. fish emite OSC 7 por sí mismo. Para zsh (P.4), Tlacuache lo lanza con `ZDOTDIR` apuntando a `$XDG_RUNTIME_DIR/tlacuache/zsh/`, cuyo único archivo es un `.zshenv` (`core::shell::ZSH_INTEGRATION_ZSHENV`) que restaura el `ZDOTDIR` del usuario (guardado en `TLACUACHE_USER_ZDOTDIR`), carga su `.zshenv` y, si es interactivo, añade un gancho `precmd` con `add-zsh-hook`; zsh sigue después con el `.zprofile`/`.zshrc` del usuario (técnica de kitty y Ghostty). fish (≥ 3.7) reporta OSC 7 siempre, en cada prompt (notas de versión de fish); el escapado de `cd` y de rutas pegadas se prueba con fish real (P.5). Se desactiva con `terminal.shell_integration = false`.
- **Consecuencias**: funciona sin tocar dotfiles y solo dentro de Tlacuache; probado con bash real (OSC 7 emitido y gancho del usuario intacto). Los bucles panel↔terminal se evitan comparando con la última carpeta conocida de la terminal. Para leer la carpeta se usa `current_directory_uri` (obsoleta desde VTE 0.78) porque su reemplazo no está expuesto en vte4 0.10.

## ADR-009 — Temas como TOML + variables CSS
- **Contexto**: se requieren varios temas (incluido uno estilo TUI) y que el usuario pueda crear los suyos (paquete de identidad, `docs/THEMES.md`).
- **Decisión**: cada tema es un TOML con tokens semánticos. La app genera `:root { --tl-* }` en un CssProvider dedicado sobre un `base.css` sin colores. Se sobrescriben las variables CSS de libadwaita para los widgets nativos. La paleta de VTE sale del mismo archivo. Requiere GTK >= 4.16 y libadwaita >= 1.6 (hay 4.22 y 1.9). Sustituye el `style.css` con colores Nord del Hito 0.3.
- **Consecuencias**: cambio de tema en vivo y temas de comunidad sin recompilar. Lo que CSS no puede expresar (títulos con corchetes, glifos) se implementa en Rust leyendo los flags de `[style]`. Regla 9 de `CLAUDE.md`: ningún color, radio ni fuente en Rust o CSS.

## ADR-010 — Íconos simbólicos teñidos por tema
- **Contexto**: los íconos de tipo de archivo deben cambiar de color con cada tema.
- **Decisión**: un solo set de SVG simbólicos de 16 px (solo `fill`, `evenodd`). El color se aplica con la clase CSS `tl-ft-<categoría>`. Los íconos genéricos se toman de Adwaita. El ícono de la app (hicolor) vive en `data/icons/` y se instala con el `.desktop`.
- **Consecuencias**: no hay sets duplicados por tema. El estilo multicolor tipo OneCommander queda descartado a favor de la consistencia.

## ADR-011 — Resaltado de sintaxis desde el tema
- **Contexto**: la vista previa de código usa GtkSourceView, que toma sus colores de un *style scheme* (XML en disco). Con los esquemas incluidos (Adwaita) los colores no coinciden con el tema y se rompería la regla 9.
- **Decisión**: `Theme::source_scheme()` genera el esquema `tlacuache-<id>`: texto, fondo y selección de `[colors]`/`[terminal]`; la sintaxis, de la paleta ANSI de `[terminal]` (palabras clave = azul, tipos = cian brillante, funciones = cian, cadenas = verde, números y constantes = magenta, preprocesador = amarillo, errores = rojo) y los comentarios en `fg_muted`. El `ThemeManager` lo escribe en `~/.cache/tlacuache/styles/` en un hilo de trabajo, añade esa carpeta a la ruta de búsqueda de `StyleSchemeManager` y emite `source-scheme-changed`.
- **Consecuencias**: el código se ve con los colores de la terminal y cambia con el tema. Una prueba exige contraste ≥ 3:1 de esos colores sobre el fondo en los temas incluidos; los temas de usuario (7.8) deberán validarlo también.

## ADR-012 — Licencia GPL-3.0-or-later
- **Contexto**: el proyecto es gratuito, con el código en GitHub. Hay que elegir licencia antes de publicarlo.
- **Decisión**: `GPL-3.0-or-later` para todo el repositorio (código, íconos y logo propios). Texto en `LICENSE` (SPDX) y `license` en los `Cargo.toml`.
- **Consecuencias**: las versiones modificadas que se distribuyan deben publicar su código con la misma licencia. Es la licencia habitual de las apps de escritorio GTK/GNOME, compatible con las dependencias (GTK, libadwaita, VTE y GtkSourceView son LGPL; los crates, MIT/Apache) y aceptada en AUR y Flathub. Se descartó MIT porque permitiría forks cerrados.

## ADR-013 — Tira de pestañas propia sobre `adw::TabView`
- **Contexto**: `adw::TabBar` centra el título de cada pestaña desde el código de `AdwTab` (sin propiedad ni CSS), y el diseño de la marca lo quiere a la izquierda (P.6).
- **Decisión**: se conserva `adw::TabView` (páginas, atajos, cierre) y se sustituye solo la barra por `TabStrip` + `TabButton` propios: ancho natural fijo y mínimo pequeño (medida propia), desplazamiento al desbordar, arrastrar para reordenar o `transfer_page` al otro panel, clic medio y menú contextual. Las señales de cada pestaña se conectan con `page-attached`/`page-detached`, así siguen a la pestaña cuando cambia de panel.
- **Consecuencias**: control total del aspecto con los tokens del tema (hover y selección con `alpha(currentColor)`, válidos sobre cualquier cabecera). A cambio, se mantiene código propio para el arrastre y el desplazamiento.

## ADR-014 — Notas por carpeta en un almacén central
- **Contexto**: la 8.8 pide notas por carpeta. Guardarlas como archivo oculto dentro de cada carpeta las haría viajar con ella, pero ensucia repositorios y no funciona en carpetas de solo lectura o remotas.
- **Decisión**: un archivo por carpeta en `~/.local/share/tlacuache/notes/<md5(uri)>.md`, con una cabecera con la URI (`core::notes`). Se editan en la vista previa de la carpeta (guardado automático; vaciar = borrar) y la barra de estado indica si la carpeta actual tiene nota. Al mover, renombrar (también en lote) o restaurar carpetas desde Tlacuache, las notas de la carpeta y sus subcarpetas se reubican; el deshacer las devuelve.
- **Consecuencias**: no se toca el contenido de las carpetas del usuario. Si una carpeta se mueve fuera de Tlacuache, su nota queda huérfana (sigue en el almacén y reaparece si la carpeta vuelve a su ruta).
