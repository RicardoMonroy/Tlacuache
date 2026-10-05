//! Sección «Favoritos» de la barra lateral: grupos plegables de carpetas.
//!
//! - Añadir: arrastrar carpetas desde las vistas (o de otra app) sobre un
//!   grupo o un favorito, o Ctrl+D con la carpeta actual.
//! - Reordenar: arrastrar un favorito dentro de su grupo o a otro.
//! - Menú contextual: abrir/quitar favoritos; crear, renombrar y eliminar
//!   grupos.
//!
//! Los cambios se guardan en `[[favorites]]` de config.toml con
//! `update_favorites_toml` (conserva comentarios), con E/S asíncrona y un
//! retardo corto para agrupar cambios seguidos.

use std::cell::{OnceCell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::{gdk, gio, glib, pango};
use tlacuache_core::config::{self, FavoriteGroup};
use tlacuache_core::favorites::{self, FavoritesError};
use tlacuache_core::path_input::expand_tilde;

use crate::fs::config_file;
use crate::fs::display::{display_name, display_path};
use crate::ui::dnd;
use crate::{strings, window};

const SAVE_DELAY: Duration = Duration::from_millis(400);

/// Qué representa cada fila de la lista, por índice.
#[derive(Clone, Copy, Debug)]
pub enum FavoriteRowKind {
    Group(usize),
    Item(usize, usize),
    /// Fila de ayuda cuando no hay favoritos (también acepta soltar).
    Placeholder,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FavoritesSection {
        pub list: gtk::ListBox,
        pub rows: RefCell<Vec<FavoriteRowKind>>,
        pub groups: RefCell<Vec<FavoriteGroup>>,
        /// Grupos plegados (por nombre; solo en memoria).
        pub collapsed: RefCell<HashSet<String>>,
        pub config_path: RefCell<Option<PathBuf>>,
        pub save_timer: RefCell<Option<glib::SourceId>>,
        pub menu: OnceCell<gtk::PopoverMenu>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FavoritesSection {
        const NAME: &'static str = "TlacuacheFavoritesSection";
        type Type = super::FavoritesSection;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            let position = <(u32, u32)>::static_variant_type();
            let index = u32::static_variant_type();
            klass.install_action("fav.open", Some(&position), |section, _, param| {
                if let Some((group, item)) = param.and_then(|p| p.get::<(u32, u32)>()) {
                    section.open_favorite(group as usize, item as usize);
                }
            });
            klass.install_action("fav.remove", Some(&position), |section, _, param| {
                if let Some((group, item)) = param.and_then(|p| p.get::<(u32, u32)>()) {
                    section.apply(|groups| {
                        favorites::remove_favorite(groups, (group as usize, item as usize))
                            .map(drop)
                    });
                }
            });
            klass.install_action("group.new", None, |section, _, _| section.new_group());
            klass.install_action("group.rename", Some(&index), |section, _, param| {
                if let Some(group) = param.and_then(|p| p.get::<u32>()) {
                    section.rename_group(group as usize);
                }
            });
            klass.install_action("group.remove", Some(&index), |section, _, param| {
                if let Some(group) = param.and_then(|p| p.get::<u32>()) {
                    section.remove_group(group as usize);
                }
            });
        }
    }

    impl ObjectImpl for FavoritesSection {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("favorite-activated")
                        .param_types([gio::File::static_type()])
                        .build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }

        fn dispose(&self) {
            if let Some(menu) = self.menu.get() {
                menu.unparent();
            }
            if let Some(id) = self.save_timer.take() {
                id.remove();
            }
        }
    }

    impl WidgetImpl for FavoritesSection {}
    impl BinImpl for FavoritesSection {}
}

