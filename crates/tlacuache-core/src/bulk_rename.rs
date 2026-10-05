//! Renombrado masivo (8.7): una regla (regex + reemplazo con contador) y el
//! plan para un lote de elementos, con los conflictos detectados antes de
//! tocar nada.
//!
//! - Buscar vacío reemplaza el nombre completo (p. ej. `foto-{n:3}`).
//! - El reemplazo admite `$1`, `${nombre}` (grupos) y `{n}` / `{n:3}`
//!   (contador desde 1, con relleno de ceros).
//! - Por omisión solo se toca el nombre sin la extensión.

use std::collections::{HashMap, HashSet};

use regex::RegexBuilder;

use crate::names::{NameError, stem_len, validate_name};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rule {
    pub find: String,
    pub replace: String,
    /// Aplicar también a la extensión.
    pub include_extension: bool,
    pub case_sensitive: bool,
}

/// Un elemento del lote. `dir` identifica su carpeta (p. ej. su URI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub dir: String,
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// El nombre no cambia.
    Unchanged,
    /// Se renombra sin problemas.
    Renamed,
    Invalid(NameError),
    /// Otro elemento del lote quedaría con el mismo nombre.
    Duplicate,
    /// Ya hay algo con ese nombre en la carpeta (que no se renombra).
    Exists,
}

impl Status {
    pub fn is_conflict(self) -> bool {
        matches!(self, Self::Invalid(_) | Self::Duplicate | Self::Exists)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub from: String,
    pub to: String,
    pub status: Status,
}

/// Resumen para habilitar «Renombrar».
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanSummary {
    pub changes: usize,
    pub conflicts: usize,
}

impl PlanSummary {
    pub fn of(plan: &[Planned]) -> Self {
        let mut summary = Self::default();
        for planned in plan {
            match planned.status {
                Status::Renamed => summary.changes += 1,
                Status::Unchanged => {}
                _ => summary.conflicts += 1,
            }
        }
        summary
    }

    /// Hay algo que renombrar y ningún conflicto.
    pub fn can_apply(self) -> bool {
        self.changes > 0 && self.conflicts == 0
    }
}

/// Nombre nuevo de cada elemento y su estado. `existing`: nombres que hay
/// en cada carpeta (los del lote incluidos). Error si la regex no es válida.
pub fn plan(
    items: &[Item],
    existing: &HashMap<String, HashSet<String>>,
    rule: &Rule,
) -> Result<Vec<Planned>, regex::Error> {
    let regex = (!rule.find.is_empty())
        .then(|| {
            RegexBuilder::new(&rule.find)
                .case_insensitive(!rule.case_sensitive)
                .build()
        })
        .transpose()?;

    let targets: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let replacement = expand_counter(&rule.replace, index + 1);
            let (stem, ext) = split(&item.name, item.is_dir, rule.include_extension);
            let new_stem = match &regex {
                Some(regex) => regex.replace_all(stem, replacement.as_str()).into_owned(),
                None if rule.replace.is_empty() => stem.to_owned(),
                None => replacement,
            };
            format!("{new_stem}{ext}")
        })
        .collect();

    // Por carpeta: lo que quedará ocupado por elementos que no cambian (de
    // fuera del lote o del lote sin cambio) y cuántas veces se pide cada
    // nombre nuevo.
    let mut leaving: HashMap<&str, HashSet<&str>> = HashMap::new();
    let mut wanted: HashMap<(&str, &str), usize> = HashMap::new();
    for (item, to) in items.iter().zip(&targets) {
        if *to != item.name {
            leaving.entry(&item.dir).or_default().insert(&item.name);
        }
        *wanted.entry((item.dir.as_str(), to.as_str())).or_default() += 1;
    }

    let plan = items
        .iter()
        .zip(targets.iter())
        .map(|(item, to)| {
            let status = if *to == item.name {
                Status::Unchanged
            } else if let Err(err) = validate_name(to) {
                Status::Invalid(err)
            } else if wanted[&(item.dir.as_str(), to.as_str())] > 1 {
                Status::Duplicate
            } else if existing
                .get(&item.dir)
                .is_some_and(|names| names.contains(to))
                && !leaving
                    .get(item.dir.as_str())
                    .is_some_and(|names| names.contains(to.as_str()))
            {
                Status::Exists
            } else {
                Status::Renamed
            };
            Planned {
                from: item.name.clone(),
                to: to.clone(),
                status,
            }
        })
        .collect();
    Ok(plan)
}

