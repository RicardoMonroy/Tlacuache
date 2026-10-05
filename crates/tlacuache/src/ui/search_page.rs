//! Pestaña de búsqueda recursiva (8.6, Ctrl+F). Busca por nombre desde la
//! carpeta del panel en un hilo de trabajo (`core::search`) y añade los
//! resultados en lotes mientras llegan. Esc o el botón detienen la
//! búsqueda. La selección funciona como en las otras vistas (copiar,
//! mover, papelera, vista previa); Enter abre el archivo, o la carpeta en
//! una pestaña nueva.

use std::cell::{OnceCell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::{gio, glib, pango};
use tlacuache_core::entry::FileEntry;
use tlacuache_core::search::{self, Found, Options, Query, Summary};
use tlacuache_core::summary::{SelectionSummary, ViewStatus};

use crate::fs::display::display_path;
use crate::fs::file_item::{FileItem, SelectedInfo};
use crate::fs::launch;
use crate::strings;
use crate::ui::file_name_cell::FileNameCell;

/// Resultados por lote y espera máxima entre lotes.
const BATCH: usize = 200;
const BATCH_INTERVAL: Duration = Duration::from_millis(100);

enum Message {
    Batch(Vec<(PathBuf, FileEntry)>),
    Done(Summary),
}

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::SearchPage)]
    pub struct SearchPage {
        /// Título de la pestaña («Buscar» o la consulta).
        #[property(get)]
        pub title: RefCell<String>,
        pub root: OnceCell<gio::File>,
        pub show_hidden: std::cell::Cell<bool>,
        pub entry: gtk::SearchEntry,
        pub status: gtk::Label,
        pub spinner: gtk::Spinner,
        pub stop: gtk::Button,
        pub store: OnceCell<gio::ListStore>,
        pub selection: OnceCell<gtk::MultiSelection>,
        pub column_view: OnceCell<gtk::ColumnView>,
        /// Búsqueda en curso (se cancela al buscar de nuevo o con Esc).
        pub cancel: RefCell<Option<Arc<AtomicBool>>>,
        /// Número de la búsqueda actual: los mensajes de una anterior se
        /// ignoran.
        pub generation: std::cell::Cell<u64>,
        /// Señales del panel que la contiene (ver `TabPage`).
        pub pane_handlers: RefCell<Vec<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SearchPage {
        const NAME: &'static str = "TlacuacheSearchPage";
        type Type = super::SearchPage;
        type ParentType = gtk::Box;
    }

    #[glib::derived_properties]
    impl ObjectImpl for SearchPage {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("directory-activated")
                        .param_types([gio::File::static_type()])
                        .build(),
                    Signal::builder("status-changed").build(),
                ]
            })
        }

        fn dispose(&self) {
            self.obj().stop_search();
        }
    }
    impl WidgetImpl for SearchPage {}
    impl BoxImpl for SearchPage {}
}

