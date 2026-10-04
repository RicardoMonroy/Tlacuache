//! Ejecuta la cola de operaciones (`tlacuache_core::ops`) con gio async.
//!
//! - Copiar: gio no copia directorios de forma recursiva, así que primero se
//!   recorre el origen (asíncrono) para conocer el total de bytes y luego se
//!   copia archivo por archivo creando las carpetas.
//! - Mover: `move_async` (instantáneo en el mismo sistema de archivos); si
//!   gio no puede mover una carpeta directamente (otro disco), se copia y
//!   después se borra el origen.
//! - Papelera: `trash_async` por elemento. Borrar (permanente): recorrido y
//!   borrado de dentro hacia fuera.
//! - Cancelar: cada operación tiene su `gio::Cancellable`; el archivo a
//!   medias se borra. Lo ya copiado se conserva.
//!
//! Conflictos (el destino ya existe) fallan con un mensaje; el diálogo de
//! Reemplazar/Omitir/Renombrar llega en la tarea 4.6.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use glib::subclass::Signal;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::ops::{OpId, OpKind, OpQueue, OpState, Operation, Progress};

use crate::strings;

/// Intervalo mínimo entre avisos de progreso a la UI.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const ATTRS: &str = "standard::name,standard::type,standard::size";
const COPY_FLAGS: gio::FileCopyFlags = gio::FileCopyFlags::NOFOLLOW_SYMLINKS;

type Report = Rc<dyn Fn(&Progress)>;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct OpsManager {
        pub queue: RefCell<OpQueue>,
        pub cancellables: RefCell<HashMap<OpId, gio::Cancellable>>,
        pub last_emit: Cell<Option<Instant>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for OpsManager {
        const NAME: &'static str = "TlacuacheOpsManager";
        type Type = super::OpsManager;
    }

    impl ObjectImpl for OpsManager {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // Cambió el estado o el progreso de alguna operación.
                    Signal::builder("changed").build(),
                    // Terminó una operación (con éxito, error o cancelada).
                    Signal::builder("finished")
                        .param_types([u64::static_type()])
                        .build(),
                ]
            })
        }
    }
}

glib::wrapper! {
    pub struct OpsManager(ObjectSubclass<imp::OpsManager>);
}

impl Default for OpsManager {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl OpsManager {
    pub fn connect_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_closure(
            "changed",
            false,
            glib::closure_local!(move |manager: &Self| f(manager)),
        );
    }

