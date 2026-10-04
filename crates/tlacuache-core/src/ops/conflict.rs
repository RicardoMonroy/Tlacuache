//! Conflictos al copiar o mover: el destino ya existe.

/// Qué hacer con un conflicto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictAction {
    /// Sobrescribir el archivo; con carpetas, combinar su contenido.
    Replace,
    /// Dejar el destino como está y no copiar/mover este elemento.
    Skip,
    /// Copiar/mover con un nombre libre («foto (2).jpg»).
    KeepBoth,
}

/// Recuerda la decisión «aplicar a todos» durante una operación.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConflictPolicy {
    remembered: Option<ConflictAction>,
}

impl ConflictPolicy {
    /// Decisión ya tomada para todos, si aplica a este conflicto. Reemplazar
    /// no se reutiliza donde no está permitido (tipos distintos o el mismo
    /// elemento): ahí se vuelve a preguntar.
    pub fn remembered(&self, replace_allowed: bool) -> Option<ConflictAction> {
        match self.remembered {
            Some(ConflictAction::Replace) if !replace_allowed => None,
            other => other,
        }
    }

    pub fn remember(&mut self, action: ConflictAction) {
        self.remembered = Some(action);
    }
}

/// Reemplazar solo tiene sentido entre elementos del mismo tipo y si no
/// son el mismo elemento (pegar sobre sí mismo = duplicar).
pub fn replace_allowed(source_is_dir: bool, dest_is_dir: bool, same_item: bool) -> bool {
    source_is_dir == dest_is_dir && !same_item
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_remembered_by_default() {
        assert_eq!(ConflictPolicy::default().remembered(true), None);
    }

    #[test]
    fn remembers_choice_for_all() {
        let mut policy = ConflictPolicy::default();
        policy.remember(ConflictAction::Skip);
        assert_eq!(policy.remembered(true), Some(ConflictAction::Skip));
        assert_eq!(policy.remembered(false), Some(ConflictAction::Skip));
    }

    #[test]
    fn remembered_replace_asks_again_where_not_allowed() {
        let mut policy = ConflictPolicy::default();
        policy.remember(ConflictAction::Replace);
        assert_eq!(policy.remembered(true), Some(ConflictAction::Replace));
        assert_eq!(policy.remembered(false), None);
    }

    #[test]
    fn replace_rules() {
        assert!(replace_allowed(false, false, false));
        assert!(replace_allowed(true, true, false));
        assert!(!replace_allowed(true, false, false));
        assert!(!replace_allowed(false, true, false));
        assert!(!replace_allowed(false, false, true));
    }
}
