//! Modelo de una carpeta: `DirectoryList` → `MapListModel` (FileItem) →
//! `FilterListModel` (core::filter) → `SortListModel` (core::sort) →
//! `MultiSelection`. `DirectoryList` lee de forma asíncrona e incremental, así
//! que la UI no se bloquea mientras llegan las entradas.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::filter::Filter;
use tlacuache_core::sort::SortSpec;

use super::file_item::FileItem;

/// Atributos que se piden a gio por cada entrada.
pub const ATTRIBUTES: &str = "standard::*,time::modified,time::created,unix::mode,owner::user";

pub struct DirectoryModel {
    dir_list: gtk::DirectoryList,
    sort_spec: Rc<Cell<SortSpec>>,
    sorter: gtk::CustomSorter,
    selection: gtk::MultiSelection,
}

impl DirectoryModel {
    pub fn new(dir: &gio::File, show_hidden: bool) -> Self {
        let dir_list = gtk::DirectoryList::new(Some(ATTRIBUTES), Some(dir));
        dir_list.set_monitored(true);

        let items = gtk::MapListModel::new(Some(dir_list.clone()), |obj| {
            match obj.downcast_ref::<gio::FileInfo>() {
                Some(info) => FileItem::from_info(info.clone()).upcast(),
                None => obj.clone(),
            }
        });

        let filter_state = Rc::new(RefCell::new(Filter::new(show_hidden, "")));
        let filter = gtk::CustomFilter::new(glib::clone!(
            #[strong]
            filter_state,
            move |obj| {
                obj.downcast_ref::<FileItem>()
                    .is_some_and(|item| filter_state.borrow().matches(&item.entry()))
            }
        ));
        let filtered = gtk::FilterListModel::new(Some(items), Some(filter));
        // No incremental: con filtrado/orden incremental la vista conserva la
        // fila visible mientras se reacomodan los lotes y termina desplazada
        // (p. ej. en `archivo_6050`). Ordenar 10 000 entradas de una vez
        // cuesta < 50 ms en debug (medido); reevaluar si hace falta más.
        filtered.set_incremental(false);

        let sort_spec = Rc::new(Cell::new(SortSpec::default()));
        let sorter = gtk::CustomSorter::new(glib::clone!(
            #[strong]
            sort_spec,
            move |a, b| match (a.downcast_ref::<FileItem>(), b.downcast_ref::<FileItem>()) {
                (Some(a), Some(b)) => sort_spec.get().compare(&a.entry(), &b.entry()).into(),
                _ => gtk::Ordering::Equal,
            }
        ));
        let sorted = gtk::SortListModel::new(Some(filtered), Some(sorter.clone()));
        sorted.set_incremental(false);

        let selection = gtk::MultiSelection::new(Some(sorted));

        Self {
            dir_list,
            sort_spec,
            sorter,
            selection,
        }
    }

    pub fn selection(&self) -> &gtk::MultiSelection {
        &self.selection
    }

    pub fn directory_list(&self) -> &gtk::DirectoryList {
        &self.dir_list
    }

    pub fn set_sort(&self, spec: SortSpec) {
        if self.sort_spec.replace(spec) != spec {
            self.sorter.changed(gtk::SorterChange::Different);
        }
    }
}
