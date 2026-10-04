//! Favoritos agrupados de la barra lateral: operaciones sobre la lista de
//! grupos que se guarda en `[[favorites]]` de config.toml.
//!
//! Las rutas se guardan como texto (con `~` para el inicio) para que la
//! configuración sea portable entre equipos.

use std::path::Path;

use crate::config::FavoriteGroup;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FavoritesError {
    #[error("el nombre del grupo está vacío")]
    EmptyName,
    #[error("ya existe un grupo llamado «{0}»")]
    DuplicateGroup(String),
    #[error("posición fuera de rango")]
    OutOfRange,
}

/// Posición de un favorito: (grupo, índice dentro del grupo).
pub type Position = (usize, usize);

fn clean_name(name: &str) -> Result<String, FavoritesError> {
    let name = name.trim();
    if name.is_empty() {
        Err(FavoritesError::EmptyName)
    } else {
        Ok(name.to_owned())
    }
}

fn check_unique(
    groups: &[FavoriteGroup],
    name: &str,
    except: Option<usize>,
) -> Result<(), FavoritesError> {
    let taken = groups
        .iter()
        .enumerate()
        .any(|(i, g)| Some(i) != except && g.group == name);
    if taken {
        Err(FavoritesError::DuplicateGroup(name.to_owned()))
    } else {
        Ok(())
    }
}

/// Crea un grupo vacío al final y devuelve su índice.
pub fn add_group(groups: &mut Vec<FavoriteGroup>, name: &str) -> Result<usize, FavoritesError> {
    let name = clean_name(name)?;
    check_unique(groups, &name, None)?;
    groups.push(FavoriteGroup {
        group: name,
        paths: Vec::new(),
    });
    Ok(groups.len() - 1)
}

pub fn rename_group(
    groups: &mut [FavoriteGroup],
    index: usize,
    name: &str,
) -> Result<(), FavoritesError> {
    let name = clean_name(name)?;
    check_unique(groups, &name, Some(index))?;
    let group = groups.get_mut(index).ok_or(FavoritesError::OutOfRange)?;
    group.group = name;
    Ok(())
}

pub fn remove_group(
    groups: &mut Vec<FavoriteGroup>,
    index: usize,
) -> Result<FavoriteGroup, FavoritesError> {
    if index >= groups.len() {
        return Err(FavoritesError::OutOfRange);
    }
    Ok(groups.remove(index))
}

/// Índice del grupo donde añadir sin elegir uno: el primero, o uno nuevo
/// llamado `default_name` si no hay grupos.
pub fn default_group(groups: &mut Vec<FavoriteGroup>, default_name: &str) -> usize {
    if groups.is_empty() {
        groups.push(FavoriteGroup {
            group: default_name.to_owned(),
            paths: Vec::new(),
        });
    }
    0
}

/// Inserta `path` en el grupo antes de `index` (`None` = al final).
/// Devuelve `false` si el grupo ya lo tenía.
pub fn insert_favorite(
    groups: &mut [FavoriteGroup],
    group: usize,
    index: Option<usize>,
    path: &str,
) -> Result<bool, FavoritesError> {
    let paths = &mut groups
        .get_mut(group)
        .ok_or(FavoritesError::OutOfRange)?
        .paths;
    if paths.iter().any(|p| p == path) {
        return Ok(false);
    }
    let index = index.unwrap_or(paths.len()).min(paths.len());
    paths.insert(index, path.to_owned());
    Ok(true)
}

pub fn remove_favorite(
    groups: &mut [FavoriteGroup],
    (group, index): Position,
) -> Result<String, FavoritesError> {
    let paths = &mut groups
        .get_mut(group)
        .ok_or(FavoritesError::OutOfRange)?
        .paths;
    if index >= paths.len() {
        return Err(FavoritesError::OutOfRange);
    }
    Ok(paths.remove(index))
}

/// Mueve un favorito. `to` indica dónde quedaría tal como se ve la lista
/// antes de mover: antes del elemento `to.1` del grupo `to.0` (o al final
/// si `to.1` es la longitud). Si el grupo destino ya tiene esa ruta, solo
/// se quita del origen.
pub fn move_favorite(
    groups: &mut [FavoriteGroup],
    from: Position,
    to: Position,
) -> Result<(), FavoritesError> {
    let (to_group, mut to_index) = to;
    if to_group >= groups.len() || to_index > groups[to_group].paths.len() {
        return Err(FavoritesError::OutOfRange);
    }
    let path = remove_favorite(groups, from)?;
    if from.0 == to_group && from.1 < to_index {
        to_index -= 1;
    }
    insert_favorite(groups, to_group, Some(to_index), &path)?;
    Ok(())
}

