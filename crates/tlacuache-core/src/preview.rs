//! Vista previa por panel (`docs/ARCHITECTURE.md` §6): qué se muestra según
//! la selección y qué renderer le toca a cada archivo.

use crate::filetype::{FileCategory, classify};
use crate::summary::SelectionSummary;

/// Espera tras el último cambio de selección antes de cargar la vista
/// previa (al recorrer la lista con las flechas no se carga cada fila).
pub const DEBOUNCE_MS: u64 = 120;

/// Qué muestra la vista previa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target<T> {
    /// Sin selección: la carpeta actual.
    Directory,
    /// Un elemento seleccionado.
    Single(T),
    /// Varios: solo el resumen.
    Many(SelectionSummary),
}

/// Objetivo según los elementos seleccionados (`summary` es su resumen).
pub fn target<T>(mut selected: Vec<T>, summary: SelectionSummary) -> Target<T> {
    match selected.len() {
        0 => Target::Directory,
        1 => selected.pop().map_or(Target::Directory, Target::Single),
        _ => Target::Many(summary),
    }
}

/// Renderer de la vista previa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Image,
    /// Texto o código (sourceview).
    Text,
    /// PDF (poppler), página por página.
    Pdf,
    /// Video (`gtk::Video`).
    Video,
    /// Audio (controles de reproducción).
    Audio,
    Folder,
    /// Ícono grande y detalles.
    Other,
}

/// Renderer para un archivo, con la misma clasificación que los íconos.
pub fn kind(content_type: Option<&str>, name: &str, is_dir: bool) -> PreviewKind {
    match classify(content_type, name, is_dir, false) {
        FileCategory::Folder => PreviewKind::Folder,
        FileCategory::Image => PreviewKind::Image,
        FileCategory::Code | FileCategory::Text => PreviewKind::Text,
        FileCategory::Pdf => PreviewKind::Pdf,
        FileCategory::Video => PreviewKind::Video,
        FileCategory::Audio => PreviewKind::Audio,
        _ => PreviewKind::Other,
    }
}

/// Lado mayor (px) al decodificar imágenes para la vista previa: las más
/// grandes se escalan en el hilo de trabajo.
pub const MAX_DECODE_SIDE: i32 = 2560;
/// Factor por paso de la rueda.
pub const ZOOM_STEP: f64 = 1.15;
pub const MAX_ZOOM: f64 = 16.0;

/// Tamaño de decodificación: el original si cabe en `max`; si no, escalado
/// conservando la proporción (al menos 1 px por lado).
pub fn decode_size(width: i32, height: i32, max: i32) -> (i32, i32) {
    let longest = width.max(height);
    if longest <= max || longest <= 0 {
        return (width, height);
    }
    let scale = f64::from(max) / f64::from(longest);
    let side = |v: i32| ((f64::from(v) * scale).round() as i32).max(1);
    (side(width), side(height))
}

/// Escala (agrandando o reduciendo) para que el lado mayor mida `side`.
pub fn scale_to_side(width: i32, height: i32, side: i32) -> (i32, i32) {
    if width <= 0 || height <= 0 {
        return (width, height);
    }
    let scale = f64::from(side) / f64::from(width.max(height));
    let px = |v: i32| ((f64::from(v) * scale).round() as i32).max(1);
    (px(width), px(height))
}

/// Duración legible: `m:ss`, o `h:mm:ss` desde una hora (`micros` en
/// microsegundos, como `gtk::MediaStream`). `None` si es desconocida (≤ 0).
pub fn format_duration(micros: i64) -> Option<String> {
    if micros <= 0 {
        return None;
    }
    let total = micros / 1_000_000;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    Some(if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    })
}

/// Lado mayor (px) de una página de PDF renderizada.
pub const PDF_RENDER_SIDE: f64 = 1600.0;

/// Escala de render de una página de `width`×`height` puntos para que su
/// lado mayor mida `side` px (como mucho ×4 para páginas diminutas).
pub fn pdf_scale(width: f64, height: f64, side: f64) -> f64 {
    // `f64::max` ignora un NaN: se validan los dos lados.
    if !(width.is_finite() && height.is_finite()) || width.max(height) <= 0.0 {
        return 1.0;
    }
    (side / width.max(height)).min(4.0)
}

