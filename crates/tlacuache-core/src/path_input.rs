//! Lógica de la barra de ruta editable: expansión de `~` y autocompletado.

use std::path::{Path, PathBuf};

use crate::sort::natural_cmp;

/// Expande `~` y `~/…` con `home`. Otras formas (`~usuario`) se dejan igual.
pub fn expand_tilde(input: &str, home: &Path) -> PathBuf {
    if input == "~" {
        return home.to_path_buf();
    }
    match input.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(input),
    }
}

/// Divide lo escrito en (carpeta tal como se escribió, nombre parcial).
/// `"/home/ric"` → `("/home/", "ric")`; `"doc"` → `("", "doc")`.
pub fn split_for_completion(input: &str) -> (&str, &str) {
    match input.rfind('/') {
        Some(i) => input.split_at(i + 1),
        None => ("", input),
    }
}

/// Candidatos que empiezan con `partial` (sin distinguir mayúsculas), en
/// orden natural. Los ocultos solo aparecen si `partial` empieza con `.`.
pub fn completion_matches<'a, I>(partial: &str, candidates: I) -> Vec<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let wants_hidden = partial.starts_with('.');
    let needle = partial.to_lowercase();
    let mut matches: Vec<&str> = candidates
        .into_iter()
        .filter(|name| wants_hidden || !name.starts_with('.'))
        .filter(|name| name.to_lowercase().starts_with(&needle))
        .collect();
    matches.sort_by(|a, b| natural_cmp(a, b));
    matches
}

/// Prefijo común más largo (por caracteres, respetando mayúsculas).
pub fn common_prefix(names: &[&str]) -> String {
    let Some((first, rest)) = names.split_first() else {
        return String::new();
    };
    let mut len = first.len();
    for name in rest {
        len = first
            .char_indices()
            .zip(name.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((i, c), _)| i + c.len_utf8())
            .min(len);
    }
    first[..len].to_owned()
}

/// Texto completado al pulsar Tab, o `None` si no se puede extender.
/// Con una sola coincidencia se completa el nombre entero más `/`.
pub fn complete(input: &str, candidates: &[&str]) -> Option<String> {
    let (dir, partial) = split_for_completion(input);
    let matches = completion_matches(partial, candidates.iter().copied());
    match matches.as_slice() {
        [] => None,
        [only] => Some(format!("{dir}{only}/")),
        many => {
            let prefix = common_prefix(many);
            (prefix.chars().count() > partial.chars().count()).then(|| format!("{dir}{prefix}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expansion() {
        let home = Path::new("/home/ana");
        assert_eq!(expand_tilde("~", home), PathBuf::from("/home/ana"));
        assert_eq!(
            expand_tilde("~/dev/x", home),
            PathBuf::from("/home/ana/dev/x")
        );
        assert_eq!(expand_tilde("/etc", home), PathBuf::from("/etc"));
        assert_eq!(expand_tilde("~otro/x", home), PathBuf::from("~otro/x"));
        assert_eq!(expand_tilde("a/~/b", home), PathBuf::from("a/~/b"));
    }

    #[test]
    fn split_cases() {
        assert_eq!(split_for_completion("/home/ric"), ("/home/", "ric"));
        assert_eq!(split_for_completion("/home/"), ("/home/", ""));
        assert_eq!(split_for_completion("/"), ("/", ""));
        assert_eq!(split_for_completion("doc"), ("", "doc"));
        assert_eq!(split_for_completion("~/Doc"), ("~/", "Doc"));
        assert_eq!(split_for_completion(""), ("", ""));
    }

    #[test]
    fn matches_are_case_insensitive_and_natural() {
        let names = ["dev10", "Dev2", "docs", "dev1", ".dev-oculto"];
        assert_eq!(completion_matches("dev", names), ["dev1", "Dev2", "dev10"]);
        assert_eq!(
            completion_matches("", names),
            ["dev1", "Dev2", "dev10", "docs"]
        );
    }

    #[test]
    fn hidden_only_when_partial_starts_with_dot() {
        let names = [".config", ".cache", "code"];
        assert_eq!(completion_matches("c", names), ["code"]);
        assert_eq!(completion_matches(".c", names), [".cache", ".config"]);
    }

    #[test]
    fn common_prefix_cases() {
        assert_eq!(common_prefix(&[]), "");
        assert_eq!(common_prefix(&["proyecto"]), "proyecto");
        assert_eq!(common_prefix(&["proyecto-a", "proyecto-b"]), "proyecto-");
        assert_eq!(common_prefix(&["abc", "xyz"]), "");
        assert_eq!(common_prefix(&["canción", "canciones"]), "canci");
        assert_eq!(common_prefix(&["ñandú1", "ñandú2"]), "ñandú");
    }

    #[test]
    fn complete_single_match_adds_slash() {
        assert_eq!(
            complete("/home/an", &["ana", "otro"]).as_deref(),
            Some("/home/ana/")
        );
    }

    #[test]
    fn complete_multiple_extends_common_prefix() {
        let names = ["proyecto-a", "proyecto-b"];
        assert_eq!(complete("~/pro", &names).as_deref(), Some("~/proyecto-"));
        // Ya en el prefijo común: no hay nada que extender.
        assert_eq!(complete("~/proyecto-", &names), None);
    }

    #[test]
    fn complete_without_matches() {
        assert_eq!(complete("/x/zz", &["aa"]), None);
    }

    #[test]
    fn complete_mixed_case_keeps_typed_dir() {
        assert_eq!(
            complete("/m/do", &["Documentos"]).as_deref(),
            Some("/m/Documentos/")
        );
    }
}
