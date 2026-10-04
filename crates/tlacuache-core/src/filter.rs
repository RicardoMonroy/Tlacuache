//! Filtro de entradas visibles: archivos ocultos + filtro rápido por texto.

use crate::entry::FileEntry;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    show_hidden: bool,
    /// Consulta ya normalizada a minúsculas.
    query: String,
}

impl Filter {
    pub fn new(show_hidden: bool, query: &str) -> Self {
        let mut filter = Self {
            show_hidden,
            query: String::new(),
        };
        filter.set_query(query);
        filter
    }

    pub fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    pub fn set_show_hidden(&mut self, show: bool) {
        self.show_hidden = show;
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// El filtro rápido está activo si hay texto (relevante para ADR-006).
    pub fn is_query_active(&self) -> bool {
        !self.query.is_empty()
    }

    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_lowercase();
    }

    /// `true` si la entrada debe mostrarse.
    pub fn matches(&self, entry: &FileEntry) -> bool {
        if entry.is_hidden && !self.show_hidden {
            return false;
        }
        self.query.is_empty() || entry.name.to_lowercase().contains(&self.query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str) -> FileEntry {
        FileEntry::new(name, false)
    }

    #[test]
    fn default_hides_hidden_and_shows_rest() {
        let f = Filter::default();
        assert!(f.matches(&e("notas.md")));
        assert!(!f.matches(&e(".bashrc")));
        assert!(!f.is_query_active());
    }

    #[test]
    fn show_hidden_reveals_dotfiles() {
        let mut f = Filter::default();
        f.set_show_hidden(true);
        assert!(f.matches(&e(".bashrc")));
    }

    #[test]
    fn hidden_flag_wins_over_name() {
        // gio también marca ocultos por el archivo `.hidden`.
        let mut entry = e("oculto-por-gio");
        entry.is_hidden = true;
        assert!(!Filter::default().matches(&entry));
    }

    #[test]
    fn query_is_case_insensitive_substring() {
        let f = Filter::new(false, "READ");
        assert!(f.is_query_active());
        assert!(f.matches(&e("README.md")));
        assert!(f.matches(&e("unread.txt")));
        assert!(!f.matches(&e("notas.md")));
    }

    #[test]
    fn query_does_not_reveal_hidden() {
        assert!(!Filter::new(false, "bash").matches(&e(".bashrc")));
        assert!(Filter::new(true, "bash").matches(&e(".bashrc")));
    }

    #[test]
    fn query_with_spaces_and_unicode() {
        let f = Filter::new(false, "mi FOTO ñ");
        assert!(f.matches(&e("Mi foto ñandú.jpg")));
        assert!(!f.matches(&e("mi_foto.jpg")));
    }

    #[test]
    fn clearing_query_restores_all() {
        let mut f = Filter::new(false, "x");
        assert!(!f.matches(&e("a")));
        f.set_query("");
        assert!(f.matches(&e("a")));
    }
}
