//! Nombres de archivo: validación, parte a seleccionar al renombrar y
//! nombres numerados para elementos nuevos.

/// Límite de bytes de un nombre en la mayoría de sistemas de archivos.
pub const MAX_NAME_BYTES: usize = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("el nombre está vacío")]
    Empty,
    #[error("«.» y «..» no son nombres válidos")]
    Reserved,
    #[error("el nombre no puede contener «/»")]
    ContainsSlash,
    #[error("el nombre contiene un carácter nulo")]
    ContainsNul,
    #[error("el nombre es demasiado largo (máximo {MAX_NAME_BYTES} bytes)")]
    TooLong,
}

/// Valida un nombre tal como se escribió (los espacios se conservan, pero
/// un nombre solo de espacios cuenta como vacío).
pub fn validate_name(name: &str) -> Result<(), NameError> {
    if name.trim().is_empty() {
        Err(NameError::Empty)
    } else if name == "." || name == ".." {
        Err(NameError::Reserved)
    } else if name.contains('/') {
        Err(NameError::ContainsSlash)
    } else if name.contains('\0') {
        Err(NameError::ContainsNul)
    } else if name.len() > MAX_NAME_BYTES {
        Err(NameError::TooLong)
    } else {
        Ok(())
    }
}

/// Caracteres a seleccionar al renombrar: el nombre sin la extensión (todo
/// si es carpeta, dotfile sin extensión o no tiene punto).
pub fn stem_len(name: &str, is_dir: bool) -> usize {
    let total = name.chars().count();
    if is_dir {
        return total;
    }
    let skip = usize::from(name.starts_with('.'));
    match name[skip..].rfind('.') {
        Some(dot) => name[..skip + dot].chars().count(),
        None => total,
    }
}

/// Candidatos para un elemento nuevo: `base`, `base (2)`, `base (3)`…
pub fn numbered_names(base: &str) -> impl Iterator<Item = String> + '_ {
    std::iter::once(base.to_owned()).chain((2..).map(move |n| format!("{base} ({n})")))
}

/// Variantes numeradas de un nombre existente para «conservar ambos»: el
/// número va antes de la extensión (`foto (2).jpg`); en carpetas y nombres
/// sin extensión, al final.
pub fn numbered_variants(name: &str, is_dir: bool) -> impl Iterator<Item = String> + '_ {
    let split = name
        .char_indices()
        .nth(stem_len(name, is_dir))
        .map_or(name.len(), |(i, _)| i);
    let (stem, ext) = name.split_at(split);
    (2..).map(move |n| format!("{stem} ({n}){ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names() {
        for name in [
            "a",
            "notas.md",
            " espacio ",
            ".bashrc",
            "canción ñ",
            "..algo",
        ] {
            assert_eq!(validate_name(name), Ok(()), "{name:?}");
        }
    }

    #[test]
    fn invalid_names() {
        assert_eq!(validate_name(""), Err(NameError::Empty));
        assert_eq!(validate_name("   "), Err(NameError::Empty));
        assert_eq!(validate_name("."), Err(NameError::Reserved));
        assert_eq!(validate_name(".."), Err(NameError::Reserved));
        assert_eq!(validate_name("a/b"), Err(NameError::ContainsSlash));
        assert_eq!(validate_name("a\0b"), Err(NameError::ContainsNul));
        assert_eq!(validate_name(&"x".repeat(256)), Err(NameError::TooLong));
        assert_eq!(validate_name(&"x".repeat(255)), Ok(()));
        // 128 «ñ» = 256 bytes.
        assert_eq!(validate_name(&"ñ".repeat(128)), Err(NameError::TooLong));
    }

    #[test]
    fn stem_selection() {
        assert_eq!(stem_len("foto.jpg", false), 4);
        assert_eq!(stem_len("archivo.tar.gz", false), 11);
        assert_eq!(stem_len("Makefile", false), 8);
        assert_eq!(stem_len(".bashrc", false), 7);
        assert_eq!(stem_len(".config.toml", false), 7);
        assert_eq!(stem_len("canción.mp3", false), 7);
        assert_eq!(stem_len("v1.2", true), 4);
    }

    #[test]
    fn numbered_variants_keep_extension() {
        let first = |name: &str, dir: bool| numbered_variants(name, dir).next().unwrap();
        assert_eq!(first("foto.jpg", false), "foto (2).jpg");
        assert_eq!(first("a.tar.gz", false), "a.tar (2).gz");
        assert_eq!(first(".bashrc", false), ".bashrc (2)");
        assert_eq!(first("Makefile", false), "Makefile (2)");
        assert_eq!(first("v1.2", true), "v1.2 (2)");
        assert_eq!(first("canción.mp3", false), "canción (2).mp3");
        let more: Vec<_> = numbered_variants("x.txt", false).take(2).collect();
        assert_eq!(more, ["x (2).txt", "x (3).txt"]);
    }

    #[test]
    fn numbered_candidates() {
        let names: Vec<_> = numbered_names("Nueva carpeta").take(3).collect();
        assert_eq!(
            names,
            ["Nueva carpeta", "Nueva carpeta (2)", "Nueva carpeta (3)"]
        );
    }
}
