//! Barra de ruta: breadcrumb clicable que se convierte en `gtk::Entry`
//! editable (clic en la zona vacía o Ctrl+L) con autocompletado de carpetas.
//!
//! `gtk::EntryCompletion` está obsoleto desde GTK 4.10, así que las
//! sugerencias se muestran en un popover propio y la lógica vive en
//! `tlacuache_core::path_input`.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::OnceLock;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::{gdk, gio, glib};
use tlacuache_core::path_input;

use crate::fs::display::{display_name, display_path};
use crate::strings;
use crate::window;

const PAGE_CRUMBS: &str = "crumbs";
const PAGE_EDIT: &str = "edit";
const MAX_SUGGESTIONS: usize = 12;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PathBar {
        pub stack: gtk::Stack,
        pub crumbs: gtk::Box,
        pub crumbs_scroll: gtk::ScrolledWindow,
        pub entry: gtk::Entry,
        pub popover: gtk::Popover,
        pub suggestions: gtk::ListBox,
        pub directory: RefCell<Option<gio::File>>,
        /// Subcarpetas (URI de la carpeta, nombres) para autocompletar.
        pub candidates: RefCell<Option<(String, Vec<String>)>>,
        pub listing: RefCell<Option<glib::JoinHandle<()>>>,
        /// Ignora `changed` cuando el texto lo pone el propio widget.
        pub programmatic: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PathBar {
        const NAME: &'static str = "TlacuachePathBar";
        type Type = super::PathBar;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for PathBar {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // El usuario eligió una carpeta (crumb o ruta escrita).
                    Signal::builder("navigate")
                        .param_types([gio::File::static_type()])
                        .build(),
                    // Terminó la edición con Enter o Esc: devolver el foco.
                    Signal::builder("editing-done").build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }

        fn dispose(&self) {
            self.popover.unparent();
            if let Some(handle) = self.listing.take() {
                handle.abort();
            }
        }
    }

    impl WidgetImpl for PathBar {}
    impl BinImpl for PathBar {}
}

