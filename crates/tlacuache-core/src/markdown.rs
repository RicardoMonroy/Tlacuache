//! Markdown → tramos de texto con estilos (vista previa, 8.3). Sin HTML ni
//! GTK: la app pinta cada estilo con etiquetas del `TextView` cuyos
//! colores salen del tema.
//!
//! Se soporta CommonMark más tablas, tachado y listas de tareas. El HTML
//! en bruto se omite; las imágenes se muestran como su texto alternativo.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Estilo de un tramo; un tramo puede tener varios (negrita en una cita…).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Style {
    /// Encabezado de nivel 1–6.
    Heading(u8),
    Strong,
    Emphasis,
    Strike,
    /// Código en línea.
    Code,
    /// Bloque de código (párrafo con fondo).
    CodeBlock,
    Quote,
    Link,
    /// Marcadores de lista, reglas, imágenes: texto secundario.
    Muted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub styles: Vec<Style>,
    /// Destino si el tramo es (parte de) un enlace.
    pub link: Option<String>,
}

const RULE: &str = "────────────────────";
const BULLET: &str = "• ";
const CELL_SEPARATOR: &str = "  │  ";

/// Convierte `markdown` en tramos listos para pintar.
pub fn render(markdown: &str) -> Vec<Span> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut renderer = Renderer::default();
    for event in Parser::new_ext(markdown, options) {
        renderer.event(event);
    }
    renderer.finish()
}

#[derive(Default)]
struct Renderer {
    out: Vec<Span>,
    styles: Vec<Style>,
    links: Vec<String>,
    quote_depth: usize,
    /// Listas abiertas: `Some(n)` = numerada (siguiente número).
    lists: Vec<Option<u64>>,
    /// Acaba de empezar un elemento de lista (su primer párrafo va pegado).
    item_start: bool,
    /// Celda actual dentro de la fila de una tabla.
    cell: usize,
    /// Dentro de metadatos o HTML: no se muestra.
    skipping: usize,
}

