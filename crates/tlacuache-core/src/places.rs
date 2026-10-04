//! Lugares de la barra lateral: Inicio, carpetas XDG y Papelera. La raíz
//! se muestra en «Unidades» con su uso de disco.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceKind {
    Home,
    Desktop,
    Documents,
    Downloads,
    Music,
    Pictures,
    Videos,
    Trash,
}

impl PlaceKind {
    /// Carpetas XDG del usuario, en el orden en que se muestran.
    pub const XDG: [Self; 6] = [
        Self::Desktop,
        Self::Documents,
        Self::Downloads,
        Self::Music,
        Self::Pictures,
        Self::Videos,
    ];
}

/// Destino de un lugar: ruta local o URI (la papelera es `trash:///`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaceTarget {
    Path(PathBuf),
    Uri(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub kind: PlaceKind,
    pub target: PlaceTarget,
}

pub const TRASH_URI: &str = "trash:///";

/// Lista de lugares. `xdg_dir` devuelve la carpeta XDG configurada; se
/// omite si no existe o si apunta a `home` (lo que hace XDG cuando no está
/// definida) para no repetir Inicio.
pub fn places<F>(home: &Path, xdg_dir: F) -> Vec<Place>
where
    F: Fn(PlaceKind) -> Option<PathBuf>,
{
    let mut list = vec![Place {
        kind: PlaceKind::Home,
        target: PlaceTarget::Path(home.to_path_buf()),
    }];
    list.extend(PlaceKind::XDG.into_iter().filter_map(|kind| {
        let dir = xdg_dir(kind).filter(|dir| dir != home)?;
        Some(Place {
            kind,
            target: PlaceTarget::Path(dir),
        })
    }));
    list.push(Place {
        kind: PlaceKind::Trash,
        target: PlaceTarget::Uri(TRASH_URI.to_owned()),
    });
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(list: &[Place]) -> Vec<PlaceKind> {
        list.iter().map(|p| p.kind).collect()
    }

    #[test]
    fn all_xdg_dirs_present() {
        let home = Path::new("/home/ana");
        let list = places(home, |kind| Some(home.join(format!("{kind:?}"))));
        assert_eq!(
            kinds(&list),
            [
                PlaceKind::Home,
                PlaceKind::Desktop,
                PlaceKind::Documents,
                PlaceKind::Downloads,
                PlaceKind::Music,
                PlaceKind::Pictures,
                PlaceKind::Videos,
                PlaceKind::Trash,
            ]
        );
        assert_eq!(list[0].target, PlaceTarget::Path(home.to_path_buf()));
        assert_eq!(
            list.last().map(|p| &p.target),
            Some(&PlaceTarget::Uri(TRASH_URI.into()))
        );
    }

    #[test]
    fn missing_or_home_xdg_dirs_are_skipped() {
        let home = Path::new("/home/ana");
        let list = places(home, |kind| match kind {
            PlaceKind::Documents => Some(home.join("Documentos")),
            PlaceKind::Desktop => Some(home.to_path_buf()),
            _ => None,
        });
        assert_eq!(
            kinds(&list),
            [PlaceKind::Home, PlaceKind::Documents, PlaceKind::Trash]
        );
    }
}
