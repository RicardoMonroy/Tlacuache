//! Deshacer: a partir de lo que el runner hizo realmente (`Journal`), la
//! operación que lo revierte.

use super::{OpKind, OpRequest};

/// Registro de lo que hizo una operación terminada (URIs).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Journal {
    /// (origen, destino final) de cada elemento copiado, movido o
    /// renombrado. El destino final puede ser «x (2)» por conflicto.
    pub pairs: Vec<(String, String)>,
    /// Se combinaron carpetas: mezcla lo nuevo con lo existente y no se
    /// puede revertir con seguridad.
    pub merged: bool,
    /// Rutas originales enviadas a la papelera.
    pub trashed: Vec<String>,
    /// Elementos creados (carpeta o archivo nuevos).
    pub created: Vec<String>,
}

/// Operación que deshace `kind` según `journal`, o `None` si no es
/// reversible (borrado permanente, combinaciones, deshacer de deshacer).
pub fn undo_request(kind: OpKind, journal: &Journal) -> Option<OpRequest> {
    if journal.merged {
        return None;
    }
    let finals = || {
        journal
            .pairs
            .iter()
            .map(|(_, to)| to.clone())
            .collect::<Vec<_>>()
    };
    let request = match kind {
        OpKind::Copy if !journal.pairs.is_empty() => OpRequest::new(OpKind::Trash, finals(), None),
        OpKind::Move if !journal.pairs.is_empty() => OpRequest {
            kind: OpKind::Restore,
            sources: finals(),
            dest: None,
            targets: journal.pairs.iter().map(|(from, _)| from.clone()).collect(),
        },
        OpKind::Rename => {
            let (from, to) = journal.pairs.first()?;
            OpRequest::new(OpKind::Rename, vec![to.clone()], Some(from.clone()))
        }
        OpKind::Trash if !journal.trashed.is_empty() => {
            OpRequest::new(OpKind::Untrash, journal.trashed.clone(), None)
        }
        OpKind::Mkdir | OpKind::CreateFile if !journal.created.is_empty() => {
            OpRequest::new(OpKind::Trash, journal.created.clone(), None)
        }
        _ => return None,
    };
    Some(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Journal {
        Journal {
            pairs: list
                .iter()
                .map(|(a, b)| ((*a).into(), (*b).into()))
                .collect(),
            ..Journal::default()
        }
    }

    #[test]
    fn copy_undo_trashes_final_copies() {
        let j = pairs(&[
            ("file:///a/x", "file:///b/x"),
            ("file:///a/y", "file:///b/y (2)"),
        ]);
        let undo = undo_request(OpKind::Copy, &j).unwrap();
        assert_eq!(undo.kind, OpKind::Trash);
        assert_eq!(undo.sources, ["file:///b/x", "file:///b/y (2)"]);
    }

    #[test]
    fn move_undo_restores_exact_original_paths() {
        let j = pairs(&[("file:///a/x", "file:///b/x (2)")]);
        let undo = undo_request(OpKind::Move, &j).unwrap();
        assert_eq!(undo.kind, OpKind::Restore);
        assert_eq!(undo.sources, ["file:///b/x (2)"]);
        assert_eq!(undo.targets, ["file:///a/x"]);
    }

    #[test]
    fn rename_undo_renames_back() {
        let j = pairs(&[("file:///a/viejo.txt", "file:///a/nuevo.txt")]);
        let undo = undo_request(OpKind::Rename, &j).unwrap();
        assert_eq!(undo.kind, OpKind::Rename);
        assert_eq!(undo.sources, ["file:///a/nuevo.txt"]);
        assert_eq!(undo.dest.as_deref(), Some("file:///a/viejo.txt"));
    }

    #[test]
    fn trash_and_create_undo() {
        let trashed = Journal {
            trashed: vec!["file:///a/x".into()],
            ..Journal::default()
        };
        assert_eq!(
            undo_request(OpKind::Trash, &trashed).unwrap().kind,
            OpKind::Untrash
        );

        let created = Journal {
            created: vec!["file:///a/Nueva carpeta".into()],
            ..Journal::default()
        };
        let undo = undo_request(OpKind::Mkdir, &created).unwrap();
        assert_eq!(undo.kind, OpKind::Trash);
        assert_eq!(undo.sources, ["file:///a/Nueva carpeta"]);
    }

    #[test]
    fn irreversible_cases() {
        let j = pairs(&[("file:///a/x", "file:///b/x")]);
        assert_eq!(undo_request(OpKind::Delete, &j), None);
        assert_eq!(undo_request(OpKind::Restore, &j), None);
        assert_eq!(undo_request(OpKind::Untrash, &j), None);
        assert_eq!(
            undo_request(OpKind::Copy, &Journal::default()),
            None,
            "nada copiado"
        );
        let merged = Journal { merged: true, ..j };
        assert_eq!(undo_request(OpKind::Copy, &merged), None);
        assert_eq!(undo_request(OpKind::Move, &merged), None);
    }
}
