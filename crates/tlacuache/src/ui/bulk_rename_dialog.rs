//! Renombrado masivo (8.7, F2 con varios seleccionados): regla regex,
//! vista previa en vivo «actual → nuevo» y conflictos marcados antes de
//! aplicar. Al confirmar se encola una operación `RenameMany` (se deshace
//! con Ctrl+Z).

use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, pango};
use tlacuache_core::bulk_rename::{self, Item, PlanSummary, Planned, Rule, Status};

use crate::strings;

/// Filas de vista previa como máximo (el plan cubre todos).
const PREVIEW_ROWS: usize = 200;

/// Recibe (orígenes, destinos) de lo que hay que renombrar.
pub type OnApply = Rc<dyn Fn(Vec<gio::File>, Vec<gio::File>)>;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct BulkRenameDialog {
        pub files: OnceCell<Vec<gio::File>>,
        pub items: OnceCell<Vec<Item>>,
        /// Nombres de cada carpeta (se cargan al abrir).
        pub existing: RefCell<Option<HashMap<String, HashSet<String>>>>,
        pub plan: RefCell<Vec<Planned>>,
        pub on_apply: OnceCell<OnApply>,
        pub find: adw::EntryRow,
        pub replace: adw::EntryRow,
        pub include_extension: adw::SwitchRow,
        pub case_sensitive: adw::SwitchRow,
        pub preview: gtk::ListBox,
        pub summary: gtk::Label,
        pub apply: gtk::Button,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BulkRenameDialog {
        const NAME: &'static str = "TlacuacheBulkRenameDialog";
        type Type = super::BulkRenameDialog;
        type ParentType = adw::Dialog;
    }

    impl ObjectImpl for BulkRenameDialog {}
    impl WidgetImpl for BulkRenameDialog {}
    impl AdwDialogImpl for BulkRenameDialog {}
}