/// Ruta para guardar: `~/…` si está dentro de `home`.
pub fn contract_home(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(name: &str, paths: &[&str]) -> FavoriteGroup {
        FavoriteGroup {
            group: name.to_owned(),
            paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        }
    }

    fn paths(groups: &[FavoriteGroup], i: usize) -> Vec<&str> {
        groups[i].paths.iter().map(String::as_str).collect()
    }

    #[test]
    fn groups_need_unique_non_empty_names() {
        let mut groups = vec![g("Proyectos", &[])];
        assert_eq!(add_group(&mut groups, "  "), Err(FavoritesError::EmptyName));
        assert_eq!(
            add_group(&mut groups, " Proyectos "),
            Err(FavoritesError::DuplicateGroup("Proyectos".into()))
        );
        assert_eq!(add_group(&mut groups, " Clases "), Ok(1));
        assert_eq!(groups[1].group, "Clases");
    }

    #[test]
    fn rename_allows_same_name_and_rejects_others() {
        let mut groups = vec![g("A", &[]), g("B", &[])];
        assert_eq!(rename_group(&mut groups, 0, "A"), Ok(()));
        assert_eq!(
            rename_group(&mut groups, 0, "B"),
            Err(FavoritesError::DuplicateGroup("B".into()))
        );
        assert_eq!(
            rename_group(&mut groups, 5, "C"),
            Err(FavoritesError::OutOfRange)
        );
        assert_eq!(rename_group(&mut groups, 1, "C"), Ok(()));
        assert_eq!(groups[1].group, "C");
    }

    #[test]
    fn remove_group_returns_it() {
        let mut groups = vec![g("A", &["~/a"]), g("B", &[])];
        assert_eq!(remove_group(&mut groups, 0), Ok(g("A", &["~/a"])));
        assert_eq!(groups.len(), 1);
        assert_eq!(
            remove_group(&mut groups, 3),
            Err(FavoritesError::OutOfRange)
        );
    }

    #[test]
    fn default_group_creates_one_when_empty() {
        let mut groups = Vec::new();
        assert_eq!(default_group(&mut groups, "Favoritos"), 0);
        assert_eq!(groups, [g("Favoritos", &[])]);
        assert_eq!(default_group(&mut groups, "Otro"), 0);
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn insert_avoids_duplicates_and_clamps() {
        let mut groups = vec![g("A", &["~/a", "~/c"])];
        assert_eq!(insert_favorite(&mut groups, 0, Some(1), "~/b"), Ok(true));
        assert_eq!(insert_favorite(&mut groups, 0, None, "~/a"), Ok(false));
        assert_eq!(insert_favorite(&mut groups, 0, Some(99), "~/z"), Ok(true));
        assert_eq!(paths(&groups, 0), ["~/a", "~/b", "~/c", "~/z"]);
        assert_eq!(
            insert_favorite(&mut groups, 1, None, "~/x"),
            Err(FavoritesError::OutOfRange)
        );
    }

    #[test]
    fn move_down_within_group() {
        let mut groups = vec![g("A", &["1", "2", "3", "4"])];
        // "1" antes de "4" (posición 3 tal como se ve).
        move_favorite(&mut groups, (0, 0), (0, 3)).unwrap();
        assert_eq!(paths(&groups, 0), ["2", "3", "1", "4"]);
        // "2" al final.
        move_favorite(&mut groups, (0, 0), (0, 4)).unwrap();
        assert_eq!(paths(&groups, 0), ["3", "1", "4", "2"]);
    }

    #[test]
    fn move_up_within_group() {
        let mut groups = vec![g("A", &["1", "2", "3"])];
        move_favorite(&mut groups, (0, 2), (0, 0)).unwrap();
        assert_eq!(paths(&groups, 0), ["3", "1", "2"]);
    }

    #[test]
    fn move_between_groups() {
        let mut groups = vec![g("A", &["1", "2"]), g("B", &["x"])];
        move_favorite(&mut groups, (0, 1), (1, 0)).unwrap();
        assert_eq!(paths(&groups, 0), ["1"]);
        assert_eq!(paths(&groups, 1), ["2", "x"]);
    }

    #[test]
    fn move_into_group_that_already_has_it_just_removes() {
        let mut groups = vec![g("A", &["1"]), g("B", &["1", "x"])];
        move_favorite(&mut groups, (0, 0), (1, 2)).unwrap();
        assert!(groups[0].paths.is_empty());
        assert_eq!(paths(&groups, 1), ["1", "x"]);
    }

    #[test]
    fn move_out_of_range_changes_nothing() {
        let mut groups = vec![g("A", &["1"])];
        assert_eq!(
            move_favorite(&mut groups, (0, 0), (3, 0)),
            Err(FavoritesError::OutOfRange)
        );
        assert_eq!(
            move_favorite(&mut groups, (0, 5), (0, 0)),
            Err(FavoritesError::OutOfRange)
        );
        assert_eq!(paths(&groups, 0), ["1"]);
    }

    #[test]
    fn contract_home_cases() {
        let home = Path::new("/home/ana");
        assert_eq!(contract_home(Path::new("/home/ana"), home), "~");
        assert_eq!(contract_home(Path::new("/home/ana/dev/x"), home), "~/dev/x");
        assert_eq!(
            contract_home(Path::new("/home/anabel"), home),
            "/home/anabel"
        );
        assert_eq!(contract_home(Path::new("/etc"), home), "/etc");
    }
}
