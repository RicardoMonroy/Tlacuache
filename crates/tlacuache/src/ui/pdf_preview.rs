//! PDF de la vista previa: una página a la vez, renderizada con poppler en
//! un hilo de trabajo (el documento se abre y se suelta en ese hilo) y
//! mostrada con el visor de imágenes (zoom con rueda). Abajo, ‹ página › .

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

use adw::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use tlacuache_core::preview::{PDF_RENDER_SIDE, page_step, pdf_scale};

use crate::strings;
use crate::ui::image_preview::ImagePreview;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PdfPreview {
        pub image: ImagePreview,
        pub prev: gtk::Button,
        pub next: gtk::Button,
        pub label: gtk::Label,
        pub path: RefCell<Option<PathBuf>>,
        /// Página mostrada (0-based) y total.
        pub page: Cell<i32>,
        pub pages: Cell<i32>,
        /// Render en curso (se aborta al pedir otra página o archivo).
        pub task: RefCell<Option<glib::JoinHandle<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PdfPreview {
        const NAME: &'static str = "TlacuachePdfPreview";
        type Type = super::PdfPreview;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for PdfPreview {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }

        fn dispose(&self) {
            if let Some(task) = self.task.take() {
                task.abort();
            }
        }
    }
    impl WidgetImpl for PdfPreview {}
    impl BoxImpl for PdfPreview {}
}

glib::wrapper! {
    pub struct PdfPreview(ObjectSubclass<imp::PdfPreview>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for PdfPreview {
    fn default() -> Self {
        glib::Object::new()
    }
}

/// Una página renderizada (sin `gdk`: la textura se crea en el hilo de GTK).
/// `data` es BGRA premultiplicado (cairo ARGB32).
pub(crate) struct Rendered {
    pub pages: i32,
    pub width: i32,
    pub height: i32,
    pub stride: usize,
    pub data: Vec<u8>,
}

impl PdfPreview {
    fn build(&self) {
        let imp = self.imp();
        self.set_orientation(gtk::Orientation::Vertical);
        imp.image.set_vexpand(true);

        imp.prev.set_icon_name("go-previous-symbolic");
        imp.prev.set_tooltip_text(Some(strings::PDF_PREVIOUS));
        imp.next.set_icon_name("go-next-symbolic");
        imp.next.set_tooltip_text(Some(strings::PDF_NEXT));
        for (button, delta) in [(&imp.prev, -1), (&imp.next, 1)] {
            button.add_css_class("flat");
            button.set_focusable(false);
            button.connect_clicked(glib::clone!(
                #[weak(rename_to = pdf)]
                self,
                move |_| pdf.step(delta)
            ));
        }
        imp.label.add_css_class("numeric");
        imp.label.add_css_class("tl-pdf-page");

        let bar = gtk::CenterBox::new();
        bar.add_css_class("tl-pdf-bar");
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        controls.append(&imp.prev);
        controls.append(&imp.label);
        controls.append(&imp.next);
        bar.set_center_widget(Some(&controls));

        self.append(&imp.image);
        self.append(&bar);
    }

    /// Abre `path` y muestra la página 1. Devuelve el número de páginas.
    pub async fn open(&self, path: PathBuf) -> Result<i32, String> {
        self.clear();
        self.imp().path.replace(Some(path.clone()));
        let rendered = render_in_worker(path, 0).await?;
        let pages = rendered.pages;
        self.imp().pages.set(pages);
        self.show(0, rendered);
        Ok(pages)
    }

    /// Suelta el documento (al cambiar de archivo).
    pub fn clear(&self) {
        let imp = self.imp();
        if let Some(task) = imp.task.take() {
            task.abort();
        }
        imp.path.replace(None);
        imp.page.set(0);
        imp.pages.set(0);
        imp.image.set_texture(None);
        imp.label.set_text("");
    }

    fn step(&self, delta: i32) {
        let imp = self.imp();
        let target = page_step(imp.page.get(), delta, imp.pages.get());
        let Some(path) = imp.path.borrow().clone() else {
            return;
        };
        if target == imp.page.get() {
            return;
        }
        if let Some(task) = imp.task.take() {
            task.abort();
        }
        imp.page.set(target);
        self.update_controls();
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = pdf)]
            self,
            async move {
                match render_in_worker(path, target).await {
                    Ok(rendered) => pdf.show(target, rendered),
                    Err(err) => tracing::debug!("página {target} del PDF: {err}"),
                }
                pdf.imp().task.take();
            }
        ));
        imp.task.replace(Some(task));
    }

    fn show(&self, page: i32, rendered: Rendered) {
        let imp = self.imp();
        let texture = gdk::MemoryTexture::new(
            rendered.width,
            rendered.height,
            gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &glib::Bytes::from_owned(rendered.data),
            rendered.stride,
        );
        imp.image.set_texture(Some(texture.upcast_ref()));
        imp.page.set(page);
        self.update_controls();
    }

    fn update_controls(&self) {
        let imp = self.imp();
        let (page, pages) = (imp.page.get(), imp.pages.get());
        imp.label.set_text(&strings::pdf_page(page + 1, pages));
        imp.prev.set_sensitive(page > 0);
        imp.next.set_sensitive(page + 1 < pages);
    }
}

