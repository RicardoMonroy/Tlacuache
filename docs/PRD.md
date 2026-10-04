# PRD — Tlacuache Browser

## 1. Visión

Un gestor de archivos para Linux, rápido y orientado a teclado, que combine lo mejor de OneCommander (columnas Miller, doble panel, vista previa, edad de archivos codificada por color) con algo que ningún gestor gráfico popular de Linux ofrece bien: **una terminal real integrada en cada panel, sincronizada con la carpeta actual**.

## 2. Problema

- Double Commander y Krusader son doble panel clásico, sin columnas Miller ni vista previa moderna.
- Files (elementary) tiene columnas Miller pero búsqueda débil y sin doble panel.
- Dolphin tiene terminal embebida (Konsole) pero es KDE-céntrico y sin columnas Miller.
- Alternar entre gestor de archivos y terminal rompe el flujo de un desarrollador.

## 3. Usuario objetivo

- Primario: el autor (desarrollador, Arch + Wayland/Hyprland).
- Secundario: usuarios Linux avanzados que trabajan con código, fotos y documentos.

## 4. Alcance

### MVP (v0.1) — "lo uso a diario"
- Ventana con barra lateral + área central con **1 o 2 paneles** (alternar con atajo).
- Por panel: pestañas, barra de ruta (breadcrumb editable), vista de **columnas Miller** y vista de **lista detallada**.
- Navegación completa con teclado y ratón; historial atrás/adelante por pestaña.
- Columnas de detalle: nombre, extensión, tamaño, fecha, **edad relativa con color** (horas/días/meses).
- Operaciones: copiar, mover, renombrar (F2), crear carpeta/archivo, enviar a papelera, arrastrar y soltar entre paneles.
- **Cola de operaciones** con progreso y cancelación (no bloquea la UI).
- **Vista previa por panel** (mostrar/ocultar): imágenes, texto/código con resaltado, metadatos básicos.
- **Terminal por panel** (mostrar/ocultar) con sincronización de carpeta en ambos sentidos.
- Barra lateral: unidades (montaje/desmontaje), lugares XDG (Inicio, Descargas, Documentos…), favoritos agrupados.
- Filtro rápido de la carpeta actual (escribir para filtrar).
- Archivos ocultos (mostrar/ocultar con Ctrl+H).
- Configuración en TOML y tema oscuro por defecto (paleta Nord).

### v0.2
- Vista previa de PDF, video/audio y Markdown renderizado.
- Renombrado masivo con regex y vista previa del resultado.
- Búsqueda recursiva (nombre y contenido) en segundo plano.
- Miniaturas (especificación freedesktop, caché compartida en `~/.cache/thumbnails`).
- Notas/pendientes por carpeta (archivo oculto `.tlacuache-notes.md`).
- Iconos de carpeta personalizables.

### v0.3+
- Etiquetas/colores de archivos (atributos extendidos `user.xdg.tags`).
- Comparar/sincronizar carpetas entre paneles.
- Archivos comprimidos navegables (zip/tar) como carpetas.
- Integración con git (estado en columna).
- Paquete en AUR.

## 5. No-objetivos

- Soporte para Windows o macOS.
- Acceso remoto (SFTP/SMB) en MVP; GVfs puede llegar después de forma gratuita vía `gio`.
- Ser un reemplazo del escritorio (no maneja el escritorio ni wallpapers).

## 6. Requisitos no funcionales

- Arranque en frío < 500 ms en hardware moderno.
- Abrir una carpeta de 10 000 archivos sin congelar la UI (listado incremental + listas virtualizadas).
- Funcionar en Wayland (Hyprland, Sway, GNOME, KDE) y X11.
- Memoria en reposo < 150 MB con dos paneles y una terminal abierta (objetivo, medir).

## 7. Métrica de éxito

El autor lo usa como gestor de archivos principal durante 2 semanas sin volver a otro.