glib::wrapper! {
    pub struct FavoritesSection(ObjectSubclass<imp::FavoritesSection>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for FavoritesSection {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl FavoritesSection {
    pub fn connect_favorite_activated<F: Fn(&Self, &gio::File) + 'static>(&self, f: F) {
        self.connect_closure(
            "favorite-activated",
            false,
            glib::closure_local!(move |section: &Self, file: &gio::File| f(section, file)),
        );
    }

    fn setup(&self) {
        let imp = self.imp();

        let add_group = gtk::Button::from_icon_name("list-add-symbolic");
        add_group.add_css_class("flat");
        add_group.add_css_class("circular");
        add_group.set_tooltip_text(Some(strings::GROUP_NEW));
        add_group.set_action_name(Some("group.new"));
        add_group.set_focusable(false);

        let heading = gtk::Label::builder()
            .label(strings::SIDEBAR_FAVORITES)
            .xalign(0.0)
            .hexpand(true)
            .build();
        heading.add_css_class("heading");
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        header.add_css_class("tl-section-title");
        header.append(&heading);
        header.append(&add_group);
        // Soltar sobre el título: al primer grupo.
        self.add_drop_target(&header, FavoriteRowKind::Placeholder);

        imp.list.add_css_class("navigation-sidebar");
        imp.list.set_selection_mode(gtk::SelectionMode::None);
        imp.list.connect_row_activated(glib::clone!(
            #[weak(rename_to = section)]
            self,
            move |_, row| section.activate_row(row.index())
        ));

        // El popover cuelga de la sección, no de la lista: `ListBox::remove_all`
        // quita hijos hasta vaciarla y un hijo que no es fila lo atasca.
        let menu = gtk::PopoverMenu::from_model(None::<&gio::MenuModel>);
        menu.set_parent(self);
        menu.set_has_arrow(false);
        let _ = imp.menu.set(menu);
        let right_click = gtk::GestureClick::new();
        right_click.set_button(gdk::BUTTON_SECONDARY);
        right_click.connect_pressed(glib::clone!(
            #[weak(rename_to = section)]
            self,
            move |_, _, x, y| section.show_menu(x, y)
        ));
        imp.list.add_controller(right_click);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&header);
        content.append(&imp.list);
        self.set_child(Some(&content));
    }

    /// Carga los favoritos de la config y dónde guardarlos.
    pub fn set_favorites(&self, groups: Vec<FavoriteGroup>, config_path: PathBuf) {
        let imp = self.imp();
        imp.groups.replace(groups);
        imp.config_path.replace(Some(config_path));
        self.rebuild();
    }

    /// Añade `dir` al primer grupo (creándolo si no hay ninguno).
    pub fn add_favorite(&self, dir: &gio::File) {
        let path = stored_path(dir);
        self.apply(move |groups| {
            let group = favorites::default_group(groups, strings::FAVORITES_DEFAULT_GROUP);
            favorites::insert_favorite(groups, group, None, &path).map(drop)
        });
    }

    /// Aplica un cambio a los grupos; si funciona, redibuja y guarda.
    fn apply<F>(&self, change: F)
    where
        F: FnOnce(&mut Vec<FavoriteGroup>) -> Result<(), FavoritesError>,
    {
        let result = change(&mut self.imp().groups.borrow_mut());
        match result {
            Ok(()) => {
                self.rebuild();
                self.schedule_save();
            }
            Err(err) => window::show_toast_from(self, &err.to_string()),
        }
    }

    fn rebuild(&self) {
        let imp = self.imp();
        imp.list.remove_all();
        let groups = imp.groups.borrow().clone();
        let collapsed = imp.collapsed.borrow().clone();
        let mut rows = Vec::new();

        if groups.iter().all(|g| g.paths.is_empty()) && groups.len() <= 1 {
            let label = gtk::Label::builder()
                .label(strings::FAVORITES_EMPTY)
                .xalign(0.0)
                .wrap(true)
                .build();
            label.add_css_class("dim-label");
            let row = gtk::ListBoxRow::builder()
                .child(&label)
                .activatable(false)
                .build();
            self.add_drop_target(&row, FavoriteRowKind::Placeholder);
            imp.list.append(&row);
            rows.push(FavoriteRowKind::Placeholder);
        }

        for (g, group) in groups.iter().enumerate() {
            let is_collapsed = collapsed.contains(&group.group);
            let kind = FavoriteRowKind::Group(g);
            let row = group_row(&group.group, is_collapsed);
            self.add_drop_target(&row, kind);
            imp.list.append(&row);
            rows.push(kind);
            if is_collapsed {
                continue;
            }
            for (i, path) in group.paths.iter().enumerate() {
                let kind = FavoriteRowKind::Item(g, i);
                let row = favorite_row(path);
                self.add_drop_target(&row, kind);
                add_favorite_drag(&row, g, i);
                imp.list.append(&row);
                rows.push(kind);
            }
        }
        imp.rows.replace(rows);
    }

    fn row_kind(&self, index: i32) -> Option<FavoriteRowKind> {
        let index = usize::try_from(index).ok()?;
        self.imp().rows.borrow().get(index).copied()
    }

    fn activate_row(&self, index: i32) {
        match self.row_kind(index) {
            Some(FavoriteRowKind::Item(g, i)) => self.open_favorite(g, i),
            Some(FavoriteRowKind::Group(g)) => self.toggle_group(g),
            Some(FavoriteRowKind::Placeholder) | None => {}
        }
    }

    fn open_favorite(&self, group: usize, index: usize) {
        let path = self
            .imp()
            .groups
            .borrow()
            .get(group)
            .and_then(|g| g.paths.get(index).cloned());
        if let Some(path) = path {
            self.emit_by_name::<()>("favorite-activated", &[&favorite_file(&path)]);
        }
    }

    fn toggle_group(&self, group: usize) {
        let imp = self.imp();
        let Some(name) = imp.groups.borrow().get(group).map(|g| g.group.clone()) else {
            return;
        };
        {
            let mut collapsed = imp.collapsed.borrow_mut();
            if !collapsed.remove(&name) {
                collapsed.insert(name);
            }
        }
        self.rebuild();
    }

    fn show_menu(&self, x: f64, y: f64) {
        let imp = self.imp();
        // Coordenadas de f64 a píxeles enteros para ubicar la fila y el menú.
        let (x, y) = (x as i32, y as i32);
        let kind = imp
            .list
            .row_at_y(y)
            .and_then(|row| self.row_kind(row.index()));
        let menu = gio::Menu::new();
        match kind {
            Some(FavoriteRowKind::Item(g, i)) => {
                let target = (g as u32, i as u32).to_variant();
                menu.append_item(&menu_item(strings::ACTION_OPEN, "fav.open", Some(&target)));
                menu.append_item(&menu_item(
                    strings::FAVORITE_REMOVE,
                    "fav.remove",
                    Some(&target),
                ));
            }
            Some(FavoriteRowKind::Group(g)) => {
                let target = (g as u32).to_variant();
                menu.append_item(&menu_item(
                    strings::GROUP_RENAME,
                    "group.rename",
                    Some(&target),
                ));
                menu.append_item(&menu_item(
                    strings::GROUP_REMOVE,
                    "group.remove",
                    Some(&target),
                ));
                menu.append_item(&menu_item(strings::GROUP_NEW, "group.new", None));
            }
            Some(FavoriteRowKind::Placeholder) | None => {
                menu.append_item(&menu_item(strings::GROUP_NEW, "group.new", None));
            }
        }
        // Coordenadas de la lista a las de la sección (padre del popover).
        let point = imp
            .list
            .compute_point(self, &gtk::graphene::Point::new(x as f32, y as f32))
            .unwrap_or_else(|| gtk::graphene::Point::new(x as f32, y as f32));
        if let Some(popover) = imp.menu.get() {
            popover.set_menu_model(Some(&menu));
            let rect = gdk::Rectangle::new(point.x() as i32, point.y() as i32, 1, 1);
            popover.set_pointing_to(Some(&rect));
            popover.popup();
        }
    }

    fn new_group(&self) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = section)]
            self,
            async move {
                let name = section
                    .ask_group_name(strings::GROUP_NEW_TITLE, strings::ACTION_CREATE, "")
                    .await;
                if let Some(name) = name {
                    section.apply(|groups| favorites::add_group(groups, &name).map(drop));
                }
            }
        ));
    }

    fn rename_group(&self, group: usize) {
        let Some(current) = self
            .imp()
            .groups
            .borrow()
            .get(group)
            .map(|g| g.group.clone())
        else {
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = section)]
            self,
            async move {
                let name = section
                    .ask_group_name(
                        strings::GROUP_RENAME_TITLE,
                        strings::ACTION_RENAME,
                        &current,
                    )
                    .await;
                if let Some(name) = name {
                    section.apply(|groups| favorites::rename_group(groups, group, &name));
                }
            }
        ));
    }

    /// Elimina un grupo; si tiene favoritos, pide confirmación.
    fn remove_group(&self, group: usize) {
        let Some((name, count)) = self
            .imp()
            .groups
            .borrow()
            .get(group)
            .map(|g| (g.group.clone(), g.paths.len()))
        else {
            return;
        };
        if count == 0 {
            self.apply(|groups| favorites::remove_group(groups, group).map(drop));
            return;
        }
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = section)]
            self,
            async move {
                let dialog = adw::AlertDialog::new(
                    Some(strings::GROUP_REMOVE),
                    Some(&strings::group_remove_confirm(&name, count)),
                );
                dialog.add_responses(&[
                    ("cancel", strings::ACTION_CANCEL),
                    ("remove", strings::ACTION_REMOVE),
                ]);
                dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
                dialog.set_default_response(Some("cancel"));
                dialog.set_close_response("cancel");
                if dialog.choose_future(Some(&section)).await == "remove" {
                    section.apply(|groups| favorites::remove_group(groups, group).map(drop));
                }
            }
        ));
    }

    async fn ask_group_name(&self, heading: &str, confirm: &str, initial: &str) -> Option<String> {
        let entry = gtk::Entry::builder()
            .text(initial)
            .placeholder_text(strings::GROUP_NAME_PLACEHOLDER)
            .activates_default(true)
            .build();
        let dialog = adw::AlertDialog::new(Some(heading), None);
        dialog.add_responses(&[("cancel", strings::ACTION_CANCEL), ("ok", confirm)]);
        dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("ok"));
        dialog.set_close_response("cancel");
        dialog.set_extra_child(Some(&entry));
        let response = dialog.choose_future(Some(self)).await;
        (response == "ok").then(|| entry.text().to_string())
    }

    /// Acepta carpetas (`gdk::FileList`) y favoritos arrastrados (texto
    /// interno) sobre `widget`, que representa `kind`.
    fn add_drop_target(&self, widget: &impl IsA<gtk::Widget>, kind: FavoriteRowKind) {
        let target = gtk::DropTarget::new(
            glib::Type::INVALID,
            gdk::DragAction::COPY | gdk::DragAction::MOVE,
        );
        target.set_types(&[gdk::FileList::static_type(), String::static_type()]);
        target.connect_drop(glib::clone!(
            #[weak(rename_to = section)]
            self,
            #[upgrade_or]
            false,
            move |_, value, _, _| section.on_drop(kind, value)
        ));
        widget.add_controller(target);
    }

    fn on_drop(&self, kind: FavoriteRowKind, value: &glib::Value) -> bool {
        // Destino: (grupo, posición) o el grupo por defecto.
        let destination = match kind {
            FavoriteRowKind::Item(g, i) => Some((g, i)),
            FavoriteRowKind::Group(g) => {
                let len = self
                    .imp()
                    .groups
                    .borrow()
                    .get(g)
                    .map_or(0, |g| g.paths.len());
                Some((g, len))
            }
            FavoriteRowKind::Placeholder => None,
        };

        if let Ok(text) = value.get::<String>() {
            let Some(from) = dnd::parse_favorite_token(&text) else {
                return false;
            };
            let to = destination.unwrap_or((0, 0));
            self.apply(|groups| favorites::move_favorite(groups, from, to));
            return true;
        }

        let Ok(files) = value.get::<gdk::FileList>() else {
            return false;
        };
        let files = files.files();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = section)]
            self,
            async move {
                let mut dirs = Vec::new();
                for file in files {
                    let is_dir = file
                        .query_info_future(
                            gio::FILE_ATTRIBUTE_STANDARD_TYPE,
                            gio::FileQueryInfoFlags::NONE,
                            glib::Priority::DEFAULT,
                        )
                        .await
                        .is_ok_and(|info| info.file_type() == gio::FileType::Directory);
                    if is_dir {
                        dirs.push(stored_path(&file));
                    }
                }
                if dirs.is_empty() {
                    window::show_toast_from(&section, strings::FAVORITES_ONLY_FOLDERS);
                    return;
                }
                section.apply(move |groups| {
                    let (group, mut index) = match destination {
                        Some(destination) => destination,
                        None => {
                            let group =
                                favorites::default_group(groups, strings::FAVORITES_DEFAULT_GROUP);
                            (group, groups.get(group).map_or(0, |g| g.paths.len()))
                        }
                    };
                    for path in &dirs {
                        if favorites::insert_favorite(groups, group, Some(index), path)? {
                            index += 1;
                        }
                    }
                    Ok(())
                });
            }
        ));
        true
    }

    fn schedule_save(&self) {
        let imp = self.imp();
        if let Some(id) = imp.save_timer.take() {
            id.remove();
        }
        let id = glib::timeout_add_local_once(
            SAVE_DELAY,
            glib::clone!(
                #[weak(rename_to = section)]
                self,
                move || {
                    section.imp().save_timer.take();
                    glib::spawn_future_local(async move { section.save().await });
                }
            ),
        );
        imp.save_timer.replace(Some(id));
    }

    /// Reescribe `[[favorites]]` en config.toml. Si el archivo tiene un error
    /// de sintaxis no se toca (se avisa con un toast).
    async fn save(&self) {
        let imp = self.imp();
        let Some(path) = imp.config_path.borrow().clone() else {
            return;
        };
        let groups = imp.groups.borrow().clone();
        let result =
            config_file::update(&path, |text| config::update_favorites_toml(text, &groups)).await;
        if let Err(err) = result {
            self.report_save_error(&err);
        }
    }

    fn report_save_error(&self, detail: &str) {
        tracing::warn!("no se guardaron los favoritos: {detail}");
        window::show_toast_from(self, &strings::favorites_save_failed(detail));
    }
}