impl Renderer {
    fn event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) if self.skipping == 0 => self.push(&text, &[]),
            Event::Code(text) | Event::InlineMath(text) | Event::DisplayMath(text) => {
                self.push(&text, &[Style::Code]);
            }
            Event::SoftBreak => self.push(" ", &[]),
            Event::HardBreak => self.push("\n", &[]),
            Event::Rule => {
                self.block_gap();
                self.push(RULE, &[Style::Muted]);
            }
            Event::TaskListMarker(done) => {
                self.push(if done { "☑ " } else { "☐ " }, &[Style::Muted]);
            }
            Event::FootnoteReference(name) => self.push(&format!("[{name}]"), &[Style::Muted]),
            // HTML en bruto (y texto dentro de bloques omitidos).
            Event::Html(_) | Event::InlineHtml(_) | Event::Text(_) => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                if !std::mem::take(&mut self.item_start) {
                    self.block_gap();
                }
            }
            Tag::Heading { level, .. } => {
                self.block_gap();
                self.styles.push(Style::Heading(heading_level(level)));
            }
            Tag::BlockQuote(_) => {
                self.block_gap();
                self.quote_depth += 1;
            }
            // El lenguaje de los bloques con ``` no se muestra.
            Tag::CodeBlock(_) => {
                self.block_gap();
                self.styles.push(Style::CodeBlock);
            }
            Tag::List(start) => {
                if self.lists.is_empty() {
                    self.block_gap();
                }
                self.lists.push(start);
            }
            Tag::Item => {
                self.ensure_newlines(1);
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let marker = format!("{n}. ");
                        *n += 1;
                        marker
                    }
                    _ => BULLET.to_owned(),
                };
                self.push(
                    &format!("{}{marker}", "    ".repeat(depth)),
                    &[Style::Muted],
                );
                self.item_start = true;
            }
            Tag::Table(_) => self.block_gap(),
            Tag::TableHead => self.styles.push(Style::Strong),
            Tag::TableRow => self.cell = 0,
            Tag::TableCell => {
                if self.cell > 0 {
                    self.push(CELL_SEPARATOR, &[Style::Muted]);
                }
                self.cell += 1;
            }
            Tag::Emphasis => self.styles.push(Style::Emphasis),
            Tag::Strong => self.styles.push(Style::Strong),
            Tag::Strikethrough => self.styles.push(Style::Strike),
            Tag::Link { dest_url, .. } => {
                self.styles.push(Style::Link);
                self.links.push(dest_url.into_string());
            }
            Tag::Image { .. } => {
                self.styles.push(Style::Muted);
                self.push("🖼 ", &[]);
            }
            Tag::HtmlBlock | Tag::MetadataBlock(_) => self.skipping += 1,
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_)
            | TagEnd::CodeBlock
            | TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Image => {
                self.styles.pop();
            }
            TagEnd::TableHead => {
                self.styles.pop();
                self.ensure_newlines(1);
            }
            TagEnd::TableRow => self.ensure_newlines(1),
            TagEnd::Link => {
                self.styles.pop();
                self.links.pop();
            }
            TagEnd::BlockQuote(_) => self.quote_depth = self.quote_depth.saturating_sub(1),
            TagEnd::List(_) => {
                self.lists.pop();
            }
            TagEnd::Item => self.item_start = false,
            TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {
                self.skipping = self.skipping.saturating_sub(1);
            }
            _ => {}
        }
    }

    /// Añade `text` con los estilos activos más `extra`; une con el tramo
    /// anterior si tiene los mismos estilos.
    fn push(&mut self, text: &str, extra: &[Style]) {
        if text.is_empty() {
            return;
        }
        let mut styles = self.styles.clone();
        if self.quote_depth > 0 {
            styles.push(Style::Quote);
        }
        styles.extend_from_slice(extra);
        let link = self.links.last().cloned();
        if let Some(last) = self.out.last_mut()
            && last.styles == styles
            && last.link == link
        {
            last.text.push_str(text);
            return;
        }
        self.out.push(Span {
            text: text.to_owned(),
            styles,
            link,
        });
    }

    /// Saltos de línea al final de lo escrito.
    fn trailing_newlines(&self) -> usize {
        let mut count = 0;
        for span in self.out.iter().rev() {
            let trailing = span.text.len() - span.text.trim_end_matches('\n').len();
            count += trailing;
            if trailing < span.text.len() {
                break;
            }
        }
        count
    }

    /// Asegura `n` saltos al final (sin estilos, para no extender fondos).
    fn ensure_newlines(&mut self, n: usize) {
        if self.out.is_empty() {
            return;
        }
        let missing = n.saturating_sub(self.trailing_newlines());
        if missing > 0 {
            self.out.push(Span {
                text: "\n".repeat(missing),
                styles: Vec::new(),
                link: None,
            });
        }
    }

    /// Separación entre bloques: línea en blanco, o un salto dentro de una
    /// lista.
    fn block_gap(&mut self) {
        self.ensure_newlines(if self.lists.is_empty() { 2 } else { 1 });
    }

    fn finish(mut self) -> Vec<Span> {
        while let Some(last) = self.out.last_mut() {
            let trimmed = last.text.trim_end_matches('\n').len();
            if trimmed == 0 {
                self.out.pop();
            } else {
                last.text.truncate(trimmed);
                break;
            }
        }
        self.out
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// El archivo es Markdown (por tipo MIME o extensión).
pub fn is_markdown(content_type: Option<&str>, name: &str) -> bool {
    content_type.is_some_and(|t| t == "text/markdown" || t == "text/x-markdown")
        || name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    fn span<'a>(spans: &'a [Span], content: &str) -> &'a Span {
        spans
            .iter()
            .find(|s| s.text.contains(content))
            .unwrap_or_else(|| panic!("sin tramo con {content:?}: {spans:#?}"))
    }

    #[test]
    fn headings_paragraphs_and_inline_styles() {
        let spans =
            render("# Título\n\nUno **dos** *tres* ~~cuatro~~ `cinco`.\n\nSegundo párrafo.");
        assert_eq!(
            text(&spans),
            "Título\n\nUno dos tres cuatro cinco.\n\nSegundo párrafo."
        );
        assert_eq!(span(&spans, "Título").styles, [Style::Heading(1)]);
        assert_eq!(span(&spans, "dos").styles, [Style::Strong]);
        assert_eq!(span(&spans, "tres").styles, [Style::Emphasis]);
        assert_eq!(span(&spans, "cuatro").styles, [Style::Strike]);
        assert_eq!(span(&spans, "cinco").styles, [Style::Code]);
    }

    #[test]
    fn nested_styles_and_links() {
        let spans = render("Ver **[la guía *rápida*](https://ejemplo.mx)** ya");
        let guia = span(&spans, "la guía");
        assert_eq!(guia.styles, [Style::Strong, Style::Link]);
        assert_eq!(guia.link.as_deref(), Some("https://ejemplo.mx"));
        let rapida = span(&spans, "rápida");
        assert_eq!(rapida.styles, [Style::Strong, Style::Link, Style::Emphasis]);
        assert_eq!(rapida.link.as_deref(), Some("https://ejemplo.mx"));
        assert_eq!(span(&spans, " ya").link, None);
    }

    #[test]
    fn code_blocks_keep_their_text() {
        let spans = render("Antes\n\n```rust\nfn main() {\n    x();\n}\n```\n\nDespués");
        assert_eq!(text(&spans), "Antes\n\nfn main() {\n    x();\n}\n\nDespués");
        assert_eq!(span(&spans, "fn main").styles, [Style::CodeBlock]);
    }

    #[test]
    fn lists_nested_ordered_and_tasks() {
        let spans = render("- uno\n- dos\n  1. a\n  2. b\n- [x] hecho\n- [ ] falta\n\nFin");
        assert_eq!(
            text(&spans),
            "• uno\n• dos\n    1. a\n    2. b\n• ☑ hecho\n• ☐ falta\n\nFin"
        );
        assert_eq!(span(&spans, "    1. ").styles, [Style::Muted]);
    }

    #[test]
    fn ordered_list_respects_start() {
        assert_eq!(text(&render("3. tres\n4. cuatro")), "3. tres\n4. cuatro");
    }

    #[test]
    fn quotes_tables_rules_and_html() {
        let spans = render(
            "> cita **fuerte**\n\n---\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n<p align=\"center\">html</p>\n\n![logo](l.svg)",
        );
        assert_eq!(
            text(&spans),
            format!("cita fuerte\n\n{RULE}\n\nA{CELL_SEPARATOR}B\n1{CELL_SEPARATOR}2\n\n🖼 logo")
        );
        assert_eq!(span(&spans, "cita").styles, [Style::Quote]);
        assert_eq!(span(&spans, "fuerte").styles, [Style::Strong, Style::Quote]);
        assert_eq!(span(&spans, "A").styles, [Style::Strong]);
        assert!(span(&spans, "logo").styles.contains(&Style::Muted));
    }

    #[test]
    fn markdown_detection() {
        assert!(is_markdown(Some("text/markdown"), "notas"));
        assert!(is_markdown(None, "README.md"));
        assert!(is_markdown(Some("text/plain"), "guia.MARKDOWN"));
        assert!(!is_markdown(Some("text/plain"), "notas.txt"));
        assert!(!is_markdown(None, "md"));
    }
}
