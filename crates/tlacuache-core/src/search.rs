//! Búsqueda recursiva por nombre (8.6). Recorre en profundidad sin seguir
//! enlaces simbólicos, se puede cancelar en cualquier momento y se corta al
//! llegar al tope de resultados. Se ejecuta en un hilo de trabajo.
//!
//! La coincidencia ignora mayúsculas y acentos (como el filtro rápido): una
//! subcadena, o un patrón con `*` (cualquier texto) y `?` (un carácter) que
//! debe cubrir el nombre completo.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::text::fold_for_search;

/// Qué buscar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    Contains(String),
    Glob(Vec<char>),
}

impl Query {
    /// `None` si el texto está vacío (o solo tiene espacios).
    pub fn new(text: &str) -> Option<Self> {
        let folded = fold_for_search(text.trim());
        if folded.is_empty() {
            return None;
        }
        Some(if folded.contains(['*', '?']) {
            Self::Glob(folded.chars().collect())
        } else {
            Self::Contains(folded)
        })
    }

    pub fn matches(&self, name: &str) -> bool {
        let name = fold_for_search(name);
        match self {
            Self::Contains(needle) => name.contains(needle.as_str()),
            Self::Glob(pattern) => glob_match(pattern, &name.chars().collect::<Vec<_>>()),
        }
    }
}

/// `*` y `?` sobre el nombre completo. Solo se retrocede hasta el último
/// `*` visto, así que el costo es como mucho patrón × nombre.
fn glob_match(pattern: &[char], text: &[char]) -> bool {
    let (mut p, mut t) = (0, 0);
    let (mut star, mut star_t) = (None, 0);
    while t < text.len() {
        match pattern.get(p) {
            Some('?') => {
                p += 1;
                t += 1;
            }
            Some('*') => {
                star = Some(p);
                star_t = t;
                p += 1;
            }
            Some(&c) if c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some(s) => {
                    p = s + 1;
                    star_t += 1;
                    t = star_t;
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Incluir archivos y carpetas ocultos (y entrar en ellas).
    pub show_hidden: bool,
    /// Tope de resultados.
    pub max_results: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            show_hidden: false,
            max_results: 10_000,
        }
    }
}

/// Un resultado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Cómo terminó una búsqueda.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub matches: usize,
    /// Carpetas recorridas.
    pub folders: usize,
    pub cancelled: bool,
    /// Se llegó al tope de resultados.
    pub truncated: bool,
}

/// Recorre `root` y llama a `on_match` por cada coincidencia. Las carpetas
/// que no se pueden leer se omiten. `cancel` se consulta en cada entrada.
pub fn walk(
    root: &Path,
    query: &Query,
    options: Options,
    cancel: &AtomicBool,
    mut on_match: impl FnMut(Found),
) -> Summary {
    let mut summary = Summary::default();
    let mut pending = vec![root.to_owned()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        summary.folders += 1;
        for entry in entries.flatten() {
            if cancel.load(Ordering::Relaxed) {
                summary.cancelled = true;
                return summary;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !options.show_hidden && name.starts_with('.') {
                continue;
            }
            // Sin seguir enlaces: un enlace a carpeta no se recorre.
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            let path = entry.path();
            if query.matches(&name) {
                if summary.matches >= options.max_results {
                    summary.truncated = true;
                    return summary;
                }
                summary.matches += 1;
                on_match(Found {
                    path: path.clone(),
                    is_dir,
                });
            }
            if is_dir {
                pending.push(path);
            }
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_kinds_and_matching() {
        assert_eq!(Query::new("   "), None);
        let q = Query::new("Canción").unwrap();
        assert!(q.matches("mi CANCION favorita.mp3"));
        assert!(!q.matches("cantar.txt"));

        let glob = Query::new("*.RS").unwrap();
        assert!(matches!(glob, Query::Glob(_)));
        assert!(glob.matches("main.rs"));
        assert!(!glob.matches("main.rs.bak"));
        let q = Query::new("informe-202?.pdf").unwrap();
        assert!(q.matches("Informe-2026.PDF"));
        assert!(!q.matches("informe-20261.pdf"));
        assert!(Query::new("a*b*c").unwrap().matches("aXXbYYc"));
        assert!(!Query::new("a*b*c").unwrap().matches("aXXcYYb"));
        assert!(Query::new("*").unwrap().matches(""));
    }

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for sub in ["src/ui", "docs", ".git/objetos", "vacía"] {
            std::fs::create_dir_all(root.join(sub)).unwrap();
        }
        for file in [
            "src/main.rs",
            "src/ui/pane.rs",
            "docs/Guía.md",
            ".git/objetos/main.rs",
            ".oculto.rs",
            "README.md",
        ] {
            std::fs::write(root.join(file), b"x").unwrap();
        }
        std::os::unix::fs::symlink(root.join("src"), root.join("enlace-src")).unwrap();
        dir
    }

    fn names(root: &Path, query: &str, options: Options) -> (Vec<String>, Summary) {
        let mut found = Vec::new();
        let summary = walk(
            root,
            &Query::new(query).unwrap(),
            options,
            &AtomicBool::new(false),
            |f| {
                found.push(
                    f.path
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            },
        );
        found.sort();
        (found, summary)
    }

    #[test]
    fn walks_recursively_without_hidden_or_symlinked_folders() {
        let dir = tree();
        let (found, summary) = names(dir.path(), "*.rs", Options::default());
        assert_eq!(found, ["src/main.rs", "src/ui/pane.rs"]);
        assert!(!summary.cancelled && !summary.truncated);

        let (found, _) = names(dir.path(), "guia", Options::default());
        assert_eq!(found, ["docs/Guía.md"]);
        // Las carpetas también coinciden (y el enlace aparece, sin recorrerse).
        let (found, _) = names(dir.path(), "src", Options::default());
        assert_eq!(found, ["enlace-src", "src"]);
    }

    #[test]
    fn hidden_on_request_and_result_limit() {
        let dir = tree();
        let all = Options {
            show_hidden: true,
            ..Options::default()
        };
        let (found, _) = names(dir.path(), "*.rs", all);
        assert_eq!(
            found,
            [
                ".git/objetos/main.rs",
                ".oculto.rs",
                "src/main.rs",
                "src/ui/pane.rs"
            ]
        );

        let limited = Options {
            show_hidden: true,
            max_results: 2,
        };
        let (found, summary) = names(dir.path(), "*.rs", limited);
        assert_eq!(found.len(), 2);
        assert!(summary.truncated);
    }

    #[test]
    fn cancel_stops_immediately() {
        let dir = tree();
        let cancel = AtomicBool::new(true);
        let mut count = 0;
        let summary = walk(
            dir.path(),
            &Query::new("*").unwrap(),
            Options::default(),
            &cancel,
            |_| {
                count += 1;
            },
        );
        assert!(summary.cancelled);
        assert_eq!(count, 0);
    }
}