glib::wrapper! {
    pub struct BulkRenameDialog(ObjectSubclass<imp::BulkRenameDialog>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl BulkRenameDialog {
    /// `files`: elementos a renombrar (con su tipo para separar la
    /// extensión).
    pub fn new(files: Vec<(gio::File, bool)>, on_apply: OnApply) -> Self {
        let dialog: Self = glib::Object::new();
        let imp = dialog.imp();
        let items = files
            .iter()
            .map(|(file, is_dir)| Item {
                dir: file
                    .parent()
                    .map(|p| p.uri().to_string())
                    .unwrap_or_default(),
                name: file
                    .basename()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                is_dir: *is_dir,
            })
            .collect();
        let _ = imp.items.set(items);
        let _ = imp.files.set(files.into_iter().map(|(f, _)| f).collect());
        let _ = imp.on_apply.set(on_apply);
        dialog.build();
        dialog.load_existing();
        dialog
    }

    fn build(&self) {
        let imp = self.imp();
        let count = imp.items.get().map_or(0, Vec::len);
        self.set_title(&strings::bulk_rename_title(count));
        self.set_content_width(560);
        self.set_content_height(560);

        imp.find.set_title(strings::BULK_FIND);
        imp.replace.set_title(strings::BULK_REPLACE);
        imp.include_extension
            .set_title(strings::BULK_INCLUDE_EXTENSION);
        imp.case_sensitive.set_title(strings::BULK_CASE_SENSITIVE);
        let rule = adw::PreferencesGroup::new();
        rule.set_description(Some(strings::BULK_HINT));
        rule.add(&imp.find);
        rule.add(&imp.replace);
        rule.add(&imp.include_extension);
        rule.add(&imp.case_sensitive);
        for entry in [&imp.find, &imp.replace] {
            entry.connect_changed(glib::clone!(
                #[weak(rename_to = dialog)]
                self,
                move |_| dialog.update()
            ));
        }
        for switch in [&imp.include_extension, &imp.case_sensitive] {
            switch.connect_active_notify(glib::clone!(
                #[weak(rename_to = dialog)]
                self,
                move |_| dialog.update()
            ));
        }

        imp.preview.add_css_class("boxed-list");
        imp.preview.set_selection_mode(gtk::SelectionMode::None);
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&imp.preview)
            .build();
        imp.summary.set_xalign(0.0);
        imp.summary.set_wrap(true);
        imp.summary.add_css_class("dim-label");

        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.set_margin_top(12);
        body.set_margin_bottom(12);
        body.set_margin_start(12);
        body.set_margin_end(12);
        body.append(&rule);
        body.append(&scrolled);
        body.append(&imp.summary);

        let cancel = gtk::Button::with_label(strings::ACTION_CANCEL);
        cancel.connect_clicked(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |_| {
                dialog.close();
            }
        ));
        imp.apply.set_label(strings::BULK_APPLY);
        imp.apply.add_css_class("suggested-action");
        imp.apply.set_sensitive(false);
        imp.apply.connect_clicked(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |_| dialog.apply()
        ));
        let header = adw::HeaderBar::new();
        header.set_show_end_title_buttons(false);
        header.set_show_start_title_buttons(false);
        header.pack_start(&cancel);
        header.pack_end(&imp.apply);
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&body));
        self.set_child(Some(&toolbar));
        self.set_focus(Some(&imp.find));
        self.update();
    }

    /// Nombres de cada carpeta implicada, para detectar «ya existe».
    fn load_existing(&self) {
        let dirs: HashSet<String> = self
            .imp()
            .items
            .get()
            .map(|items| items.iter().map(|i| i.dir.clone()).collect())
            .unwrap_or_default();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                let mut existing = HashMap::new();
                for dir in dirs {
                    let names = list_names(&gio::File::for_uri(&dir)).await;
                    existing.insert(dir, names);
                }
                dialog.imp().existing.replace(Some(existing));
                dialog.update();
            }
        ));
    }

    fn rule(&self) -> Rule {
        let imp = self.imp();
        Rule {
            find: imp.find.text().to_string(),
            replace: imp.replace.text().to_string(),
            include_extension: imp.include_extension.is_active(),
            case_sensitive: imp.case_sensitive.is_active(),
        }
    }

    /// Recalcula el plan y la vista previa.
    fn update(&self) {
        let imp = self.imp();
        let Some(items) = imp.items.get() else {
            return;
        };
        let loaded = imp.existing.borrow().is_some();
        let existing = imp.existing.borrow().clone().unwrap_or_default();
        let plan = match bulk_rename::plan(items, &existing, &self.rule()) {
            Ok(plan) => plan,
            Err(err) => {
                imp.summary
                    .set_text(&strings::bulk_bad_regex(&err.to_string()));
                imp.apply.set_sensitive(false);
                imp.find.add_css_class("error");
                return;
            }
        };
        imp.find.remove_css_class("error");

        while let Some(row) = imp.preview.first_child() {
            imp.preview.remove(&row);
        }
        for planned in plan.iter().take(PREVIEW_ROWS) {
            imp.preview.append(&preview_row(planned));
        }
        let summary = PlanSummary::of(&plan);
        imp.summary.set_text(&strings::bulk_summary(
            summary.changes,
            summary.conflicts,
            plan.len().saturating_sub(PREVIEW_ROWS),
        ));
        imp.apply.set_sensitive(loaded && summary.can_apply());
        imp.plan.replace(plan);
    }

    fn apply(&self) {
        let imp = self.imp();
        let plan = imp.plan.borrow();
        if !PlanSummary::of(&plan).can_apply() {
            return;
        }
        let (Some(files), Some(on_apply)) = (imp.files.get(), imp.on_apply.get()) else {
            return;
        };
        let (sources, targets): (Vec<_>, Vec<_>) = files
            .iter()
            .zip(plan.iter())
            .filter(|(_, p)| p.status == Status::Renamed)
            .filter_map(|(file, p)| Some((file.clone(), file.parent()?.child(&p.to))))
            .unzip();
        drop(plan);
        on_apply(sources, targets);
        self.close();
    }
}

/// Fila «actual → nuevo»; los conflictos en rojo con su motivo.
fn preview_row(planned: &Planned) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_margin_top(6);
    row.set_margin_bottom(6);
    row.set_margin_start(10);
    row.set_margin_end(10);
    let label = |text: &str| {
        gtk::Label::builder()
            .label(text)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(pango::EllipsizeMode::Middle)
            .build()
    };
    let from = label(&planned.from);
    let arrow = gtk::Label::new(Some("→"));
    arrow.add_css_class("dim-label");
    let to = label(&planned.to);
    match planned.status {
        Status::Unchanged => to.add_css_class("dim-label"),
        Status::Renamed => {}
        conflict => {
            to.add_css_class("error");
            row.set_tooltip_text(Some(&strings::bulk_conflict(conflict)));
        }
    }
    row.append(&from);
    row.append(&arrow);
    row.append(&to);
    row
}

async fn list_names(dir: &gio::File) -> HashSet<String> {
    let mut names = HashSet::new();
    let Ok(enumerator) = dir
        .enumerate_children_future(
            "standard::name",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            glib::Priority::DEFAULT,
        )
        .await
    else {
        return names;
    };
    while let Ok(batch) = enumerator
        .next_files_future(512, glib::Priority::DEFAULT)
        .await
    {
        if batch.is_empty() {
            break;
        }
        names.extend(
            batch
                .iter()
                .map(|info| info.name().to_string_lossy().into_owned()),
        );
    }
    names
}
