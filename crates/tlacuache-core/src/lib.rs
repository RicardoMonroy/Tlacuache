//! Lógica de Tlacuache Browser sin dependencias de GTK.
//!
//! Todo lo testeable (orden, filtros, historial, configuración, keymap,
//! cola de operaciones) vive aquí.

pub mod config;
pub mod entry;
pub mod filter;
pub mod history;
pub mod sort;

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