glib::wrapper! {
    pub struct SearchPage(ObjectSubclass<imp::SearchPage>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl SearchPage {
    /// Búsqueda desde `root` (sus ocultos solo si `show_hidden`).
    pub fn new(root: &gio::File, show_hidden: bool) -> Self {
        let page: Self = glib::Object::new();
        let _ = page.imp().root.set(root.clone());
        page.imp().show_hidden.set(show_hidden);
        page.build();
        page
    }

    pub fn connect_status_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "status-changed",
            false,
            glib::closure_local!(move |page: &Self| f(page)),
        )
    }

    pub fn connect_directory_activated<F: Fn(&Self, &gio::File) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "directory-activated",
            false,
            glib::closure_local!(move |page: &Self, dir: &gio::File| f(page, dir)),
        )
    }

    fn build(&self) {
        let imp = self.imp();
        self.set_orientation(gtk::Orientation::Vertical);
        self.add_css_class("tl-search-page");
        self.set_title_text(strings::SEARCH_TAB);

        let root = imp.root.get().map(display_path).unwrap_or_default();
        imp.entry
            .set_placeholder_text(Some(&strings::search_placeholder(&root)));
        imp.entry.set_hexpand(true);
        imp.entry.connect_activate(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |entry| page.start(&entry.text())
        ));
        imp.entry.connect_stop_search(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_| page.stop_search()
        ));
        imp.stop.set_icon_name("process-stop-symbolic");
        imp.stop.set_tooltip_text(Some(strings::SEARCH_STOP));
        imp.stop.add_css_class("flat");
        imp.stop.set_sensitive(false);
        imp.stop.connect_clicked(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_| page.stop_search()
        ));
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        header.add_css_class("tl-pane-header");
        header.append(&imp.entry);
        header.append(&imp.spinner);
        header.append(&imp.stop);

        let store = gio::ListStore::new::<FileItem>();
        let selection = gtk::MultiSelection::new(Some(store.clone()));
        selection.connect_selection_changed(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_, _, _| page.emit_by_name::<()>("status-changed", &[])
        ));
        let column_view = gtk::ColumnView::new(Some(selection.clone()));
        column_view.add_css_class("data-table");
        column_view.set_enable_rubberband(true);
        let root_path = imp.root.get().and_then(gio::File::path);
        for column in Column::ALL {
            let col = gtk::ColumnViewColumn::new(
                Some(column.title()),
                Some(column.factory(root_path.clone())),
            );
            col.set_resizable(true);
            if matches!(column, Column::Name) {
                col.set_fixed_width(300);
            }
            if matches!(column, Column::Folder) {
                col.set_expand(true);
            }
            column_view.append_column(&col);
        }
        column_view.connect_activate(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_, position| page.activate(position)
        ));
        let scrolled = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&column_view)
            .build();

        imp.status.add_css_class("tl-status-bar");
        imp.status.set_xalign(0.0);
        imp.status.set_text(strings::SEARCH_HINT);

        self.append(&header);
        self.append(&scrolled);
        self.append(&imp.status);

        // Esc sobre los resultados también detiene la búsqueda.
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = page)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                if key == gtk::gdk::Key::Escape && page.imp().cancel.borrow().is_some() {
                    page.stop_search();
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            }
        ));
        self.add_controller(keys);

        let _ = imp.store.set(store);
        let _ = imp.selection.set(selection);
        let _ = imp.column_view.set(column_view);
    }

    fn set_title_text(&self, title: &str) {
        self.imp().title.replace(title.to_owned());
        self.notify_title();
    }

    pub fn set_pane_handlers(&self, handlers: Vec<glib::SignalHandlerId>) {
        self.imp().pane_handlers.replace(handlers);
    }

    pub fn disconnect_pane_handlers(&self) {
        for id in self.imp().pane_handlers.take() {
            self.disconnect(id);
        }
    }

    /// Lleva el foco al campo de búsqueda.
    pub fn focus_entry(&self) {
        self.imp().entry.grab_focus();
    }

    /// Lleva el foco a los resultados (o al campo si no hay).
    pub fn focus_results(&self) {
        let imp = self.imp();
        match imp.column_view.get() {
            Some(view) if imp.store.get().is_some_and(|s| s.n_items() > 0) => {
                view.grab_focus();
            }
            _ => self.focus_entry(),
        }
    }

    /// Detiene la búsqueda en curso (si hay).
    pub fn stop_search(&self) {
        if let Some(cancel) = self.imp().cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    fn start(&self, text: &str) {
        self.stop_search();
        let imp = self.imp();
        let (Some(query), Some(root), Some(store)) = (
            Query::new(text),
            imp.root.get().and_then(gio::File::path),
            imp.store.get(),
        ) else {
            return;
        };
        store.remove_all();
        self.set_title_text(&strings::search_title(text.trim()));
        imp.status.set_text(strings::SEARCHING);
        imp.spinner.start();
        imp.stop.set_sensitive(true);

        let cancel = Arc::new(AtomicBool::new(false));
        imp.cancel.replace(Some(cancel.clone()));
        let generation = imp.generation.get() + 1;
        imp.generation.set(generation);
        let options = Options {
            show_hidden: imp.show_hidden.get(),
            ..Options::default()
        };
        let (tx, rx) = async_channel::unbounded();
        std::thread::spawn(move || worker(&root, &query, options, &cancel, &tx));

        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = page)]
            self,
            async move {
                while let Ok(message) = rx.recv().await {
                    if page.imp().generation.get() != generation {
                        break;
                    }
                    match message {
                        Message::Batch(batch) => page.add_results(batch),
                        Message::Done(summary) => page.finish(summary),
                    }
                }
            }
        ));
    }

    fn add_results(&self, batch: Vec<(PathBuf, FileEntry)>) {
        let Some(store) = self.imp().store.get() else {
            return;
        };
        let items: Vec<FileItem> = batch
            .into_iter()
            .map(|(path, entry)| FileItem::from_entry(entry, gio::File::for_path(path)))
            .collect();
        store.extend_from_slice(&items);
        self.imp()
            .status
            .set_text(&strings::search_progress(store.n_items()));
        self.emit_by_name::<()>("status-changed", &[]);
    }

    fn finish(&self, summary: Summary) {
        let imp = self.imp();
        imp.spinner.stop();
        imp.stop.set_sensitive(false);
        imp.status.set_text(&strings::search_done(
            summary.matches,
            summary.cancelled,
            summary.truncated,
        ));
        imp.cancel.replace(None);
    }

    fn selected_items(&self) -> Vec<FileItem> {
        let imp = self.imp();
        let (Some(store), Some(selection)) = (imp.store.get(), imp.selection.get()) else {
            return Vec::new();
        };
        let bitset = selection.selection();
        gtk::BitsetIter::init_first(&bitset)
            .map(|(iter, first)| std::iter::once(first).chain(iter))
            .into_iter()
            .flatten()
            .filter_map(|position| store.item(position).and_downcast::<FileItem>())
            .collect()
    }

    pub fn selected_infos(&self) -> Vec<SelectedInfo> {
        self.selected_items()
            .iter()
            .filter_map(SelectedInfo::from_item)
            .collect()
    }

    pub fn selected_files(&self) -> Vec<gio::File> {
        self.selected_items()
            .iter()
            .filter_map(FileItem::file)
            .collect()
    }

    pub fn status(&self) -> ViewStatus {
        let items = self.selected_items();
        let entries: Vec<_> = items.iter().map(FileItem::entry).collect();
        ViewStatus {
            items: self.imp().store.get().map_or(0, gio::ListStore::n_items),
            selection: SelectionSummary::from_entries(entries.iter().map(|e| &**e)),
        }
    }

    fn activate(&self, position: u32) {
        let Some(item) = self
            .imp()
            .store
            .get()
            .and_then(|s| s.item(position))
            .and_downcast::<FileItem>()
        else {
            return;
        };
        let Some(file) = item.file() else {
            return;
        };
        if item.entry().is_dir {
            self.emit_by_name::<()>("directory-activated", &[&file]);
        } else {
            launch::open_file(self, file, item.entry().name.clone());
        }
    }
}

