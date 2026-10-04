# UI Spec — Tlacuache Browser

Referencia visual: OneCommander V3 (tema Nord). No copiar assets ni iconos de OneCommander; solo la distribución y el comportamiento.

## 1. Layout general

```
┌──────────────┬───────────────────────────────┬───────────────────────────────┐
│ HeaderBar: menú · 1/2 paneles · buscar · ajustes                            │
├──────────────┼───────────────────────────────┼───────────────────────────────┤
│ SIDEBAR      │ PANEL IZQUIERDO               │ PANEL DERECHO (opcional)      │
│              │ [pestaña][pestaña][+]         │ [pestaña][pestaña][+]         │
│ Unidades     │ ‹ › ↑  /home/user/dev  (ruta) │ ‹ › ↑  /tmp                   │
│  ▸ /  79%    │ ┌────────┬────────┬────────┐  │ ┌──────────────────────────┐  │
│  ▸ /home     │ │ col 1  │ col 2  │ col 3  │  │ │ vista lista detallada    │  │
│ Lugares      │ │ Miller │ Miller │ activa │  │ │ nombre ext tam fecha edad│  │
│  Inicio      │ └────────┴────────┴────────┘  │ └──────────────────────────┘  │
│  Descargas   │ ╞═ Vista previa ═╪═ Detalles ═╡│ ╞═ Vista previa ═╪═ Detalles ═╡│
│ Favoritos    │ ╞════════ Terminal ══════════╡│ ╞════════ Terminal ══════════╡│
│  ▸ Grupo 1   │ $ _                           │ $ _                           │
└──────────────┴───────────────────────────────┴───────────────────────────────┘
```

- La sidebar es colapsable (F9). Ancho redimensionable.
- El área central es un `gtk::Paned` horizontal con 1 o 2 paneles (alternar con F3).
- Cada panel es un `gtk::Paned` vertical anidado: **contenido** / **vista previa+detalles** / **terminal**. Ambas zonas inferiores se muestran u ocultan de forma independiente y recuerdan su altura.
- Posición de la vista previa configurable: abajo (por defecto) o a la derecha del panel.

## 2. Panel

- **Pestañas**: `adw::TabBar` + `adw::TabView`. Ctrl+T nueva, Ctrl+W cerrar, Ctrl+Tab siguiente.
- **Barra de ruta**: breadcrumb clicable; clic en la zona vacía o Ctrl+L la convierte en `gtk::Entry` editable con autocompletado de rutas.
- **Vistas**:
  - Columnas Miller: cada nivel es una columna; seleccionar carpeta abre la siguiente columna a la derecha y hace scroll automático. La última columna puede mostrarse como lista detallada (como OneCommander).
  - Lista detallada (`gtk::ColumnView`): columnas visibles según ancho del panel.
- **Panel activo**: el que tiene el foco; en modo dual se marca con una línea de acento arriba. Tab cambia de panel activo (salvo escribiendo en un campo de texto, donde autocompleta). Las operaciones F5/F6 usan el otro panel como destino. F3 o el botón de la barra ocultan el panel derecho, que conserva sus pestañas.
- **Edad relativa**: chip de color en la columna "Edad": < 1 día verde, < 7 días cian, < 30 días azul, < 1 año gris, ≥ 1 año gris tenue.
- **Barra de estado** del panel: elementos, seleccionados, tamaño seleccionado, espacio libre de la unidad.

## 3. Vista previa y detalles

- Imagen: `gtk::Picture` con ajuste a contenedor y zoom con rueda.
- Texto/código: `sourceview5::View` solo lectura, lenguaje detectado por tipo MIME; límite de 1 MB (mostrar primeras N líneas si excede).
- Carpeta: número de elementos y tamaño (calculado en segundo plano, cancelable).
- Detalles: nombre, tipo MIME, tamaño, creado, modificado, edad, permisos, propietario; dimensiones para imágenes.
- La vista previa se actualiza con debounce (~120 ms) al mover la selección.

## 4. Terminal por panel

