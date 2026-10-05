//! Estado de sesión que se restaura al reabrir: ventana, paneles y pestañas.
//! Se guarda en `~/.local/state/tlacuache/session.toml` (no es configuración:
//! la app lo reescribe al cerrar).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::ViewMode;

pub const FILE_NAME: &str = "session.toml";

/// Límites razonables para no restaurar ventanas o barras absurdas.
const MIN_WINDOW: i32 = 400;
const MAX_WINDOW: i32 = 16_384;
const MIN_SIDEBAR: i32 = 140;
const MAX_SIDEBAR: i32 = 800;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub window: WindowSession,
    /// Izquierdo y derecho (en ese orden).
    pub panes: Vec<PaneSession>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSession {
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
    pub sidebar_visible: bool,
    pub sidebar_width: i32,
    pub dual_pane: bool,
    /// Posición del divisor entre paneles; `None` = mitad y mitad.
    pub panes_position: Option<i32>,
    pub active_pane: usize,
}

impl Default for WindowSession {
    fn default() -> Self {
        Self {
            width: 1200,
            height: 800,
            maximized: false,
            sidebar_visible: true,
            sidebar_width: 200,
            dual_pane: true,
            panes_position: None,
            active_pane: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PaneSession {
    pub selected: usize,
    pub tabs: Vec<TabSession>,
    /// Vista previa visible.
    pub preview: bool,
    /// Alto (px) de la zona de pestañas sobre la vista previa.
    pub preview_position: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TabSession {
    /// URI de la carpeta (gio admite rutas no locales).
    pub uri: String,
    pub view: ViewMode,
    pub show_hidden: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("no se pudo leer o escribir {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("sesión dañada en {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("no se pudo serializar la sesión: {0}")]
    Serialize(#[from] toml::ser::Error),
}

impl Session {
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str::<Self>(text).map(Self::sanitized)
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Lee la sesión; `Ok(None)` si aún no existe.
    pub fn load(path: &Path) -> Result<Option<Self>, SessionError> {
        match fs::read_to_string(path) {
            Ok(text) => Self::from_toml(&text)
                .map(Some)
                .map_err(|source| SessionError::Parse {
                    path: path.to_owned(),
                    source,
                }),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(SessionError::Io {
                path: path.to_owned(),
                source,
            }),
        }
    }

    /// Escribe de forma atómica (temporal + renombrado), creando la carpeta.
    /// Hace E/S bloqueante: llamar desde un hilo de trabajo.
    pub fn save(&self, path: &Path) -> Result<(), SessionError> {
        let text = self.to_toml()?;
        let io_err = |source| SessionError::Io {
            path: path.to_owned(),
            source,
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(io_err)?;
        }
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, text).map_err(io_err)?;
        fs::rename(&tmp, path).map_err(io_err)
    }

    /// Corrige valores fuera de rango para que restaurar nunca falle.
    pub fn sanitized(mut self) -> Self {
        let w = &mut self.window;
        w.width = w.width.clamp(MIN_WINDOW, MAX_WINDOW);
        w.height = w.height.clamp(MIN_WINDOW, MAX_WINDOW);
        w.sidebar_width = w.sidebar_width.clamp(MIN_SIDEBAR, MAX_SIDEBAR);
        w.panes_position = w.panes_position.filter(|p| *p > 0);
        if w.active_pane > 1 || (w.active_pane == 1 && !w.dual_pane) {
            w.active_pane = 0;
        }
        self.panes.truncate(2);
        for pane in &mut self.panes {
            pane.tabs.retain(|tab| !tab.uri.trim().is_empty());
            if pane.selected >= pane.tabs.len() {
                pane.selected = 0;
            }
            pane.preview_position = pane.preview_position.filter(|p| *p > 0);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(uri: &str) -> TabSession {
        TabSession {
            uri: uri.to_owned(),
            view: ViewMode::Columns,
            show_hidden: false,
        }
    }

    fn sample() -> Session {
        Session {
            window: WindowSession {
                width: 1400,
                height: 900,
                maximized: true,
                sidebar_visible: false,
                sidebar_width: 260,
                dual_pane: true,
                panes_position: Some(700),
                active_pane: 1,
            },
            panes: vec![
                PaneSession {
                    selected: 1,
                    tabs: vec![
                        tab("file:///home/ana"),
                        TabSession {
                            uri: "file:///home/ana/dev".into(),
                            view: ViewMode::Details,
                            show_hidden: true,
                        },
                    ],
                    preview: true,
                    preview_position: Some(420),
                },
                PaneSession {
                    selected: 0,
                    tabs: vec![tab("sftp://servidor/srv")],
                    ..PaneSession::default()
                },
            ],
        }
    }

    #[test]
    fn round_trip() {
        let session = sample();
        let text = session.to_toml().unwrap();
        assert_eq!(Session::from_toml(&text).unwrap(), session);
    }

    #[test]
    fn empty_file_gives_defaults() {
        let session = Session::from_toml("").unwrap();
        assert_eq!(session.window, WindowSession::default());
        assert!(session.panes.is_empty());
    }

    #[test]
    fn partial_file_keeps_defaults() {
        let session = Session::from_toml("[window]\nmaximized = true\n").unwrap();
        assert!(session.window.maximized);
        assert_eq!(session.window.width, 1200);
    }

    #[test]
    fn sanitizes_out_of_range_values() {
        let mut session = sample();
        session.window.width = 10;
        session.window.height = 1_000_000;
        session.window.sidebar_width = 5;
        session.window.panes_position = Some(-3);
        session.window.active_pane = 7;
        session.panes[0].selected = 9;
        session.panes[0].preview_position = Some(0);
        session.panes[1].tabs.push(tab("   "));
        session.panes.push(PaneSession::default());

        let s = session.sanitized();
        assert_eq!(s.window.width, MIN_WINDOW);
        assert_eq!(s.window.height, MAX_WINDOW);
        assert_eq!(s.window.sidebar_width, MIN_SIDEBAR);
        assert_eq!(s.window.panes_position, None);
        assert_eq!(s.window.active_pane, 0);
        assert_eq!(s.panes.len(), 2);
        assert_eq!(s.panes[0].selected, 0);
        assert_eq!(s.panes[0].preview_position, None);
        assert_eq!(s.panes[1].tabs.len(), 1);
    }

    #[test]
    fn right_pane_active_requires_dual_mode() {
        let mut session = sample();
        session.window.dual_pane = false;
        assert_eq!(session.sanitized().window.active_pane, 0);
    }

    #[test]
    fn invalid_view_is_an_error() {
        assert!(
            Session::from_toml(
                "[[panes]]\n[[panes.tabs]]\nuri = \"file:///\"\nview = \"iconos\"\n"
            )
            .is_err()
        );
    }

    #[test]
    fn load_missing_is_none_and_save_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("estado").join(FILE_NAME);
        assert!(Session::load(&path).unwrap().is_none());

        sample().save(&path).unwrap();
        assert_eq!(Session::load(&path).unwrap(), Some(sample()));
        assert!(!path.with_extension("toml.tmp").exists());
    }

    #[test]
    fn load_reports_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        fs::write(&path, "[window\n").unwrap();
        assert!(matches!(
            Session::load(&path),
            Err(SessionError::Parse { .. })
        ));
    }
}
