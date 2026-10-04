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
- [ ] **6.1 Contenedor de preview + detalles** por panel (Espacio alterna), debounce y cancelación.
- [ ] **6.2 Imágenes** con escalado en segundo plano y zoom.
- [ ] **6.3 Texto/código** con sourceview5, límite de tamaño.
- [ ] **6.4 Carpetas**: conteo y tamaño en segundo plano.
- [ ] **6.5 Posición configurable** (abajo/derecha).

**→ Release v0.1 (MVP).** Checklist: usar 3 días seguidos como gestor principal y registrar fallos en `docs/BUGS.md`.

## Hito 7 — v0.2
- [ ] PDF (poppler), video/audio (`gtk::Video`), Markdown renderizado.
- [ ] Vista de iconos (cuadrícula con `gtk::GridView`) como tercera opción del selector de vista.
- [ ] Miniaturas freedesktop.
- [ ] Búsqueda recursiva en segundo plano.
- [ ] Renombrado masivo con regex y vista previa.
- [ ] Notas por carpeta.
- [ ] PKGBUILD, archivo `.desktop` e icono; publicar en AUR.

## Pendientes y mejoras anotadas (sin hito asignado)
Surgieron al probar; revisar al cerrar el MVP o asignarlas a un hito.
- [ ] Archivos cortados (Ctrl+X) atenuados en la vista hasta pegarlos.
- [ ] Diálogo de conflictos con tamaño y fecha de cada versión.
- [ ] Integración OSC 7 automática para zsh (`ZDOTDIR`); hoy solo bash (fish la trae). Ver ADR-008.
- [ ] Guardar en la sesión la altura y visibilidad de la terminal de cada panel.
- [ ] Probar el escapado de rutas con fish real (solo se probó con bash y sh).
