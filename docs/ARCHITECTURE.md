# Arquitectura — Tlacuache Browser

## 1. Estructura del workspace

```
tlacuache/
├── Cargo.toml                  # [workspace]
├── crates/
│   ├── tlacuache-core/        # lógica pura, sin GTK, 100% testeable
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs       # Config TOML (serde), defaults, merge
│   │       ├── keymap.rs       # acciones <-> atajos
│   │       ├── history.rs      # historial atrás/adelante por pestaña
│   │       ├── entry.rs        # FileEntry (nombre, tamaño, fechas, tipo, oculto)
│   │       ├── sort.rs         # ordenamiento natural, carpetas primero
│   │       ├── filter.rs       # filtro rápido (substring/fuzzy)
│   │       ├── age.rs          # edad relativa -> bucket de color
│   │       ├── ops/            # cola de operaciones (modelo, estados, progreso)
│   │       └── shell.rs        # escapado seguro de rutas para shells
│   └── tlacuache/             # binario GTK
│       ├── build.rs            # compila recursos (gresource) si hace falta
│       ├── resources/          # style.css, iconos propios
│       └── src/
│           ├── main.rs         # adw::Application, app-id
│           ├── app.rs          # acciones globales, carga de config y CSS
│           ├── window.rs       # ApplicationWindow: sidebar + área de paneles
│           ├── ui/
│           │   ├── sidebar.rs
│           │   ├── pane.rs         # Pane: pestañas + preview + terminal (Paned)
│           │   ├── tab_page.rs     # contenido de una pestaña: ‹ › ↑ + ruta + vista; dueña del historial y atajos de navegación
│           │   ├── path_bar.rs     # breadcrumb/Entry con autocompletado propio (EntryCompletion está obsoleto)
│           │   ├── miller_view.rs
│           │   ├── list_view.rs    # ColumnView detallado
│           │   ├── preview.rs      # despachador de previews por tipo MIME
│           │   ├── details.rs
│           │   ├── terminal.rs     # wrapper de vte4::Terminal
│           │   ├── ops_panel.rs    # progreso de la cola
│           │   └── dialogs.rs
│           ├── fs/
│           │   ├── listing.rs      # gtk::DirectoryList + mapeo a FileEntry
│           │   ├── ops_runner.rs   # ejecuta la cola con gio async
│           │   └── volumes.rs      # gio::VolumeMonitor
│           └── state.rs            # estado compartido de la ventana
└── docs/
```

## 2. Modelo de estado

- `WindowState` (en `state.rs`): panel activo, modo 1/2 paneles, visibilidad de sidebar. Vive en el `Window` como `RefCell` dentro del imp del GObject.
- Cada `Pane` posee sus pestañas; cada `TabPage` posee: ruta actual (`gio::File`), `History` (core), modo de vista, filtro, selección.
- Comunicación entre widgets por **señales GObject** y **acciones `gio::SimpleAction`** con alcance (`win.*`, `pane.*`). Evitar referencias cruzadas fuertes: usar `glib::WeakRef` o `clone!(#[weak] …)`.
- La config se carga al inicio en `Rc<Config>`; cambios en vivo vía recarga y señal `config-changed`.

## 3. Listado de directorios

- Usar `gtk::DirectoryList` (asíncrono, incremental, con `monitored = true` para refrescar ante cambios del sistema de archivos).
- Atributos a solicitar: `standard::*,time::modified,time::created,unix::mode,owner::user`.
- Encadenar modelos: `DirectoryList` → `gtk::MapListModel` (envuelve cada `gio::FileInfo` en un `FileItem` que cachea su `FileEntry`) → `gtk::FilterListModel` (`gtk::CustomFilter` que delega en `tlacuache-core::filter`) → `gtk::SortListModel` (`gtk::CustomSorter` que delega en `tlacuache-core::sort`) → `gtk::MultiSelection`.
- Filtro y orden **no incrementales**: en modo incremental la vista se queda anclada a una fila intermedia mientras llegan lotes. Medido: 10 000 entradas sin pausas perceptibles del hilo principal.
- Los encabezados del `ColumnView` tienen sorters simbólicos; al cambiar el `ColumnViewSorter` se traduce columna/dirección a `SortSpec`, de modo que las carpetas quedan primero también en orden descendente.
- Vistas por pestaña: `TabPage` guarda la activa en el enum `PageView` (una variante por vista, misma interfaz: `directory`, `show_directory`, `focus`, `set_show_hidden`, señal `directory-activated`). Cambiar de vista recrea la vista en la carpeta actual; historial y ocultos viven en `TabPage`.
- Columnas Miller: un `gtk::ListView` por nivel dentro de un `gtk::Box` horizontal en `gtk::ScrolledWindow`; al seleccionar una carpeta se truncan las columnas a la derecha y se añade una nueva. Reutilizar factories (`SignalListItemFactory`).
- Listas virtualizadas: `ListView`/`ColumnView` solo crean filas visibles, requisito para 10 000+ entradas.

