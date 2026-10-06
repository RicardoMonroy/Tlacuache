//! Diferencias entre lo que muestra el listado de una carpeta y lo que hay
//! en ella (R.2/R.3). La app vuelve a leer la carpeta y aplica solo estas
//! diferencias: las filas que no cambian siguen siendo los mismos objetos,
//! así que la selección y el desplazamiento se conservan.
//!
//! Cada entrada va con su clave: el nombre real del archivo (único dentro
//! de una carpeta; el nombre para mostrar podría repetirse).

use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ListingDiff {
    /// Posiciones (en el listado actual, de menor a mayor) que ya no están.
    pub removed: Vec<usize>,
    /// Pares (posición actual, posición nueva) de entradas que siguen ahí
    /// pero cambiaron (tamaño, fecha, tipo…).
    pub changed: Vec<(usize, usize)>,
    /// Posiciones (en la lectura nueva) de entradas que no estaban.
    pub added: Vec<usize>,
}

impl ListingDiff {
    pub fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.changed.is_empty() && self.added.is_empty()
    }
}

/// Compara el listado `current` con la lectura nueva `fresh`; cada
/// elemento es (clave, contenido) y cambia si su contenido es distinto.
pub fn diff_listing<K: Hash + Eq, T: PartialEq>(
    current: &[(K, T)],
    fresh: &[(K, T)],
) -> ListingDiff {
    let fresh_by_key: HashMap<&K, usize> = fresh
        .iter()
        .enumerate()
        .map(|(i, (key, _))| (key, i))
        .collect();
    let mut diff = ListingDiff::default();
    let mut seen = vec![false; fresh.len()];
    for (i, (key, entry)) in current.iter().enumerate() {
        match fresh_by_key.get(key) {
            Some(&j) => {
                seen[j] = true;
                if fresh[j].1 != *entry {
                    diff.changed.push((i, j));
                }
            }
            None => diff.removed.push(i),
        }
    }
    diff.added = (0..fresh.len()).filter(|&j| !seen[j]).collect();
    diff
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::FileEntry;

    fn file(name: &str, size: u64) -> (String, FileEntry) {
        let entry = FileEntry {
            size,
            ..FileEntry::new(name, false)
        };
        (name.to_owned(), entry)
    }

    #[test]
    fn same_listing_has_no_changes() {
        let a = [file("a", 1), file("b", 2)];
        // El orden de lectura no importa.
        let b = [file("b", 2), file("a", 1)];
        assert!(diff_listing(&a, &b).is_empty());
    }

    #[test]
    fn detects_added_removed_and_changed() {
        let current = [file("borrado", 1), file("crece", 10), file("igual", 5)];
        let fresh = [file("igual", 5), file("nuevo", 3), file("crece", 20)];
        assert_eq!(
            diff_listing(&current, &fresh),
            ListingDiff {
                removed: vec![0],
                changed: vec![(1, 2)],
                added: vec![1],
            }
        );
    }

    /// Un renombrado es una baja y un alta (como `x.opdownload` → `x`).
    #[test]
    fn rename_is_remove_plus_add() {
        let current = [file("foto.jpg.opdownload", 100)];
        let fresh = [file("foto.jpg", 100)];
        let diff = diff_listing(&current, &fresh);
        assert_eq!(diff.removed, [0]);
        assert_eq!(diff.added, [0]);
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn any_field_counts_as_a_change() {
        let current = [file("a", 1)];
        let mut moved = file("a", 1);
        moved.1.modified = Some(std::time::SystemTime::UNIX_EPOCH);
        assert_eq!(diff_listing(&current, &[moved]).changed, [(0, 0)]);
        let folder = ("a".to_owned(), FileEntry::new("a", true));
        assert_eq!(diff_listing(&current, &[folder]).changed, [(0, 0)]);
    }

    #[test]
    fn empty_sides() {
        let some = [file("a", 1), file("b", 1)];
        let none: [(String, FileEntry); 0] = [];
        assert_eq!(diff_listing(&none, &some).added, [0, 1]);
        assert_eq!(diff_listing(&some, &none).removed, [0, 1]);
        assert!(diff_listing(&none, &none).is_empty());
    }

    /// Dos archivos con el mismo nombre para mostrar pero distinta clave
    /// (nombres reales distintos) no se confunden.
    #[test]
    fn keys_not_display_names_identify_entries() {
        let entry = FileEntry::new("nombre\u{FFFD}", false);
        let current = [("a\u{FF}".to_owned(), entry.clone())];
        let fresh = [
            ("a\u{FF}".to_owned(), entry.clone()),
            ("a\u{FE}".to_owned(), entry),
        ];
        let diff = diff_listing(&current, &fresh);
        assert_eq!(diff.added, [1]);
        assert!(diff.removed.is_empty() && diff.changed.is_empty());
    }
}
