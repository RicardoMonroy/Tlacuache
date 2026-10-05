//! Preferencias → Apariencia: tema fijo (lista con miniaturas) o seguir el
//! modo claro/oscuro del sistema con un tema para cada uno. Los cambios se
//! aplican al instante y se guardan en `[theme]` de config.toml.

use std::cell::{Cell, OnceCell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use tlacuache_core::config;
use tlacuache_core::theme::{Theme, Variant};

use crate::strings;
use crate::ui::theme_manager;
use crate::ui::theme_thumbnail::ThemeThumbnail;

/// Temas incluidos, en el orden de `Theme::builtin_ids`.
fn themes() -> Vec<Theme> {
    Theme::builtin_ids().filter_map(Theme::builtin).collect()
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PreferencesDialog {
        pub follow: adw::SwitchRow,
        pub light: adw::ComboRow,
        pub dark: adw::ComboRow,
        pub list: adw::PreferencesGroup,
        /// Casilla de cada tema de la lista, por id.
        pub checks: RefCell<Vec<(String, gtk::CheckButton)>>,
        /// Ids de las listas de `light` y `dark`.
        pub light_ids: OnceCell<Vec<String>>,
        pub dark_ids: OnceCell<Vec<String>>,
        /// Se están cargando los valores (no guardar).
        pub syncing: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesDialog {
        const NAME: &'static str = "TlacuachePreferencesDialog";
        type Type = super::PreferencesDialog;
        type ParentType = adw::PreferencesDialog;
    }

    impl ObjectImpl for PreferencesDialog {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }
    }
    impl WidgetImpl for PreferencesDialog {}
    impl AdwDialogImpl for PreferencesDialog {}
    impl PreferencesDialogImpl for PreferencesDialog {}
}

glib::wrapper! {
    pub struct PreferencesDialog(ObjectSubclass<imp::PreferencesDialog>)
        @extends adw::PreferencesDialog, adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for PreferencesDialog {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PreferencesDialog {
    fn build(&self) {
        let imp = self.imp();
        self.set_title(strings::PREFERENCES);
        let all = themes();

        imp.follow.set_title(strings::PREF_FOLLOW_SYSTEM);
        imp.follow.set_subtitle(strings::PREF_FOLLOW_SYSTEM_HINT);
        let (light_ids, light_names) = by_variant(&all, Variant::Light);
        let (dark_ids, dark_names) = by_variant(&all, Variant::Dark);
        imp.light.set_title(strings::PREF_LIGHT_THEME);
        imp.light
            .set_model(Some(&gtk::StringList::new(&light_names)));
        imp.dark.set_title(strings::PREF_DARK_THEME);
        imp.dark.set_model(Some(&gtk::StringList::new(&dark_names)));
        let _ = imp.light_ids.set(light_ids);
        let _ = imp.dark_ids.set(dark_ids);

        let mode = adw::PreferencesGroup::new();
        mode.set_title(strings::PREF_THEME);
        mode.add(&imp.follow);
        mode.add(&imp.light);
        mode.add(&imp.dark);

        imp.list.set_title(strings::PREF_THEMES);
        let mut group_leader: Option<gtk::CheckButton> = None;
        let mut checks = Vec::new();
        for theme in all {
            let check = gtk::CheckButton::new();
            check.set_valign(gtk::Align::Center);
            if let Some(leader) = &group_leader {
                check.set_group(Some(leader));
            } else {
                group_leader = Some(check.clone());
            }
            let row = adw::ActionRow::builder()
                .title(&theme.meta.name)
                .subtitle(&theme.meta.description)
                .activatable_widget(&check)
                .build();
            row.add_prefix(&check);
            let id = theme.meta.id.clone();
            row.add_suffix(&ThemeThumbnail::new(theme));
            check.connect_toggled(glib::clone!(
                #[weak(rename_to = dialog)]
                self,
                #[strong]
                id,
                move |check| {
                    if check.is_active() {
                        dialog.update(|s| s.name = id.clone());
                    }
                }
            ));
            imp.list.add(&row);
            checks.push((id, check));
        }
        imp.checks.replace(checks);

        let page = adw::PreferencesPage::builder()
            .title(strings::PREF_APPEARANCE)
            .icon_name("applications-graphics-symbolic")
            .build();
        page.add(&mode);
        page.add(&imp.list);
        self.add(&page);

        self.sync();

        imp.follow.connect_active_notify(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |row| {
                let follow = row.is_active();
                dialog.update(|s| s.follow_system = follow);
                dialog.update_visibility();
            }
        ));
        imp.light.connect_selected_notify(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |row| {
                if let Some(id) = dialog.selected_id(row, false) {
                    dialog.update(|s| s.light = id);
                }
            }
        ));
        imp.dark.connect_selected_notify(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |row| {
                if let Some(id) = dialog.selected_id(row, true) {
                    dialog.update(|s| s.dark = id);
                }
            }
        ));
    }

    /// Carga los valores de la config en los controles.
    fn sync(&self) {
        let Some(themes) = theme_manager::get() else {
            return;
        };
        let imp = self.imp();
        let settings = themes.settings();
        imp.syncing.set(true);
        imp.follow.set_active(settings.follow_system);
        for (row, ids, id) in [
            (&imp.light, imp.light_ids.get(), &settings.light),
            (&imp.dark, imp.dark_ids.get(), &settings.dark),
        ] {
            if let Some(index) = ids.and_then(|ids| ids.iter().position(|i| i == id)) {
                row.set_selected(index as u32);
            }
        }
        for (id, check) in imp.checks.borrow().iter() {
            if *id == settings.name {
                check.set_active(true);
            }
        }
        imp.syncing.set(false);
        self.update_visibility();
    }

    /// Con `follow_system`: los dos selectores; si no, la lista.
    fn update_visibility(&self) {
        let imp = self.imp();
        let follow = imp.follow.is_active();
        imp.light.set_visible(follow);
        imp.dark.set_visible(follow);
        imp.list.set_visible(!follow);
    }

    fn selected_id(&self, row: &adw::ComboRow, dark: bool) -> Option<String> {
        let imp = self.imp();
        let ids = if dark {
            imp.dark_ids.get()
        } else {
            imp.light_ids.get()
        }?;
        ids.get(row.selected() as usize).cloned()
    }

    /// Cambia `[theme]` y lo aplica (y guarda) al instante.
    fn update(&self, change: impl FnOnce(&mut config::Theme)) {
        if self.imp().syncing.get() {
            return;
        }
        if let Some(themes) = theme_manager::get() {
            let mut settings = themes.settings();
            change(&mut settings);
            themes.set_settings(settings);
        }
    }
}

/// Ids y nombres de los temas de una variante, en orden.
fn by_variant(themes: &[Theme], variant: Variant) -> (Vec<String>, Vec<&str>) {
    themes
        .iter()
        .filter(|t| t.meta.variant == variant)
        .map(|t| (t.meta.id.clone(), t.meta.name.as_str()))
        .unzip()
}
