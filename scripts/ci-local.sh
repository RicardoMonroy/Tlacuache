#!/usr/bin/env bash
# Reproduce localmente con Docker los trabajos de `.github/workflows/ci.yml`
# (9.2). El repositorio se monta en solo lectura; lo compilado queda dentro
# del contenedor, que se borra al terminar.
# Uso: scripts/ci-local.sh [arch] [debian] [ubuntu] [fedora]   (sin argumentos: todos)
# Requiere permisos para usar Docker (grupo `docker` o sudo).
set -euo pipefail

cd "$(dirname "$0")/.."

image_for() {
    case "$1" in
        arch) echo archlinux:latest ;;
        debian) echo debian:trixie ;;
        ubuntu) echo ubuntu:26.04 ;;
        fedora) echo fedora:43 ;;
        *) echo "distribución desconocida: $1" >&2; exit 2 ;;
    esac
}

# Lo mismo que hacen los pasos del workflow tras instalar dependencias.
steps_for() {
    case "$1" in
        arch) echo 'scripts/check.sh && python scripts/check-theme-contrast.py crates/tlacuache-core/themes' ;;
        *) echo 'cargo build --locked --workspace --all-targets && cargo test --locked --workspace' ;;
    esac
}

distros=("$@")
if [[ ${#distros[@]} -eq 0 ]]; then
    distros=(arch debian ubuntu fedora)
fi

declare -A result
for distro in "${distros[@]}"; do
    image=$(image_for "$distro")
    echo "==> $distro ($image)"
    # `bash` existe en todas las imágenes usadas. El PATH incluye el rustup
    # de Debian.
    if docker run --rm -v "$PWD:/src:ro" -w /src \
        -e CARGO_TARGET_DIR=/tmp/target -e CARGO_TERM_COLOR=always \
        "$image" bash -c "scripts/ci-setup.sh $distro >/tmp/setup.log 2>&1 || { cat /tmp/setup.log; exit 1; }
            export PATH=\"\$HOME/.cargo/bin:\$PATH\"; $(steps_for "$distro")"; then
        result[$distro]="ok"
    else
        result[$distro]="FALLÓ"
    fi
done

echo
for distro in "${distros[@]}"; do
    echo "$distro: ${result[$distro]}"
done
[[ ! " ${result[*]} " =~ FALLÓ ]]