/// Hilo de trabajo: recorre y manda lotes; al final, el resumen.
fn worker(
    root: &Path,
    query: &Query,
    options: Options,
    cancel: &AtomicBool,
    tx: &async_channel::Sender<Message>,
) {
    let mut batch = Vec::new();
    let mut last = Instant::now();
    let summary = search::walk(root, query, options, cancel, |found| {
        batch.push((found.path.clone(), entry_for(&found)));
        if batch.len() >= BATCH || last.elapsed() >= BATCH_INTERVAL {
            let _ = tx.send_blocking(Message::Batch(std::mem::take(&mut batch)));
            last = Instant::now();
        }
    });
    if !batch.is_empty() {
        let _ = tx.send_blocking(Message::Batch(batch));
    }
    let _ = tx.send_blocking(Message::Done(summary));
}

/// Datos de un resultado (en el hilo de trabajo, sin GTK).
fn entry_for(found: &Found) -> FileEntry {
    use std::os::unix::fs::PermissionsExt;

    let name = found
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut entry = FileEntry::new(name.clone(), found.is_dir);
    entry.is_symlink = std::fs::symlink_metadata(&found.path).is_ok_and(|m| m.is_symlink());
    if let Ok(meta) = std::fs::metadata(&found.path) {
        entry.is_dir = meta.is_dir();
        entry.size = if meta.is_dir() { 0 } else { meta.len() };
        entry.modified = meta.modified().ok();
        entry.created = meta.created().ok();
        entry.is_executable = !meta.is_dir() && meta.permissions().mode() & 0o111 != 0;
    }
    entry.content_type = Some(if entry.is_dir {
        "inode/directory".to_owned()
    } else {
        gio::content_type_guess(Some(name.as_str()), None)
            .0
            .to_string()
    });
    entry
}

#[derive(Clone, Copy)]
enum Column {
    Name,
    Folder,
    Size,
    Modified,
}

impl Column {
    const ALL: [Self; 4] = [Self::Name, Self::Folder, Self::Size, Self::Modified];

