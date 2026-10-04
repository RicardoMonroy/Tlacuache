//! Historial de navegación atrás/adelante (uno por pestaña).

use std::mem;

/// Máximo de entradas que se guardan hacia atrás.
pub const DEFAULT_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History<T> {
    back: Vec<T>,
    current: T,
    forward: Vec<T>,
    limit: usize,
}

impl<T: PartialEq> History<T> {
    pub fn new(initial: T) -> Self {
        Self::with_limit(initial, DEFAULT_LIMIT)
    }

    pub fn with_limit(initial: T, limit: usize) -> Self {
        Self {
            back: Vec::new(),
            current: initial,
            forward: Vec::new(),
            limit: limit.max(1),
        }
    }

    pub fn current(&self) -> &T {
        &self.current
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Navega a un destino nuevo: el actual pasa al historial y se descarta
    /// el "adelante". Devuelve `false` si ya estábamos ahí (no cambia nada).
    pub fn navigate(&mut self, to: T) -> bool {
        if to == self.current {
            return false;
        }
        let previous = mem::replace(&mut self.current, to);
        self.back.push(previous);
        if self.back.len() > self.limit {
            self.back.remove(0);
        }
        self.forward.clear();
        true
    }

    /// Retrocede una posición y devuelve el nuevo actual.
    pub fn back(&mut self) -> Option<&T> {
        let target = self.back.pop()?;
        let previous = mem::replace(&mut self.current, target);
        self.forward.push(previous);
        Some(&self.current)
    }

    /// Avanza una posición y devuelve el nuevo actual.
    pub fn forward(&mut self) -> Option<&T> {
        let target = self.forward.pop()?;
        let previous = mem::replace(&mut self.current, target);
        self.back.push(previous);
        Some(&self.current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_without_back_or_forward() {
        let h = History::new("/");
        assert_eq!(*h.current(), "/");
        assert!(!h.can_go_back());
        assert!(!h.can_go_forward());
    }

    #[test]
    fn back_and_forward_walk_the_path() {
        let mut h = History::new("/");
        h.navigate("/home");
        h.navigate("/home/dev");

        assert_eq!(h.back(), Some(&"/home"));
        assert_eq!(h.back(), Some(&"/"));
        assert_eq!(h.back(), None);
        assert_eq!(*h.current(), "/");

        assert_eq!(h.forward(), Some(&"/home"));
        assert_eq!(h.forward(), Some(&"/home/dev"));
        assert_eq!(h.forward(), None);
    }

    #[test]
    fn navigating_clears_forward() {
        let mut h = History::new("a");
        h.navigate("b");
        h.back();
        assert!(h.can_go_forward());

        h.navigate("c");
        assert!(!h.can_go_forward());
        assert_eq!(h.back(), Some(&"a"));
    }

    #[test]
    fn navigating_to_current_is_a_noop() {
        let mut h = History::new("a");
        h.navigate("b");
        h.back();
        assert!(!h.navigate("a"));
        // El "adelante" se conserva porque no hubo navegación real.
        assert!(h.can_go_forward());
    }

    #[test]
    fn limit_drops_oldest_entries() {
        let mut h = History::with_limit(0, 3);
        for i in 1..=5 {
            h.navigate(i);
        }
        let mut seen = Vec::new();
        while let Some(&v) = h.back() {
            seen.push(v);
        }
        assert_eq!(seen, [4, 3, 2]);
    }

    #[test]
    fn zero_limit_is_clamped_to_one() {
        let mut h = History::with_limit("a", 0);
        h.navigate("b");
        h.navigate("c");
        assert_eq!(h.back(), Some(&"b"));
        assert_eq!(h.back(), None);
    }
}
