# Identidad visual — Tlacuache Browser

Referencia visual completa: `branding/brand-board.html`, que se abre en el navegador.

## 1. Concepto

El tlacuache (zarigüeya mexicana) es nocturno, curioso, lo carga todo en la espalda y no se pierde en la oscuridad. Así queremos que se sienta la app: un gestor que **carga tus archivos, se mueve rápido entre carpetas y vive cómodo junto a la terminal**.

Personalidad: técnica, ágil, con humor mexicano discreto. Nunca infantil.

## 2. Logo

| Archivo | Uso |
|---|---|
| `branding/logo-app.svg` | **Logo principal** (concepto C, "Doble panel"): cara geométrica sobre dos paneles. Es el ícono de la app. |
| `branding/logo-mono-prompt.svg` | Marca alterna (concepto B, "Prompt"): perfil con hocico en forma de `>` y cursor. Para el tema Consola, splash de la terminal y material técnico. |
| `branding/lockup-horizontal-dark.svg` / `-light.svg` | Logo + wordmark para README, AUR y sitio. |
| `branding/lockup-consola.svg` | Lockup monocromo estilo terminal. |
| `branding/concepts/a-asomado.svg` | Concepto descartado (ilustrativo); se conserva como posible mascota o ilustración de estados vacíos. |

Instalación de íconos de la app:

- `data/icons/hicolor/scalable/apps/io.github.RicardoMonroy.Tlacuache.svg` → `/usr/share/icons/hicolor/scalable/apps/`
- `data/icons/hicolor/symbolic/apps/io.github.RicardoMonroy.Tlacuache-symbolic.svg` → `/usr/share/icons/hicolor/symbolic/apps/`

Reglas de uso:

- Espacio libre alrededor del logo: como mínimo el radio de una oreja (≈ 11 % del lado).
- Por debajo de 24 px, usar la versión simbólica.
- No deformar, rotar, añadir sombras ni cambiar los colores de las mitades del fondo.
- El wordmark es texto convertido a trazos (JetBrains Mono ExtraBold). No recrearlo con una fuente instalada.

## 3. Paleta de marca

La paleta de marca es fija y no depende del tema activo. Se usa en el logo, el README y el sitio.

| Nombre | Hex | Rol |
|---|---|---|
| Noche ártica | `#2E3440` | Fondo principal, orejas, texto sobre claro |
| Pizarra | `#3B4252` | Superficies secundarias |
| Escarcha | `#88C0D0` | Acento primario, mitad izquierda del logo |
| Azul fiordo | `#5E81AC` | Acento secundario |
| Nieve | `#ECEFF4` | Cara del tlacuache, texto sobre oscuro |
| Rosa hocico | `#E5A3B3` | Nariz. Usarlo solo como detalle, nunca como fondo grande |
| Musgo | `#A3BE8C` | Cursor de la marca Prompt, éxito |

La paleta deriva de Nord (licencia MIT); hay que dar crédito en el README.

## 4. Tipografía

- **UI:** la fuente del sistema. No se empaquetan fuentes de UI.
- **Monoespaciada:** JetBrains Mono (licencia OFL), paquete `ttf-jetbrains-mono` en Arch, con `monospace` como respaldo. Se usa en la terminal, rutas, tamaños y todo el tema Consola.
- **Wordmark:** JetBrains Mono ExtraBold, en minúsculas, convertido a trazos.

## 5. Íconos

Todos los íconos de la UI son **simbólicos**: SVG de 16×16, un solo color (`#2e3436`), con huecos marcados con `fill-rule="evenodd"` y sin trazos (`stroke`). GTK los recolorea con el `color` de CSS, y así cada tema los tiñe sin duplicar archivos.

| Carpeta | Contenido |
|---|---|
| `crates/tlacuache/resources/icons/scalable/actions/` | `tl-pane-single`, `tl-pane-dual`, `tl-view-miller`, `tl-terminal-toggle`, `tl-preview-toggle`, `tl-terminal-sync`, `tl-terminal-desync` |
| `crates/tlacuache/resources/icons/scalable/mimetypes/` | `tl-folder` y `tl-file-{other,code,image,video,audio,archive,document,text,pdf,spreadsheet,executable}` |

- Para acciones genéricas (atrás, buscar, menú, papelera, sidebar) usar los íconos simbólicos de Adwaita. Solo se dibujan los íconos propios de Tlacuache.
- Un ícono nuevo se dibuja en una cuadrícula de 16 px, con contornos de 1.5 px, esquinas de 1–2.5 px y solo `fill`. Se valida a 16 px y a 32 px.
- La clasificación por categoría (tipo MIME → `tl-file-*`) vive en `tlacuache-core::filetype` y tiene pruebas.

## 6. Voz y microcopia

- Español neutro de México, directo y en segunda persona: "Copiar 3 elementos a Descargas".
- Humor solo en estados vacíos y en easter eggs, nunca en errores ni en confirmaciones destructivas.
  - Carpeta vacía: "Aquí no hay nada… ni para el tlacuache."
  - Búsqueda sin resultados: "Ni escarbando lo encontramos."
- Los errores dicen qué pasó y qué hacer: "No tienes permiso para escribir en /etc. Ábrelo como administrador o elige otra carpeta."
