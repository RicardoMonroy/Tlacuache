//! Ordenamiento de entradas: carpetas primero y orden natural de nombres.

use std::cmp::Ordering;
use std::time::SystemTime;

use crate::entry::FileEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Name,
    Extension,
    Size,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SortSpec {
    pub key: SortKey,
    pub direction: SortDirection,
}

impl SortSpec {
    /// Compara dos entradas. Las carpetas van siempre primero sin importar la
    /// dirección; los empates se resuelven por nombre natural ascendente.
    pub fn compare(&self, a: &FileEntry, b: &FileEntry) -> Ordering {
        b.is_dir.cmp(&a.is_dir).then_with(|| {
            let by_key = match self.key {
                SortKey::Name => natural_cmp(&a.name, &b.name),
                SortKey::Extension => {
                    natural_cmp(a.extension().unwrap_or(""), b.extension().unwrap_or(""))
                }
                SortKey::Size => a.size.cmp(&b.size),
                SortKey::Modified => mtime(a).cmp(&mtime(b)),
            };
            let by_key = match self.direction {
                SortDirection::Ascending => by_key,
                SortDirection::Descending => by_key.reverse(),
            };
            by_key.then_with(|| natural_cmp(&a.name, &b.name))
        })
    }

    /// Ordena en sitio (orden estable).
    pub fn sort(&self, entries: &mut [FileEntry]) {
        entries.sort_by(|a, b| self.compare(a, b));
    }
}

