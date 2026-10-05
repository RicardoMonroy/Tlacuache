//! Texto o código de la vista previa: `sourceview5::View` de solo lectura
//! con el lenguaje adivinado por nombre y tipo MIME, y el esquema de
//! sintaxis del tema activo (ADR-011). Avisa si el archivo se recortó.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use sourceview5::prelude::*;

use crate::strings;
use crate::ui::theme_manager;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TextPreview {
        pub buffer: sourceview5::Buffer,
        pub note: gtk::Label,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TextPreview {
        const NAME: &'static str = "TlacuacheTextPreview";
        type Type = super::TextPreview;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for TextPreview {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }
    }
    impl WidgetImpl for TextPreview {}
    impl BinImpl for TextPreview {}
}

glib::wrapper! {
    pub struct TextPreview(ObjectSubclass<imp::TextPreview>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for TextPreview {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl TextPreview {
    fn build(&self) {
        let imp = self.imp();
        imp.buffer.set_highlight_syntax(true);
        imp.buffer.set_highlight_matching_brackets(false);
        let view = sourceview5::View::with_buffer(&imp.buffer);
        view.set_editable(false);
        view.set_cursor_visible(false);
        view.set_monospace(true);
        view.set_show_line_numbers(true);
        view.add_css_class("tl-preview-text");
        let scroll = gtk::ScrolledWindow::builder()
            .child(&view)
            .vexpand(true)
            .build();

        imp.note.add_css_class("tl-preview-note");
        imp.note.set_xalign(0.0);
        imp.note.set_visible(false);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&scroll);
        content.append(&imp.note);
        self.set_child(Some(&content));

        if let Some(themes) = theme_manager::get() {
            imp.buffer.set_style_scheme(themes.source_scheme().as_ref());
            themes.connect_source_scheme_changed(glib::clone!(
                #[weak(rename_to = preview)]
                self,
                move |themes| {
                    preview
                        .imp()
                        .buffer
                        .set_style_scheme(themes.source_scheme().as_ref());
                }
            ));
        }
    }

    /// Muestra `text`; `name` y `content_type` eligen el resaltado.
    /// `truncated_at`: el archivo se recortó a ese número de bytes.
    pub fn set_text(
        &self,
        text: &str,
        name: &str,
        content_type: Option<&str>,
        truncated_at: Option<u64>,
    ) {
        let imp = self.imp();
        let language =
            sourceview5::LanguageManager::default().guess_language(Some(name), content_type);
        imp.buffer.set_language(language.as_ref());
        imp.buffer.set_text(text);
        imp.buffer.place_cursor(&imp.buffer.start_iter());
        match truncated_at {
            Some(bytes) => {
                imp.note.set_text(&strings::preview_truncated(bytes));
                imp.note.set_visible(true);
            }
            None => imp.note.set_visible(false),
        }
    }

    /// Suelta el texto (al cambiar a otro archivo).
    pub fn clear(&self) {
        let imp = self.imp();
        imp.buffer.set_text("");
        imp.buffer.set_language(None);
        imp.note.set_visible(false);
    }
}
