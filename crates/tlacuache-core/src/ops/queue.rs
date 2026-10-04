//! Cola FIFO de operaciones con límite de concurrencia.
//!
//! Transiciones válidas:
//!
//! ```text
//! Queued ──start──▶ Running ──complete──▶ Done
//!   │                 │  └────fail──────▶ Failed
//!   │               cancel
//!   │                 ▼
//!   │             Cancelling ──cancelled──▶ Cancelled
//!   │                 └── complete/fail (terminó antes de detenerse)
//!   └────cancel──────────────────────────▶ Cancelled
//! ```

use super::{OpId, OpKind, OpRequest, OpState, Operation, Progress};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpsError {
    #[error("no existe la operación {0}")]
    NotFound(OpId),
    #[error("la operación {id} no admite «{event}» en el estado {state:?}")]
    InvalidTransition {
        id: OpId,
        event: &'static str,
        state: OpState,
    },
}

#[derive(Debug, Clone)]
pub struct OpQueue {
    ops: Vec<Operation>,
    next_id: OpId,
    max_concurrent: usize,
}

impl Default for OpQueue {
    fn default() -> Self {
        Self::new(1)
    }
}

impl OpQueue {
    /// Cola que ejecuta hasta `max_concurrent` operaciones a la vez (mínimo 1).
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            ops: Vec::new(),
            next_id: 1,
            max_concurrent: max_concurrent.max(1),
        }
    }

    pub fn enqueue(&mut self, kind: OpKind, sources: Vec<String>, dest: Option<String>) -> OpId {
        self.enqueue_request(OpRequest::new(kind, sources, dest))
    }

    pub fn enqueue_request(&mut self, request: OpRequest) -> OpId {
        let id = self.next_id;
        self.next_id += 1;
        self.ops.push(Operation {
            id,
            kind: request.kind,
            sources: request.sources,
            dest: request.dest,
            targets: request.targets,
            state: OpState::Queued,
            progress: Progress::default(),
            undo: None,
        });
        id
    }

    pub fn get(&self, id: OpId) -> Option<&Operation> {
        self.ops.iter().find(|op| op.id == id)
    }

    /// Todas, en orden de llegada.
    pub fn iter(&self) -> impl Iterator<Item = &Operation> {
        self.ops.iter()
    }

    /// Sin operaciones en cola ni en curso.
    pub fn is_idle(&self) -> bool {
        self.ops.iter().all(|op| op.state.is_finished())
    }

    /// Pasa a `Running` las siguientes en cola (FIFO) mientras haya huecos y
    /// devuelve sus ids para que el runner las ejecute.
    pub fn start_next(&mut self) -> Vec<OpId> {
        let running = self.ops.iter().filter(|op| op.state.is_active()).count();
        let free = self.max_concurrent.saturating_sub(running);
        self.ops
            .iter_mut()
            .filter(|op| op.state == OpState::Queued)
            .take(free)
            .map(|op| {
                op.state = OpState::Running;
                op.id
            })
            .collect()
    }

    pub fn update_progress(&mut self, id: OpId, progress: Progress) -> Result<(), OpsError> {
        let op = self.find_mut(id)?;
        if !op.state.is_active() {
            return Err(invalid(op, "progress"));
        }
        op.progress = progress;
        Ok(())
    }

    /// El runner terminó con éxito (también si se pidió cancelar tarde).
    pub fn complete(&mut self, id: OpId) -> Result<(), OpsError> {
        self.complete_with_undo(id, None)
    }

    /// Termina con éxito guardando cómo deshacerla.
    pub fn complete_with_undo(
        &mut self,
        id: OpId,
        undo: Option<OpRequest>,
    ) -> Result<(), OpsError> {
        let op = self.find_mut(id)?;
        if !op.state.is_active() {
            return Err(invalid(op, "complete"));
        }
        op.state = OpState::Done;
        op.undo = undo;
        Ok(())
    }

    /// Entrega (una sola vez) la operación que deshace `id`.
    pub fn take_undo(&mut self, id: OpId) -> Option<OpRequest> {
        self.find_mut(id).ok()?.undo.take()
    }

    pub fn fail(&mut self, id: OpId, message: impl Into<String>) -> Result<(), OpsError> {
        let op = self.find_mut(id)?;
        if !op.state.is_active() {
            return Err(invalid(op, "fail"));
        }
        op.state = OpState::Failed(message.into());
        Ok(())
    }

    /// Pide cancelar. En cola se cancela al instante; en curso pasa a
    /// `Cancelling` y devuelve `true` para que la app aborte el trabajo.
    pub fn cancel(&mut self, id: OpId) -> Result<bool, OpsError> {
        let op = self.find_mut(id)?;
        match op.state {
            OpState::Queued => {
                op.state = OpState::Cancelled;
                Ok(false)
            }
            OpState::Running => {
                op.state = OpState::Cancelling;
                Ok(true)
            }
            // Pedirlo dos veces no es un error.
            OpState::Cancelling => Ok(false),
            _ => Err(invalid(op, "cancel")),
        }
    }

    /// El runner confirma que se detuvo tras cancelar.
    pub fn cancelled(&mut self, id: OpId) -> Result<(), OpsError> {
        let op = self.find_mut(id)?;
        if op.state != OpState::Cancelling {
            return Err(invalid(op, "cancelled"));
        }
        op.state = OpState::Cancelled;
        Ok(())
    }

    /// Quita las terminadas (p. ej. al limpiar el panel de operaciones).
    pub fn clear_finished(&mut self) {
        self.ops.retain(|op| !op.state.is_finished());
    }

    fn find_mut(&mut self, id: OpId) -> Result<&mut Operation, OpsError> {
        self.ops
            .iter_mut()
            .find(|op| op.id == id)
            .ok_or(OpsError::NotFound(id))
    }
}

