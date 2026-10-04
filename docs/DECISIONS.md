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
- **Consecuencias**: el filtro admite espacios y se puede corregir sin salir de él. El despacho de teclas del panel debe consultar primero el estado del filtro (lógica testeable en `core::keymap`).

## ADR-007 — Orden natural propio con plegado de acentos
- **Contexto**: los nombres deben ordenarse de forma natural (`img2` < `img10`), y el autor usa nombres en español con acentos y `ñ`. Comparar por código Unicode deja `Árbol` después de `zeta`.
- **Decisión**: implementar `core::sort::natural_cmp` a mano, sin crates: tramos numéricos por valor (sin desbordes), texto sin distinguir mayúsculas ni acentos latinos comunes, `ñ` entre `n` y `o`. Empates resueltos por ceros a la izquierda y luego por bytes para un orden total. Carpetas siempre primero.
- **Consecuencias**: sin dependencias y con orden razonable para español. No es una collation completa por locale (p. ej. no maneja ligaduras ni alfabetos no latinos de forma especial); si hiciera falta, evaluar `icu_collator` con un ADR nuevo.