- Widget `vte4::Terminal` con el shell del usuario (`$SHELL`, fallback `/bin/bash`).
- Se crea de forma perezosa la primera vez que se abre (F4 o botón).
- **Panel → terminal**: al cambiar de carpeta en el panel, si la terminal está inactiva (sin proceso en primer plano distinto al shell) se envía `cd` con la ruta escapada. Si hay un proceso corriendo, no se envía nada y se muestra un indicador "carpeta desincronizada" con botón para sincronizar.
- **Terminal → panel**: si el shell emite OSC 7 (`current-directory-uri`), el panel navega a esa carpeta. Opción configurable.
- Copiar/pegar: Ctrl+Shift+C / Ctrl+Shift+V. Fuente monoespaciada configurable.
- Se oculta con F4 (recuerda su altura) y se descarta si el shell termina (`exit`); el siguiente F4 lanza uno nuevo en la carpeta actual. Dentro de la terminal, Tab es del shell (autocompletar).
- Arrastrar un archivo del panel a la terminal pega su ruta escapada.

## 5. Sidebar

- **Lugares**: Inicio + carpetas XDG del usuario (`glib::user_special_dir`; se omiten las que no existen o apuntan a `~`) + Papelera.
- **Unidades**: «Sistema» (`/`) y los volúmenes/montajes de `gio::VolumeMonitor`; barra de uso (aviso ≥ 80 %, crítico ≥ 90 %) con el espacio libre en el tooltip; botón de expulsar/desmontar y menú contextual (Abrir, Montar, Desmontar, Expulsar). Abrir un volumen sin montar lo monta primero.
- **Favoritos**: grupos nombrados, arrastrar carpetas para añadir, reordenar con arrastrar. Persistidos en config.

## 6. Atajos de teclado (por defecto, remapeables en config)

| Acción | Atajo |
|---|---|
| Abrir / entrar | Enter, doble clic |
| Subir nivel | Backspace, Alt+↑ |
| Atrás / Adelante | Alt+← / Alt+→ |
| Cambiar panel activo | Tab |
| Alternar 1/2 paneles | F3 |
| Mostrar/ocultar terminal del panel | F4 |
| Copiar a otro panel | F5 |
| Mover a otro panel | F6 |
| Nueva carpeta | F7 / Ctrl+Shift+N |
| Nuevo archivo vacío | Ctrl+Alt+N |
| Papelera | Supr |
| Borrado permanente | Shift+Supr (con confirmación) |
| Renombrar | F2 |
| Copiar / cortar / pegar archivos | Ctrl+C / Ctrl+X / Ctrl+V |
| Deshacer la última operación | Ctrl+Z |
| Vista previa | Espacio (alternar) |
| Sidebar | F9 |
| Filtro rápido | escribir directamente; Esc limpia |
| Mostrar ocultos | Ctrl+H |
| Ruta editable | Ctrl+L |
| Vista columnas / detalles | Ctrl+1 / Ctrl+2 |
| Nueva pestaña / cerrar | Ctrl+T / Ctrl+W |
| Abrir terminal externa aquí | Ctrl+Alt+T |

**Filtro rápido**: subcadena sin distinguir mayúsculas ni acentos (`cancion` encuentra `Canción`; la `ñ` cuenta como `n`), así funciona sin teclas muertas. Se muestra en un indicador flotante con el número de coincidencias y se limpia al cambiar de carpeta. Ctrl+H alterna los ocultos de la pestaña (inicial: `general.show_hidden`).

**Filtro rápido activo**: mientras el filtro tiene texto, Backspace borra un carácter del filtro y Espacio inserta un espacio; Esc limpia el filtro. Sin filtro activo, Backspace sube de nivel y Espacio alterna la vista previa.

Los atajos de la ventana no deben capturarse cuando la terminal tiene el foco (excepto F4 y Tab con modificador configurable para salir de la terminal: Ctrl+Shift+Tab).

## 7. Tema

- Oscuro por defecto con paleta Nord (`#2E3440`, `#3B4252`, `#434C5E`, `#4C566A`, `#D8DEE9`, `#E5E9F0`, `#ECEFF4`, `#8FBCBB`, `#88C0D0`, `#81A1C1`, `#5E81AC`, `#BF616A`, `#D08770`, `#EBCB8B`, `#A3BE8C`, `#B48EAD`).
- Implementar vía `resources/style.css` en un gresource: `adw::Application` lo carga solo desde su ruta base (`/io/github/rmonroy/Tlacuache/`). La paleta se expone como variables `--nord0`…`--nord15` y se mapea sobre los colores con nombre de libadwaita (`--window-bg-color`, `--accent-bg-color`, …) dentro de `@media (prefers-color-scheme: dark)` (`style-dark.css` está obsoleto desde libadwaita 1.9). Respetar `adw::StyleManager` (claro/oscuro del sistema) como opción.
- La paleta de la terminal VTE usa los mismos 16 colores.
- Densidad compacta: filas de 24–26 px.
