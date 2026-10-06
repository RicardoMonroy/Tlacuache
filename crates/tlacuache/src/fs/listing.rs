//! Modelo de una carpeta: `DirectoryList` → `MapListModel` (FileItem) →
//! `FilterListModel` (core::filter) → `SortListModel` (core::sort). Cada
//! vista envuelve el resultado en su propio modelo de selección.
//! `DirectoryList` lee de forma asíncrona e incremental, así que la UI no se
//! bloquea mientras llegan las entradas.
//!
//! Diagnóstico (R.1): con `RUST_LOG=tlacuache=debug` se registra cada evento
//! del monitor de la carpeta y cada cambio que `DirectoryList` aplica al
//! modelo, para comparar lo que avisa el sistema con lo que se muestra.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::filter::Filter;
use tlacuache_core::sort::SortSpec;

use super::file_item::FileItem;

/// Atributos que se piden a gio por cada entrada.
pub const ATTRIBUTES: &str =
    "standard::*,time::modified,time::created,unix::mode,owner::user,access::can-execute";

#[derive(Clone)]
pub struct DirectoryModel {
    dir_list: gtk::DirectoryList,
    filter_state: Rc<RefCell<Filter>>,
    filter: gtk::CustomFilter,
    sort_spec: Rc<Cell<SortSpec>>,
    sorter: gtk::CustomSorter,
    sorted: gtk::SortListModel,
    /// Monitor solo para el registro de diagnóstico (R.1).
    debug_monitor: Rc<RefCell<Option<gio::FileMonitor>>>,
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
        let filtered = gtk::FilterListModel::new(Some(items), Some(filter.clone()));
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

        let model = Self {
            dir_list,
            filter_state,
            filter,
            sort_spec,
            sorter,
            sorted,
            debug_monitor: Rc::default(),
        };
        if tracing::enabled!(tracing::Level::DEBUG) {
            model.log_list_changes();
            model.watch_for_debug(dir);
        }
        model
    }

    /// Registra los cambios que `DirectoryList` aplica (lo que la vista ve).
    fn log_list_changes(&self) {
        self.dir_list
            .connect_items_changed(|list, position, removed, added| {
                if list.is_loading() {
                    return; // carga inicial: no es un cambio de la carpeta
                }
                let names: Vec<String> = (position..position + added)
                    .filter_map(|i| list.item(i).and_downcast::<gio::FileInfo>())
                    .map(|info| info.name().to_string_lossy().into_owned())
                    .collect();
                let dir = list.file().map(|d| display_dir(&d)).unwrap_or_default();
                tracing::debug!("modelo {dir}: posición {position}, -{removed} +{added} {names:?}");
            });
    }

    /// Monitor propio de `dir` que solo registra sus eventos.
    fn watch_for_debug(&self, dir: &gio::File) {
        let monitor = match dir.monitor_directory(
            gio::FileMonitorFlags::WATCH_MOVES,
            None::<&gio::Cancellable>,
        ) {
            Ok(monitor) => monitor,
            Err(err) => {
                tracing::debug!("sin monitor para {}: {err}", dir.uri());
                self.debug_monitor.replace(None);
                return;
            }
        };
        let label = display_dir(dir);
        tracing::debug!("monitor {label}: vigilando");
        monitor.connect_changed(move |_, file, other, event| {
            let name = |f: &gio::File| {
                f.basename()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| f.uri().to_string())
            };
            match other {
                Some(other) => {
                    tracing::debug!(
                        "monitor {label}: {event:?} {} → {}",
                        name(file),
                        name(other)
                    )
                }
                None => tracing::debug!("monitor {label}: {event:?} {}", name(file)),
            }
        });
        // Deja de vigilar la carpeta anterior.
        if let Some(old) = self.debug_monitor.replace(Some(monitor)) {
            old.cancel();
        }
    }

    /// Entradas visibles, filtradas y ordenadas.
    pub fn model(&self) -> &gtk::SortListModel {
        &self.sorted
    }

    pub fn item(&self, position: u32) -> Option<FileItem> {
        self.sorted.item(position).and_downcast()
    }

    pub fn directory_list(&self) -> &gtk::DirectoryList {
        &self.dir_list
    }

    pub fn set_directory(&self, dir: &gio::File) {
        self.dir_list.set_file(Some(dir));
        if tracing::enabled!(tracing::Level::DEBUG) {
            self.watch_for_debug(dir);
        }
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

/// Ruta de `dir` para el registro (o su URI si no es local).
fn display_dir(dir: &gio::File) -> String {
    dir.path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| dir.uri().to_string())
}