/// Página válida (0-based) tras moverse `delta` desde `current`.
pub fn page_step(current: i32, delta: i32, pages: i32) -> i32 {
    if pages <= 0 {
        return 0;
    }
    current.saturating_add(delta).clamp(0, pages - 1)
}

/// Escala que encaja una imagen de `image` px en `area` px sin agrandarla.
pub fn fit_scale(image: (f64, f64), area: (f64, f64)) -> f64 {
    if image.0 <= 0.0 || image.1 <= 0.0 || area.0 <= 0.0 || area.1 <= 0.0 {
        return 1.0;
    }
    (area.0 / image.0).min(area.1 / image.1).min(1.0)
}

/// Zoom tras un paso de rueda (`dy` < 0 acerca). `None` = volver a encajar
/// (no se aleja más allá de la escala de encaje).
pub fn zoom(current: f64, fit: f64, dy: f64) -> Option<f64> {
    let next = current * ZOOM_STEP.powf(-dy);
    (next > fit * 1.001).then(|| next.min(MAX_ZOOM))
}

/// Desplazamiento nuevo para que el punto bajo el puntero (`pointer`, en
/// coordenadas de la vista) siga en su sitio al pasar de `old` a `new`.
pub fn anchored_offset(offset: f64, pointer: f64, old: f64, new: f64) -> f64 {
    if old <= 0.0 {
        return 0.0;
    }
    ((offset + pointer) / old * new - pointer).max(0.0)
}

