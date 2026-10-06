#!/usr/bin/env bash
# Instala las dependencias de compilación de cada distribución de la CI
# (9.2). Lo usan `.github/workflows/ci.yml` y `scripts/ci-local.sh`.
# Uso: scripts/ci-setup.sh arch|debian|ubuntu|fedora
set -euo pipefail

case "${1:-}" in
    arch)
        pacman -Syu --noconfirm --needed base-devel git rust python \
            gtk4 libadwaita vte4 gtksourceview5 poppler-glib zsh fish
        ;;
    debian)
        # Debian 13 trae Rust 1.85: se instala con rustup la versión mínima
        # del workspace (`rust-version`).
        export DEBIAN_FRONTEND=noninteractive
        apt-get update
        apt-get install -y --no-install-recommends build-essential pkgconf curl ca-certificates \
            libgtk-4-dev libadwaita-1-dev libvte-2.91-gtk4-dev libgtksourceview-5-dev \
            libpoppler-glib-dev shared-mime-info
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
            sh -s -- -y --profile minimal --default-toolchain 1.92.0
        if [[ -n "${GITHUB_PATH:-}" ]]; then
            echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"
        fi
        ;;
    ubuntu)
        export DEBIAN_FRONTEND=noninteractive
        apt-get update
        apt-get install -y --no-install-recommends build-essential pkgconf ca-certificates \
            cargo rustc libgtk-4-dev libadwaita-1-dev libvte-2.91-gtk4-dev \
            libgtksourceview-5-dev libpoppler-glib-dev shared-mime-info
        ;;
    fedora)
        dnf install -y gcc pkgconf-pkg-config rust cargo gtk4-devel libadwaita-devel \
            vte291-gtk4-devel gtksourceview5-devel poppler-glib-devel shared-mime-info
        ;;
    *)
        echo "uso: $0 arch|debian|ubuntu|fedora" >&2
        exit 2
        ;;
esac
