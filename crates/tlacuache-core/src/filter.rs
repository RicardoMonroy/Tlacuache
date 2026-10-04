//! Filtro de entradas visibles: archivos ocultos + filtro rápido por texto.

use crate::entry::FileEntry;
use crate::text::fold_for_search;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    show_hidden: bool,
    /// Consulta tal como se escribió (para mostrarla).
    query: String,
    /// Consulta normalizada con `fold_for_search`.
    needle: String,
}

/// Tecla relevante para el filtro rápido (ya sin modificadores Ctrl/Alt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKey {
    Char(char),
    Backspace,
    Escape,
}

impl Filter {
    pub fn new(show_hidden: bool, query: &str) -> Self {
        let mut filter = Self {
            show_hidden,
            ..Self::default()
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

    /// El filtro rápido está activo si hay texto (ADR-006).
    pub fn is_query_active(&self) -> bool {
        !self.query.is_empty()
    }

    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_owned();
        self.needle = fold_for_search(query);
    }

    /// `true` si la entrada debe mostrarse. La búsqueda es por subcadena sin
    /// distinguir mayúsculas ni acentos.
    pub fn matches(&self, entry: &FileEntry) -> bool {
        if entry.is_hidden && !self.show_hidden {
            return false;
        }
        self.needle.is_empty() || fold_for_search(&entry.name).contains(&self.needle)
    }
}

/// Aplica una tecla al texto del filtro (ADR-006). Devuelve la consulta nueva
/// si la tecla la consume, o `None` si debe seguir a su acción normal:
/// Backspace sube de nivel, Espacio alterna la vista previa y Esc no hace
/// nada cuando el filtro está vacío.
pub fn apply_key(query: &str, key: FilterKey) -> Option<String> {
    match key {
        FilterKey::Char(c) if c.is_control() => None,
        FilterKey::Char(' ') if query.is_empty() => None,
        FilterKey::Char(c) => Some(format!("{query}{c}")),
        FilterKey::Backspace if query.is_empty() => None,
        FilterKey::Backspace => {
            let mut q = query.to_owned();
            q.pop();
            Some(q)
        }
        FilterKey::Escape if query.is_empty() => None,
        FilterKey::Escape => Some(String::new()),
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
    fn query_ignores_accents_both_ways() {
        assert!(Filter::new(false, "cancion").matches(&e("Canción.mp3")));
        assert!(Filter::new(false, "canción").matches(&e("cancion.mp3")));
        assert!(Filter::new(false, "nandu").matches(&e("Ñandú.png")));
    }

    #[test]
    fn query_keeps_original_text() {
        let f = Filter::new(false, "Canción");
        assert_eq!(f.query(), "Canción");
    }

    #[test]
    fn query_does_not_reveal_hidden() {
        assert!(!Filter::new(false, "bash").matches(&e(".bashrc")));
        assert!(Filter::new(true, "bash").matches(&e(".bashrc")));
    }

    #[test]
    fn query_with_spaces() {
        let f = Filter::new(false, "mi FOTO");
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

    #[test]
    fn keys_with_empty_query_fall_through() {
        assert_eq!(apply_key("", FilterKey::Backspace), None);
        assert_eq!(apply_key("", FilterKey::Char(' ')), None);
        assert_eq!(apply_key("", FilterKey::Escape), None);
    }

    #[test]
    fn typing_builds_the_query() {
        assert_eq!(apply_key("", FilterKey::Char('a')).as_deref(), Some("a"));
        assert_eq!(
            apply_key("ab", FilterKey::Char('ñ')).as_deref(),
            Some("abñ")
        );
        assert_eq!(apply_key("", FilterKey::Char('.')).as_deref(), Some("."));
    }

    #[test]
    fn active_query_consumes_space_and_backspace() {
        assert_eq!(
            apply_key("mi", FilterKey::Char(' ')).as_deref(),
            Some("mi ")
        );
        assert_eq!(
            apply_key("mió", FilterKey::Backspace).as_deref(),
            Some("mi")
        );
        assert_eq!(apply_key("x", FilterKey::Backspace).as_deref(), Some(""));
    }

    #[test]
    fn escape_clears_active_query() {
        assert_eq!(apply_key("algo", FilterKey::Escape).as_deref(), Some(""));
    }

    #[test]
    fn control_chars_are_ignored() {
        assert_eq!(apply_key("a", FilterKey::Char('\t')), None);
        assert_eq!(apply_key("a", FilterKey::Char('\r')), None);
    }
}
