//! Lógica de Tlacuache Browser sin dependencias de GTK.
//!
//! Todo lo testeable (orden, filtros, historial, configuración, keymap,
//! cola de operaciones) vive aquí.

pub mod age;
pub mod clipboard;
pub mod config;
pub mod entry;
pub mod favorites;
pub mod filetype;
pub mod filter;
pub mod history;
pub mod keymap;
pub mod markdown;
pub mod names;
pub mod ops;
pub mod path_input;
pub mod perms;
pub mod places;
pub mod preview;
pub mod search;
pub mod session;
pub mod shell;
pub mod sort;
pub mod summary;
pub mod text;
pub mod theme;
pub mod theme_catalog;
pub mod thumbnail;
pub mod usage;

/// Identificador de la aplicación (D-Bus / desktop file).
pub const APP_ID: &str = "io.github.rmonroy.Tlacuache";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_id_is_reverse_dns() {
        assert_eq!(APP_ID.split('.').count(), 4);
    }
}
