//! Normalización de texto compartida por orden y filtro.

/// Quita los acentos latinos comunes de una minúscula (`á` → `a`, `ü` → `u`).
/// No toca la `ñ`: el orden la necesita como letra propia.
pub fn strip_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        other => other,
    }
}

/// Forma de búsqueda: minúsculas, sin acentos y con `ñ` → `n`, para que
/// escribir sin acentos (o sin `ñ` en el teclado) encuentre el nombre.
pub fn fold_for_search(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'ñ' => 'n',
            other => strip_accent(other),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_accents_and_enye() {
        assert_eq!(fold_for_search("Canción ÑANDÚ Über"), "cancion nandu uber");
        assert_eq!(fold_for_search("abc-123_x"), "abc-123_x");
    }

    #[test]
    fn strip_accent_keeps_enye() {
        assert_eq!(strip_accent('ñ'), 'ñ');
        assert_eq!(strip_accent('é'), 'e');
    }
}
