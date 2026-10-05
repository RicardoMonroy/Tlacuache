//! Celda de nombre de las vistas: ícono `tl-*` coloreado y nombre, o con
//! `icon_style = "glyph"` un glifo monoespaciado y el nombre coloreado por
//! tipo (las carpetas con `/`), al estilo `LS_COLORS` (`docs/THEMES.md`).
//! Se vuelve a pintar sola al cambiar de tema, y se atenúa si el archivo
//! está cortado (Ctrl+X) pendiente de pegar.

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, pango};
use tlacuache_core::entry::FileEntry;
use tlacuache_core::filetype::{FileCategory, classify};
use tlacuache_core::theme::{GlyphKind, Theme};

use crate::fs::cut_marks;
use crate::ui::{set_category_icon, theme_manager};

/// Ícono y ancho del nombre en la vista de íconos.
const TILE_ICON_SIZE: i32 = 48;
const TILE_CHARS: i32 = 12;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FileNameCell {
        pub icon: gtk::Image,
        pub glyph: gtk::Label,
        pub name: gtk::Label,
        /// Última entrada mostrada (para repintar al cambiar de tema).
        pub entry: RefCell<Option<FileEntry>>,
        /// URI del archivo (para saber si está cortado).
        pub uri: RefCell<Option<String>>,
        pub theme_handler: RefCell<Option<glib::SignalHandlerId>>,
        pub cut_handler: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileNameCell {
        const NAME: &'static str = "TlacuacheFileNameCell";
        type Type = super::FileNameCell;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for FileNameCell {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }

        fn dispose(&self) {
            if let (Some(themes), Some(id)) = (theme_manager::get(), self.theme_handler.take()) {
                themes.disconnect(id);
            }
            if let Some(id) = self.cut_handler.take() {
                cut_marks::get().disconnect(id);
            }
        }
    }
    impl WidgetImpl for FileNameCell {}
    impl BoxImpl for FileNameCell {}
}

glib::wrapper! {
    pub struct FileNameCell(ObjectSubclass<imp::FileNameCell>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl FileNameCell {
    /// `ellipsize`: cómo recortar nombres largos.
    pub fn new(ellipsize: pango::EllipsizeMode) -> Self {
        let cell: Self = glib::Object::new();
        cell.imp().name.set_ellipsize(ellipsize);
        cell
    }

    /// Mosaico de la vista de íconos: ícono grande arriba y el nombre
    /// centrado debajo, en hasta dos líneas.
    pub fn new_tile() -> Self {
        let cell: Self = glib::Object::new();
        let imp = cell.imp();
        cell.set_orientation(gtk::Orientation::Vertical);
        cell.add_css_class("tl-tile");
        imp.icon.set_pixel_size(TILE_ICON_SIZE);
        imp.name.set_xalign(0.5);
        imp.name.set_justify(gtk::Justification::Center);
        imp.name.set_wrap(true);
        imp.name.set_wrap_mode(pango::WrapMode::WordChar);
        imp.name.set_lines(2);
        imp.name.set_ellipsize(pango::EllipsizeMode::Middle);
        imp.name.set_width_chars(TILE_CHARS);
        imp.name.set_max_width_chars(TILE_CHARS);
        imp.name.set_hexpand(false);
        cell
    }

    fn build(&self) {
        let imp = self.imp();
        self.set_orientation(gtk::Orientation::Horizontal);
        self.set_spacing(6);
        imp.icon.set_pixel_size(16);
        imp.glyph.add_css_class("tl-glyph");
        imp.glyph.set_width_chars(1);
        imp.glyph.set_visible(false);
        imp.name.add_css_class("tl-file-name");
        imp.name.set_xalign(0.0);
        imp.name.set_hexpand(true);
        self.append(&imp.icon);
        self.append(&imp.glyph);
        self.append(&imp.name);

        if let Some(themes) = theme_manager::get() {
            let id = themes.connect_theme_changed(glib::clone!(
                #[weak(rename_to = cell)]
                self,
                move |themes| cell.render(&themes.theme())
            ));
            imp.theme_handler.replace(Some(id));
        }
        let id = cut_marks::get().connect_changed(glib::clone!(
            #[weak(rename_to = cell)]
            self,
            move |_| cell.update_cut()
        ));
        imp.cut_handler.replace(Some(id));
    }

    /// Atenuada si su archivo está cortado.
    fn update_cut(&self) {
        let cut = self
            .imp()
            .uri
            .borrow()
            .as_deref()
            .is_some_and(|uri| cut_marks::get().contains(uri));
        if cut {
            self.add_css_class("tl-cut");
        } else {
            self.remove_css_class("tl-cut");
        }
    }

    /// Muestra `entry`; `uri` es la del archivo (para las marcas de cortado).
    pub fn set_entry(&self, entry: &FileEntry, uri: Option<String>) {
        self.imp().entry.replace(Some(entry.clone()));
        self.imp().uri.replace(uri);
        self.update_cut();
        let theme = theme_manager::get().map_or_else(Theme::default_theme, |t| t.theme());
        self.render(&theme);
    }

    fn render(&self, theme: &Theme) {
        let imp = self.imp();
        let entry = imp.entry.borrow();
        let Some(entry) = entry.as_ref() else {
            return;
        };
        let category = classify(
            entry.content_type.as_deref(),
            &entry.name,
            entry.is_dir,
            entry.is_executable,
        );
        let kind = GlyphKind::of(entry.is_dir, entry.is_symlink, entry.is_executable);
        for other in FileCategory::ALL {
            imp.glyph.remove_css_class(other.css_class());
            imp.name.remove_css_class(other.css_class());
        }
        match theme.glyph(kind) {
            Some(glyph) => {
                imp.glyph.set_text(glyph);
                imp.glyph.add_css_class(category.css_class());
                imp.name.add_css_class(category.css_class());
                imp.glyph.set_visible(true);
                imp.icon.set_visible(false);
            }
            None => {
                set_category_icon(&imp.icon, category);
                imp.icon.set_visible(true);
                imp.glyph.set_visible(false);
            }
        }
        imp.name
            .set_text(&theme.entry_name(&entry.name, entry.is_dir));
    }
}
