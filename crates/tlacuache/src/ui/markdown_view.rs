//! Markdown renderizado (8.3): pinta los tramos de `core::markdown` en un
//! `TextView` de solo lectura. Las etiquetas toman colores y fuente del
//! tema activo (como la terminal) y se actualizan al cambiarlo. Los
//! enlaces web y de correo se abren con un clic.

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, pango};
use tlacuache_core::markdown::{self, Style};
use tlacuache_core::theme::Theme;

use crate::ui::{rgba, theme_manager};

/// Escala de los encabezados 1–6.
const HEADING_SCALES: [f64; 6] = [1.6, 1.4, 1.2, 1.1, 1.0, 1.0];

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct MarkdownView {
        /// Enlaces: (inicio, fin, destino) en desplazamientos de carácter.
        pub links: RefCell<Vec<(i32, i32, String)>>,
        pub theme_handler: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MarkdownView {
        const NAME: &'static str = "TlacuacheMarkdownView";
        type Type = super::MarkdownView;
        type ParentType = gtk::TextView;
    }

    impl ObjectImpl for MarkdownView {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }

        fn dispose(&self) {
            if let (Some(themes), Some(id)) = (theme_manager::get(), self.theme_handler.take()) {
                themes.disconnect(id);
            }
        }
    }
    impl WidgetImpl for MarkdownView {}
    impl TextViewImpl for MarkdownView {}
}

glib::wrapper! {
    pub struct MarkdownView(ObjectSubclass<imp::MarkdownView>)
        @extends gtk::TextView, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
}

impl Default for MarkdownView {
    fn default() -> Self {
        glib::Object::new()
    }
}

/// Nombre de la etiqueta de cada estilo.
fn tag_name(style: Style) -> String {
    match style {
        Style::Heading(level) => format!("h{level}"),
        Style::Strong => "strong".into(),
        Style::Emphasis => "em".into(),
        Style::Strike => "strike".into(),
        Style::Code => "code".into(),
        Style::CodeBlock => "codeblock".into(),
        Style::Quote => "quote".into(),
        Style::Link => "link".into(),
        Style::Muted => "muted".into(),
    }
}

impl MarkdownView {
    fn build(&self) {
        self.set_editable(false);
        self.set_cursor_visible(false);
        self.set_wrap_mode(gtk::WrapMode::WordChar);
        self.set_left_margin(12);
        self.set_right_margin(12);
        self.set_top_margin(12);
        self.set_bottom_margin(12);
        self.add_css_class("tl-markdown");

        let table = self.buffer().tag_table();
        let mut names: Vec<String> = (1..=6).map(|l| format!("h{l}")).collect();
        names.extend(
            [
                "strong",
                "em",
                "strike",
                "code",
                "codeblock",
                "quote",
                "link",
                "muted",
            ]
            .map(String::from),
        );
        for name in names {
            table.add(&gtk::TextTag::new(Some(&name)));
        }

        let theme = theme_manager::get().map_or_else(Theme::default_theme, |t| t.theme());
        self.apply_theme(&theme);
        if let Some(themes) = theme_manager::get() {
            let id = themes.connect_theme_changed(glib::clone!(
                #[weak(rename_to = view)]
                self,
                move |themes| view.apply_theme(&themes.theme())
            ));
            self.imp().theme_handler.replace(Some(id));
        }

        // Clic (sin seleccionar texto) en un enlace: abrirlo.
        let click = gtk::GestureClick::new();
        click.connect_released(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, _, x, y| {
                if !view.buffer().has_selection()
                    && let Some(url) = view.link_at(x, y)
                {
                    view.open_link(&url);
                }
            }
        ));
        self.add_controller(click);

        // Puntero de mano sobre los enlaces.
        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, x, y| {
                let cursor = view.link_at(x, y).map(|_| "pointer");
                view.set_cursor_from_name(cursor);
            }
        ));
        self.add_controller(motion);
    }

    /// Colores y fuentes de las etiquetas desde `theme`.
    fn apply_theme(&self, theme: &Theme) {
        let c = &theme.colors;
        let mono = theme.style.font_mono.as_str();
        let table = self.buffer().tag_table();
        let tag = |name: &str| table.lookup(name);

        for (level, scale) in (1..=6).zip(HEADING_SCALES) {
            if let Some(tag) = tag(&format!("h{level}")) {
                tag.set_scale(scale);
                tag.set_weight(if level == 1 { 800 } else { 700 });
                tag.set_pixels_above_lines(4);
            }
        }
        if let Some(tag) = tag("strong") {
            tag.set_weight(700);
        }
        if let Some(tag) = tag("em") {
            tag.set_style(pango::Style::Italic);
        }
        if let Some(tag) = tag("strike") {
            tag.set_strikethrough(true);
        }
        if let Some(tag) = tag("code") {
            tag.set_family(Some(mono));
            tag.set_background_rgba(Some(&rgba(c.hover_bg)));
        }
        if let Some(tag) = tag("codeblock") {
            tag.set_family(Some(mono));
            tag.set_paragraph_background_rgba(Some(&rgba(c.pane_alt_row)));
            tag.set_left_margin(24);
        }
        if let Some(tag) = tag("quote") {
            tag.set_foreground_rgba(Some(&rgba(c.fg_muted)));
            tag.set_style(pango::Style::Italic);
            tag.set_left_margin(24);
        }
        if let Some(tag) = tag("link") {
            tag.set_foreground_rgba(Some(&rgba(c.link)));
            tag.set_underline(pango::Underline::Single);
        }
        if let Some(tag) = tag("muted") {
            tag.set_foreground_rgba(Some(&rgba(c.fg_muted)));
        }
    }

    /// Muestra `text` (Markdown) renderizado.
    pub fn set_markdown(&self, text: &str) {
        let buffer = self.buffer();
        buffer.set_text("");
        let mut links = Vec::new();
        for span in markdown::render(text) {
            let start = buffer.end_iter().offset();
            let mut end = buffer.end_iter();
            buffer.insert(&mut end, &span.text);
            let (start_iter, end_iter) = (buffer.iter_at_offset(start), buffer.end_iter());
            for style in &span.styles {
                buffer.apply_tag_by_name(&tag_name(*style), &start_iter, &end_iter);
            }
            if let Some(url) = span.link {
                links.push((start, end_iter.offset(), url));
            }
        }
        buffer.place_cursor(&buffer.start_iter());
        self.imp().links.replace(links);
    }

    /// Destino del enlace bajo el punto (`x`, `y`) del widget.
    fn link_at(&self, x: f64, y: f64) -> Option<String> {
        let (bx, by) =
            self.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
        let iter = self.iter_at_location(bx, by)?;
        let offset = iter.offset();
        self.imp()
            .links
            .borrow()
            .iter()
            .find(|(start, end, _)| (*start..*end).contains(&offset))
            .map(|(_, _, url)| url.clone())
    }

    /// Solo enlaces web y de correo (los relativos no tienen destino aquí).
    fn open_link(&self, url: &str) {
        let allowed = ["http://", "https://", "mailto:"]
            .iter()
            .any(|scheme| url.starts_with(scheme));
        if !allowed {
            return;
        }
        let window = self.root().and_downcast::<gtk::Window>();
        gtk::UriLauncher::new(url).launch(
            window.as_ref(),
            None::<&gtk::gio::Cancellable>,
            |result| {
                if let Err(err) = result {
                    tracing::warn!("no se pudo abrir el enlace: {err}");
                }
            },
        );
    }
}
