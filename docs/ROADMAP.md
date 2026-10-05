# Roadmap — Tlacuache Browser

Trabajar en orden. Cada tarea = un commit. Marcar `[x]` al cumplir el criterio de aceptación (CA).

## Hito 0 — Esqueleto
- [x] **0.1 Workspace**: crear workspace con `tlacuache-core` y `tlacuache`. CA: `cargo build` y `cargo test` pasan.
- [x] **0.2 Ventana vacía**: `adw::Application` con app-id `io.github.rmonroy.Tlacuache`, `adw::ApplicationWindow` con HeaderBar. CA: `cargo run` abre ventana.
- [x] **0.3 CSS y tema Nord**: cargar `style.css` desde recursos. CA: fondo y acentos Nord visibles.
- [x] **0.4 Config**: `core::config` con defaults y lectura de `config.toml`; crear archivo si no existe. CA: pruebas de parseo y defaults.
- [x] **0.5 CI local**: script `scripts/check.sh` que corre fmt, clippy y tests. CA: script en verde.

## Hito 1 — Un panel que navega
- [x] **1.1 FileEntry + sort + filter** en core con pruebas (orden natural, carpetas primero, ocultos).
- [x] **1.2 Lista detallada**: `ColumnView` con `DirectoryList` → filtro → orden. CA: abre `~` y una carpeta con 10 000 archivos sin congelar la UI.
- [x] **1.3 Navegación**: Enter/doble clic entra, Backspace sube, Alt+←/→ con `core::history`. CA: pruebas de history + prueba manual.
- [x] **1.4 Barra de ruta**: breadcrumb clicable y modo editable (Ctrl+L) con autocompletado. CA: navegar escribiendo una ruta.
- [x] **1.5 Columna edad**: `core::age` con buckets y chip de color. CA: pruebas de buckets.
- [x] **1.6 Abrir archivos**: con la app predeterminada (`gio::AppInfo::launch_default_for_uri_async`). CA: abre PDF/imagen en su app.
- [x] **1.7 Filtro rápido y ocultos** (escribir para filtrar, Esc limpia, Ctrl+H).

## Hito 2 — Columnas Miller y pestañas
- [x] **2.1 Vista Miller** con scroll automático a la columna activa. CA: navegar 6 niveles solo con flechas.
- [x] **2.2 Alternar vista** Miller/lista por pestaña. Diseñar el selector para admitir más vistas (iconos en v0.2); cada pestaña, y por tanto cada panel del modo dual, tiene su propia vista.
- [x] **2.3 Pestañas** con `adw::TabView` (Ctrl+T, Ctrl+W, Ctrl+Tab).
- [x] **2.4 Barra de estado** del panel (elementos, selección, espacio libre).

## Hito 3 — Doble panel y sidebar
- [x] **3.1 Dos paneles** en `gtk::Paned`, F3 alterna, Tab cambia el panel activo con acento visual.
- [x] **3.2 Sidebar: lugares XDG** + Inicio + Raíz.
- [x] **3.3 Sidebar: unidades** con `gio::VolumeMonitor`, uso de disco y montar/desmontar.
- [x] **3.4 Sidebar: favoritos** agrupados, persistidos en config, arrastrar para añadir.
- [x] **3.5 Sesión**: restaurar pestañas y tamaños al reabrir.

## Hito 4 — Operaciones de archivo
- [x] **4.1 Cola de ops en core** (estados, progreso, cancelación) con pruebas.
- [x] **4.2 Runner gio**: copiar/mover archivos y directorios (recursivo) con progreso. CA: copiar 2 GB sin congelar la UI y cancelar a mitad.
- [x] **4.3 F5/F6** hacia el otro panel; arrastrar y soltar entre paneles. Franja central entre paneles (solo modo dual) con botones Copiar → / Mover → cuyas flechas apuntan del panel activo al otro, deshabilitados sin selección. Reemplaza los atajos temporales Ctrl+Shift+F5/F6.
- [x] **4.4 Papelera** (Supr) y borrado permanente (Shift+Supr, confirmación).
- [x] **4.5 Renombrar (F2), nueva carpeta (F7), nuevo archivo.**
- [x] **4.6 Conflictos**: diálogo Reemplazar/Omitir/Renombrar/Aplicar a todos.
- [x] **4.7 Panel de operaciones**: el indicador de la barra superior despliega un popover con el avance individual de cada operación (cancelar, resultado de las terminadas) y toasts con Deshacer.
- [x] **4.8 Portapapeles**: Ctrl+C / Ctrl+X / Ctrl+V entre paneles y con otras apps (`text/uri-list` y `x-special/gnome-copied-files` para cortar).
- [x] **4.9 Menú contextual** de archivos y carpetas: Abrir, Abrir con…, Cortar, Copiar, Pegar, Renombrar, Mover a la papelera, Añadir a favoritos, Copiar ruta, Propiedades.

