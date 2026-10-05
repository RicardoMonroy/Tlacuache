# Entorno de desarrollo en Arch Linux

## Dependencias del sistema

```bash
sudo pacman -S --needed rustup base-devel pkgconf \
  gtk4 libadwaita vte4 gtksourceview5 poppler-glib \
  gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav
rustup default stable
```

- `vte4` es la variante de VTE para GTK4 (la de GTK3 es `vte3`).
- `poppler-glib` y los plugins de GStreamer solo se necesitan desde el Hito 8 (v0.2: PDF, video y audio).
- `ttf-jetbrains-mono`: la fuente monoespaciada de los temas (si falta, se usa `monospace`).
- Si algún nombre de paquete cambió, buscar con `pacman -Ss <nombre>` antes de suponer.

Verificar que pkg-config encuentra las bibliotecas:

```bash
pkg-config --modversion gtk4 libadwaita-1 vte-2.91-gtk4 gtksourceview-5
```

## Integración del shell (para sincronizar terminal → panel)

VTE necesita que el shell informe su carpeta actual (OSC 7). Para **bash** Tlacuache lo configura solo (ADR-008): no hay que tocar `.bashrc`. **fish** lo emite por sí mismo. Para **zsh** (aún sin integración automática) añadir al `.zshrc`:

```bash
[ -f /etc/profile.d/vte.sh ] && source /etc/profile.d/vte.sh
```

Se puede desactivar con `shell_integration = false` en `[terminal]`.

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
