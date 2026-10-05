//! Indicador de operaciones en la barra superior. Muestra el progreso de la
//! operación en curso y despliega un popover con cada operación: progreso,
//! estado, cancelar o deshacer. «Limpiar» quita las terminadas; sin
//! operaciones, el indicador se oculta.

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, pango};
use tlacuache_core::ops::{OpId, OpKind, OpState, Operation};

use crate::fs::display::display_name;
use crate::fs::ops_runner::OpsManager;
use crate::strings;

/// Widgets de una fila del popover (se actualizan en el sitio).
struct OpRow {
    root: gtk::Box,
    title: gtk::Label,
    progress: gtk::ProgressBar,
    status: gtk::Label,
    action: gtk::Button,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct OpsIndicator {
        pub button: gtk::MenuButton,
        pub summary: gtk::Label,
        pub progress: gtk::ProgressBar,
        pub icon: gtk::Image,
        pub list: gtk::Box,
        pub clear: gtk::Button,
        pub(super) rows: RefCell<HashMap<OpId, OpRow>>,
        pub manager: OnceCell<OpsManager>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for OpsIndicator {
        const NAME: &'static str = "TlacuacheOpsIndicator";
        type Type = super::OpsIndicator;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for OpsIndicator {}
    impl WidgetImpl for OpsIndicator {}
    impl BinImpl for OpsIndicator {}
}

glib::wrapper! {
    pub struct OpsIndicator(ObjectSubclass<imp::OpsIndicator>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl OpsIndicator {
    pub fn new(manager: &OpsManager) -> Self {
        let indicator: Self = glib::Object::new();
        let _ = indicator.imp().manager.set(manager.clone());
        indicator.setup();
        indicator
    }

    fn setup(&self) {
        let imp = self.imp();

        // Botón: resumen de la operación en curso o ✓ al terminar.
        imp.summary.set_ellipsize(pango::EllipsizeMode::Middle);
        imp.summary.set_max_width_chars(32);
        imp.summary.add_css_class("caption");
        imp.progress.set_width_request(140);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_valign(gtk::Align::Center);
        text.append(&imp.summary);
        text.append(&imp.progress);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        content.append(&imp.icon);
        content.append(&text);
        imp.button.set_child(Some(&content));
        imp.button.add_css_class("flat");
        imp.button.set_focusable(false);
        imp.button.set_tooltip_text(Some(strings::OPS_PANEL));

        // Popover: lista de operaciones.
        let heading = gtk::Label::builder()
            .label(strings::OPS_PANEL)
            .xalign(0.0)
            .hexpand(true)
            .build();
        heading.add_css_class("heading");
        imp.clear.set_label(strings::OPS_CLEAR);
        imp.clear.add_css_class("flat");
        imp.clear.connect_clicked(glib::clone!(
            #[weak(rename_to = indicator)]
            self,
            move |_| {
                if let Some(manager) = indicator.imp().manager.get() {
                    manager.clear_finished();
                }
            }
        ));
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        header.append(&heading);
        header.append(&imp.clear);

        imp.list.set_orientation(gtk::Orientation::Vertical);
        imp.list.set_spacing(10);
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(420)
            .child(&imp.list)
            .build();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.set_width_request(380);
        body.append(&header);
        body.append(&scrolled);
        let popover = gtk::Popover::new();
        popover.set_child(Some(&body));
        imp.button.set_popover(Some(&popover));

        self.set_child(Some(&imp.button));
        self.set_visible(false);

        if let Some(manager) = imp.manager.get() {
            manager.connect_changed(glib::clone!(
                #[weak(rename_to = indicator)]
                self,
                move |_| indicator.update()
            ));
        }
    }

    fn update(&self) {
        let imp = self.imp();
        let Some(manager) = imp.manager.get() else {
            return;
        };
        let ops = manager.operations();
        if ops.is_empty() {
            imp.rows.borrow_mut().clear();
            while let Some(child) = imp.list.first_child() {
                imp.list.remove(&child);
            }
            if let Some(popover) = imp.button.popover() {
                popover.popdown();
            }
            self.set_visible(false);
            return;
        }
        self.set_visible(true);
        self.update_summary(&ops);
        self.update_rows(&ops);
        imp.clear
            .set_sensitive(ops.iter().any(|op| op.state.is_finished()));
    }

    /// El botón: progreso de la primera en curso, o ✓/⚠ si todo terminó.
    fn update_summary(&self, ops: &[Operation]) {
        let imp = self.imp();
        let current = ops
            .iter()
            .find(|op| op.state.is_active())
            .or_else(|| ops.iter().find(|op| op.state == OpState::Queued));
        match current {
            Some(op) => {
                imp.icon.set_visible(false);
                imp.progress.set_visible(true);
                imp.summary.set_text(&status_text(op));
                match op.progress.fraction() {
                    Some(fraction) => imp.progress.set_fraction(fraction),
                    None => imp.progress.pulse(),
                }
            }
            None => {
                let failed = ops.iter().any(|op| matches!(op.state, OpState::Failed(_)));
                imp.icon.set_icon_name(Some(if failed {
                    "dialog-warning-symbolic"
                } else {
                    "object-select-symbolic"
                }));
                imp.icon.set_visible(true);
                imp.progress.set_visible(false);
                imp.summary.set_text(&strings::ops_finished(ops.len()));
            }
        }
    }

    fn update_rows(&self, ops: &[Operation]) {
        let imp = self.imp();
        let mut rows = imp.rows.borrow_mut();
        // Quitar las filas de operaciones que ya no están (limpiadas).
        rows.retain(|id, row| {
            let keep = ops.iter().any(|op| op.id == *id);
            if !keep {
                imp.list.remove(&row.root);
            }
            keep
        });
        for op in ops {
            let row = rows.entry(op.id).or_insert_with(|| {
                let row = self.new_row(op.id);
                imp.list.append(&row.root);
                row
            });
            fill_row(row, op);
        }
    }

    fn new_row(&self, id: OpId) -> OpRow {
        let title = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(pango::EllipsizeMode::Middle)
            .build();
        let action = gtk::Button::new();
        action.add_css_class("flat");
        action.add_css_class("circular");
        action.connect_clicked(glib::clone!(
            #[weak(rename_to = indicator)]
            self,
            move |button| {
                let Some(manager) = indicator.imp().manager.get() else {
                    return;
                };
                let Some(op) = manager.operation(id) else {
                    return;
                };
                if op.state.is_finished() {
                    manager.undo(id);
                } else {
                    manager.cancel(id);
                }
                button.set_sensitive(false);
            }
        ));
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        top.append(&title);
        top.append(&action);

        let progress = gtk::ProgressBar::new();
        let status = gtk::Label::builder().xalign(0.0).wrap(true).build();
        status.add_css_class("caption");
        status.add_css_class("dim-label");

        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        root.append(&top);
        root.append(&progress);
        root.append(&status);
        OpRow {
            root,
            title,
            progress,
            status,
            action,
        }
    }
}

fn fill_row(row: &OpRow, op: &Operation) {
    row.title.set_text(&title(op));
    row.status.set_text(&status_text(op));
    let failed = matches!(op.state, OpState::Failed(_));
    if failed {
        row.status.add_css_class("error");
        row.status.remove_css_class("dim-label");
    }

    let active = op.state.is_active() || op.state == OpState::Queued;
    row.progress.set_visible(op.state.is_active());
    if op.state.is_active() {
        match op.progress.fraction() {
            Some(fraction) => row.progress.set_fraction(fraction),
            None => row.progress.pulse(),
        }
    }

    if active {
        row.action.set_icon_name("process-stop-symbolic");
        row.action.set_tooltip_text(Some(strings::OP_CANCEL));
        row.action.set_visible(true);
        row.action.set_sensitive(op.state != OpState::Cancelling);
    } else if op.undo.is_some() {
        row.action.set_icon_name("edit-undo-symbolic");
        row.action.set_tooltip_text(Some(strings::ACTION_UNDO));
        row.action.set_visible(true);
        row.action.set_sensitive(true);
    } else {
        row.action.set_visible(false);
    }
}

/// «Copiar 3 elementos → Datos», «Renombrar «a» → «b»»…
fn title(op: &Operation) -> String {
    let name = |uri: &str| display_name(&gio::File::for_uri(uri));
    let what = match op.sources.as_slice() {
        [one] => format!("«{}»", name(one)),
        many => strings::items_count(many.len()),
    };
    let dest = op.dest.as_deref().map(name);
    strings::op_title(op.kind, &what, dest.as_deref())
}

fn status_text(op: &Operation) -> String {
    match &op.state {
        OpState::Queued => strings::OP_QUEUED.to_owned(),
        OpState::Running if op.progress.files_total.is_none() => strings::OP_PREPARING.to_owned(),
        OpState::Running => strings::op_progress(
            verb(op.kind),
            op.progress.files_done,
            op.progress.files_total,
            op.progress.current.as_deref(),
        ),
        OpState::Cancelling => strings::OP_CANCELLING.to_owned(),
        OpState::Done => strings::OP_DONE.to_owned(),
        OpState::Failed(message) => message.clone(),
        OpState::Cancelled => strings::OP_CANCELLED.to_owned(),
    }
}

fn verb(kind: OpKind) -> &'static str {
    match kind {
        OpKind::Copy => strings::OP_COPYING,
        OpKind::Move => strings::OP_MOVING,
        OpKind::Trash => strings::OP_TRASHING,
        OpKind::Delete => strings::OP_DELETING,
        OpKind::Mkdir => strings::OP_CREATING_FOLDER,
        OpKind::CreateFile => strings::OP_CREATING_FILE,
        OpKind::Rename | OpKind::RenameMany => strings::OP_RENAMING,
        OpKind::Restore | OpKind::Untrash => strings::OP_RESTORING,
    }
}
