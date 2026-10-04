//! Indicador de operaciones en la barra superior: progreso de la operación
//! en curso y botón para cancelarla. Se oculta cuando no hay ninguna. El
//! panel completo (lista, deshacer) llega en la tarea 4.7.

use std::cell::{Cell, OnceCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{glib, pango};
use tlacuache_core::ops::{OpId, OpKind, OpState};

use crate::fs::ops_runner::OpsManager;
use crate::strings;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct OpsIndicator {
        pub progress: gtk::ProgressBar,
        pub label: gtk::Label,
        pub cancel: gtk::Button,
        pub manager: OnceCell<OpsManager>,
        pub current: Cell<Option<OpId>>,
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
        imp.progress.set_valign(gtk::Align::Center);
        imp.progress.set_width_request(140);
        imp.label.set_ellipsize(pango::EllipsizeMode::Middle);
        imp.label.set_max_width_chars(36);
        imp.label.add_css_class("caption");
        imp.cancel.set_icon_name("process-stop-symbolic");
        imp.cancel.add_css_class("flat");
        imp.cancel.set_tooltip_text(Some(strings::OP_CANCEL));
        imp.cancel.set_focusable(false);
        imp.cancel.connect_clicked(glib::clone!(
            #[weak(rename_to = indicator)]
            self,
            move |_| {
                let imp = indicator.imp();
                if let (Some(id), Some(manager)) = (imp.current.get(), imp.manager.get()) {
                    manager.cancel(id);
                }
            }
        ));

        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_valign(gtk::Align::Center);
        text.append(&imp.label);
        text.append(&imp.progress);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        content.append(&text);
        content.append(&imp.cancel);
        self.set_child(Some(&content));
        self.set_visible(false);

        if let Some(manager) = imp.manager.get() {
            manager.connect_changed(glib::clone!(
                #[weak(rename_to = indicator)]
                self,
                move |_| indicator.update()
            ));
        }
    }

    /// Muestra la primera operación en curso (o en cola).
    fn update(&self) {
        let imp = self.imp();
        let Some(manager) = imp.manager.get() else {
            return;
        };
        let ops = manager.operations();
        let shown = ops
            .iter()
            .find(|op| op.state.is_active())
            .or_else(|| ops.iter().find(|op| op.state == OpState::Queued));
        let Some(op) = shown else {
            imp.current.set(None);
            self.set_visible(false);
            return;
        };
        imp.current.set(Some(op.id));
        self.set_visible(true);

        let verb = match op.kind {
            OpKind::Move => strings::OP_MOVING,
            _ => strings::OP_COPYING,
        };
        let progress = &op.progress;
        let text = if progress.files_total.is_none() {
            strings::OP_PREPARING.to_owned()
        } else {
            strings::op_progress(
                verb,
                progress.files_done,
                progress.files_total,
                progress.current.as_deref(),
            )
        };
        imp.label.set_text(&text);
        match progress.fraction() {
            Some(fraction) => imp.progress.set_fraction(fraction),
            None => imp.progress.pulse(),
        }
        imp.cancel.set_sensitive(op.state != OpState::Cancelling);
    }
}
