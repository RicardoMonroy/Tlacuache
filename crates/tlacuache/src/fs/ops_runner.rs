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
//! Conflictos (el destino ya existe): se pregunta a la UI con un
//! `Resolver` asíncrono (Reemplazar/Combinar, Omitir, Conservar ambos o
//! Cancelar, con «aplicar a todos»). Al mover combinando carpetas se mueve
//! archivo por archivo y solo se borran las carpetas de origen vacías, así
//! lo omitido no se pierde.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use glib::subclass::Signal;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::names::numbered_variants;
use tlacuache_core::ops::{
    ConflictAction, ConflictPolicy, Journal, OpId, OpKind, OpQueue, OpRequest, OpState, Operation,
    Progress, replace_allowed, undo_request,
};

use crate::strings;

/// Intervalo mínimo entre avisos de progreso a la UI.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const ATTRS: &str = "standard::name,standard::type,standard::size";
const COPY_FLAGS: gio::FileCopyFlags = gio::FileCopyFlags::NOFOLLOW_SYMLINKS;

type Report = Rc<dyn Fn(&Progress)>;

/// Un elemento ya existe en el destino.
pub struct Conflict {
    pub source: gio::File,
    pub dest: gio::File,
    pub source_is_dir: bool,
    pub dest_is_dir: bool,
    /// `false` si los tipos difieren o es el mismo elemento.
    pub replace_allowed: bool,
}

/// Respuesta de la UI a un conflicto. `action: None` cancela la operación.
pub struct ConflictDecision {
    pub action: Option<ConflictAction>,
    pub apply_to_all: bool,
}