/// Sin fecha cuenta como la más antigua.
fn mtime(e: &FileEntry) -> SystemTime {
    e.modified.unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Orden natural: los tramos de dígitos se comparan por valor numérico
/// (`img2` < `img10`) y el texto sin distinguir mayúsculas ni acentos, con la
/// `ñ` tras la `n` (no es collation por locale). Si dos nombres son
/// equivalentes, desempata por ceros a la izquierda y luego por bytes, para
/// que el orden sea total y determinista.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut tie = Ordering::Equal;
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();

    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return tie.then_with(|| a.cmp(b)),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let da = take_digits(&mut ai);
                let db = take_digits(&mut bi);
                let (va, vb) = (da.trim_start_matches('0'), db.trim_start_matches('0'));
                // Comparar por longitud y luego lexicográfico evita desbordes.
                let ord = va.len().cmp(&vb.len()).then_with(|| va.cmp(vb));
                if ord != Ordering::Equal {
                    return ord;
                }
                if tie == Ordering::Equal {
                    // Menos ceros a la izquierda primero: `a1` < `a01`.
                    tie = da.len().cmp(&db.len());
                }
            }
            (Some(ca), Some(cb)) => {
                ai.next();
                bi.next();
                let ord = Iterator::cmp(collation_weights(ca), collation_weights(cb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}

/// Peso primario de un carácter: minúsculas, sin acentos latinos comunes y
/// con la `ñ` entre la `n` y la `o` (orden del español). Los pesos son el
/// código del carácter base × 2, dejando huecos impares para letras extra.
fn collation_weights(c: char) -> impl Iterator<Item = u32> {
    c.to_lowercase().map(|l| match l {
        'ñ' => u32::from('n') * 2 + 1,
        other => u32::from(strip_accent(other)) * 2,
    })
}

fn strip_accent(c: char) -> char {
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

fn take_digits(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut digits = String::new();
    while let Some(c) = it.next_if(char::is_ascii_digit) {
        digits.push(c);
    }
    digits
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sorted(mut names: Vec<&str>) -> Vec<&str> {
        names.sort_by(|a, b| natural_cmp(a, b));
        names
    }

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(
            sorted(vec!["img10.png", "img2.png", "img1.png", "img20.png"]),
            ["img1.png", "img2.png", "img10.png", "img20.png"]
        );
    }

    #[test]
    fn multiple_number_runs() {
        assert_eq!(
            sorted(vec!["v1.10.0", "v1.2.10", "v1.2.9", "v1.9"]),
            ["v1.2.9", "v1.2.10", "v1.9", "v1.10.0"]
        );
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            sorted(vec!["beta", "Alpha", "alpha2", "Gamma"]),
            ["Alpha", "alpha2", "beta", "Gamma"]
        );
    }

    #[test]
    fn ties_are_deterministic() {
        assert_eq!(natural_cmp("a1", "a01"), Ordering::Less);
        assert_eq!(natural_cmp("A", "a"), Ordering::Less);
        assert_eq!(natural_cmp("abc", "abc"), Ordering::Equal);
        assert_eq!(sorted(vec!["file", "File"]), ["File", "file"]);
    }

    #[test]
    fn prefix_goes_first() {
        assert_eq!(natural_cmp("doc", "doc1"), Ordering::Less);
        assert_eq!(natural_cmp("", "a"), Ordering::Less);
    }

    #[test]
    fn huge_numbers_do_not_overflow() {
        let big = "x99999999999999999999999999999";
        let bigger = "x100000000000000000000000000000";
        assert_eq!(natural_cmp(big, bigger), Ordering::Less);
    }

    #[test]
    fn digits_vs_letters_use_char_order() {
        // '1' (0x31) < 'a' (0x61), como en la mayoría de gestores.
        assert_eq!(sorted(vec!["abc", "123"]), ["123", "abc"]);
    }

    #[test]
    fn accents_sort_with_base_letter() {
        assert_eq!(
            sorted(vec!["zeta", "Árbol", "arena", "éxito", "Edad"]),
            ["Árbol", "arena", "Edad", "éxito", "zeta"]
        );
        // Mismo peso primario: desempata por bytes (sin acento primero).
        assert_eq!(sorted(vec!["árbol", "arbol"]), ["arbol", "árbol"]);
        assert_eq!(natural_cmp("ÉSTE", "éste"), Ordering::Less);
    }

    #[test]
    fn enye_sorts_between_n_and_o() {
        assert_eq!(
            sorted(vec!["ñu", "oso", "nube", "Ñandú", "nz"]),
            ["nube", "nz", "Ñandú", "ñu", "oso"]
        );
    }

    fn file(name: &str, size: u64, secs: u64) -> FileEntry {
        let mut e = FileEntry::new(name, false);
        e.size = size;
        e.modified = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs));
        e
    }

    fn dir(name: &str) -> FileEntry {
        FileEntry::new(name, true)
    }

    fn names(entries: &[FileEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn dirs_first_by_name() {
        let mut v = vec![
            file("b.txt", 1, 1),
            dir("zeta"),
            file("a10", 1, 1),
            dir("Alfa"),
            file("a2", 1, 1),
        ];
        SortSpec::default().sort(&mut v);
        assert_eq!(names(&v), ["Alfa", "zeta", "a2", "a10", "b.txt"]);
    }

    #[test]
    fn descending_keeps_dirs_first() {
        let mut v = vec![file("a", 1, 1), dir("d1"), file("b", 1, 1), dir("d2")];
        SortSpec {
            key: SortKey::Name,
            direction: SortDirection::Descending,
        }
        .sort(&mut v);
        assert_eq!(names(&v), ["d2", "d1", "b", "a"]);
    }

    #[test]
    fn by_size_with_name_tiebreak() {
        let mut v = vec![
            file("c", 10, 0),
            file("a", 5, 0),
            file("b", 10, 0),
            dir("x"),
        ];
        SortSpec {
            key: SortKey::Size,
            direction: SortDirection::Ascending,
        }
        .sort(&mut v);
        assert_eq!(names(&v), ["x", "a", "b", "c"]);
        SortSpec {
            key: SortKey::Size,
            direction: SortDirection::Descending,
        }
        .sort(&mut v);
        assert_eq!(names(&v), ["x", "b", "c", "a"]);
    }

    #[test]
    fn by_modified_missing_is_oldest() {
        let mut nodate = FileEntry::new("sin-fecha", false);
        nodate.modified = None;
        let mut v = vec![file("nuevo", 0, 300), nodate, file("viejo", 0, 100)];
        SortSpec {
            key: SortKey::Modified,
            direction: SortDirection::Ascending,
        }
        .sort(&mut v);
        assert_eq!(names(&v), ["sin-fecha", "viejo", "nuevo"]);
    }

    #[test]
    fn by_extension() {
        let mut v = vec![
            file("z.rs", 0, 0),
            file("a.toml", 0, 0),
            file("README", 0, 0),
            file("b.rs", 0, 0),
        ];
        SortSpec {
            key: SortKey::Extension,
            direction: SortDirection::Ascending,
        }
        .sort(&mut v);
        assert_eq!(names(&v), ["README", "b.rs", "z.rs", "a.toml"]);
    }
}
