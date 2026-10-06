# Entorno de desarrollo (Arch Linux y otras distribuciones)

## Dependencias del sistema

```bash
sudo pacman -S --needed rustup base-devel pkgconf \
  gtk4 libadwaita vte4 gtksourceview5 poppler-glib \
  gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav
rustup default stable
```

- `vte4` es la variante de VTE para GTK4 (la de GTK3 es `vte3`).
- `poppler-glib` se usa desde la 8.1 (vista previa de PDF); los plugins de GStreamer, desde la 8.2 (video y audio).
- `ttf-jetbrains-mono`: la fuente monoespaciada de los temas (si falta, se usa `monospace`).
- Si algún nombre de paquete cambió, buscar con `pacman -Ss <nombre>` antes de suponer.

## Requisitos mínimos

| Biblioteca | Versión mínima | Por qué |
|---|---|---|
| GTK | 4.18 | Features de compilación (`v4_18`); el código necesita al menos 4.16 (variables CSS de los temas) |
| libadwaita | 1.7 | Features `v1_7`; el código necesita al menos 1.6 |
| VTE (GTK4) | 0.80 | Features `v0_80` |
| GtkSourceView | 5.16 | Features `v5_16` |
| GLib | 2.72 | `user_state_dir`, `move_async` |
| poppler-glib | cualquiera reciente | Vista previa de PDF |
| Rust | 1.92 | Lo exige gtk-rs 0.22 (`rust-version` del workspace) |

Son las de Debian 13; Ubuntu 26.04 LTS, Fedora 43/44 y Arch las superan. Ubuntu 24.04 LTS (GTK 4.14) no alcanza: ahí la opción será Flatpak (Hito 9).

## Otras distribuciones

Debian 13 trae Rust 1.85: instala Rust con [rustup](https://rustup.rs) (`rustup default stable`). Ubuntu 26.04 (Rust 1.93) y Fedora (Rust 1.98) pueden usar el de la distribución o rustup.

**Debian 13 / Ubuntu 26.04:**

```bash
sudo apt install build-essential pkgconf libgtk-4-dev libadwaita-1-dev \
  libvte-2.91-gtk4-dev libgtksourceview-5-dev libpoppler-glib-dev \
  gstreamer1.0-plugins-good gstreamer1.0-libav fonts-jetbrains-mono
```

**Fedora 43/44:**

```bash
sudo dnf install gcc pkgconf-pkg-config gtk4-devel libadwaita-devel \
  vte291-gtk4-devel gtksourceview5-devel poppler-glib-devel \
  gstreamer1-plugins-good jetbrains-mono-fonts
```

En Fedora, los códecs de `gst-libav` (más formatos de video) están en RPM Fusion, no en los repositorios oficiales.

Después, igual que en Arch: `cargo run -p tlacuache`.

## Verificar las bibliotecas

Verificar que pkg-config encuentra las bibliotecas:

```bash
pkg-config --modversion gtk4 libadwaita-1 vte-2.91-gtk4 gtksourceview-5
```

## Integración del shell (para sincronizar terminal → panel)

VTE necesita que el shell informe su carpeta actual (OSC 7). Para **bash** y **zsh** Tlacuache lo configura solo (ADR-008), sin tocar `.bashrc` ni `.zshrc`; **fish** lo emite por sí mismo. Se puede desactivar con `shell_integration = false` en `[terminal]`.

## Ejecutar con depuración

```bash
RUST_LOG=tlacuache=debug cargo run -p tlacuache
GTK_DEBUG=interactive cargo run -p tlacuache   # inspector de GTK
```

## Instalar en el usuario (lanzador e ícono)

```bash
scripts/install-local.sh              # compila en release e instala en ~/.local
scripts/install-local.sh --uninstall  # lo quita
```

Instala el binario en `~/.local/bin/tlacuache`, el `.desktop` en `~/.local/share/applications/` y el ícono de la app (scalable y symbolic) en `~/.local/share/icons/hicolor/`. El `.desktop` declara `MimeType=inode/directory`: Tlacuache aparece como opción para abrir carpetas, pero no cambia el gestor predeterminado.
