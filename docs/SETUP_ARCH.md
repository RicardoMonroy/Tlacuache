# Entorno de desarrollo en Arch Linux

## Dependencias del sistema

```bash
sudo pacman -S --needed rustup base-devel pkgconf \
  gtk4 libadwaita vte4 gtksourceview5 poppler-glib \
  gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav
rustup default stable
```

- `vte4` es la variante de VTE para GTK4 (la de GTK3 es `vte3`).
- `poppler-glib` y los plugins de GStreamer solo se necesitan desde el Hito 7.
- Si algún nombre de paquete cambió, buscar con `pacman -Ss <nombre>` antes de suponer.

Verificar que pkg-config encuentra las bibliotecas:

```bash
pkg-config --modversion gtk4 libadwaita-1 vte-2.91-gtk4 gtksourceview-5
```

## Integración del shell (para sincronizar terminal → panel)

VTE necesita que el shell informe su carpeta actual (OSC 7). En bash o zsh, añadir al rc:

```bash
[ -f /etc/profile.d/vte.sh ] && source /etc/profile.d/vte.sh
```

Para fish y otros shells, verificar si emiten OSC 7 o añadir un hook equivalente.

## Ejecutar con depuración

```bash
RUST_LOG=tlacuache=debug cargo run -p tlacuache
GTK_DEBUG=interactive cargo run -p tlacuache   # inspector de GTK
```