/// El lote necesita pasar por nombres temporales: algún nombre nuevo es el
/// actual de otro elemento de la misma carpeta (cadenas o intercambios).
pub fn needs_staging(items: &[Item], plan: &[Planned]) -> bool {
    let sources: HashSet<(&str, &str)> = items
        .iter()
        .zip(plan)
        .filter(|(_, p)| p.status == Status::Renamed)
        .map(|(item, _)| (item.dir.as_str(), item.name.as_str()))
        .collect();
    items
        .iter()
        .zip(plan)
        .filter(|(_, p)| p.status == Status::Renamed)
        .any(|(item, p)| sources.contains(&(item.dir.as_str(), p.to.as_str())))
}

/// Nombre temporal (oculto y único) para la primera fase.
pub fn temp_name(index: usize, pid: u32) -> String {
    format!(".tlacuache-renombrando-{pid}-{index}")
}

/// Parte a la que se aplica la regla y la que se conserva al final.
fn split(name: &str, is_dir: bool, include_extension: bool) -> (&str, &str) {
    if include_extension {
        return (name, "");
    }
    let chars = stem_len(name, is_dir);
    let byte = name
        .char_indices()
        .nth(chars)
        .map_or(name.len(), |(byte, _)| byte);
    name.split_at(byte)
}

