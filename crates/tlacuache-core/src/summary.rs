//! Resumen de una vista para la barra de estado: elementos visibles y
//! selección (cuántos, de qué tipo y cuánto pesan).

use crate::entry::FileEntry;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SelectionSummary {
    pub files: u32,
    pub dirs: u32,
    /// Suma de tamaños de los archivos; las carpetas no cuentan (su tamaño
    /// real requiere un recorrido recursivo).
    pub bytes: u64,
}

impl SelectionSummary {
    pub fn from_entries<'a, I>(entries: I) -> Self
    where
        I: IntoIterator<Item = &'a FileEntry>,
    {
        entries.into_iter().fold(Self::default(), |mut acc, entry| {
            if entry.is_dir {
                acc.dirs += 1;
            } else {
                acc.files += 1;
                acc.bytes = acc.bytes.saturating_add(entry.size);
            }
            acc
        })
    }

    pub fn count(&self) -> u32 {
        self.files + self.dirs
    }

    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }
}

/// Estado de una vista: entradas visibles (tras filtro) y selección.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewStatus {
    pub items: u32,
    pub selection: SelectionSummary,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(size: u64) -> FileEntry {
        let mut e = FileEntry::new("f", false);
        e.size = size;
        e
    }

    #[test]
    fn empty_selection() {
        let s = SelectionSummary::from_entries([]);
        assert!(s.is_empty());
        assert_eq!(s.bytes, 0);
    }

    #[test]
    fn counts_files_and_dirs_and_sums_only_files() {
        let mut dir = FileEntry::new("d", true);
        dir.size = 4096; // gio puede reportar tamaño de inodo; se ignora.
        let entries = [file(100), file(250), dir];
        let s = SelectionSummary::from_entries(&entries);
        assert_eq!(s.files, 2);
        assert_eq!(s.dirs, 1);
        assert_eq!(s.count(), 3);
        assert_eq!(s.bytes, 350);
    }

    #[test]
    fn huge_sizes_saturate() {
        let entries = [file(u64::MAX), file(10)];
        assert_eq!(SelectionSummary::from_entries(&entries).bytes, u64::MAX);
    }
}