fn invalid(op: &Operation, event: &'static str) -> OpsError {
    OpsError::InvalidTransition {
        id: op.id,
        event,
        state: op.state.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(queue: &mut OpQueue, name: &str) -> OpId {
        queue.enqueue(
            OpKind::Copy,
            vec![format!("file:///a/{name}")],
            Some("file:///b".into()),
        )
    }

    fn state(queue: &OpQueue, id: OpId) -> OpState {
        queue.get(id).map(|op| op.state.clone()).unwrap()
    }

    #[test]
    fn ids_are_unique_and_ops_start_queued() {
        let mut q = OpQueue::default();
        let a = copy(&mut q, "a");
        let b = copy(&mut q, "b");
        assert_ne!(a, b);
        assert_eq!(state(&q, a), OpState::Queued);
        assert!(!q.is_idle());
    }

    #[test]
    fn fifo_with_one_slot() {
        let mut q = OpQueue::new(1);
        let a = copy(&mut q, "a");
        let b = copy(&mut q, "b");

        assert_eq!(q.start_next(), [a]);
        assert_eq!(q.start_next(), Vec::<OpId>::new(), "sin huecos libres");

        q.complete(a).unwrap();
        assert_eq!(q.start_next(), [b]);
        q.complete(b).unwrap();
        assert!(q.is_idle());
    }

    #[test]
    fn concurrency_limit_is_respected() {
        let mut q = OpQueue::new(2);
        let ids: Vec<_> = ["a", "b", "c"].iter().map(|n| copy(&mut q, n)).collect();
        assert_eq!(q.start_next(), [ids[0], ids[1]]);
        q.fail(ids[0], "sin espacio").unwrap();
        assert_eq!(q.start_next(), [ids[2]]);
        assert_eq!(state(&q, ids[0]), OpState::Failed("sin espacio".into()));
    }

    #[test]
    fn zero_concurrency_is_clamped_to_one() {
        let mut q = OpQueue::new(0);
        let a = copy(&mut q, "a");
        assert_eq!(q.start_next(), [a]);
    }

    #[test]
    fn cancel_queued_is_immediate_and_frees_nothing() {
        let mut q = OpQueue::new(1);
        let a = copy(&mut q, "a");
        let b = copy(&mut q, "b");
        q.start_next();
        assert_eq!(q.cancel(b), Ok(false));
        assert_eq!(state(&q, b), OpState::Cancelled);
        q.complete(a).unwrap();
        assert!(q.start_next().is_empty());
        assert!(q.is_idle());
    }

    #[test]
    fn cancel_running_waits_for_runner() {
        let mut q = OpQueue::new(1);
        let a = copy(&mut q, "a");
        let b = copy(&mut q, "b");
        q.start_next();

        assert_eq!(q.cancel(a), Ok(true));
        assert_eq!(state(&q, a), OpState::Cancelling);
        assert_eq!(q.cancel(a), Ok(false), "repetir no es error");
        // Sigue ocupando el hueco hasta que el runner confirme.
        assert!(q.start_next().is_empty());

        q.cancelled(a).unwrap();
        assert_eq!(state(&q, a), OpState::Cancelled);
        assert_eq!(q.start_next(), [b]);
    }

    #[test]
    fn late_completion_after_cancel_request_counts_as_done() {
        let mut q = OpQueue::new(1);
        let a = copy(&mut q, "a");
        q.start_next();
        q.cancel(a).unwrap();
        q.complete(a).unwrap();
        assert_eq!(state(&q, a), OpState::Done);
    }

    #[test]
    fn progress_only_while_active() {
        let mut q = OpQueue::new(1);
        let a = copy(&mut q, "a");
        let progress = Progress {
            bytes_done: 10,
            bytes_total: Some(100),
            ..Progress::default()
        };
        assert!(matches!(
            q.update_progress(a, progress.clone()),
            Err(OpsError::InvalidTransition { .. })
        ));
        q.start_next();
        q.update_progress(a, progress.clone()).unwrap();
        assert_eq!(q.get(a).unwrap().progress, progress);
        q.complete(a).unwrap();
        assert!(q.update_progress(a, progress).is_err());
    }

    #[test]
    fn invalid_transitions_are_errors() {
        let mut q = OpQueue::new(1);
        let a = copy(&mut q, "a");
        assert!(q.complete(a).is_err(), "aún en cola");
        assert!(q.cancelled(a).is_err());
        q.start_next();
        q.complete(a).unwrap();
        assert!(q.cancel(a).is_err(), "ya terminó");
        assert!(q.fail(a, "x").is_err());
        assert_eq!(q.complete(99), Err(OpsError::NotFound(99)));
    }

    #[test]
    fn undo_is_stored_and_taken_once() {
        let mut q = OpQueue::new(1);
        let a = copy(&mut q, "a");
        q.start_next();
        let undo = OpRequest::new(OpKind::Trash, vec!["file:///b/a".into()], None);
        q.complete_with_undo(a, Some(undo.clone())).unwrap();
        assert_eq!(q.get(a).unwrap().undo, Some(undo.clone()));
        assert_eq!(q.take_undo(a), Some(undo));
        assert_eq!(q.take_undo(a), None);
        assert_eq!(q.take_undo(99), None);
    }

    #[test]
    fn enqueue_request_keeps_targets() {
        let mut q = OpQueue::new(1);
        let request = OpRequest {
            kind: OpKind::Restore,
            sources: vec!["file:///b/x".into()],
            dest: None,
            targets: vec!["file:///a/x".into()],
        };
        let id = q.enqueue_request(request);
        assert_eq!(q.get(id).unwrap().targets, ["file:///a/x"]);
    }

    #[test]
    fn clear_finished_keeps_pending_and_running() {
        let mut q = OpQueue::new(1);
        let a = copy(&mut q, "a");
        let b = copy(&mut q, "b");
        let c = copy(&mut q, "c");
        q.start_next();
        q.complete(a).unwrap();
        q.start_next();
        q.cancel(c).unwrap();
        q.clear_finished();
        let left: Vec<_> = q.iter().map(|op| op.id).collect();
        assert_eq!(left, [b]);
    }
}