glib::wrapper! {
    pub struct PathBar(ObjectSubclass<imp::PathBar>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for PathBar {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PathBar {
    pub fn connect_navigate<F: Fn(&Self, &gio::File) + 'static>(&self, f: F) {
        self.connect_closure(
            "navigate",
            false,
            glib::closure_local!(move |bar: &Self, dir: &gio::File| f(bar, dir)),
        );
    }

    pub fn connect_editing_done<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_closure(
            "editing-done",
            false,
            glib::closure_local!(move |bar: &Self| f(bar)),
        );
    }

    fn setup(&self) {
        let imp = self.imp();
        self.add_css_class("tl-path-bar");

        imp.crumbs.add_css_class("path-crumbs");
        imp.crumbs_scroll
            .set_policy(gtk::PolicyType::External, gtk::PolicyType::Never);
        imp.crumbs_scroll.set_hexpand(true);
        imp.crumbs_scroll.set_child(Some(&imp.crumbs));
        // Mantener visible el final de la ruta cuando crece. Se difiere a un
        // idle porque `changed` llega antes de que termine la asignación.
        imp.crumbs_scroll.hadjustment().connect_changed(|adj| {
            glib::idle_add_local_once(glib::clone!(
                #[weak]
                adj,
                move || adj.set_value(adj.upper() - adj.page_size())
            ));
        });

        // Clic en la zona vacía: los botones reclaman sus propios clics.
        let click = gtk::GestureClick::new();
        click.connect_released(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            move |_, _, _, _| bar.start_editing()
        ));
        imp.crumbs_scroll.add_controller(click);

        imp.entry.set_hexpand(true);
        imp.entry.connect_changed(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            move |_| bar.on_text_changed()
        ));
        imp.entry.connect_activate(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            move |_| bar.on_activate()
        ));

        // Fase de captura: Tab no debe mover el foco a otro widget.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, mods| bar.on_key(key, mods)
        ));
        imp.entry.add_controller(keys);

        // Las sugerencias no son enfocables, así que salir del entry
        // siempre significa ir a otro widget.
        let focus = gtk::EventControllerFocus::new();
        focus.connect_leave(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            move |_| bar.stop_editing()
        ));
        imp.entry.add_controller(focus);

        imp.suggestions
            .set_selection_mode(gtk::SelectionMode::Single);
        imp.suggestions.set_activate_on_single_click(true);
        imp.suggestions.set_focusable(false);
        imp.suggestions.connect_row_activated(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            move |_, row| {
                if let Some(name) = suggestion_name(row) {
                    bar.apply_suggestion(&name);
                    bar.imp().entry.grab_focus_without_selecting();
                }
            }
        ));

        imp.popover.set_child(Some(&imp.suggestions));
        imp.popover.set_autohide(false);
        imp.popover.set_has_arrow(false);
        imp.popover.set_focusable(false);
        imp.popover.set_position(gtk::PositionType::Bottom);
        imp.popover.set_halign(gtk::Align::Start);
        imp.popover.add_css_class("menu");
        imp.popover.set_parent(&imp.entry);

        imp.stack.set_vhomogeneous(true);
        imp.stack.add_named(&imp.crumbs_scroll, Some(PAGE_CRUMBS));
        imp.stack.add_named(&imp.entry, Some(PAGE_EDIT));
        imp.stack.set_visible_child_name(PAGE_CRUMBS);
        self.set_child(Some(&imp.stack));
    }

    pub fn set_directory(&self, dir: &gio::File) {
        self.imp().directory.replace(Some(dir.clone()));
        self.rebuild_crumbs(dir);
        self.stop_editing();
    }

    pub fn start_editing(&self) {
        let imp = self.imp();
        let text = imp
            .directory
            .borrow()
            .as_ref()
            .map(display_path)
            .unwrap_or_default();
        self.set_text_silently(&text);
        imp.entry.remove_css_class("error");
        imp.stack.set_visible_child_name(PAGE_EDIT);
        imp.entry.grab_focus();
    }

    fn stop_editing(&self) {
        let imp = self.imp();
        imp.popover.popdown();
        if let Some(handle) = imp.listing.take() {
            handle.abort();
        }
        imp.candidates.replace(None);
        imp.stack.set_visible_child_name(PAGE_CRUMBS);
    }

    fn is_editing(&self) -> bool {
        self.imp().stack.visible_child_name().as_deref() == Some(PAGE_EDIT)
    }

    fn finish_editing(&self) {
        self.stop_editing();
        self.emit_by_name::<()>("editing-done", &[]);
    }

    fn rebuild_crumbs(&self, dir: &gio::File) {
        let crumbs = &self.imp().crumbs;
        while let Some(child) = crumbs.first_child() {
            crumbs.remove(&child);
        }

        let mut chain = vec![dir.clone()];
        while let Some(parent) = chain.last().and_then(|f| f.parent()) {
            chain.push(parent);
        }
        chain.reverse();

        let last = chain.len().saturating_sub(1);
        for (i, file) in chain.into_iter().enumerate() {
            if i > 0 {
                let sep = gtk::Label::new(Some("›"));
                sep.add_css_class("dim-label");
                crumbs.append(&sep);
            }
            let button = gtk::Button::with_label(&display_name(&file));
            button.add_css_class("flat");
            button.add_css_class("tl-crumb");
            button.set_focusable(false);
            if i == last {
                button.add_css_class("current");
            }
            button.connect_clicked(glib::clone!(
                #[weak(rename_to = bar)]
                self,
                move |_| bar.emit_by_name::<()>("navigate", &[&file])
            ));
            crumbs.append(&button);
        }
    }

    fn set_text_silently(&self, text: &str) {
        let imp = self.imp();
        imp.programmatic.set(true);
        imp.entry.set_text(text);
        imp.entry.set_position(-1);
        imp.programmatic.set(false);
    }

    fn on_text_changed(&self) {
        if self.imp().programmatic.get() || !self.is_editing() {
            return;
        }
        self.imp().entry.remove_css_class("error");
        self.refresh_suggestions();
    }

    /// Muestra las subcarpetas que coinciden; si la carpeta escrita aún no
    /// se ha listado, la lista en segundo plano y vuelve a llamarse.
    fn refresh_suggestions(&self) {
        let imp = self.imp();
        let text = imp.entry.text();
        let (dir_typed, partial) = path_input::split_for_completion(&text);
        let Some(dir) = self.resolve(dir_typed) else {
            imp.popover.popdown();
            return;
        };
        let uri = dir.uri().to_string();

        let cached = imp
            .candidates
            .borrow()
            .as_ref()
            .filter(|(cached_uri, _)| *cached_uri == uri)
            .map(|(_, names)| {
                path_input::completion_matches(partial, names.iter().map(String::as_str))
                    .into_iter()
                    .take(MAX_SUGGESTIONS)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            });
        match cached {
            Some(matches) => self.show_suggestions(partial, &matches),
            None => self.load_candidates(dir, uri),
        }
    }

    fn load_candidates(&self, dir: gio::File, uri: String) {
        let imp = self.imp();
        if let Some(handle) = imp.listing.take() {
            handle.abort();
        }
        let handle = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            async move {
                // Si falla (no existe, sin permiso) se cachea vacío para no
                // reintentar en cada tecla.
                let names = list_subdirs(&dir).await.unwrap_or_default();
                bar.imp().listing.take();
                bar.imp().candidates.replace(Some((uri, names)));
                if bar.is_editing() {
                    bar.refresh_suggestions();
                }
            }
        ));
        imp.listing.replace(Some(handle));
    }

    fn show_suggestions(&self, partial: &str, matches: &[String]) {
        let imp = self.imp();
        imp.suggestions.remove_all();
        let exact_only = matches.len() == 1 && matches[0] == partial;
        if matches.is_empty() || exact_only {
            imp.popover.popdown();
            return;
        }
        for name in matches {
            let label = gtk::Label::builder().label(name).xalign(0.0).build();
            let row = gtk::ListBoxRow::builder()
                .child(&label)
                .focusable(false)
                .build();
            imp.suggestions.append(&row);
        }
        imp.popover.popup();
    }

    fn on_key(&self, key: gdk::Key, mods: gdk::ModifierType) -> glib::Propagation {
        let imp = self.imp();
        let popup = imp.popover.is_visible();
        let plain = !mods.intersects(
            gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::ALT_MASK
                | gdk::ModifierType::SHIFT_MASK,
        );
        match key {
            gdk::Key::Tab if plain => {
                self.complete();
                glib::Propagation::Stop
            }
            gdk::Key::Down if popup => {
                self.move_selection(1);
                glib::Propagation::Stop
            }
            gdk::Key::Up if popup => {
                self.move_selection(-1);
                glib::Propagation::Stop
            }
            gdk::Key::Escape => {
                if popup {
                    imp.popover.popdown();
                } else {
                    self.finish_editing();
                }
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        }
    }

    fn move_selection(&self, delta: i32) {
        let list = &self.imp().suggestions;
        let current = list.selected_row().map_or(-1, |r| r.index());
        let target = (current + delta).max(0);
        if let Some(row) = list.row_at_index(target) {
            list.select_row(Some(&row));
        }
    }

    /// Tab: usa la sugerencia resaltada o extiende con el prefijo común.
    fn complete(&self) {
        let imp = self.imp();
        if imp.popover.is_visible()
            && let Some(name) = imp
                .suggestions
                .selected_row()
                .as_ref()
                .and_then(suggestion_name)
        {
            self.apply_suggestion(&name);
            return;
        }
        let text = imp.entry.text();
        let completed = imp.candidates.borrow().as_ref().and_then(|(_, names)| {
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            path_input::complete(&text, &names)
        });
        if let Some(completed) = completed {
            imp.entry.set_text(&completed);
            imp.entry.set_position(-1);
        }
    }

    fn apply_suggestion(&self, name: &str) {
        let entry = &self.imp().entry;
        let text = entry.text();
        let (dir_typed, _) = path_input::split_for_completion(&text);
        entry.set_text(&format!("{dir_typed}{name}/"));
        entry.set_position(-1);
    }

    /// Enter: navega a la sugerencia resaltada o a la ruta escrita. Si es un
    /// archivo, va a su carpeta.
    fn on_activate(&self) {
        let imp = self.imp();
        if imp.popover.is_visible()
            && let Some(name) = imp
                .suggestions
                .selected_row()
                .as_ref()
                .and_then(suggestion_name)
        {
            self.apply_suggestion(&name);
        }
        let text = imp.entry.text().to_string();
        let Some(target) = self.resolve(&text) else {
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            async move {
                let info = target
                    .query_info_future(
                        gio::FILE_ATTRIBUTE_STANDARD_TYPE,
                        gio::FileQueryInfoFlags::NONE,
                        glib::Priority::DEFAULT,
                    )
                    .await;
                let dir = match info.map(|i| i.file_type()) {
                    Ok(gio::FileType::Directory) => Some(target),
                    Ok(_) => target.parent(),
                    Err(err) => {
                        tracing::debug!("ruta inválida {text:?}: {err}");
                        None
                    }
                };
                match dir {
                    Some(dir) => {
                        bar.emit_by_name::<()>("navigate", &[&dir]);
                        bar.finish_editing();
                    }
                    None => {
                        bar.imp().entry.add_css_class("error");
                        window::show_toast_from(&bar, &strings::path_not_found(&text));
                    }
                }
            }
        ));
    }

    /// Convierte texto escrito en archivo: expande `~`, admite URIs y rutas
    /// relativas a la carpeta actual. Texto vacío = carpeta actual.
    fn resolve(&self, text: &str) -> Option<gio::File> {
        let current = self.imp().directory.borrow().clone();
        let text = text.trim();
        if text.is_empty() {
            return current;
        }
        let home = glib::home_dir();
        let expanded = path_input::expand_tilde(text, &home);
        let cwd: PathBuf = current.and_then(|d| d.path()).unwrap_or(home);
        Some(gio::File::for_commandline_arg_and_cwd(expanded, cwd))
    }
}

async fn list_subdirs(dir: &gio::File) -> Result<Vec<String>, glib::Error> {
    let attrs = format!(
        "{},{}",
        gio::FILE_ATTRIBUTE_STANDARD_NAME,
        gio::FILE_ATTRIBUTE_STANDARD_TYPE
    );
    let enumerator = dir
        .enumerate_children_future(
            &attrs,
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await?;
    let mut names = Vec::new();
    loop {
        let batch = enumerator
            .next_files_future(256, glib::Priority::DEFAULT)
            .await?;
        if batch.is_empty() {
            return Ok(names);
        }
        names.extend(
            batch
                .iter()
                .filter(|info| info.file_type() == gio::FileType::Directory)
                .map(|info| info.name().to_string_lossy().into_owned()),
        );
    }
}

fn suggestion_name(row: &gtk::ListBoxRow) -> Option<String> {
    row.child()
        .and_downcast::<gtk::Label>()
        .map(|label| label.text().to_string())
}
