//! Notas por carpeta (8.8) en un almacén central: un archivo por carpeta en
//! `~/.local/share/tlacuache/notes/`, nombrado por el MD5 de su URI (lo
//! calcula la app). El archivo empieza con una cabecera con la URI para
//! poder reubicar las notas cuando la carpeta se mueve o renombra dentro de
//! Tlacuache.

/// Carpeta del almacén dentro de los datos de la app.
pub const DIR: &str = "notes";

const HEADER_START: &str = "<!-- tlacuache: ";
const HEADER_END: &str = " -->";

/// Contenido del archivo de una nota.
pub fn encode(uri: &str, text: &str) -> String {
    format!("{HEADER_START}{uri}{HEADER_END}\n{text}")
}

/// (URI, texto) de un archivo de nota; `None` si no tiene la cabecera.
pub fn decode(content: &str) -> Option<(String, String)> {
    let (header, text) = content.split_once('\n').unwrap_or((content, ""));
    let uri = header
        .strip_prefix(HEADER_START)?
        .strip_suffix(HEADER_END)?;
    (!uri.is_empty()).then(|| (uri.to_owned(), text.to_owned()))
}

/// Nueva URI de una nota si su carpeta es `from` o está dentro de ella y
/// `from` pasó a llamarse `to`. `None` si no le afecta.
pub fn relocated(uri: &str, from: &str, to: &str) -> Option<String> {
    let from = from.trim_end_matches('/');
    let to = to.trim_end_matches('/');
    if uri.trim_end_matches('/') == from {
        return Some(to.to_owned());
    }
    let rest = uri.strip_prefix(from)?.strip_prefix('/')?;
    Some(format!("{to}/{rest}"))
}

/// Una nota sin texto (solo espacios) se borra en vez de guardarse.
pub fn is_empty(text: &str) -> bool {
    text.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_and_decode_round_trip() {
        let uri = "file:///home/ana/Proyectos/tl%C3%A1cuache";
        let text = "Pendiente:\n- revisar --> esto\n";
        let content = encode(uri, text);
        assert!(content.starts_with("<!-- tlacuache: file:///"));
        assert_eq!(decode(&content), Some((uri.to_owned(), text.to_owned())));
        assert_eq!(
            decode(&encode(uri, "")),
            Some((uri.to_owned(), String::new()))
        );
        assert_eq!(decode("sin cabecera"), None);
        assert_eq!(decode("<!-- tlacuache:  -->\nx"), None);
    }

    #[test]
    fn relocation_follows_folder_and_descendants_only() {
        let from = "file:///home/ana/dev";
        let to = "file:///home/ana/codigo";
        assert_eq!(relocated(from, from, to).as_deref(), Some(to));
        assert_eq!(
            relocated("file:///home/ana/dev/tlacuache/src", from, to).as_deref(),
            Some("file:///home/ana/codigo/tlacuache/src")
        );
        // Un hermano con el mismo prefijo no se toca.
        assert_eq!(relocated("file:///home/ana/devops", from, to), None);
        assert_eq!(relocated("file:///otra", from, to), None);
    }

    #[test]
    fn blank_notes_are_empty() {
        assert!(is_empty(" \n\t"));
        assert!(!is_empty(" x "));
    }
}
