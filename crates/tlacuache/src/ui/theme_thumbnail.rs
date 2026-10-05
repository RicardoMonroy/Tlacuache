//! Miniatura de un tema para Preferencias: ventana, sidebar y un panel con
//! filas, selección y chips de edad, pintados con los colores del propio
//! tema (no con el activo).

use std::cell::RefCell;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use tlacuache_core::theme::{Color, Theme};

const WIDTH: i32 = 96;
const HEIGHT: i32 = 60;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ThemeThumbnail {
        pub theme: RefCell<Option<Theme>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ThemeThumbnail {
        const NAME: &'static str = "TlacuacheThemeThumbnail";
        type Type = super::ThemeThumbnail;
        type ParentType = gtk::DrawingArea;
    }

    impl ObjectImpl for ThemeThumbnail {}
    impl WidgetImpl for ThemeThumbnail {}
    impl DrawingAreaImpl for ThemeThumbnail {}
}

glib::wrapper! {
    pub struct ThemeThumbnail(ObjectSubclass<imp::ThemeThumbnail>)
        @extends gtk::DrawingArea, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ThemeThumbnail {
    pub fn new(theme: Theme) -> Self {
        let thumbnail: Self = glib::Object::new();
        thumbnail.set_content_width(WIDTH);
        thumbnail.set_content_height(HEIGHT);
        thumbnail.set_valign(gtk::Align::Center);
        thumbnail.imp().theme.replace(Some(theme));
        thumbnail.set_draw_func(|area, cr, _, _| {
            if let Some(thumbnail) = area.downcast_ref::<ThemeThumbnail>()
                && let Some(theme) = thumbnail.imp().theme.borrow().as_ref()
            {
                draw(cr, theme);
            }
        });
        thumbnail
    }
}

fn fill(cr: &gtk::cairo::Context, color: Color, x: f64, y: f64, w: f64, h: f64) {
    cr.set_source_rgb(
        f64::from(color.r) / 255.0,
        f64::from(color.g) / 255.0,
        f64::from(color.b) / 255.0,
    );
    cr.rectangle(x, y, w, h);
    let _ = cr.fill();
}

fn draw(cr: &gtk::cairo::Context, theme: &Theme) {
    let c = &theme.colors;
    let f = &theme.filetypes;
    let a = &theme.age;
    let (w, h) = (f64::from(WIDTH), f64::from(HEIGHT));

    fill(cr, c.border_strong, 0.0, 0.0, w, h);
    fill(cr, c.window_bg, 1.0, 1.0, w - 2.0, h - 2.0);
    // Sidebar con tres lugares.
    fill(cr, c.sidebar_bg, 4.0, 4.0, 22.0, h - 8.0);
    for (i, color) in [f.folder, f.folder, f.folder].into_iter().enumerate() {
        let y = 9.0 + i as f64 * 7.0;
        fill(cr, color, 7.0, y, 3.0, 3.0);
        fill(cr, c.sidebar_fg, 12.0, y + 0.5, 11.0, 2.0);
    }
    // Panel activo: borde, cabecera y filas.
    let (px, py, pw, ph) = (30.0, 4.0, w - 34.0, h - 8.0);
    fill(cr, c.pane_active_border, px, py, pw, ph);
    fill(cr, c.pane_bg, px + 1.0, py + 1.0, pw - 2.0, ph - 2.0);
    fill(cr, c.header_bg, px + 1.0, py + 1.0, pw - 2.0, 8.0);
    fill(cr, c.header_fg, px + 4.0, py + 4.0, 18.0, 2.0);
    let rows = [
        (f.folder, a.hour, false),
        (f.code, a.day, true),
        (f.image, a.week, false),
        (f.pdf, a.month, false),
        (f.archive, a.year, false),
    ];
    for (i, (icon, age, selected)) in rows.into_iter().enumerate() {
        let y = py + 11.0 + i as f64 * 8.0;
        let background = if selected {
            Some(c.selection_bg)
        } else if i % 2 == 1 {
            Some(c.pane_alt_row)
        } else {
            None
        };
        if let Some(background) = background {
            fill(cr, background, px + 1.0, y, pw - 2.0, 8.0);
        }
        let text = if selected { c.selection_fg } else { c.fg };
        fill(cr, icon, px + 4.0, y + 2.5, 3.0, 3.0);
        fill(cr, text, px + 9.0, y + 3.0, 22.0, 2.0);
        fill(cr, age, px + pw - 12.0, y + 2.0, 8.0, 4.0);
    }
}
