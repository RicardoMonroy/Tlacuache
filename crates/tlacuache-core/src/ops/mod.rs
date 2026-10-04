//! Operaciones de archivo: modelo (tipo, estado, progreso) y cola.
//!
//! Aquí no hay E/S: la app ejecuta cada operación con gio (`fs/ops_runner`)
//! y va informando a la cola. Las ubicaciones son URIs en texto para cubrir
//! también rutas no locales (GVfs) sin depender de gio.

mod conflict;
mod queue;

pub use conflict::{ConflictAction, ConflictPolicy, replace_allowed};
pub use queue::{OpQueue, OpsError};

pub type OpId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Copy,
    Move,
    Trash,
    /// Borrado permanente (requiere confirmación en la UI).
    Delete,
    Mkdir,
    /// Crear un archivo vacío.
    CreateFile,
    Rename,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpState {
    Queued,
    Running,
    /// Se pidió cancelar; el runner aún no confirma que se detuvo.
    Cancelling,
    Done,
    Failed(String),
    Cancelled,
}

impl OpState {
    /// Terminada (con éxito o no): ya no ocupa un hueco de ejecución.
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Done | Self::Failed(_) | Self::Cancelled)
    }

    /// Ocupa un hueco de ejecución.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Cancelling)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    pub bytes_done: u64,
    /// `None` mientras no se conoce (p. ej. aún se recorre un directorio).
    pub bytes_total: Option<u64>,
    pub files_done: u64,
    pub files_total: Option<u64>,
    /// Elemento que se está procesando, para mostrarlo.
    pub current: Option<String>,
}

impl Progress {
    /// Fracción completada (0.0–1.0): por bytes si se conocen, si no por
    /// archivos; `None` si aún no hay totales.
    pub fn fraction(&self) -> Option<f64> {
        let ratio = |done: u64, total: u64| {
            if total == 0 {
                1.0
            } else {
                (done as f64 / total as f64).clamp(0.0, 1.0)
            }
        };
        match (self.bytes_total, self.files_total) {
            (Some(total), _) if total > 0 => Some(ratio(self.bytes_done, total)),
            (_, Some(total)) => Some(ratio(self.files_done, total)),
            (Some(total), None) => Some(ratio(self.bytes_done, total)),
            (None, None) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub id: OpId,
    pub kind: OpKind,
    /// URIs de origen (Mkdir/CreateFile: el elemento a crear).
    pub sources: Vec<String>,
    /// URI de destino: carpeta para Copy/Move; para Rename, el elemento ya
    /// renombrado (misma carpeta, nombre nuevo).
    pub dest: Option<String>,
    pub state: OpState,
    pub progress: Progress,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_prefers_bytes_then_files() {
        let mut p = Progress::default();
        assert_eq!(p.fraction(), None);

        p.files_total = Some(4);
        p.files_done = 1;
        assert_eq!(p.fraction(), Some(0.25));

        p.bytes_total = Some(200);
        p.bytes_done = 150;
        assert_eq!(p.fraction(), Some(0.75));
    }

    #[test]
    fn fraction_with_zero_totals_is_complete_and_clamped() {
        let p = Progress {
            bytes_total: Some(0),
            files_total: Some(0),
            ..Progress::default()
        };
        assert_eq!(p.fraction(), Some(1.0));

        let over = Progress {
            bytes_done: 500,
            bytes_total: Some(100),
            ..Progress::default()
        };
        assert_eq!(over.fraction(), Some(1.0));
    }

    #[test]
    fn state_classification() {
        assert!(OpState::Running.is_active());
        assert!(OpState::Cancelling.is_active());
        assert!(!OpState::Queued.is_active());
        assert!(OpState::Done.is_finished());
        assert!(OpState::Failed("x".into()).is_finished());
        assert!(OpState::Cancelled.is_finished());
        assert!(!OpState::Cancelling.is_finished());
    }
}