    pub fn connect_finished<F: Fn(&Self, OpId) + 'static>(&self, f: F) {
        self.connect_closure(
            "finished",
            false,
            glib::closure_local!(move |manager: &Self, id: u64| f(manager, id)),
        );
    }

    /// Encola una operación y la arranca si hay hueco.
    pub fn enqueue(&self, kind: OpKind, sources: &[gio::File], dest: Option<&gio::File>) -> OpId {
        let uris = sources.iter().map(|f| f.uri().to_string()).collect();
        let dest = dest.map(|d| d.uri().to_string());
        let id = self.imp().queue.borrow_mut().enqueue(kind, uris, dest);
        self.emit_changed();
        self.pump();
        id
    }

    pub fn cancel(&self, id: OpId) {
        let result = self.imp().queue.borrow_mut().cancel(id);
        match result {
            Ok(true) => {
                if let Some(cancellable) = self.imp().cancellables.borrow().get(&id) {
                    cancellable.cancel();
                }
            }
            Ok(false) => {}
            Err(err) => tracing::warn!("{err}"),
        }
        self.emit_changed();
    }

    pub fn operation(&self, id: OpId) -> Option<Operation> {
        self.imp().queue.borrow().get(id).cloned()
    }

    /// Copia de todas las operaciones (en curso, en cola y terminadas).
    pub fn operations(&self) -> Vec<Operation> {
        self.imp().queue.borrow().iter().cloned().collect()
    }

    fn emit_changed(&self) {
        self.imp().last_emit.set(Some(Instant::now()));
        self.emit_by_name::<()>("changed", &[]);
    }

    fn pump(&self) {
        let ids = self.imp().queue.borrow_mut().start_next();
        for id in ids {
            self.run(id);
        }
    }

    fn run(&self, id: OpId) {
        let Some(op) = self.operation(id) else {
            return;
        };
        let cancellable = gio::Cancellable::new();
        self.imp()
            .cancellables
            .borrow_mut()
            .insert(id, cancellable.clone());
        self.emit_changed();

        let report: Report = Rc::new(glib::clone!(
            #[weak(rename_to = manager)]
            self,
            move |progress: &Progress| manager.report_progress(id, progress)
        ));
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = manager)]
            self,
            async move {
                let sources: Vec<gio::File> =
                    op.sources.iter().map(|u| gio::File::for_uri(u)).collect();
                let dest = op.dest.as_deref().map(gio::File::for_uri);
                tracing::debug!("operación {id} ({:?}) iniciada", op.kind);
                let result = match (op.kind, dest) {
                    (OpKind::Copy, Some(dest)) => {
                        copy_tree(&sources, &dest, &cancellable, &report).await
                    }
                    (OpKind::Move, Some(dest)) => {
                        move_tree(&sources, &dest, &cancellable, &report).await
                    }
                    (OpKind::Trash, _) => trash_all(&sources, &cancellable, &report).await,
                    (OpKind::Delete, _) => delete_all(&sources, &cancellable, &report).await,
                    _ => Err(glib::Error::new(
                        gio::IOErrorEnum::NotSupported,
                        strings::OP_NOT_SUPPORTED,
                    )),
                };
                manager.finish(id, result);
            }
        ));
    }

    fn report_progress(&self, id: OpId, progress: &Progress) {
        if let Err(err) = self
            .imp()
            .queue
            .borrow_mut()
            .update_progress(id, progress.clone())
        {
            tracing::debug!("{err}");
            return;
        }
        let due = self
            .imp()
            .last_emit
            .get()
            .is_none_or(|last| last.elapsed() >= PROGRESS_INTERVAL);
        if due {
            self.emit_changed();
        }
    }

    fn finish(&self, id: OpId, result: Result<(), glib::Error>) {
        self.imp().cancellables.borrow_mut().remove(&id);
        {
            let mut queue = self.imp().queue.borrow_mut();
            let outcome = match result {
                Ok(()) => queue.complete(id),
                Err(err) if err.matches(gio::IOErrorEnum::Cancelled) => {
                    // Si se canceló sin pasar por `cancel` (no debería), se
                    // registra igual como cancelada.
                    if queue.get(id).is_some_and(|op| op.state == OpState::Running) {
                        let _ = queue.cancel(id);
                    }
                    queue.cancelled(id)
                }
                Err(err) => {
                    tracing::warn!("operación {id} fallida: {err}");
                    queue.fail(id, err.message())
                }
            };
            if let Err(err) = outcome {
                tracing::warn!("{err}");
            }
        }
        self.emit_changed();
        self.emit_by_name::<()>("finished", &[&id]);
        self.pump();
    }
}

fn error(kind: gio::IOErrorEnum, message: &str) -> glib::Error {
    glib::Error::new(kind, message)
}

fn check_cancelled(cancellable: &gio::Cancellable) -> Result<(), glib::Error> {
    if cancellable.is_cancelled() {
        Err(error(gio::IOErrorEnum::Cancelled, strings::OP_CANCELLED))
    } else {
        Ok(())
    }
}

/// Espera el resultado de una función gio con callback.
async fn wait(rx: async_channel::Receiver<Result<(), glib::Error>>) -> Result<(), glib::Error> {
    rx.recv()
        .await
        .unwrap_or_else(|_| Err(error(gio::IOErrorEnum::Failed, strings::OP_INTERRUPTED)))
}

async fn copy_file(
    src: &gio::File,
    dst: &gio::File,
    cancellable: &gio::Cancellable,
    progress: Box<dyn FnMut(i64, i64)>,
) -> Result<(), glib::Error> {
    let (tx, rx) = async_channel::bounded(1);
    src.copy_async(
        dst,
        COPY_FLAGS,
        glib::Priority::DEFAULT,
        Some(cancellable),
        Some(progress),
        move |result| {
            let _ = tx.try_send(result);
        },
    );
    wait(rx).await
}