/// `{n}` → número; `{n:3}` → número con 3 dígitos como mínimo.
fn expand_counter(template: &str, n: usize) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{n") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        if let Some(tail) = after.strip_prefix('}') {
            out.push_str(&n.to_string());
            rest = tail;
        } else if let Some((width, tail)) = after
            .strip_prefix(':')
            .and_then(|s| s.split_once('}'))
            .filter(|(w, _)| !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit()))
        {
            let width: usize = width.parse().unwrap_or(0).min(12);
            out.push_str(&format!("{n:0width$}"));
            rest = tail;
        } else {
            out.push_str("{n");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(names: &[&str]) -> Vec<Item> {
        names
            .iter()
            .map(|name| Item {
                dir: "d".into(),
                name: (*name).into(),
                is_dir: false,
            })
            .collect()
    }

    fn existing(names: &[&str]) -> HashMap<String, HashSet<String>> {
        HashMap::from([(
            "d".to_owned(),
            names.iter().map(|n| (*n).to_owned()).collect(),
        )])
    }

    fn rule(find: &str, replace: &str) -> Rule {
        Rule {
            find: find.into(),
            replace: replace.into(),
            ..Rule::default()
        }
    }

    fn targets(plan: &[Planned]) -> Vec<&str> {
        plan.iter().map(|p| p.to.as_str()).collect()
    }

    #[test]
    fn regex_with_groups_keeps_extension() {
        let list = items(&["IMG_2041.JPG", "IMG_2042.jpg", "notas.txt"]);
        let plan = plan(&list, &existing(&[]), &rule(r"img_(\d+)", "foto-$1")).unwrap();
        assert_eq!(
            targets(&plan),
            ["foto-2041.JPG", "foto-2042.jpg", "notas.txt"]
        );
        assert_eq!(plan[2].status, Status::Unchanged);
        assert_eq!(
            PlanSummary::of(&plan),
            PlanSummary {
                changes: 2,
                conflicts: 0
            }
        );

        let mut sensitive = rule("img", "x");
        sensitive.case_sensitive = true;
        let plan2 = super::plan(&list, &existing(&[]), &sensitive).unwrap();
        assert_eq!(plan2[0].status, Status::Unchanged);
    }

    #[test]
    fn counter_and_whole_name_replacement() {
        let list = items(&["b.jpg", "a.jpg", "c.png"]);
        let plan = plan(&list, &existing(&[]), &rule("", "viaje-{n:3}")).unwrap();
        assert_eq!(
            targets(&plan),
            ["viaje-001.jpg", "viaje-002.jpg", "viaje-003.png"]
        );
        assert_eq!(expand_counter("{n}-{n:2}-{x}-{n:}", 7), "7-07-{x}-{n:}");
        // Sin buscar ni reemplazar: nada cambia.
        let same = super::plan(&list, &existing(&[]), &rule("", "")).unwrap();
        assert!(same.iter().all(|p| p.status == Status::Unchanged));
    }

    #[test]
    fn extension_only_when_asked() {
        let list = items(&["informe.TXT"]);
        let mut with_ext = rule(r"\.txt$", ".md");
        assert_eq!(
            targets(&plan(&list, &existing(&[]), &with_ext).unwrap()),
            ["informe.TXT"]
        );
        with_ext.include_extension = true;
        assert_eq!(
            targets(&plan(&list, &existing(&[]), &with_ext).unwrap()),
            ["informe.md"]
        );
    }

    #[test]
    fn conflicts_are_detected_before_applying() {
        let list = items(&["a1.txt", "a2.txt", "b.txt"]);
        // Dos quedarían iguales.
        let dup = plan(
            &list,
            &existing(&["a1.txt", "a2.txt", "b.txt"]),
            &rule(r"\d", ""),
        )
        .unwrap();
        assert_eq!(dup[0].status, Status::Duplicate);
        assert_eq!(dup[1].status, Status::Duplicate);
        // Ya existe en la carpeta (y no se mueve).
        let exists = plan(
            &items(&["b.txt"]),
            &existing(&["b.txt", "c.txt"]),
            &rule("b", "c"),
        )
        .unwrap();
        assert_eq!(exists[0].status, Status::Exists);
        // Inválido.
        let invalid = plan(&items(&["x.txt"]), &existing(&[]), &rule("x", "a/b")).unwrap();
        assert_eq!(invalid[0].status, Status::Invalid(NameError::ContainsSlash));
        assert!(!PlanSummary::of(&invalid).can_apply());
        // Regex mal formada.
        assert!(plan(&list, &existing(&[]), &rule("(", "")).is_err());
    }

    #[test]
    fn swaps_and_chains_are_allowed_but_need_staging() {
        // a→b, b→a (intercambio): ambos nombres quedan libres en el lote.
        let list = items(&["a.txt", "b.txt"]);
        let swap = plan(
            &list,
            &existing(&["a.txt", "b.txt"]),
            &rule("^(a|b)$", "${1}x"),
        )
        .unwrap();
        assert_eq!(targets(&swap), ["ax.txt", "bx.txt"]);
        assert!(!needs_staging(&list, &swap));

        let chain_list = items(&["1.txt", "2.txt"]);
        let chain = plan(
            &chain_list,
            &existing(&["1.txt", "2.txt"]),
            &rule("", "{n}"),
        )
        .unwrap();
        // 1→1 sin cambio, 2→2 sin cambio.
        assert!(chain.iter().all(|p| p.status == Status::Unchanged));

        let shift = items(&["2.txt", "1.txt"]);
        let shifted = plan(&shift, &existing(&["1.txt", "2.txt"]), &rule("", "{n}")).unwrap();
        assert_eq!(targets(&shifted), ["1.txt", "2.txt"]);
        assert!(shifted.iter().all(|p| p.status == Status::Renamed));
        assert!(needs_staging(&shift, &shifted));
        assert_eq!(temp_name(3, 42), ".tlacuache-renombrando-42-3");
    }

    #[test]
    fn folders_rename_whole_name_and_accents_survive() {
        let list = vec![Item {
            dir: "d".into(),
            name: "Fotos.2026".into(),
            is_dir: true,
        }];
        let plan = plan(&list, &existing(&[]), &rule(r"\.2026", " año 2026")).unwrap();
        assert_eq!(targets(&plan), ["Fotos año 2026"]);
        let accents =
            super::plan(&items(&["canción.mp3"]), &existing(&[]), &rule("ó", "o")).unwrap();
        assert_eq!(targets(&accents), ["cancion.mp3"]);
    }
}
