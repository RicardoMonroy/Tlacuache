#!/usr/bin/env bash
# Instala Tlacuache para el usuario actual en ~/.local (sin root):
# binario de release, .desktop e íconos de la app. Uso:
#   scripts/install-local.sh            compila e instala
#   scripts/install-local.sh --uninstall  quita lo instalado
set -euo pipefail

cd "$(dirname "$0")/.."

APP_ID="io.github.rmonroy.Tlacuache"
PREFIX="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN_DIR="$HOME/.local/bin"
BIN="$BIN_DIR/tlacuache"
DESKTOP="$PREFIX/applications/$APP_ID.desktop"
ICON="$PREFIX/icons/hicolor/scalable/apps/$APP_ID.svg"
ICON_SYMBOLIC="$PREFIX/icons/hicolor/symbolic/apps/$APP_ID-symbolic.svg"

refresh_caches() {
    if command -v gtk-update-icon-cache >/dev/null && [ -f "$PREFIX/icons/hicolor/index.theme" ]; then
        gtk-update-icon-cache -q -t "$PREFIX/icons/hicolor" || true
    fi
    if command -v update-desktop-database >/dev/null; then
        update-desktop-database -q "$PREFIX/applications" || true
    fi
}

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -f "$BIN" "$DESKTOP" "$ICON" "$ICON_SYMBOLIC"
    refresh_caches
    echo "Tlacuache desinstalado de ~/.local"
    exit 0
fi

cargo build --release -p tlacuache

install -Dm755 target/release/tlacuache "$BIN"
install -Dm644 "data/icons/hicolor/scalable/apps/$APP_ID.svg" "$ICON"
install -Dm644 "data/icons/hicolor/symbolic/apps/$APP_ID-symbolic.svg" "$ICON_SYMBOLIC"
# Ruta absoluta en Exec: el lanzador puede no tener ~/.local/bin en su PATH.
sed "s|^Exec=tlacuache |Exec=$BIN |" "data/$APP_ID.desktop" | install -Dm644 /dev/stdin "$DESKTOP"
refresh_caches

echo "Instalado: $BIN"
echo "Lanzador:  $DESKTOP"