## Hito 5 — Terminal integrada
- [x] **5.1 `core::shell::quote`** con pruebas (espacios, `'`, `\n`, prefijo `-`, unicode).
- [x] **5.2 Terminal por panel** (VTE) creada perezosamente, F4 alterna, paleta y fuente desde config.
- [x] **5.3 Panel → terminal**: `cd` al navegar, solo si el shell está en primer plano; indicador de desincronizado.
- [x] **5.4 Terminal → panel** vía OSC 7 sin bucles. CA: `cd ~/dev` en la terminal mueve el panel.
- [x] **5.5 Arrastrar archivo a la terminal** pega la ruta escapada.
- [x] **5.6 Gestión de foco**: atajos globales no interfieren con la terminal.

## Hito 6 — Vista previa
- [x] **6.1 Contenedor de preview + detalles** por panel (Espacio alterna), debounce y cancelación.
- [x] **6.2 Imágenes** con escalado en segundo plano y zoom.
- [x] **6.3 Texto/código** con sourceview5, límite de tamaño.
- [x] **6.4 Carpetas**: conteo y tamaño en segundo plano.
- [x] **6.5 Posición configurable** (abajo/derecha).

## Hito 7 — Identidad visual y temas
Orden acordado: **7.1–7.5 antes del Hito 6** (la vista previa nace usando los tokens del tema); 7.6–7.10 después del Hito 6. Referencia visual: `branding/brand-board.html`.

- [x] **7.1 Íconos e identidad**: instalar el ícono de la app (hicolor scalable + symbolic, en `data/icons/`) con un `.desktop` y un script de instalación local; registrar los íconos `tl-*` en el gresource y usarlos en los botones de la HeaderBar y la barra de cada panel (reemplazan a `view-columns-symbolic`). CA: el ícono aparece en el lanzador (Walker/rofi/GNOME) tras instalar el `.desktop`; los íconos se ven a 16 px.
- [x] **7.2 `core::filetype`**: tipo MIME → categoría (`folder`, `code`, `image`…) con pruebas para al menos 30 tipos comunes.
- [x] **7.3 `core::theme`**: parseo con `deny_unknown_fields`, validación de contraste portada de `scripts/check-theme-contrast.py` y `css_variables()` con prueba de snapshot. Edad con los buckets de THEMES §3 y formato compacto (`ahora · 12min · 2h · 3d · 3M · 1a`). CA: prueba que valida los 5 temas incluidos.
- [x] **7.4 ThemeManager**: `base.css` (adaptado a los widgets reales) + provider de variables + `StyleManager` + clases de flags + sobrescritura de las variables de libadwaita (verificando sus nombres). Config `[theme]` de THEMES §5.4. CA: el tema `nord` se ve igual que en `branding/brand-board.html`.
- [x] **7.5 Terminal temática**: paleta, cursor, selección y fuente de VTE desde el tema (tamaño desde la config); se actualiza con `theme-changed`.
- [x] **7.6 Flags en Rust**: `bracket_titles` (en el título de la pestaña), `icon_style = "glyph"` (glifo + nombre coloreado + `/` en carpetas), `age_style = "text"`, `uppercase_headers`. CA: el tema `consola` se parece a su vista en el board.
- [x] **7.7 Selector de temas**: Preferencias (`adw::PreferencesDialog`) con lista de temas, cambio en vivo y `follow_system` con temas light/dark. Persistencia en config.
- [x] **7.8 Temas de usuario**: cargar `~/.config/tlacuache/themes/*.toml`; toast con los pares de contraste fallidos; respaldo a `nord` si hay error de parseo.
- [x] **7.9 Decor Talavera**: franja `talavera-strip.svg` en el pie de la sidebar cuando el tema define `[decor]`.
- [x] **7.10 README**: lockup, captura de cada tema y créditos (paleta Nord MIT, JetBrains Mono OFL).

