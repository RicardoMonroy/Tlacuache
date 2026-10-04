//! Acciones con atajo remapeable desde `[keys]` de config.toml.
//!
//! Los aceleradores usan la sintaxis de GTK (`F4`, `<Control><Shift>Tab`);
//! la app los interpreta con `gtk::accelerator_parse`.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Mostrar/ocultar la terminal del panel.
    ToggleTerminal,
    /// Salir de la terminal (el foco vuelve a la lista) sin cerrarla.
    LeaveTerminal,
}

impl Action {
    pub const ALL: [Self; 2] = [Self::ToggleTerminal, Self::LeaveTerminal];

    /// Nombre en `[keys]`.
    pub fn id(self) -> &'static str {
        match self {
            Self::ToggleTerminal => "toggle_terminal",
            Self::LeaveTerminal => "leave_terminal",
        }
    }

    pub fn default_accelerator(self) -> &'static str {
        match self {
            Self::ToggleTerminal => "F4",
            Self::LeaveTerminal => "<Control><Shift>Tab",
        }
    }
}

/// Acelerador de `action`: el de `[keys]` si hay uno no vacío, si no el
/// predeterminado.
pub fn accelerator(action: Action, overrides: &BTreeMap<String, String>) -> &str {
    overrides
        .get(action.id())
        .map(|accel| accel.trim())
        .filter(|accel| !accel.is_empty())
        .unwrap_or_else(|| action.default_accelerator())
}

/// Nombres de `[keys]` que no corresponden a ninguna acción (para avisar
/// de erratas).
pub fn unknown_actions(overrides: &BTreeMap<String, String>) -> Vec<&str> {
    overrides
        .keys()
        .map(String::as_str)
        .filter(|name| Action::ALL.iter().all(|a| a.id() != *name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    }

    #[test]
    fn defaults_without_overrides() {
        let empty = BTreeMap::new();
        assert_eq!(accelerator(Action::ToggleTerminal, &empty), "F4");
        assert_eq!(
            accelerator(Action::LeaveTerminal, &empty),
            "<Control><Shift>Tab"
        );
    }

    #[test]
    fn overrides_win_and_blank_is_ignored() {
        let map = keys(&[("toggle_terminal", " F12 "), ("leave_terminal", "  ")]);
        assert_eq!(accelerator(Action::ToggleTerminal, &map), "F12");
        assert_eq!(
            accelerator(Action::LeaveTerminal, &map),
            "<Control><Shift>Tab"
        );
    }

    #[test]
    fn detects_unknown_action_names() {
        let map = keys(&[("toggle_terminal", "F4"), ("toggle_termnal", "F5")]);
        assert_eq!(unknown_actions(&map), ["toggle_termnal"]);
    }

    #[test]
    fn ids_are_unique() {
        let ids: std::collections::HashSet<_> = Action::ALL.iter().map(|a| a.id()).collect();
        assert_eq!(ids.len(), Action::ALL.len());
    }
}
