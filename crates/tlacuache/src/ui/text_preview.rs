//! Texto o código de la vista previa: `sourceview5::View` de solo lectura
//! con el lenguaje adivinado por nombre y tipo MIME, y el esquema de
//! sintaxis del tema activo (ADR-011). Avisa si el archivo se recortó.
//!
//! Markdown (8.3) se muestra renderizado por defecto, con un selector
//! «Vista / Código» que se recuerda mientras la app está abierta.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use sourceview5::prelude::*;

use crate::strings;
use crate::ui::markdown_view::MarkdownView;
use crate::ui::theme_manager;

const PAGE_CODE: &str = "code";
const PAGE_RENDERED: &str = "rendered";

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TextPreview {
        pub buffer: sourceview5::Buffer,
        pub note: gtk::Label,
        pub stack: gtk::Stack,
        pub markdown: MarkdownView,
        /// Selector «Vista / Código» (solo con Markdown).
        pub switcher: gtk::Box,
        pub show_rendered: gtk::ToggleButton,
        pub show_code: gtk::ToggleButton,
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
        let rendered = gtk::ScrolledWindow::builder()
            .child(&imp.markdown)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        imp.stack.add_named(&scroll, Some(PAGE_CODE));
        imp.stack.add_named(&rendered, Some(PAGE_RENDERED));

        // «Vista» y «Código», exclusivos; arranca en «Vista».
        imp.show_rendered.set_label(strings::MARKDOWN_RENDERED);
        imp.show_code.set_label(strings::MARKDOWN_SOURCE);
        imp.show_code.set_group(Some(&imp.show_rendered));
        imp.show_rendered.set_active(true);
        for button in [&imp.show_rendered, &imp.show_code] {
            button.set_focusable(false);
            button.connect_toggled(glib::clone!(
                #[weak(rename_to = preview)]
                self,
                move |button| {
                    if button.is_active() {
                        preview.update_page();
                    }
                }
            ));
        }
        imp.switcher.add_css_class("linked");
        imp.switcher.set_halign(gtk::Align::End);
        imp.switcher.set_margin_top(6);
        imp.switcher.set_margin_end(6);
        imp.switcher.append(&imp.show_rendered);
        imp.switcher.append(&imp.show_code);
        imp.switcher.set_visible(false);

        imp.note.add_css_class("tl-preview-note");
        imp.note.set_xalign(0.0);
        imp.note.set_visible(false);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&imp.switcher);
        content.append(&imp.stack);
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
        let is_markdown = tlacuache_core::markdown::is_markdown(content_type, name);
        if is_markdown {
            imp.markdown.set_markdown(text);
        }
        imp.switcher.set_visible(is_markdown);
        self.update_page();
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
        imp.markdown.buffer().set_text("");
        imp.note.set_visible(false);
    }

    /// Código, o Markdown renderizado si es Markdown y está elegido.
    fn update_page(&self) {
        let imp = self.imp();
        let rendered = imp.switcher.is_visible() && imp.show_rendered.is_active();
        imp.stack
            .set_visible_child_name(if rendered { PAGE_RENDERED } else { PAGE_CODE });
    }
}
