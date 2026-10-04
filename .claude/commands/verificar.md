---
description: Ejecuta formato, clippy y pruebas, y corrige lo que falle
---

Ejecuta en orden:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Corrige cada error o warning sin desactivar lints salvo con justificación escrita en el código. Repite hasta que todo pase y reporta el resultado en una línea.