## 4. Operaciones de archivo

- `tlacuache-core::ops` define `Operation { kind: Copy|Move|Trash|Delete|Mkdir|Rename, sources, dest, state, progress }` y la cola (FIFO, una operación activa a la vez por defecto; concurrencia configurable).
- `fs/ops_runner.rs` ejecuta con `gio::File::copy_future` / `move_future` / `trash_future`, reporta progreso por callback y admite `gio::Cancellable`.
- Copia de directorios: recorrido recursivo asíncrono (gio no copia directorios de forma recursiva).
- Conflictos (archivo existente): diálogo con Reemplazar / Omitir / Renombrar / Aplicar a todos.
- Al terminar: `adw::Toast` con opción "Deshacer" para mover/renombrar/papelera cuando sea posible.

## 5. Terminal embebida (VTE)

- `ui/terminal.rs` envuelve `vte4::Terminal`. Se crea al primer uso.
- Arranque: `terminal.spawn_async(PtyFlags::DEFAULT, Some(cwd), &[shell], &env, SpawnFlags::DEFAULT, …)`. Verificar en docs.rs la firma exacta de la versión fijada.
- **Panel → terminal**: para cambiar de carpeta se envía `cd -- <ruta escapada>\n` con `feed_child`. El escapado lo hace `tlacuache-core::shell::quote` (comillas simples POSIX: `'` → `'\''`), con pruebas para espacios, comillas, saltos de línea y rutas que empiezan con `-`.
  - Antes de enviar, comprobar que el shell está en primer plano: comparar el PGID de primer plano del PTY (`tcgetpgrp` sobre el fd del `vte::Pty`) con el PID del shell. Si difieren, no enviar y marcar "desincronizado".
- **Terminal → panel**: escuchar `notify::current-directory-uri`. Requiere integración del shell que emita OSC 7:
  - bash/zsh en Arch: el paquete de VTE incluye `/etc/profile.d/vte.sh`; documentar en el README que el usuario debe hacer `source /etc/profile.d/vte.sh` en su `.bashrc`/`.zshrc`.
  - Ofrecer alternativamente inyectar la integración vía variable de entorno o archivo rc temporal (decidir en ADR cuando se implemente).
- Evitar bucles: si el panel navegó por OSC 7, no reenviar `cd`.
- Paleta y fuente desde la config (`set_colors`, `set_font`).

## 6. Vista previa

- `ui/preview.rs` recibe el `FileEntry` seleccionado (con debounce de ~120 ms) y elige un renderer por tipo MIME (`gio::content_type_guess` o el atributo `standard::content-type`):
  - `image/*` → `gtk::Picture` cargando `gdk::Texture` en segundo plano (escalar imágenes grandes).
  - texto/código → `sourceview5::Buffer` + `LanguageManager::guess_language`; lectura limitada a 1 MB en hilo de trabajo.
  - `video/*`, `audio/*` → `gtk::Video` (v0.2).
  - `application/pdf` → `poppler` renderiza la página 1 a textura (v0.2).
  - carpetas → conteo/tamaño en segundo plano.
  - resto → icono grande + detalles.
- Cada solicitud de preview lleva un `Cancellable`; al cambiar la selección se cancela la anterior.

## 7. Configuración

`~/.config/tlacuache/config.toml` (ruta vía `glib::user_config_dir()`):

```toml
[general]
show_hidden = false
dual_pane = true
confirm_trash = false
default_view = "columns"   # columns (Miller) | details (lista)

[preview]
position = "bottom"   # bottom | right
max_text_bytes = 1048576

[terminal]
shell = ""            # vacío = $SHELL
font = "JetBrains Mono 11"
sync_panel_to_terminal = true
sync_terminal_to_panel = true

[theme]
name = "nord"
color_scheme = "dark"   # dark | light | system

[[favorites]]
group = "Proyectos"
paths = ["~/dev", "~/Documentos/clases"]

[keys]
toggle_terminal = "F4"
```

Todas las claves son opcionales (`#[serde(default)]`): un archivo parcial se combina con los defaults y las claves desconocidas se ignoran. Si no existe, se escribe la plantilla comentada `tlacuache-core/src/config_default.toml`. Si el TOML es inválido no se sobrescribe: se usan defaults y se avisa con un toast.

Estado de sesión (pestañas abiertas, tamaños de paneles) en `~/.local/state/tlacuache/session.toml`.

## 8. Errores y logging

- `tracing` + `tracing-subscriber`; nivel por `RUST_LOG`.
- Errores visibles para el usuario: `adw::Toast` (no bloqueantes) o `adw::AlertDialog` (decisiones).

## 9. Pruebas

- `tlacuache-core`: pruebas unitarias para sort, filter, age, history, shell::quote, config (defaults y parseo), cola de ops (transiciones de estado).
- Pruebas de integración de `fs/ops_runner` con `tempfile` (requiere contexto glib; usar `glib::MainContext` en el test).
- UI: checklist de prueba manual por hito en `ROADMAP.md`.
