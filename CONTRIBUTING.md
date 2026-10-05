# Contribuir a Tlacuache Browser

¡Gracias por tu interés! Las contribuciones son bienvenidas: código, temas, íconos, documentación, traducciones y reportes de errores.

## Antes de empezar: el CLA

Para aceptar contribuciones necesitamos que firmes el **[Acuerdo de Licencia para Contribuidores](CLA.md)** (CLA), una sola vez. Al abrir tu primer pull request, CLA Assistant deja un comentario con un enlace: entras con tu cuenta de GitHub y aceptas. El PR no se puede fusionar hasta entonces.

En resumen:

- **Conservas los derechos de autor** de lo que aportas.
- Le das al mantenedor una licencia amplia para usar tu contribución en el proyecto y también en otras ediciones bajo otras licencias (por ejemplo, una versión para macOS).
- A cambio, el proyecto se compromete a que todo lo que incluya **siga disponible como software libre** (hoy, GPL-3.0-or-later).

Lee el [texto completo](CLA.md) antes de firmar. Si contribuyes en nombre de tu empleador, asegúrate de tener su permiso.

## Reportar errores y proponer ideas

Abre un issue con:

- qué hiciste, qué esperabas y qué pasó;
- tu distribución y las versiones de GTK, libadwaita y VTE (`pkg-config --modversion gtk4 libadwaita-1 vte-2.91-gtk4`);
- la salida con `RUST_LOG=tlacuache=debug tlacuache` si hay algo relevante.

Para cambios grandes o de arquitectura, abre primero un issue para discutir el enfoque.

## Preparar el entorno

Dependencias y compilación: [`docs/SETUP_ARCH.md`](docs/SETUP_ARCH.md). Antes de tocar algo, lee el documento que corresponda:

| Documento | Para |
|---|---|
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Qué está hecho y qué sigue |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Crates, hilos, vista previa, terminal |
| [`docs/UI_SPEC.md`](docs/UI_SPEC.md) | Comportamiento de la interfaz y atajos |
| [`docs/DECISIONS.md`](docs/DECISIONS.md) | Decisiones tomadas (ADR) |
| [`docs/THEMES.md`](docs/THEMES.md) | Temas y estilos |
| [`docs/BRAND.md`](docs/BRAND.md) | Logo, íconos y voz |

## Reglas del código

1. **No bloquear el hilo principal de GTK.** Listados, tamaños, copias, miniaturas y búsquedas van en async o en hilos de trabajo que devuelven resultados por canal.
2. **`tlacuache-core` no depende de GTK.** La lógica que se puede probar (orden, filtros, operaciones, configuración, temas…) va ahí, con pruebas unitarias.
3. **Sin `unwrap()`/`expect()`** en rutas de interfaz o de E/S. Los errores se propagan y se muestran con un toast o un diálogo.
4. **Rutas** con `PathBuf`/`gio::File`. Nunca construyas comandos de shell concatenando rutas sin escaparlas (`core::shell`).
5. **Borrar manda a la papelera** por omisión; el borrado permanente pide confirmación.
6. **Widgets propios** como subclases GObject en `crates/tlacuache/src/ui/`, uno por archivo.
7. **Nombres de código en inglés; textos de la interfaz en español** a través de `strings.rs`.
8. **Ningún color, radio ni fuente** escritos en Rust o CSS: todo sale del tema activo (`docs/THEMES.md`).
9. **Solo Linux** por ahora; no añadas código condicional para otros sistemas sin un ADR.

## Antes de abrir el pull request

```bash
scripts/check.sh            # cargo fmt --check, clippy -D warnings y pruebas
```

- Todo en verde. Si cambias la interfaz, describe cómo probarlo a mano.
- Commits pequeños, uno por cambio, con mensaje en imperativo al estilo `feat(pane): add miller columns view`.
- Si tomas una decisión de arquitectura nueva, añade un ADR en `docs/DECISIONS.md`.
- Si completas una tarea del ROADMAP, márcala con `[x]`.

## Temas

Los temas de la comunidad son muy bienvenidos. Sigue la guía de [`docs/THEMES.md`](docs/THEMES.md) §6 y comprueba el contraste:

```bash
python scripts/check-theme-contrast.py crates/tlacuache-core/themes
```

## Licencia

El proyecto se distribuye bajo la [GPL-3.0-or-later](LICENSE). Al contribuir, y con el CLA firmado, tu aporte se incluye bajo esa licencia y bajo los términos del CLA.
