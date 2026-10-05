//! Almacén de notas por carpeta (8.8): `~/.local/share/tlacuache/notes/`,
//! un archivo `<md5(uri)>.md` por carpeta con el formato de `core::notes`.
//! Toda la E/S va en hilos de trabajo; `changed` avisa a la interfaz.

use std::cell::OnceCell;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use glib::subclass::Signal;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::notes;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Notes {}

    #[glib::object_subclass]
    impl ObjectSubclass for Notes {
        const NAME: &'static str = "TlacuacheNotes";
        type Type = super::Notes;
    }

    impl ObjectImpl for Notes {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                // Cambió (o apareció, o se borró) la nota de una URI.
                vec![
                    Signal::builder("changed")
                        .param_types([String::static_type()])
                        .build(),
                ]
            })
        }
    }
}

glib::wrapper! {
    pub struct Notes(ObjectSubclass<imp::Notes>);
}

thread_local! {
    static NOTES: OnceCell<Notes> = const { OnceCell::new() };
}

/// El almacén de notas de la app (hilo de GTK).
pub fn get() -> Notes {
    NOTES.with(|n| n.get_or_init(glib::Object::new).clone())
}

/// Carpeta del almacén.
fn store_dir() -> PathBuf {
    glib::user_data_dir().join("tlacuache").join(notes::DIR)
}

impl Notes {
    pub fn connect_changed<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "changed",
            false,
            glib::closure_local!(move |notes: &Self, uri: String| f(notes, &uri)),
        )
    }

    /// Texto de la nota de `uri` (vacío si no hay).
    pub async fn load(&self, uri: &str) -> String {
        let uri = uri.to_owned();
        gio::spawn_blocking(move || load_from(&store_dir(), &uri))
            .await
            .unwrap_or_default()
    }

    /// ¿Tiene nota la carpeta `uri`?
    pub async fn exists(&self, uri: &str) -> bool {
        let uri = uri.to_owned();
        gio::spawn_blocking(move || note_path(&store_dir(), &uri).is_file())
            .await
            .unwrap_or(false)
    }

    /// Guarda (o borra si queda vacía) y avisa con `changed`.
    pub async fn save(&self, uri: &str, text: &str) {
        let (owned_uri, text) = (uri.to_owned(), text.to_owned());
        let result = gio::spawn_blocking(move || save_to(&store_dir(), &owned_uri, &text)).await;
        match result {
            Ok(Ok(())) => self.emit_by_name::<()>("changed", &[&uri.to_owned()]),
            Ok(Err(err)) => tracing::warn!("no se guardó la nota: {err}"),
            Err(_) => tracing::warn!("falló el hilo que guarda la nota"),
        }
    }

    /// Las notas siguen a las carpetas movidas o renombradas (`pairs`:
    /// URIs de origen y destino de una operación terminada).
    pub fn relocate(&self, pairs: Vec<(String, String)>) {
        if pairs.is_empty() {
            return;
        }
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = notes)]
            self,
            async move {
                let changed = gio::spawn_blocking(move || relocate_in(&store_dir(), &pairs))
                    .await
                    .unwrap_or_default();
                for uri in changed {
                    notes.emit_by_name::<()>("changed", &[&uri]);
                }
            }
        ));
    }
}

fn note_path(dir: &Path, uri: &str) -> PathBuf {
    let md5 = glib::compute_checksum_for_string(glib::ChecksumType::Md5, uri)
        .map(|s| s.to_string())
        .unwrap_or_default();
    dir.join(format!("{md5}.md"))
}

fn load_from(dir: &Path, uri: &str) -> String {
    std::fs::read_to_string(note_path(dir, uri))
        .ok()
        .and_then(|content| notes::decode(&content))
        // Otra URI con el mismo MD5 (casi imposible): no es su nota.
        .filter(|(stored, _)| stored == uri)
        .map(|(_, text)| text)
        .unwrap_or_default()
}

fn save_to(dir: &Path, uri: &str, text: &str) -> std::io::Result<()> {
    let path = note_path(dir, uri);
    if notes::is_empty(text) {
        return match std::fs::remove_file(&path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(dir)?;
    // Escritura atómica: temporal y renombrar.
    let tmp = path.with_extension(format!("md.tmp-{}", std::process::id()));
    std::fs::write(&tmp, notes::encode(uri, text))?;
    std::fs::rename(&tmp, &path)
}

/// Reescribe con su URI nueva las notas afectadas; devuelve las URIs
/// (viejas y nuevas) que cambiaron.
fn relocate_in(dir: &Path, pairs: &[(String, String)]) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut changed = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let Some((uri, text)) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| notes::decode(&content))
        else {
            continue;
        };
        let Some(new_uri) = pairs
            .iter()
            .find_map(|(from, to)| notes::relocated(&uri, from, to))
        else {
            continue;
        };
        // No pisar una nota que ya tenga el destino.
        if note_path(dir, &new_uri).exists() {
            continue;
        }
        if save_to(dir, &new_uri, &text).is_ok() && std::fs::remove_file(&path).is_ok() {
            changed.push(uri);
            changed.push(new_uri);
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_and_delete_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("notes");
        let uri = "file:///home/ana/dev";
        assert_eq!(load_from(&store, uri), "");
        save_to(&store, uri, "comprar café\n").unwrap();
        assert_eq!(load_from(&store, uri), "comprar café\n");
        assert_eq!(
            std::fs::read_dir(&store).unwrap().count(),
            1,
            "sin temporales"
        );
        save_to(&store, uri, "  \n").unwrap();
        assert!(!note_path(&store, uri).exists());
        // Borrar algo que no existe no es error.
        save_to(&store, uri, "").unwrap();
    }

    #[test]
    fn notes_follow_moved_folders_and_their_children() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("notes");
        save_to(&store, "file:///a/dev", "raíz").unwrap();
        save_to(&store, "file:///a/dev/src", "hija").unwrap();
        save_to(&store, "file:///a/devops", "vecina").unwrap();

        let changed = relocate_in(
            &store,
            &[("file:///a/dev".to_owned(), "file:///b/codigo".to_owned())],
        );
        assert_eq!(changed.len(), 4);
        assert_eq!(load_from(&store, "file:///b/codigo"), "raíz");
        assert_eq!(load_from(&store, "file:///b/codigo/src"), "hija");
        assert_eq!(load_from(&store, "file:///a/dev"), "");
        assert_eq!(load_from(&store, "file:///a/devops"), "vecina");
    }
}