/// Pregunta a la UI qué hacer con un conflicto (normalmente un diálogo).
pub type Resolver = Rc<dyn Fn(Conflict) -> Pin<Box<dyn Future<Output = ConflictDecision>>>>;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct OpsManager {
        pub queue: RefCell<OpQueue>,
        pub cancellables: RefCell<HashMap<OpId, gio::Cancellable>>,
        pub last_emit: Cell<Option<Instant>>,
        pub resolver: RefCell<Option<Resolver>>,
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

    /// Quién resuelve los conflictos (la ventana muestra un diálogo).
    pub fn set_conflict_resolver(&self, resolver: Resolver) {
        self.imp().resolver.replace(Some(resolver));
    }

    /// Encola una operación y la arranca si hay hueco.
    pub fn enqueue(&self, kind: OpKind, sources: &[gio::File], dest: Option<&gio::File>) -> OpId {
        let uris = sources.iter().map(|f| f.uri().to_string()).collect();
        let dest = dest.map(|d| d.uri().to_string());
        self.enqueue_request(OpRequest::new(kind, uris, dest))
    }

    pub fn enqueue_request(&self, request: OpRequest) -> OpId {
        let id = self.imp().queue.borrow_mut().enqueue_request(request);
        self.emit_changed();
        self.pump();
        id
    }

    /// Encola la operación que deshace `id` (una sola vez).
    pub fn undo(&self, id: OpId) -> Option<OpId> {
        let request = self.imp().queue.borrow_mut().take_undo(id)?;
        Some(self.enqueue_request(request))
    }

    /// Deshace la operación reversible más reciente.
    pub fn undo_last(&self) -> Option<OpId> {
        let id = self
            .imp()
            .queue
            .borrow()
            .iter()
            .filter(|op| op.undo.is_some())
            .map(|op| op.id)
            .max()?;
        self.undo(id)
    }

    /// Quita del panel las operaciones terminadas.
    pub fn clear_finished(&self) {
        self.imp().queue.borrow_mut().clear_finished();
        self.emit_changed();
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

        // Sin resolver configurado, cualquier conflicto cancela.
        let resolver: Resolver = self.imp().resolver.borrow().clone().unwrap_or_else(|| {
            Rc::new(|_| {
                Box::pin(async {
                    ConflictDecision {
                        action: None,
                        apply_to_all: false,
                    }
                })
            })
        });
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
                        copy_tree(&sources, &dest, &cancellable, &report, &resolver).await
                    }
                    (OpKind::Move, Some(dest)) => {
                        move_tree(&sources, &dest, &cancellable, &report, &resolver).await
                    }
                    (OpKind::Trash, _) => trash_all(&sources, &cancellable, &report).await,
                    (OpKind::Delete, _) => delete_all(&sources, &cancellable, &report).await,
                    (OpKind::Mkdir, _) => create_items(&sources, true).await,
                    (OpKind::CreateFile, _) => create_items(&sources, false).await,
                    (OpKind::Rename, Some(dest)) => rename(&sources, &dest).await,
                    (OpKind::RenameMany, _) => {
                        let targets: Vec<gio::File> =
                            op.targets.iter().map(|u| gio::File::for_uri(u)).collect();
                        rename_many(&sources, &targets).await
                    }
                    (OpKind::Restore, _) => {
                        let targets: Vec<gio::File> =
                            op.targets.iter().map(|u| gio::File::for_uri(u)).collect();
                        restore(&sources, &targets, &cancellable, &report).await
                    }
                    (OpKind::Untrash, _) => untrash(&sources, &cancellable, &report).await,
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

    fn finish(&self, id: OpId, result: Result<Journal, glib::Error>) {
        self.imp().cancellables.borrow_mut().remove(&id);
        {
            let mut queue = self.imp().queue.borrow_mut();
            let outcome = match result {
                Ok(journal) => {
                    // Las notas siguen a las carpetas movidas o renombradas.
                    if queue.get(id).is_some_and(|op| {
                        matches!(
                            op.kind,
                            OpKind::Move | OpKind::Rename | OpKind::RenameMany | OpKind::Restore
                        )
                    }) {
                        crate::fs::notes::get().relocate(journal.pairs.clone());
                    }
                    let undo = queue.get(id).and_then(|op| undo_request(op.kind, &journal));
                    queue.complete_with_undo(id, undo)
                }
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
    flags: gio::FileCopyFlags,
    cancellable: &gio::Cancellable,
    progress: Box<dyn FnMut(i64, i64)>,
) -> Result<(), glib::Error> {
    let (tx, rx) = async_channel::bounded(1);
    src.copy_async(
        dst,
        flags,
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
    flags: gio::FileCopyFlags,
    cancellable: &gio::Cancellable,
    progress: Box<dyn FnMut(i64, i64)>,
) -> Result<(), glib::Error> {
    let (tx, rx) = async_channel::bounded(1);
    src.move_async(
        dst,
        flags,
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

/// `Some(es_carpeta)` si `file` existe (sin seguir enlaces).
async fn existing(file: &gio::File) -> Result<Option<bool>, glib::Error> {
    let info = file
        .query_info_future(
            gio::FILE_ATTRIBUTE_STANDARD_TYPE,
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            glib::Priority::DEFAULT,
        )
        .await;
    match info {
        Ok(info) => Ok(Some(info.file_type() == gio::FileType::Directory)),
        Err(err) if err.matches(gio::IOErrorEnum::NotFound) => Ok(None),
        Err(err) => Err(err),
    }
}

/// Primer nombre libre junto a `dst` para «conservar ambos».
async fn free_variant(dst: &gio::File, is_dir: bool) -> Result<gio::File, glib::Error> {
    let parent = dst
        .parent()
        .ok_or_else(|| error(gio::IOErrorEnum::InvalidFilename, strings::OP_NO_NAME))?;
    let name = dst
        .basename()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| error(gio::IOErrorEnum::InvalidFilename, strings::OP_NO_NAME))?;
    for candidate in numbered_variants(&name, is_dir).take(10_000) {
        let file = parent.child(&candidate);
        if existing(&file).await?.is_none() {
            return Ok(file);
        }
    }
    Err(error(gio::IOErrorEnum::Exists, &strings::op_exists(&name)))
}

/// Resuelve conflictos preguntando a la UI y recordando «aplicar a todos».
struct Conflicts<'a> {
    resolver: &'a Resolver,
    policy: ConflictPolicy,
}

impl Conflicts<'_> {
    async fn resolve(
        &mut self,
        source: &gio::File,
        dest: &gio::File,
        source_is_dir: bool,
        dest_is_dir: bool,
    ) -> Result<ConflictAction, glib::Error> {
        let allowed = replace_allowed(source_is_dir, dest_is_dir, source.equal(dest));
        if let Some(action) = self.policy.remembered(allowed) {
            return Ok(action);
        }
        let decision = (self.resolver)(Conflict {
            source: source.clone(),
            dest: dest.clone(),
            source_is_dir,
            dest_is_dir,
            replace_allowed: allowed,
        })
        .await;
        let action = decision
            .action
            .ok_or_else(|| error(gio::IOErrorEnum::Cancelled, strings::OP_CANCELLED))?;
        // Defensa: la UI no debería ofrecer Reemplazar donde no se permite.
        let action = if action == ConflictAction::Replace && !allowed {
            ConflictAction::Skip
        } else {
            action
        };
        if decision.apply_to_all {
            self.policy.remember(action);
        }
        Ok(action)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Copy,
    Move,
}

/// Progreso de un archivo en curso sobre el acumulado `base`.
fn file_progress(report: &Report, progress: &Progress, base: u64) -> Box<dyn FnMut(i64, i64)> {
    let report = report.clone();
    let snapshot = progress.clone();
    Box::new(move |current: i64, _total: i64| {
        let mut partial = snapshot.clone();
        partial.bytes_done = base + u64::try_from(current).unwrap_or(0);
        report(&partial);
    })
}

fn display(file: &gio::File) -> Option<String> {
    file.basename().map(|n| n.to_string_lossy().into_owned())
}

/// Copia o mueve `sources` dentro de `dest_dir`, resolviendo conflictos.
async fn transfer(
    mode: Mode,
    sources: &[gio::File],
    dest_dir: &gio::File,
    cancellable: &gio::Cancellable,
    report: &Report,
    resolver: &Resolver,
) -> Result<Journal, glib::Error> {
    let mut journal = Journal::default();
    // 1. Recorrer los orígenes para los totales.
    let mut plans = Vec::new();
    for src in sources {
        check_cancelled(cancellable)?;
        let name = src
            .basename()
            .ok_or_else(|| error(gio::IOErrorEnum::InvalidFilename, strings::OP_NO_NAME))?;
        if dest_dir.equal(src) || dest_dir.has_prefix(src) {
            return Err(error(
                gio::IOErrorEnum::WouldRecurse,
                strings::OP_INTO_ITSELF,
            ));
        }
        let entries = scan(src, cancellable).await?;
        plans.push((src.clone(), dest_dir.child(&name), entries));
    }
    let all_files = plans
        .iter()
        .flat_map(|(_, _, e)| e)
        .filter(|e| e.kind == EntryKind::Other);
    let mut progress = Progress {
        bytes_total: Some(all_files.clone().map(|e| e.size).sum()),
        files_total: Some(all_files.count() as u64),
        ..Progress::default()
    };
    report(&progress);

    // 2. Transferir cada origen.
    let mut conflicts = Conflicts {
        resolver,
        policy: ConflictPolicy::default(),
    };
    for (src, mut dst, entries) in plans {
        check_cancelled(cancellable)?;
        let src_is_dir = entries.first().is_some_and(|e| e.kind == EntryKind::Dir);
        let files = entries.iter().filter(|e| e.kind == EntryKind::Other);
        let (bytes, file_count) = (
            files.clone().map(|e| e.size).sum::<u64>(),
            files.count() as u64,
        );

        let mut merge = false;
        let mut overwrite_root = false;
        if let Some(dst_is_dir) = existing(&dst).await? {
            match conflicts
                .resolve(&src, &dst, src_is_dir, dst_is_dir)
                .await?
            {
                ConflictAction::Skip => {
                    progress.bytes_done += bytes;
                    progress.files_done += file_count;
                    report(&progress);
                    continue;
                }
                ConflictAction::KeepBoth => dst = free_variant(&dst, src_is_dir).await?,
                ConflictAction::Replace if src_is_dir => {
                    merge = true;
                    journal.merged = true;
                }
                ConflictAction::Replace => overwrite_root = true,
            }
        }

        // Mover sin combinar: intento directo (instantáneo en el mismo
        // disco). Una carpeta a otro disco se mueve por archivos.
        if mode == Mode::Move && !merge {
            let mut flags = COPY_FLAGS;
            if overwrite_root {
                flags |= gio::FileCopyFlags::OVERWRITE;
            }
            progress.current = display(&src);
            let base = progress.bytes_done;
            let on_progress = file_progress(report, &progress, base);
            match move_file(&src, &dst, flags, cancellable, on_progress).await {
                Ok(()) => {
                    progress.bytes_done = base + bytes;
                    progress.files_done += file_count;
                    report(&progress);
                    journal
                        .pairs
                        .push((src.uri().to_string(), dst.uri().to_string()));
                    continue;
                }
                Err(err)
                    if err.matches(gio::IOErrorEnum::WouldRecurse)
                        || err.matches(gio::IOErrorEnum::NotSupported) => {}
                Err(err) => return Err(err),
            }
        }

        let target = Target {
            src_root: &src,
            dst_root: &dst,
            overwrite_root,
        };
        transfer_entries(
            mode,
            &target,
            &entries,
            &mut conflicts,
            cancellable,
            report,
            &mut progress,
        )
        .await?;
        journal
            .pairs
            .push((src.uri().to_string(), dst.uri().to_string()));
    }
    Ok(journal)
}

struct Target<'a> {
    src_root: &'a gio::File,
    dst_root: &'a gio::File,
    /// El usuario ya eligió reemplazar el elemento raíz (un archivo).
    overwrite_root: bool,
}

/// Copia o mueve el contenido recorrido de un origen, archivo por archivo.
async fn transfer_entries(
    mode: Mode,
    target: &Target<'_>,
    entries: &[Entry],
    conflicts: &mut Conflicts<'_>,
    cancellable: &gio::Cancellable,
    report: &Report,
    progress: &mut Progress,
) -> Result<(), glib::Error> {
    for entry in entries {
        check_cancelled(cancellable)?;
        let mut dst = match target.src_root.relative_path(&entry.file) {
            Some(relative) => target.dst_root.resolve_relative_path(relative),
            None => target.dst_root.clone(),
        };
        match entry.kind {
            EntryKind::Dir => match existing(&dst).await? {
                // Combinando: la carpeta ya está.
                Some(true) => {}
                Some(false) => {
                    let name = display(&dst).unwrap_or_default();
                    return Err(error(gio::IOErrorEnum::Exists, &strings::op_exists(&name)));
                }
                None => dst.make_directory_future(glib::Priority::DEFAULT).await?,
            },
            EntryKind::Other => {
                let mut flags = COPY_FLAGS;
                let is_root = entry.file.equal(target.src_root);
                if is_root && target.overwrite_root {
                    flags |= gio::FileCopyFlags::OVERWRITE;
                } else if let Some(dst_is_dir) = existing(&dst).await? {
                    match conflicts
                        .resolve(&entry.file, &dst, false, dst_is_dir)
                        .await?
                    {
                        ConflictAction::Skip => {
                            progress.bytes_done += entry.size;
                            progress.files_done += 1;
                            report(progress);
                            continue;
                        }
                        ConflictAction::KeepBoth => dst = free_variant(&dst, false).await?,
                        ConflictAction::Replace => flags |= gio::FileCopyFlags::OVERWRITE,
                    }
                }

                progress.current = display(&entry.file);
                let base = progress.bytes_done;
                let on_progress = file_progress(report, progress, base);
                let result = match mode {
                    Mode::Copy => {
                        copy_file(&entry.file, &dst, flags, cancellable, on_progress).await
                    }
                    Mode::Move => {
                        move_file(&entry.file, &dst, flags, cancellable, on_progress).await
                    }
                };
                if let Err(err) = result {
                    // Sin archivos a medias. Al reemplazar, gio escribe en
                    // un temporal: el original sigue intacto y no se borra.
                    if mode == Mode::Copy && !flags.contains(gio::FileCopyFlags::OVERWRITE) {
                        let _ = dst.delete_future(glib::Priority::DEFAULT).await;
                    }
                    return Err(err);
                }
                progress.bytes_done = base + entry.size;
                progress.files_done += 1;
                report(progress);
            }
        }
    }

    // Movido por archivos: quitar las carpetas de origen que quedaron
    // vacías; las que conservan algo omitido se dejan.
    if mode == Mode::Move {
        for entry in entries.iter().rev().filter(|e| e.kind == EntryKind::Dir) {
            match entry.file.delete_future(glib::Priority::DEFAULT).await {
                Ok(()) => {}
                Err(err) if err.matches(gio::IOErrorEnum::NotEmpty) => {}
                Err(err) => return Err(err),
            }
        }
    }
    Ok(())
}

async fn copy_tree(
    sources: &[gio::File],
    dest_dir: &gio::File,
    cancellable: &gio::Cancellable,
    report: &Report,
    resolver: &Resolver,
) -> Result<Journal, glib::Error> {
    transfer(Mode::Copy, sources, dest_dir, cancellable, report, resolver).await
}

async fn move_tree(
    sources: &[gio::File],
    dest_dir: &gio::File,
    cancellable: &gio::Cancellable,
    report: &Report,
    resolver: &Resolver,
) -> Result<Journal, glib::Error> {
    transfer(Mode::Move, sources, dest_dir, cancellable, report, resolver).await
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
) -> Result<Journal, glib::Error> {
    let mut trashed = Vec::new();
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
        trashed.push(file.uri().to_string());
        progress.files_done += 1;
        report(&progress);
    }
    Ok(Journal {
        trashed,
        ..Journal::default()
    })
}

/// Borra `sources` de forma permanente (carpetas con su contenido).
async fn delete_all(
    sources: &[gio::File],
    cancellable: &gio::Cancellable,
    report: &Report,
) -> Result<Journal, glib::Error> {
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
    Ok(Journal::default())
}

/// Crea carpetas (`dirs`) o archivos vacíos. Falla si ya existen.
async fn create_items(items: &[gio::File], dirs: bool) -> Result<Journal, glib::Error> {
    for item in items {
        if dirs {
            item.make_directory_future(glib::Priority::DEFAULT).await?;
        } else {
            let stream = item
                .create_future(gio::FileCreateFlags::NONE, glib::Priority::DEFAULT)
                .await?;
            stream.close_future(glib::Priority::DEFAULT).await?;
        }
    }
    Ok(Journal {
        created: items.iter().map(|f| f.uri().to_string()).collect(),
        ..Journal::default()
    })
}

/// Renombra el primer origen al nombre de `dest`. `set_display_name`
/// falla si ya existe un elemento con ese nombre (no lo pisa).
async fn rename(sources: &[gio::File], dest: &gio::File) -> Result<Journal, glib::Error> {
    let source = sources
        .first()
        .ok_or_else(|| error(gio::IOErrorEnum::InvalidArgument, strings::OP_NO_NAME))?;
    let name = dest
        .basename()
        .ok_or_else(|| error(gio::IOErrorEnum::InvalidFilename, strings::OP_NO_NAME))?;
    let renamed = source
        .set_display_name_future(&name.to_string_lossy(), glib::Priority::DEFAULT)
        .await?;
    Ok(Journal {
        pairs: vec![(source.uri().to_string(), renamed.uri().to_string())],
        ..Journal::default()
    })
}

/// Renombrado masivo: cada origen toma el nombre de su destino (misma
/// carpeta). Si algún destino es el nombre actual de otro origen (cadenas o
/// intercambios), primero pasan todos por nombres temporales. Si algo
/// falla, lo ya hecho se revierte en lo posible y se devuelve el error.
async fn rename_many(sources: &[gio::File], targets: &[gio::File]) -> Result<Journal, glib::Error> {
    if sources.len() != targets.len() {
        return Err(error(
            gio::IOErrorEnum::InvalidArgument,
            strings::OP_NO_NAME,
        ));
    }
    let names = targets
        .iter()
        .map(|t| {
            t.basename()
                .map(|n| n.to_string_lossy().into_owned())
                .ok_or_else(|| error(gio::IOErrorEnum::InvalidFilename, strings::OP_NO_NAME))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let source_uris: std::collections::HashSet<String> =
        sources.iter().map(|s| s.uri().to_string()).collect();
    let staging = targets
        .iter()
        .any(|t| source_uris.contains(t.uri().as_str()));

    // Fase 1 (si hace falta): a nombres temporales ocultos.
    let mut current: Vec<gio::File> = sources.to_vec();
    if staging {
        let pid = std::process::id();
        for i in 0..current.len() {
            let temp = tlacuache_core::bulk_rename::temp_name(i, pid);
            match current[i]
                .set_display_name_future(&temp, glib::Priority::DEFAULT)
                .await
            {
                Ok(moved) => current[i] = moved,
                Err(err) => {
                    revert(&current[..i], &sources[..i]).await;
                    return Err(err);
                }
            }
        }
    }

    // Fase 2: a los nombres finales.
    let mut journal = Journal::default();
    for (i, name) in names.iter().enumerate() {
        match current[i]
            .set_display_name_future(name, glib::Priority::DEFAULT)
            .await
        {
            Ok(renamed) => {
                journal
                    .pairs
                    .push((sources[i].uri().to_string(), renamed.uri().to_string()));
                current[i] = renamed;
            }
            Err(err) => {
                // Lo pendiente (temporal) vuelve a su nombre original; lo ya
                // renombrado se queda (se puede deshacer a mano).
                if staging {
                    revert(&current[i..], &sources[i..]).await;
                }
                return Err(err);
            }
        }
    }
    Ok(journal)
}

/// Devuelve cada archivo de `current` al nombre de su `original` (mejor
/// esfuerzo, para deshacer un renombrado a medias).
async fn revert(current: &[gio::File], originals: &[gio::File]) {
    for (file, original) in current.iter().zip(originals) {
        if let Some(name) = original.basename()
            && let Err(err) = file
                .set_display_name_future(&name.to_string_lossy(), glib::Priority::DEFAULT)
                .await
        {
            tracing::warn!("no se pudo revertir {}: {err}", file.uri());
        }
    }
}

/// Devuelve cada origen a su ruta exacta (deshacer mover). No pisa nada:
/// si el destino ya existe, falla.
async fn restore(
    sources: &[gio::File],
    targets: &[gio::File],
    cancellable: &gio::Cancellable,
    report: &Report,
) -> Result<Journal, glib::Error> {
    let mut progress = Progress {
        files_total: Some(sources.len() as u64),
        ..Progress::default()
    };
    report(&progress);
    for (src, target) in sources.iter().zip(targets) {
        check_cancelled(cancellable)?;
        if existing(target).await?.is_some() {
            let name = display(target).unwrap_or_default();
            return Err(error(gio::IOErrorEnum::Exists, &strings::op_exists(&name)));
        }
        progress.current = display(src);
        report(&progress);
        let on_progress = file_progress(report, &progress, 0);
        match move_file(src, target, COPY_FLAGS, cancellable, on_progress).await {
            Ok(()) => {}
            // Carpeta a otro disco: por archivos (el destino no existe, así
            // que no hay conflictos).
            Err(err)
                if err.matches(gio::IOErrorEnum::WouldRecurse)
                    || err.matches(gio::IOErrorEnum::NotSupported) =>
            {
                let entries = scan(src, cancellable).await?;
                let target = Target {
                    src_root: src,
                    dst_root: target,
                    overwrite_root: false,
                };
                let never: Resolver = Rc::new(|_| {
                    Box::pin(async {
                        ConflictDecision {
                            action: None,
                            apply_to_all: false,
                        }
                    })
                });
                let mut conflicts = Conflicts {
                    resolver: &never,
                    policy: ConflictPolicy::default(),
                };
                let mut inner = Progress::default();
                transfer_entries(
                    Mode::Move,
                    &target,
                    &entries,
                    &mut conflicts,
                    cancellable,
                    report,
                    &mut inner,
                )
                .await?;
            }
            Err(err) => return Err(err),
        }
        progress.files_done += 1;
        report(&progress);
    }
    Ok(Journal::default())
}

/// Saca de la papelera los elementos cuya ruta original está en
/// `originals` (deshacer papelera). Si un original se envió varias veces,
/// restaura el borrado más reciente.
async fn untrash(
    originals: &[gio::File],
    cancellable: &gio::Cancellable,
    report: &Report,
) -> Result<Journal, glib::Error> {
    let trash = gio::File::for_uri("trash:///");
    let attrs = "standard::name,trash::orig-path,trash::deletion-date";
    let enumerator = trash
        .enumerate_children_future(
            attrs,
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await?;
    // (ruta original, fecha de borrado ISO, elemento en la papelera)
    let mut items = Vec::new();
    loop {
        let batch = enumerator
            .next_files_future(256, glib::Priority::DEFAULT)
            .await?;
        if batch.is_empty() {
            break;
        }
        for info in batch {
            let Some(orig) = info.attribute_byte_string("trash::orig-path") else {
                continue;
            };
            let date = info
                .attribute_string("trash::deletion-date")
                .unwrap_or_default();
            items.push((orig.to_string(), date.to_string(), trash.child(info.name())));
        }
    }

    let mut sources = Vec::new();
    let mut targets = Vec::new();
    for original in originals {
        let Some(path) = original.path() else {
            continue;
        };
        let wanted = path.to_string_lossy();
        let newest = items
            .iter()
            .filter(|(orig, _, _)| *orig == wanted)
            .max_by(|a, b| a.1.cmp(&b.1));
        match newest {
            Some((_, _, item)) => {
                sources.push(item.clone());
                targets.push(original.clone());
            }
            None => {
                let name = display(original).unwrap_or_default();
                return Err(error(
                    gio::IOErrorEnum::NotFound,
                    &strings::untrash_missing(&name),
                ));
            }
        }
    }
    restore(&sources, &targets, cancellable, report).await
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

    /// Resolver de prueba: responde en orden con `decisions` y cuenta las
    /// preguntas. Si se acaban, cancela.
    fn scripted(decisions: Vec<(Option<ConflictAction>, bool)>) -> (Resolver, Rc<Cell<u32>>) {
        let asked = Rc::new(Cell::new(0));
        let queue = Rc::new(RefCell::new(std::collections::VecDeque::from(decisions)));
        let count = asked.clone();
        let resolver: Resolver = Rc::new(move |_conflict| {
            count.set(count.get() + 1);
            let (action, apply_to_all) = queue.borrow_mut().pop_front().unwrap_or((None, false));
            Box::pin(async move {
                ConflictDecision {
                    action,
                    apply_to_all,
                }
            })
        });
        (resolver, asked)
    }

    fn never() -> Resolver {
        scripted(Vec::new()).0
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
            &never(),
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

    fn copy_with(
        sources: &[&Path],
        dest: &Path,
        resolver: &Resolver,
    ) -> Result<Journal, glib::Error> {
        let sources: Vec<gio::File> = sources.iter().map(|p| file(p)).collect();
        block_on(copy_tree(
            &sources,
            &file(dest),
            &gio::Cancellable::new(),
            &no_report(),
            resolver,
        ))
    }

    #[test]
    fn copy_into_itself_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("carpeta");
        fs::create_dir(&src).unwrap();
        let err = copy_with(&[&src], &src.join("dentro"), &never()).unwrap_err();
        assert!(err.matches(gio::IOErrorEnum::WouldRecurse));
    }

    #[test]
    fn skip_and_cancel_leave_destination_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("x.txt");
        fs::write(&src, "nuevo").unwrap();
        let dest = tmp.path().join("destino");
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("x.txt"), "ya estaba").unwrap();

        let (skip, asked) = scripted(vec![(Some(ConflictAction::Skip), false)]);
        copy_with(&[&src], &dest, &skip).unwrap();
        assert_eq!(asked.get(), 1);
        assert_eq!(fs::read_to_string(dest.join("x.txt")).unwrap(), "ya estaba");

        let err = copy_with(&[&src], &dest, &never()).unwrap_err();
        assert!(err.matches(gio::IOErrorEnum::Cancelled));
        assert_eq!(fs::read_to_string(dest.join("x.txt")).unwrap(), "ya estaba");
    }

    #[test]
    fn replace_overwrites_and_keep_both_numbers() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("foto.jpg");
        fs::write(&src, "nueva").unwrap();
        let dest = tmp.path().join("destino");
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("foto.jpg"), "vieja").unwrap();
        fs::write(dest.join("foto (2).jpg"), "otra").unwrap();

        let (keep, _) = scripted(vec![(Some(ConflictAction::KeepBoth), false)]);
        copy_with(&[&src], &dest, &keep).unwrap();
        assert_eq!(
            fs::read_to_string(dest.join("foto (3).jpg")).unwrap(),
            "nueva"
        );
        assert_eq!(fs::read_to_string(dest.join("foto.jpg")).unwrap(), "vieja");

        let (replace, _) = scripted(vec![(Some(ConflictAction::Replace), false)]);
        copy_with(&[&src], &dest, &replace).unwrap();
        assert_eq!(fs::read_to_string(dest.join("foto.jpg")).unwrap(), "nueva");
    }

    #[test]
    fn copying_onto_itself_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("nota.md");
        fs::write(&src, "x").unwrap();
        let (keep, _) = scripted(vec![(Some(ConflictAction::KeepBoth), false)]);
        copy_with(&[&src], tmp.path(), &keep).unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join("nota (2).md")).unwrap(),
            "x"
        );
    }

    #[test]
    fn merge_folders_asks_per_file_and_applies_to_all() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("origen/carpeta");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("a.txt"), "nuevo a").unwrap();
        fs::write(src.join("b.txt"), "nuevo b").unwrap();
        fs::write(src.join("sub/c.txt"), "nuevo c").unwrap();
        let dest = tmp.path().join("destino");
        fs::create_dir_all(dest.join("carpeta/sub")).unwrap();
        fs::write(dest.join("carpeta/a.txt"), "viejo a").unwrap();
        fs::write(dest.join("carpeta/sub/c.txt"), "viejo c").unwrap();

        // Combinar la carpeta y omitir todos los archivos que choquen.
        let (resolver, asked) = scripted(vec![
            (Some(ConflictAction::Replace), false),
            (Some(ConflictAction::Skip), true),
        ]);
        copy_with(&[&src], &dest, &resolver).unwrap();

        assert_eq!(asked.get(), 2, "la segunda decisión se aplicó a todos");
        let merged = dest.join("carpeta");
        assert_eq!(fs::read_to_string(merged.join("a.txt")).unwrap(), "viejo a");
        assert_eq!(fs::read_to_string(merged.join("b.txt")).unwrap(), "nuevo b");
        assert_eq!(
            fs::read_to_string(merged.join("sub/c.txt")).unwrap(),
            "viejo c"
        );
    }

    #[test]
    fn move_merge_keeps_skipped_files_in_source() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("origen/carpeta");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), "nuevo a").unwrap();
        fs::write(src.join("b.txt"), "nuevo b").unwrap();
        let dest = tmp.path().join("destino");
        fs::create_dir_all(dest.join("carpeta")).unwrap();
        fs::write(dest.join("carpeta/a.txt"), "viejo a").unwrap();

        let (resolver, _) = scripted(vec![
            (Some(ConflictAction::Replace), false),
            (Some(ConflictAction::Skip), false),
        ]);
        block_on(move_tree(
            &[file(&src)],
            &file(&dest),
            &gio::Cancellable::new(),
            &no_report(),
            &resolver,
        ))
        .unwrap();

        // b se movió; a se omitió y sigue en el origen junto con su carpeta.
        assert_eq!(
            fs::read_to_string(dest.join("carpeta/b.txt")).unwrap(),
            "nuevo b"
        );
        assert_eq!(
            fs::read_to_string(dest.join("carpeta/a.txt")).unwrap(),
            "viejo a"
        );
        assert!(!src.join("b.txt").exists());
        assert_eq!(fs::read_to_string(src.join("a.txt")).unwrap(), "nuevo a");
    }

    #[test]
    fn replace_is_refused_between_different_types() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("cosa");
        fs::create_dir(&src).unwrap();
        let dest = tmp.path().join("destino");
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("cosa"), "soy archivo").unwrap();

        // Aunque la UI respondiera Reemplazar, se trata como Omitir.
        let (resolver, _) = scripted(vec![(Some(ConflictAction::Replace), false)]);
        copy_with(&[&src], &dest, &resolver).unwrap();
        assert_eq!(
            fs::read_to_string(dest.join("cosa")).unwrap(),
            "soy archivo"
        );
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
            &never(),
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
            &never(),
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
                        &never(),
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
    fn creates_and_renames_without_overwriting() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Nueva carpeta");
        let empty = tmp.path().join("vacío.txt");
        block_on(create_items(&[file(&dir)], true)).unwrap();
        block_on(create_items(&[file(&empty)], false)).unwrap();
        assert!(dir.is_dir());
        assert_eq!(fs::read(&empty).unwrap().len(), 0);

        let err = block_on(create_items(&[file(&dir)], true)).unwrap_err();
        assert!(err.matches(gio::IOErrorEnum::Exists));

        let renamed = tmp.path().join("renombrado.txt");
        block_on(rename(&[file(&empty)], &file(&renamed))).unwrap();
        assert!(renamed.exists() && !empty.exists());

        // No pisa un archivo existente.
        fs::write(tmp.path().join("otro.txt"), "contenido").unwrap();
        let err = block_on(rename(
            &[file(&renamed)],
            &file(&tmp.path().join("otro.txt")),
        ))
        .unwrap_err();
        assert!(err.matches(gio::IOErrorEnum::Exists));
        assert_eq!(
            fs::read_to_string(tmp.path().join("otro.txt")).unwrap(),
            "contenido"
        );
    }

    #[test]
    fn journal_records_final_names_and_move_can_be_restored() {
        let tmp = tempfile::tempdir().unwrap();
        let origin = tmp.path().join("origen");
        let dest = tmp.path().join("destino");
        fs::create_dir_all(origin.join("carpeta")).unwrap();
        fs::write(origin.join("x.txt"), "x").unwrap();
        fs::write(origin.join("carpeta/y.txt"), "y").unwrap();
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("x.txt"), "ya estaba").unwrap();

        // Mover con «conservar ambos» para x.txt.
        let (keep, _) = scripted(vec![(Some(ConflictAction::KeepBoth), false)]);
        let journal = block_on(move_tree(
            &[file(&origin.join("x.txt")), file(&origin.join("carpeta"))],
            &file(&dest),
            &gio::Cancellable::new(),
            &no_report(),
            &keep,
        ))
        .unwrap();
        assert!(!journal.merged);
        assert_eq!(journal.pairs.len(), 2);
        assert!(journal.pairs[0].1.ends_with("x%20(2).txt"));

        // Deshacer: cada elemento vuelve a su ruta exacta.
        let undo = undo_request(OpKind::Move, &journal).unwrap();
        assert_eq!(undo.kind, OpKind::Restore);
        let sources: Vec<gio::File> = undo.sources.iter().map(|u| gio::File::for_uri(u)).collect();
        let targets: Vec<gio::File> = undo.targets.iter().map(|u| gio::File::for_uri(u)).collect();
        block_on(restore(
            &sources,
            &targets,
            &gio::Cancellable::new(),
            &no_report(),
        ))
        .unwrap();

        assert_eq!(fs::read_to_string(origin.join("x.txt")).unwrap(), "x");
        assert_eq!(
            fs::read_to_string(origin.join("carpeta/y.txt")).unwrap(),
            "y"
        );
        assert_eq!(fs::read_to_string(dest.join("x.txt")).unwrap(), "ya estaba");
        assert!(!dest.join("x (2).txt").exists() && !dest.join("carpeta").exists());
    }

    #[test]
    fn restore_never_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.txt");
        let b = tmp.path().join("b.txt");
        fs::write(&a, "a").unwrap();
        fs::write(&b, "b").unwrap();
        let err = block_on(restore(
            &[file(&a)],
            &[file(&b)],
            &gio::Cancellable::new(),
            &no_report(),
        ))
        .unwrap_err();
        assert!(err.matches(gio::IOErrorEnum::Exists));
        assert_eq!(fs::read_to_string(&b).unwrap(), "b");
    }

    #[test]
    fn rename_journal_allows_renaming_back() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("viejo.txt");
        fs::write(&old, "contenido").unwrap();
        let journal =
            block_on(rename(&[file(&old)], &file(&tmp.path().join("nuevo.txt")))).unwrap();
        let undo = undo_request(OpKind::Rename, &journal).unwrap();
        let source = gio::File::for_uri(&undo.sources[0]);
        let dest = gio::File::for_uri(undo.dest.as_deref().unwrap());
        block_on(rename(&[source], &dest)).unwrap();
        assert_eq!(fs::read_to_string(&old).unwrap(), "contenido");
    }

    #[test]
    fn rename_many_swaps_through_temporary_names_and_undoes() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.txt");
        let b = tmp.path().join("b.txt");
        fs::write(&a, "A").unwrap();
        fs::write(&b, "B").unwrap();
        // Intercambio: a→b y b→a.
        let journal = block_on(rename_many(&[file(&a), file(&b)], &[file(&b), file(&a)])).unwrap();
        assert_eq!(fs::read_to_string(&a).unwrap(), "B");
        assert_eq!(fs::read_to_string(&b).unwrap(), "A");
        assert_eq!(journal.pairs.len(), 2);
        // Ningún temporal se quedó.
        let leftovers: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(".tlacuache"))
            .collect();
        assert!(leftovers.is_empty());

        // Deshacer lo devuelve.
        let undo = undo_request(OpKind::RenameMany, &journal).unwrap();
        let sources: Vec<_> = undo.sources.iter().map(|u| gio::File::for_uri(u)).collect();
        let targets: Vec<_> = undo.targets.iter().map(|u| gio::File::for_uri(u)).collect();
        block_on(rename_many(&sources, &targets)).unwrap();
        assert_eq!(fs::read_to_string(&a).unwrap(), "A");
        assert_eq!(fs::read_to_string(&b).unwrap(), "B");
    }

    #[test]
    fn rename_many_direct_and_failure_reverts() {
        let tmp = tempfile::tempdir().unwrap();
        let one = tmp.path().join("IMG_1.jpg");
        let two = tmp.path().join("IMG_2.jpg");
        fs::write(&one, "1").unwrap();
        fs::write(&two, "2").unwrap();
        let journal = block_on(rename_many(
            &[file(&one), file(&two)],
            &[
                file(&tmp.path().join("foto-1.jpg")),
                file(&tmp.path().join("foto-2.jpg")),
            ],
        ))
        .unwrap();
        assert_eq!(journal.pairs.len(), 2);
        assert!(tmp.path().join("foto-2.jpg").exists() && !two.exists());

        // Un origen que ya no existe: falla y no deja temporales.
        let ghost = tmp.path().join("no-existe.jpg");
        let x = tmp.path().join("foto-1.jpg");
        let result = block_on(rename_many(
            &[file(&x), file(&ghost)],
            &[file(&ghost), file(&x)],
        ));
        assert!(result.is_err());
        assert!(x.exists(), "se revirtió el primero");
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