**→ Release v0.1 (MVP).** Checklist: usar 3 días seguidos como gestor principal y registrar fallos en `docs/BUGS.md`.

## Pendientes del MVP (antes del Hito 8)
Surgieron al probar. Acordado: atenderlos antes del Hito 8, en este orden.

- [x] **P.1 Cortados atenuados**: los archivos cortados con Ctrl+X se ven atenuados en todas las vistas hasta pegarlos o cancelar (copiar otra cosa o Esc). CA: cortar, ver atenuados en ambos paneles; pegar o copiar otra cosa los restaura.
- [x] **P.2 Conflictos con detalle**: el diálogo de conflictos muestra tamaño y fecha de modificación de la versión existente y de la nueva, y cuál es más reciente. CA: copiar un archivo sobre otro distinto muestra ambos datos.
- [x] **P.3 Terminal en la sesión**: guardar por panel si la terminal estaba visible y su altura. CA: abrir la terminal, ajustar la altura, reiniciar: vuelve igual (con un shell nuevo en la carpeta del panel).
- [x] **P.4 OSC 7 para zsh**: integración automática con `ZDOTDIR` propio que carga la config del usuario (ver ADR-008). CA: con zsh, el panel sigue los `cd`. Requiere instalar `zsh` para probar.
- [x] **P.5 Escapado con fish**: probar `cd` y pegar rutas con espacios, comillas y `$` en fish real. CA: prueba automática con fish (se omite si no está instalado). Requiere instalar `fish`.
- [x] **P.6 Barra de pestañas propia** con el título a la izquierda (como en el board): `adw::TabBar` centra el título en el código de `AdwTab`, sin propiedad ni CSS para cambiarlo. Implica reimplementar reordenar arrastrando, desbordamiento con scroll y menú de pestaña. CA: mismas funciones que hoy, título alineado a la izquierda.

## Hito 8 — v0.2
Orden acordado: completar la vista previa, luego vistas, búsqueda, renombrado, notas y AUR.

- [x] **8.1 PDF** en la vista previa con `poppler-rs` (0.26, compatible con glib 0.22): página renderizada en un hilo de trabajo y botones ‹ › para cambiar de página. CA: un PDF de 100+ páginas muestra la página 1 sin trabar la app y el número de páginas.
- [x] **8.2 Video y audio** con `gtk::Video` (GStreamer). CA: mp4, webm y mp3 se reproducen en la vista previa y se detienen al cambiar de selección. Requiere `gst-plugins-good` y `gst-libav`.
- [x] **8.3 Markdown renderizado** (`pulldown-cmark` → `TextView` con estilos del tema, sin webview), alternable con la vista de código. CA: encabezados, listas, énfasis, código y enlaces con los colores del tema.
- [x] **8.4 Vista de íconos** (`gtk::GridView`) como tercera opción del selector (Ctrl+3). CA: navegación, selección, arrastrar y soltar y menú contextual como en las otras vistas.
- [x] **8.5 Miniaturas freedesktop** (caché compartida `~/.cache/thumbnails`), generadas en segundo plano. CA: las imágenes muestran miniatura en la vista de íconos y no se regeneran si ya existen.
- [x] **8.6 Búsqueda recursiva** (Ctrl+F) en segundo plano y cancelable, con resultados en una pestaña. CA: buscar en `~` no traba la app; Esc cancela.
- [x] **8.7 Renombrado masivo** con regex y vista previa del resultado. CA: detecta conflictos antes de aplicar y se deshace con Ctrl+Z.
- [x] **8.8 Notas por carpeta**: almacén central (ADR-014), editor en la vista previa de la carpeta con guardado automático e indicador en la barra de estado; las notas siguen a las carpetas movidas o renombradas desde Tlacuache. CA: escribir una nota, cambiar de carpeta, reiniciar y verla igual; renombrar la carpeta y que la nota la siga.
- [ ] **8.9 PKGBUILD para AUR** (el `.desktop` y el ícono existen desde la 7.1). CA: `makepkg -si` instala y la app aparece en el lanzador; la publicación en AUR la hace el autor.