async fn render_in_worker(path: PathBuf, index: i32) -> Result<Rendered, String> {
    gio::spawn_blocking(move || render_page(&path, index, PDF_RENDER_SIDE))
        .await
        .map_err(|_| "falló el hilo de render".to_owned())?
}

/// En un hilo de trabajo: abre el PDF y dibuja la página `index` sobre
/// blanco (el papel del documento, no un color del tema; cairo ARGB32 =
/// BGRA premultiplicado en memoria).
pub(crate) fn render_page(path: &Path, index: i32, side: f64) -> Result<Rendered, String> {
    use gtk::cairo;

    let uri = glib::filename_to_uri(path, None).map_err(|e| e.to_string())?;
    let document = poppler::Document::from_file(&uri, None).map_err(|e| e.to_string())?;
    let pages = document.n_pages();
    let page = document
        .page(index)
        .ok_or_else(|| format!("el PDF no tiene la página {}", index + 1))?;
    let (width, height) = page.size();
    let scale = pdf_scale(width, height, side);
    let (w, h) = (
        ((width * scale).ceil() as i32).max(1),
        ((height * scale).ceil() as i32).max(1),
    );
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).map_err(|e| e.to_string())?;
    {
        let context = cairo::Context::new(&surface).map_err(|e| e.to_string())?;
        context.set_source_rgb(1.0, 1.0, 1.0);
        context.paint().map_err(|e| e.to_string())?;
        context.scale(scale, scale);
        page.render(&context);
    }
    surface.flush();
    let stride = usize::try_from(surface.stride()).map_err(|e| e.to_string())?;
    let data = surface.data().map_err(|e| e.to_string())?.to_vec();
    Ok(Rendered {
        pages,
        width: w,
        height: h,
        stride,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PDF mínimo válido de `pages` páginas tamaño carta (612×792 pt).
    fn minimal_pdf(pages: usize) -> Vec<u8> {
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            format!(
                "<< /Type /Pages /Kids [{}] /Count {pages} >>",
                (0..pages)
                    .map(|i| format!("{} 0 R", i + 3))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        ];
        for _ in 0..pages {
            objects.push("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_owned());
        }
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
        }
        let xref = pdf.len();
        pdf.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
        );
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }

    #[test]
    fn renders_any_page_of_a_long_pdf_on_white() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("largo con espacio.pdf");
        std::fs::write(&path, minimal_pdf(120)).unwrap();

        let first = render_page(&path, 0, PDF_RENDER_SIDE).unwrap();
        assert_eq!(first.pages, 120);
        // Lado mayor a PDF_RENDER_SIDE; proporción de carta.
        assert_eq!(first.height, PDF_RENDER_SIDE as i32);
        assert_eq!(
            first.width,
            (612.0 * PDF_RENDER_SIDE / 792.0_f64).ceil() as i32
        );
        assert_eq!(first.data.len(), first.stride * first.height as usize);
        // Página en blanco: el fondo es blanco opaco (BGRA).
        assert_eq!(&first.data[..4], &[255, 255, 255, 255]);

        assert_eq!(render_page(&path, 119, PDF_RENDER_SIDE).unwrap().pages, 120);
        assert!(render_page(&path, 120, PDF_RENDER_SIDE).is_err());
    }

    #[test]
    fn damaged_pdf_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roto.pdf");
        std::fs::write(&path, b"%PDF-1.4\nesto no es un PDF").unwrap();
        assert!(render_page(&path, 0, PDF_RENDER_SIDE).is_err());
    }
}