async fn move_file(
    src: &gio::File,
    dst: &gio::File,
    cancellable: &gio::Cancellable,
    progress: Box<dyn FnMut(i64, i64)>,
) -> Result<(), glib::Error> {
    let (tx, rx) = async_channel::bounded(1);
    src.move_async(
        dst,
        COPY_FLAGS,
        glib::Priority::DEFAULT,
        Some(cancellable),
        Some(progress),
        move |result| {
            let _ = tx.try_send(result);
        },
    );
    wait(rx).await
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EntryKind {
    Dir,
    /// Archivo o enlace simbólico (se copia el enlace, no su destino).
    Other,
}

struct Entry {
    file: gio::File,
    kind: EntryKind,
    size: u64,
}

/// Recorre `root` sin seguir enlaces. Cada carpeta aparece antes que su
/// contenido.
async fn scan(root: &gio::File, cancellable: &gio::Cancellable) -> Result<Vec<Entry>, glib::Error> {
    let flags = gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS;
    let priority = glib::Priority::DEFAULT;
    let info = root.query_info_future(ATTRS, flags, priority).await?;
    let size = |info: &gio::FileInfo| u64::try_from(info.size()).unwrap_or(0);
    if info.file_type() != gio::FileType::Directory {
        return Ok(vec![Entry {
            file: root.clone(),
            kind: EntryKind::Other,
            size: size(&info),
        }]);
    }

    let mut entries = vec![Entry {
        file: root.clone(),
        kind: EntryKind::Dir,
        size: 0,
    }];
    let mut pending = vec![root.clone()];
    while let Some(dir) = pending.pop() {
        check_cancelled(cancellable)?;
        let enumerator = dir
            .enumerate_children_future(ATTRS, flags, priority)
            .await?;
        loop {
            let batch = enumerator.next_files_future(256, priority).await?;
            if batch.is_empty() {
                break;
            }
            for info in batch {
                let child = dir.child(info.name());
                if info.file_type() == gio::FileType::Directory {
                    pending.push(child.clone());
                    entries.push(Entry {
                        file: child,
                        kind: EntryKind::Dir,
                        size: 0,
                    });
                } else {
                    entries.push(Entry {
                        size: size(&info),
                        file: child,
                        kind: EntryKind::Other,
                    });
                }
            }
        }
    }
    Ok(entries)
}

/// Destino de `src` dentro de `dest_dir`, comprobando que no se copie una
/// carpeta dentro de sí misma y que el destino no exista.
async fn destination_for(src: &gio::File, dest_dir: &gio::File) -> Result<gio::File, glib::Error> {
    let name = src
        .basename()
        .ok_or_else(|| error(gio::IOErrorEnum::InvalidFilename, strings::OP_NO_NAME))?;
    if dest_dir.equal(src) || dest_dir.has_prefix(src) {
        return Err(error(
            gio::IOErrorEnum::WouldRecurse,
            strings::OP_INTO_ITSELF,
        ));
    }
    let dst = dest_dir.child(&name);
    let exists = dst
        .query_info_future(
            gio::FILE_ATTRIBUTE_STANDARD_TYPE,
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            glib::Priority::DEFAULT,
        )
        .await;
    match exists {
        Ok(_) => Err(error(
            gio::IOErrorEnum::Exists,
            &strings::op_exists(&name.to_string_lossy()),
        )),
        Err(err) if err.matches(gio::IOErrorEnum::NotFound) => Ok(dst),
        Err(err) => Err(err),
    }
}

/// Copia `sources` (archivos o carpetas) dentro de `dest_dir`.
async fn copy_tree(
    sources: &[gio::File],
    dest_dir: &gio::File,
    cancellable: &gio::Cancellable,
    report: &Report,
) -> Result<(), glib::Error> {
    // 1. Planificar: destinos válidos y totales.
    let mut plans = Vec::new();
    for src in sources {
        check_cancelled(cancellable)?;
        let dst = destination_for(src, dest_dir).await?;
        let entries = scan(src, cancellable).await?;
        plans.push((src.clone(), dst, entries));
    }
    let files = plans
        .iter()
        .flat_map(|(_, _, e)| e)
        .filter(|e| e.kind == EntryKind::Other);
    let mut progress = Progress {
        bytes_total: Some(files.clone().map(|e| e.size).sum()),
        files_total: Some(files.count() as u64),
        ..Progress::default()
    };
    report(&progress);

    // 2. Copiar.
    for (src_root, dst_root, entries) in plans {
        for entry in entries {
            check_cancelled(cancellable)?;
            let dst = match src_root.relative_path(&entry.file) {
                Some(relative) => dst_root.resolve_relative_path(relative),
                None => dst_root.clone(),
            };
            match entry.kind {
                EntryKind::Dir => dst.make_directory_future(glib::Priority::DEFAULT).await?,
                EntryKind::Other => {
                    progress.current = entry
                        .file
                        .basename()
                        .map(|n| n.to_string_lossy().into_owned());
                    let base = progress.bytes_done;
                    let snapshot = progress.clone();
                    let report_file = report.clone();
                    let on_progress = Box::new(move |current: i64, _total: i64| {
                        let mut partial = snapshot.clone();
                        partial.bytes_done = base + u64::try_from(current).unwrap_or(0);
                        report_file(&partial);
                    });
                    if let Err(err) = copy_file(&entry.file, &dst, cancellable, on_progress).await {
                        // Sin archivos a medias en el destino.
                        let _ = dst.delete_future(glib::Priority::DEFAULT).await;
                        return Err(err);
                    }
                    progress.bytes_done = base + entry.size;
                    progress.files_done += 1;
                    report(&progress);
                }
            }
        }
    }
    Ok(())
}

/// Borra `root` y su contenido (de dentro hacia fuera).
async fn delete_tree(root: &gio::File, cancellable: &gio::Cancellable) -> Result<(), glib::Error> {
    let entries = scan(root, cancellable).await?;
    for entry in entries.iter().rev() {
        check_cancelled(cancellable)?;
        entry.file.delete_future(glib::Priority::DEFAULT).await?;
    }
    Ok(())
}

async fn trash_file(file: &gio::File, cancellable: &gio::Cancellable) -> Result<(), glib::Error> {
    let (tx, rx) = async_channel::bounded(1);
    file.trash_async(glib::Priority::DEFAULT, Some(cancellable), move |result| {
        let _ = tx.try_send(result);
    });
    wait(rx).await
}

/// Envía `sources` a la papelera (gio mueve carpetas enteras).
async fn trash_all(
    sources: &[gio::File],
    cancellable: &gio::Cancellable,
    report: &Report,
) -> Result<(), glib::Error> {
    let mut progress = Progress {
        files_total: Some(sources.len() as u64),
        ..Progress::default()
    };
    report(&progress);
    for file in sources {
        check_cancelled(cancellable)?;
        progress.current = file.basename().map(|n| n.to_string_lossy().into_owned());
        report(&progress);
        trash_file(file, cancellable).await?;
        progress.files_done += 1;
        report(&progress);
    }
    Ok(())
}

/// Borra `sources` de forma permanente (carpetas con su contenido).
async fn delete_all(
    sources: &[gio::File],
    cancellable: &gio::Cancellable,
    report: &Report,
) -> Result<(), glib::Error> {
    let mut progress = Progress {
        files_total: Some(sources.len() as u64),
        ..Progress::default()
    };
    report(&progress);
    for file in sources {
        check_cancelled(cancellable)?;
        progress.current = file.basename().map(|n| n.to_string_lossy().into_owned());
        report(&progress);
        delete_tree(file, cancellable).await?;
        progress.files_done += 1;
        report(&progress);
    }
    Ok(())
}

/// Mueve `sources` dentro de `dest_dir`.
async fn move_tree(
    sources: &[gio::File],
    dest_dir: &gio::File,
    cancellable: &gio::Cancellable,
    report: &Report,
) -> Result<(), glib::Error> {
    let mut progress = Progress {
        files_total: Some(sources.len() as u64),
        ..Progress::default()
    };
    report(&progress);
    for src in sources {
        check_cancelled(cancellable)?;
        let dst = destination_for(src, dest_dir).await?;
        progress.current = src.basename().map(|n| n.to_string_lossy().into_owned());
        let snapshot = progress.clone();
        let report_file = report.clone();
        let on_progress = Box::new(move |current: i64, total: i64| {
            let mut partial = snapshot.clone();
            partial.bytes_done = u64::try_from(current).unwrap_or(0);
            partial.bytes_total = u64::try_from(total).ok().filter(|t| *t > 0);
            report_file(&partial);
        });
        match move_file(src, &dst, cancellable, on_progress).await {
            Ok(()) => {}
            // Carpeta a otro sistema de archivos: copiar y borrar el origen.
            Err(err)
                if err.matches(gio::IOErrorEnum::WouldRecurse)
                    || err.matches(gio::IOErrorEnum::NotSupported) =>
            {
                copy_tree(std::slice::from_ref(src), dest_dir, cancellable, report).await?;
                delete_tree(src, cancellable).await?;
            }
            Err(err) => return Err(err),
        }
        progress.files_done += 1;
        progress.bytes_total = None;
        progress.bytes_done = 0;
        report(&progress);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::future::Future;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    use super::*;

    /// Ejecuta `future` en un contexto glib propio (gio despacha ahí los
    /// callbacks de las operaciones asíncronas).
    fn block_on<F: Future>(future: F) -> F::Output {
        let context = glib::MainContext::new();
        context
            .with_thread_default(|| context.block_on(future))
            .expect("contexto glib del hilo de prueba")
    }

    fn file(path: &Path) -> gio::File {
        gio::File::for_path(path)
    }

    fn no_report() -> Report {
        Rc::new(|_: &Progress| {})
    }

    #[test]
    fn copies_a_tree_with_symlinks_and_reports_totals() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("origen");
        fs::create_dir_all(src.join("sub/vacía")).unwrap();
        fs::write(src.join("a.txt"), "hola").unwrap();
        fs::write(src.join("sub/b.bin"), vec![7u8; 1000]).unwrap();
        symlink("a.txt", src.join("enlace")).unwrap();
        let dest = tmp.path().join("destino");
        fs::create_dir(&dest).unwrap();

        let last = Rc::new(RefCell::new(Progress::default()));
        let sink = last.clone();
        let report: Report = Rc::new(move |p: &Progress| *sink.borrow_mut() = p.clone());
        block_on(copy_tree(
            &[file(&src)],
            &file(&dest),
            &gio::Cancellable::new(),
            &report,
        ))
        .unwrap();

        let copied = dest.join("origen");
        assert_eq!(fs::read_to_string(copied.join("a.txt")).unwrap(), "hola");
        assert_eq!(fs::read(copied.join("sub/b.bin")).unwrap().len(), 1000);
        assert!(copied.join("sub/vacía").is_dir());
        assert_eq!(
            fs::read_link(copied.join("enlace")).unwrap(),
            Path::new("a.txt")
        );

        let last = last.borrow();
        assert_eq!(last.files_total, Some(3));
        assert_eq!(last.files_done, 3);
        assert_eq!(last.bytes_done, last.bytes_total.unwrap());
        assert_eq!(last.fraction(), Some(1.0));
    }

    #[test]
    fn refuses_existing_destination_and_copy_into_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("carpeta");
        fs::create_dir(&src).unwrap();
        fs::write(tmp.path().join("x.txt"), "1").unwrap();
        let dest = tmp.path().join("otra");
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("x.txt"), "ya estaba").unwrap();
        let c = gio::Cancellable::new();

        let err = block_on(copy_tree(
            &[file(&tmp.path().join("x.txt"))],
            &file(&dest),
            &c,
            &no_report(),
        ))
        .unwrap_err();
        assert!(err.matches(gio::IOErrorEnum::Exists));
        assert_eq!(fs::read_to_string(dest.join("x.txt")).unwrap(), "ya estaba");

        let err = block_on(copy_tree(
            &[file(&src)],
            &file(&src.join("dentro")),
            &c,
            &no_report(),
        ))
        .unwrap_err();
        assert!(err.matches(gio::IOErrorEnum::WouldRecurse));
    }

    #[test]
    fn cancelled_copy_leaves_nothing_half_done() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("grande.bin");
        fs::write(&src, vec![1u8; 64 * 1024]).unwrap();
        let dest = tmp.path().join("destino");
        fs::create_dir(&dest).unwrap();
        let cancellable = gio::Cancellable::new();
        cancellable.cancel();

        let err = block_on(copy_tree(
            &[file(&src)],
            &file(&dest),
            &cancellable,
            &no_report(),
        ))
        .unwrap_err();
        assert!(err.matches(gio::IOErrorEnum::Cancelled));
        assert!(!dest.join("grande.bin").exists());
    }

    #[test]
    fn moves_files_and_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let src_file = tmp.path().join("nota.md");
        let src_dir = tmp.path().join("proyecto");
        fs::write(&src_file, "x").unwrap();
        fs::create_dir_all(src_dir.join("src")).unwrap();
        fs::write(src_dir.join("src/main.rs"), "fn main() {}").unwrap();
        let dest = tmp.path().join("destino");
        fs::create_dir(&dest).unwrap();

        block_on(move_tree(
            &[file(&src_file), file(&src_dir)],
            &file(&dest),
            &gio::Cancellable::new(),
            &no_report(),
        ))
        .unwrap();

        assert!(!src_file.exists() && !src_dir.exists());
        assert_eq!(fs::read_to_string(dest.join("nota.md")).unwrap(), "x");
        assert!(dest.join("proyecto/src/main.rs").is_file());
    }

    /// Criterio de la tarea 4.2: copiar un archivo grande no bloquea el
    /// bucle principal. Mide la pausa más larga entre ticks de un
    /// temporizador en el mismo contexto mientras se copia. Lenta (crea el
    /// archivo): `cargo test -p tlacuache -- --ignored big_copy`.
    /// Tamaño con `TLACUACHE_BIG_MB` (por defecto 2048).
    #[test]
    #[ignore]
    fn big_copy_keeps_main_loop_responsive() {
        use std::io::Write;

        let megabytes: usize = std::env::var("TLACUACHE_BIG_MB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2048);
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("grande.bin");
        let mut out = fs::File::create(&src).unwrap();
        let chunk = vec![0x5au8; 1024 * 1024];
        for _ in 0..megabytes {
            out.write_all(&chunk).unwrap();
        }
        drop(out);
        let dest = tmp.path().join("destino");
        fs::create_dir(&dest).unwrap();

        let context = glib::MainContext::new();
        // (pausa máxima, ticks); `Send` porque lo exige la fuente de glib.
        let stats = std::sync::Arc::new(std::sync::Mutex::new((Duration::ZERO, 0u32)));
        let reports = Rc::new(Cell::new(0u32));
        let started = Instant::now();
        context
            .with_thread_default(|| {
                let tick_stats = stats.clone();
                let mut last = Instant::now();
                let tick = glib::timeout_source_new(
                    Duration::from_millis(5),
                    None,
                    glib::Priority::DEFAULT,
                    move || {
                        let now = Instant::now();
                        if let Ok(mut stats) = tick_stats.lock() {
                            stats.0 = stats.0.max(now - last);
                            stats.1 += 1;
                        }
                        last = now;
                        glib::ControlFlow::Continue
                    },
                );
                tick.attach(Some(&context));
                let count = reports.clone();
                let report: Report = Rc::new(move |_: &Progress| count.set(count.get() + 1));
                context
                    .block_on(copy_tree(
                        &[file(&src)],
                        &file(&dest),
                        &gio::Cancellable::new(),
                        &report,
                    ))
                    .unwrap();
                tick.destroy();
            })
            .unwrap();

        let elapsed = started.elapsed();
        let copied = fs::metadata(dest.join("grande.bin")).unwrap().len();
        let (max_gap, ticks) = *stats.lock().unwrap();
        eprintln!(
            "{megabytes} MB en {elapsed:?}; {ticks} ticks; pausa máxima del bucle {max_gap:?}; {} avisos de progreso",
            reports.get()
        );
        assert_eq!(copied, (megabytes * 1024 * 1024) as u64);
        assert!(ticks > 0, "el temporizador no corrió: la medición no vale");
        assert!(
            max_gap < Duration::from_millis(100),
            "el bucle se bloqueó {max_gap:?}"
        );
    }

    #[test]
    fn delete_all_removes_files_and_folders_and_reports() {
        let tmp = tempfile::tempdir().unwrap();
        let file_a = tmp.path().join("a.txt");
        let dir_b = tmp.path().join("b");
        let keep = tmp.path().join("conservar.txt");
        fs::write(&file_a, "a").unwrap();
        fs::create_dir_all(dir_b.join("c")).unwrap();
        fs::write(dir_b.join("c/d.txt"), "d").unwrap();
        fs::write(&keep, "k").unwrap();

        let last = Rc::new(RefCell::new(Progress::default()));
        let sink = last.clone();
        let report: Report = Rc::new(move |p: &Progress| *sink.borrow_mut() = p.clone());
        block_on(delete_all(
            &[file(&file_a), file(&dir_b)],
            &gio::Cancellable::new(),
            &report,
        ))
        .unwrap();

        assert!(!file_a.exists() && !dir_b.exists());
        assert!(keep.exists());
        assert_eq!(last.borrow().files_done, 2);
    }

    #[test]
    fn cancelled_delete_touches_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("x.txt");
        fs::write(&target, "x").unwrap();
        let cancellable = gio::Cancellable::new();
        cancellable.cancel();
        let err = block_on(delete_all(&[file(&target)], &cancellable, &no_report())).unwrap_err();
        assert!(err.matches(gio::IOErrorEnum::Cancelled));
        assert!(target.exists());
    }

    #[test]
    fn delete_tree_removes_everything() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("borrar");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/c.txt"), "c").unwrap();
        block_on(delete_tree(&file(&root), &gio::Cancellable::new())).unwrap();
        assert!(!root.exists());
    }
}