    fn title(self) -> &'static str {
        match self {
            Self::Name => strings::COLUMN_NAME,
            Self::Folder => strings::COLUMN_FOLDER,
            Self::Size => strings::COLUMN_SIZE,
            Self::Modified => strings::COLUMN_MODIFIED,
        }
    }

    /// `root`: para mostrar la carpeta relativa a donde se buscó.
    fn factory(self, root: Option<PathBuf>) -> gtk::SignalListItemFactory {
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(move |_, obj| {
            let Some(list_item) = obj.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let widget: gtk::Widget = match self {
                Self::Name => FileNameCell::new(pango::EllipsizeMode::Middle).upcast(),
                Self::Folder => {
                    let label = gtk::Label::builder()
                        .xalign(0.0)
                        .ellipsize(pango::EllipsizeMode::Start)
                        .build();
                    label.add_css_class("tl-date");
                    label.upcast()
                }
                Self::Size => {
                    let label = gtk::Label::builder().xalign(1.0).build();
                    label.add_css_class("numeric");
                    label.add_css_class("tl-size");
                    label.upcast()
                }
                Self::Modified => {
                    let label = gtk::Label::builder().xalign(0.0).build();
                    label.add_css_class("numeric");
                    label.add_css_class("tl-date");
                    label.upcast()
                }
            };
            list_item.set_child(Some(&widget));
        });
        factory.connect_bind(move |_, obj| {
            let Some(list_item) = obj.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let (Some(item), Some(child)) = (
                list_item.item().and_downcast::<FileItem>(),
                list_item.child(),
            ) else {
                return;
            };
            let entry = item.entry();
            match self {
                Self::Name => {
                    if let Some(cell) = child.downcast_ref::<FileNameCell>() {
                        cell.set_entry(&entry, item.file().map(|f| f.uri().to_string()));
                    }
                }
                Self::Folder => {
                    let folder = item
                        .file()
                        .and_then(|f| f.parent())
                        .and_then(|p| p.path())
                        .map(|p| relative_folder(&p, root.as_deref()))
                        .unwrap_or_default();
                    set_text(&child, &folder);
                }
                Self::Size => {
                    let text = if entry.is_dir {
                        String::new()
                    } else {
                        glib::format_size(entry.size).to_string()
                    };
                    set_text(&child, &text);
                }
                Self::Modified => set_text(&child, &date_text(entry.modified)),
            }
        });
        factory
    }
}

/// Carpeta de un resultado relativa a la raíz de la búsqueda («.» = raíz).
fn relative_folder(folder: &Path, root: Option<&Path>) -> String {
    match root.and_then(|r| folder.strip_prefix(r).ok()) {
        Some(rel) if rel.as_os_str().is_empty() => ".".to_owned(),
        Some(rel) => rel.display().to_string(),
        None => folder.display().to_string(),
    }
}

fn set_text(widget: &gtk::Widget, text: &str) {
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        label.set_text(text);
    }
}

fn date_text(time: Option<SystemTime>) -> String {
    time.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .and_then(|secs| glib::DateTime::from_unix_local(secs).ok())
        .and_then(|dt| dt.format("%Y-%m-%d %H:%M").ok())
        .map(|text| text.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_relative_to_search_root() {
        let root = Path::new("/home/ana/dev");
        assert_eq!(relative_folder(Path::new("/home/ana/dev"), Some(root)), ".");
        assert_eq!(
            relative_folder(Path::new("/home/ana/dev/src/ui"), Some(root)),
            "src/ui"
        );
        assert_eq!(relative_folder(Path::new("/otra"), Some(root)), "/otra");
        assert_eq!(relative_folder(Path::new("/otra"), None), "/otra");
    }

    #[test]
    fn worker_streams_batches_and_a_summary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("a/b/nota uno.txt"), b"hola").unwrap();
        std::fs::write(dir.path().join("a/otra nota.md"), b"").unwrap();
        let (tx, rx) = async_channel::unbounded();
        worker(
            dir.path(),
            &Query::new("nota").unwrap(),
            Options::default(),
            &AtomicBool::new(false),
            &tx,
        );
        drop(tx);
        let mut results = Vec::new();
        let mut summary = None;
        while let Ok(message) = rx.recv_blocking() {
            match message {
                Message::Batch(batch) => results.extend(batch),
                Message::Done(done) => summary = Some(done),
            }
        }
        assert_eq!(summary.map(|s| s.matches), Some(2));
        let txt = results
            .iter()
            .find(|(p, _)| p.ends_with("nota uno.txt"))
            .unwrap();
        assert_eq!(txt.1.size, 4);
        assert_eq!(txt.1.content_type.as_deref(), Some("text/plain"));
        assert!(txt.1.modified.is_some());
    }
}