/// Texto a mostrar de los primeros bytes de un archivo (`truncated`: el
/// archivo sigue). `None` si parece binario (tiene bytes NUL). Si está
/// truncado, se corta en el último salto de línea para no dejar una línea
/// (ni un carácter UTF-8) a medias; lo que no sea UTF-8 válido se sustituye.
pub fn text_excerpt(bytes: &[u8], truncated: bool) -> Option<String> {
    if bytes.contains(&0) {
        return None;
    }
    let end = if truncated {
        bytes
            .iter()
            .rposition(|b| *b == b'\n')
            .map_or(bytes.len(), |i| i + 1)
    } else {
        bytes.len()
    };
    Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_excerpt_cases() {
        assert_eq!(
            text_excerpt(b"hola\nmundo", false).as_deref(),
            Some("hola\nmundo")
        );
        // Truncado: hasta el último salto de línea.
        assert_eq!(
            text_excerpt(b"uno\ndos\ntr", true).as_deref(),
            Some("uno\ndos\n")
        );
        // Una sola línea larga truncada se muestra completa.
        assert_eq!(text_excerpt(b"abc", true).as_deref(), Some("abc"));
        // Binario.
        assert_eq!(text_excerpt(b"PK\x03\x04\x00\x00", false), None);
        // UTF-8 cortado a la mitad de «ñ» sin salto de línea: se sustituye.
        let cut = &"año".as_bytes()[..2];
        assert_eq!(text_excerpt(cut, true).as_deref(), Some("a\u{FFFD}"));
        assert_eq!(text_excerpt(b"", false).as_deref(), Some(""));
    }

    #[test]
    fn decode_size_scales_only_large_images() {
        assert_eq!(decode_size(800, 600, 2560), (800, 600));
        assert_eq!(decode_size(5120, 2560, 2560), (2560, 1280));
        assert_eq!(decode_size(1000, 8000, 2000), (250, 2000));
        assert_eq!(decode_size(10000, 1, 100), (100, 1));
        assert_eq!(decode_size(0, 0, 100), (0, 0));
    }

    #[test]
    fn scale_to_side_enlarges_and_shrinks() {
        assert_eq!(scale_to_side(64, 32, 1024), (1024, 512));
        assert_eq!(scale_to_side(4096, 4096, 1024), (1024, 1024));
        assert_eq!(scale_to_side(0, 5, 1024), (0, 5));
    }

    #[test]
    fn duration_format() {
        assert_eq!(format_duration(0), None);
        assert_eq!(format_duration(-5), None);
        assert_eq!(format_duration(999_999).as_deref(), Some("0:00"));
        assert_eq!(format_duration(65_000_000).as_deref(), Some("1:05"));
        assert_eq!(format_duration(3_600_000_000).as_deref(), Some("1:00:00"));
        assert_eq!(format_duration(4_000_000_000).as_deref(), Some("1:06:40"));
    }

    #[test]
    fn pdf_scale_and_page_steps() {
        // Carta (612×792 pt) a 1600 px de alto.
        assert!((pdf_scale(612.0, 792.0, 1600.0) - 1600.0 / 792.0).abs() < 1e-9);
        assert_eq!(pdf_scale(100.0, 50.0, 1600.0), 4.0);
        assert_eq!(pdf_scale(0.0, 0.0, 1600.0), 1.0);
        assert_eq!(pdf_scale(f64::NAN, 10.0, 1600.0), 1.0);

        assert_eq!(page_step(0, -1, 10), 0);
        assert_eq!(page_step(0, 1, 10), 1);
        assert_eq!(page_step(9, 1, 10), 9);
        assert_eq!(page_step(3, 1, 0), 0);
    }

    #[test]
    fn fit_never_enlarges() {
        assert_eq!(fit_scale((100.0, 50.0), (400.0, 400.0)), 1.0);
        assert_eq!(fit_scale((1000.0, 500.0), (500.0, 500.0)), 0.5);
        assert_eq!(fit_scale((1000.0, 2000.0), (500.0, 500.0)), 0.25);
        assert_eq!(fit_scale((0.0, 10.0), (500.0, 500.0)), 1.0);
    }

    #[test]
    fn zoom_steps_and_limits() {
        let fit = 0.5;
        let zoomed = zoom(fit, fit, -1.0).unwrap();
        assert!((zoomed - 0.575).abs() < 1e-9);
        // Alejar desde el encaje vuelve a encajar.
        assert_eq!(zoom(fit, fit, 1.0), None);
        assert_eq!(zoom(zoomed, fit, 1.0), None);
        assert_eq!(zoom(15.0, fit, -5.0), Some(MAX_ZOOM));
    }

    #[test]
    fn anchor_keeps_point_under_pointer() {
        // Punto 100 px dentro de la vista, sin desplazamiento, al doble.
        assert_eq!(anchored_offset(0.0, 100.0, 1.0, 2.0), 100.0);
        assert_eq!(anchored_offset(50.0, 100.0, 1.0, 2.0), 200.0);
        assert_eq!(anchored_offset(200.0, 100.0, 2.0, 1.0), 50.0);
        assert_eq!(anchored_offset(0.0, 100.0, 2.0, 1.0), 0.0);
    }

    #[test]
    fn target_depends_on_selection_size() {
        let none = SelectionSummary::default();
        assert_eq!(target(Vec::<&str>::new(), none), Target::Directory);
        assert_eq!(target(vec!["a"], none), Target::Single("a"));
        let two = SelectionSummary {
            files: 1,
            dirs: 1,
            bytes: 10,
        };
        assert_eq!(target(vec!["a", "b"], two), Target::Many(two));
    }

    #[test]
    fn kind_by_type() {
        let cases = [
            (Some("image/png"), "a.png", false, PreviewKind::Image),
            (Some("image/svg+xml"), "a.svg", false, PreviewKind::Image),
            (Some("text/plain"), "notas.txt", false, PreviewKind::Text),
            (Some("text/markdown"), "README.md", false, PreviewKind::Text),
            (Some("text/rust"), "main.rs", false, PreviewKind::Text),
            (Some("application/json"), "a.json", false, PreviewKind::Text),
            (
                Some("application/x-shellscript"),
                "a.sh",
                false,
                PreviewKind::Text,
            ),
            (None, "Cargo.toml", false, PreviewKind::Text),
            (Some("inode/directory"), "src", false, PreviewKind::Folder),
            (None, "src", true, PreviewKind::Folder),
            (Some("application/pdf"), "a.pdf", false, PreviewKind::Pdf),
            (Some("application/zip"), "a.zip", false, PreviewKind::Other),
            (Some("video/mp4"), "a.mp4", false, PreviewKind::Video),
            (Some("video/webm"), "a.webm", false, PreviewKind::Video),
            (Some("audio/mpeg"), "a.mp3", false, PreviewKind::Audio),
            (
                Some("audio/x-vorbis+ogg"),
                "a.oga",
                false,
                PreviewKind::Audio,
            ),
            (
                Some("application/octet-stream"),
                "a.bin",
                false,
                PreviewKind::Other,
            ),
        ];
        for (mime, name, is_dir, expected) in cases {
            assert_eq!(kind(mime, name, is_dir), expected, "{mime:?} {name}");
        }
    }
}
