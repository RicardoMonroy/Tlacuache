#!/usr/bin/env bash
# CI local: formato, lints y pruebas. Sale con error en el primer paso que falle.
# Uso: scripts/check.sh [--fix]   (--fix aplica cargo fmt en lugar de solo verificar)
set -euo pipefail

cd "$(dirname "$0")/.."

fmt_args=(--all -- --check)
if [[ "${1:-}" == "--fix" ]]; then
    fmt_args=(--all)
fi

step() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }

step "cargo fmt ${fmt_args[*]}"
cargo fmt "${fmt_args[@]}"

step "cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets -- -D warnings

step "cargo test --workspace"
cargo test --workspace

printf '\n\033[1;32m✔ todo en verde\033[0m\n'