/// Texto que se guarda para una carpeta: `~/…` si es local, URI si no.
fn stored_path(file: &gio::File) -> String {
    match file.path() {
        Some(path) => favorites::contract_home(&path, &glib::home_dir()),
        None => file.uri().to_string(),
    }
}

/// Archivo de un favorito guardado (ruta con `~` o URI).
fn favorite_file(stored: &str) -> gio::File {
    if stored.contains("://") {
        gio::File::for_uri(stored)
    } else {
        gio::File::for_path(expand_tilde(stored, &glib::home_dir()))
    }
}

fn menu_item(label: &str, action: &str, target: Option<&glib::Variant>) -> gio::MenuItem {
    let item = gio::MenuItem::new(Some(label), None);
    item.set_action_and_target_value(Some(action), target);
    item
}

fn group_row(name: &str, collapsed: bool) -> gtk::ListBoxRow {
    let arrow = gtk::Image::from_icon_name(if collapsed {
        "pan-end-symbolic"
    } else {
        "pan-down-symbolic"
    });
    let label = gtk::Label::builder()
        .label(name)
        .xalign(0.0)
        .ellipsize(pango::EllipsizeMode::End)
        .build();
    label.add_css_class("tl-favorite-group");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.append(&arrow);
    content.append(&label);
    gtk::ListBoxRow::builder().child(&content).build()
}

fn favorite_row(stored: &str) -> gtk::ListBoxRow {
    let file = favorite_file(stored);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    content.set_margin_start(16);
    content.append(&gtk::Image::from_icon_name("folder-symbolic"));
    content.append(
        &gtk::Label::builder()
            .label(display_name(&file))
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .build(),
    );
    let row = gtk::ListBoxRow::builder().child(&content).build();
    row.set_tooltip_text(Some(&display_path(&file)));
    row
}

fn add_favorite_drag(row: &gtk::ListBoxRow, group: usize, index: usize) {
    let source = gtk::DragSource::new();
    source.set_actions(gdk::DragAction::MOVE);
    source.connect_prepare(move |_, _, _| {
        let token = dnd::favorite_token(group, index);
        Some(gdk::ContentProvider::for_value(&token.to_value()))
    });
    source.connect_drag_begin(|source, _| {
        if let Some(widget) = source.widget() {
            source.set_icon(Some(&gtk::WidgetPaintable::new(Some(&widget))), 0, 0);
        }
    });
    row.add_controller(source);
}
