# Tlacuache Browser — guía para Claude Code

Gestor de archivos para Linux (prioridad Arch Linux) inspirado en OneCommander: barra lateral de navegación, área central con uno o dos paneles, columnas Miller, vista previa por panel y **terminal integrada por panel**.


## Documentos del proyecto (léelos antes de cambiar algo relacionado)

| Archivo | Cuándo leerlo |
|---|---|
| `docs/PRD.md` | Qué construimos, alcance del MVP, no-objetivos |
| `docs/UI_SPEC.md` | Layout, comportamiento de paneles, atajos de teclado, tema |
| `docs/ARCHITECTURE.md` | Estructura de crates/módulos, estado, hilos, integración VTE, previews |
| `docs/ROADMAP.md` | Hitos y tareas con criterios de aceptación. **Trabaja en orden** |
| `docs/DECISIONS.md` | Decisiones de arquitectura (ADR). Añade una entrada si tomas una decisión nueva |
| `docs/SETUP_ARCH.md` | Dependencias del sistema y cómo compilar en Arch |
| `docs/BRAND.md` | Logo, paleta de marca, íconos, voz |
| `docs/THEMES.md` | Esquema de temas, tokens CSS, validación de contraste. Léelo antes de tocar cualquier estilo |

## Stack (no cambiar sin un ADR nuevo)

- Rust (edición 2021 o superior, toolchain estable)
- `gtk4` (gtk4-rs) + `libadwaita` (libadwaita-rs)
- `vte4` para la terminal embebida
- `sourceview5` para vista previa de texto/código
- `poppler-rs` para vista previa de PDF (hito posterior)
- Reproducción de video/audio con `gtk::Video` (backend GStreamer de GTK4)
- Configuración en TOML (`serde` + `toml`), en `~/.config/tlacuache/`
- Sin Electron, sin webview, sin Tauri. Solo Linux: **no** añadir código condicional para Windows/macOS

Antes de fijar versiones en `Cargo.toml`, consulta la versión estable vigente de cada crate y verifica que las versiones de `gtk4`, `libadwaita`, `vte4` y `sourceview5` sean compatibles entre sí (comparten `glib`/`gio`). No inventes versiones.

## Comandos

```bash
cargo build                     # compilar
cargo run -p tlacuache         # ejecutar la app
cargo test --workspace          # pruebas (la lógica vive en tlacuache-core)
cargo fmt --all                 # formato
cargo clippy --workspace --all-targets -- -D warnings
scripts/check.sh                # fmt --check + clippy + tests (--fix aplica fmt)
python scripts/check-theme-contrast.py crates/tlacuache/resources/themes   # contraste de los temas
scripts/install-local.sh        # instala binario, .desktop e ícono en ~/.local
```

Definición de "terminado" para cualquier tarea: compila, `cargo fmt` limpio, `clippy` sin warnings, pruebas en verde y criterio de aceptación de la tarea en `ROADMAP.md` cumplido. Marca la casilla `[x]` en el roadmap al terminar.

## Reglas de código

1. **Nunca bloquear el hilo principal de GTK.** Listado de directorios, cálculo de tamaños, copias, hashing y generación de miniaturas van en async (`gio` async / `glib::spawn_future_local`) o en hilos de trabajo que devuelven resultados vía canal (`async-channel`) al hilo principal.
2. Separación estricta: `tlacuache-core` no depende de GTK. Toda lógica testeable (ordenamiento, filtros, cola de operaciones, historial de navegación, configuración, keymap) va ahí con pruebas unitarias.
3. Sin `unwrap()`/`expect()` en rutas de UI o de E/S; propaga errores con `anyhow` en la app y `thiserror` en core, y muéstralos con `adw::Toast` o diálogos.
4. Rutas: usa `std::path::PathBuf` / `gio::File`. Nunca construyas comandos de shell concatenando rutas sin escaparlas (ver sección VTE en `ARCHITECTURE.md`).
5. Operaciones destructivas: borrar manda a la papelera (`gio::File::trash_async`) por defecto. El borrado permanente requiere confirmación explícita.
6. Widgets propios como subclases GObject (`glib::subclass`) en `crates/tlacuache/src/ui/`, un archivo por widget.
7. Nombres de código en inglés; textos de UI en español a través de una capa de strings (preparado para i18n con `gettext` más adelante).
8. Commits pequeños, uno por tarea del roadmap, mensaje en imperativo: `feat(pane): add miller columns view`.
9. Ningún color, radio ni fuente escrito en Rust o CSS: todo sale del tema activo (`docs/THEMES.md`). Los íconos nuevos siguen `docs/BRAND.md` §5.

## Flujo de trabajo esperado

1. Lee la siguiente tarea pendiente en `docs/ROADMAP.md`.
2. Si la tarea es ambigua o afecta la arquitectura, propón el enfoque en 3–5 líneas y espera confirmación.
3. Implementa, prueba y verifica (comandos de arriba). Para cambios de UI, describe cómo probarlo manualmente.
4. Marca la tarea y resume en 2–3 líneas qué cambió.

## Contexto del usuario

- Desarrollador full stack y docente; usa Arch Linux (Wayland). Prefiere respuestas directas y que se le corrija si se equivoca.
- Si una API de gtk4-rs/vte4 no está clara, consulta docs.rs de la versión fijada en vez de suponer.
