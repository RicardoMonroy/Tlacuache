//! Modelo de una carpeta: `DirectorySource` (lectura asíncrona por lotes,
//! monitor y resincronización; ADR-017) → `FilterListModel` (core::filter)
//! → `SortListModel` (core::sort). Cada vista envuelve el resultado en su
//! propio modelo de selección.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::filter::Filter;
use tlacuache_core::sort::SortSpec;

use super::directory_source::DirectorySource;
use super::file_item::FileItem;

/// Atributos que se piden a gio por cada entrada.
pub const ATTRIBUTES: &str =
    "standard::*,time::modified,time::created,unix::mode,owner::user,access::can-execute";

#[derive(Clone)]
pub struct DirectoryModel {
    source: DirectorySource,
    filter_state: Rc<RefCell<Filter>>,
    filter: gtk::CustomFilter,
    sort_spec: Rc<Cell<SortSpec>>,
    sorter: gtk::CustomSorter,
    sorted: gtk::SortListModel,
}

impl DirectoryModel {
    pub fn new(dir: &gio::File, show_hidden: bool) -> Self {
        let source = DirectorySource::new(dir);

        let filter_state = Rc::new(RefCell::new(Filter::new(show_hidden, "")));
        let filter = gtk::CustomFilter::new(glib::clone!(
            #[strong]
            filter_state,
            move |obj| {
                obj.downcast_ref::<FileItem>()
                    .is_some_and(|item| filter_state.borrow().matches(&item.entry()))
            }
        ));
        let filtered =
            gtk::FilterListModel::new(Some(source.store().clone()), Some(filter.clone()));
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

        Self {
            source,
            filter_state,
            filter,
            sort_spec,
            sorter,
            sorted,
        }
    }

    /// Entradas visibles, filtradas y ordenadas.
    pub fn model(&self) -> &gtk::SortListModel {
        &self.sorted
    }

    pub fn item(&self, position: u32) -> Option<FileItem> {
        self.sorted.item(position).and_downcast()
    }

    /// Origen de los datos (estado de carga y errores).
    pub fn source(&self) -> &DirectorySource {
        &self.source
    }

    pub fn set_directory(&self, dir: &gio::File) {
        self.source.set_file(dir);
    }

    /// La vista muestra (o dejó de mostrar) la carpeta: las que no avisan
    /// de sus cambios solo se revisan mientras se ven (R.4).
    pub fn set_visible(&self, visible: bool) {
        self.source.set_visible(visible);
    }

    /// Vuelve a leer la carpeta y aplica solo las diferencias (Ctrl+R).
    pub fn reload(&self) {
        self.source.resync();
    }

    /// Posición (en el modelo ordenado y filtrado) de la entrada cuyo archivo
    /// es `file`.
    pub fn position_of(&self, file: &gio::File) -> Option<u32> {
        (0..self.sorted.n_items()).find(|&i| {
            self.item(i)
                .and_then(|item| item.file())
                .is_some_and(|f| f.equal(file))
        })
    }

    /// Cambia el texto del filtro rápido.
    pub fn set_query(&self, query: &str) {
        let old_len = self.filter_state.borrow().query().chars().count();
        let new_len = query.chars().count();
        self.filter_state.borrow_mut().set_query(query);
        // Pistas para que GTK no recalcule todo cuando solo se añade texto.
        let change = if new_len > old_len && old_len > 0 {
            gtk::FilterChange::MoreStrict
        } else if new_len < old_len && new_len > 0 {
            gtk::FilterChange::LessStrict
        } else {
            gtk::FilterChange::Different
        };
        self.filter.changed(change);
    }

    pub fn show_hidden(&self) -> bool {
        self.filter_state.borrow().show_hidden()
    }

    pub fn set_show_hidden(&self, show: bool) {
        self.filter_state.borrow_mut().set_show_hidden(show);
        self.filter.changed(gtk::FilterChange::Different);
    }

    pub fn set_sort(&self, spec: SortSpec) {
        if self.sort_spec.replace(spec) != spec {
            self.sorter.changed(gtk::SorterChange::Different);
        }
    }
}
