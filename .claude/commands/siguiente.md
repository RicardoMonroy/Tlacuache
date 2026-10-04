---
description: Toma la siguiente tarea pendiente del roadmap y la implementa
---

1. Lee `docs/ROADMAP.md` y localiza la primera tarea sin marcar `[ ]`.
2. Lee las secciones relevantes de `docs/ARCHITECTURE.md` y `docs/UI_SPEC.md`.
3. Si la tarea es ambigua o toca la arquitectura, propón el enfoque en 3–5 líneas y espera confirmación.
4. Implementa la tarea respetando las reglas de `CLAUDE.md`.
5. Ejecuta `scripts/check.sh` (o fmt + clippy + tests si aún no existe) hasta que pase.
6. Marca la tarea como `[x]`, haz un commit con mensaje en imperativo y resume en 2–3 líneas qué cambió y cómo probarlo manualmente.

$ARGUMENTS
