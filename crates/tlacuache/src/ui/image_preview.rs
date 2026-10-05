//! Imagen de la vista previa: encajada sin agrandar por defecto; la rueda
//! acerca o aleja anclando el punto bajo el puntero, arrastrar desplaza y
//! el doble clic vuelve a encajar.

use std::cell::Cell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, glib};
use tlacuache_core::preview::{anchored_offset, fit_scale, zoom};

use crate::strings;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ImagePreview {
        pub scroll: gtk::ScrolledWindow,
        pub picture: gtk::Picture,
        /// Tamaño de la textura (px).
        pub size: Cell<(f64, f64)>,
        /// Escala actual; `None` = encajada.
        pub zoom: Cell<Option<f64>>,
        /// Posición del puntero en la vista.
        pub pointer: Cell<(f64, f64)>,
        /// Desplazamiento al empezar a arrastrar.
        pub drag_start: Cell<(f64, f64)>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ImagePreview {
        const NAME: &'static str = "TlacuacheImagePreview";
        type Type = super::ImagePreview;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for ImagePreview {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }
    }
    impl WidgetImpl for ImagePreview {}
    impl BinImpl for ImagePreview {}
}

glib::wrapper! {
    pub struct ImagePreview(ObjectSubclass<imp::ImagePreview>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for ImagePreview {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl ImagePreview {
    fn build(&self) {
        let imp = self.imp();
        imp.picture.set_can_shrink(true);
        imp.picture.set_halign(gtk::Align::Center);
        imp.picture.set_valign(gtk::Align::Center);
        imp.scroll.set_child(Some(&imp.picture));
        imp.scroll
            .set_tooltip_text(Some(strings::PREVIEW_IMAGE_HINT));
        self.set_child(Some(&imp.scroll));

        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, x, y| view.imp().pointer.set((x, y))
        ));
        imp.scroll.add_controller(motion);

        // En captura: si no, el ScrolledWindow usaría la rueda para desplazar.
        let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
        wheel.connect_scroll(glib::clone!(
            #[weak(rename_to = view)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, _, dy| {
                view.zoom_by(dy);
                glib::Propagation::Stop
            }
        ));
        imp.scroll.add_controller(wheel);

        let drag = gtk::GestureDrag::new();
        drag.connect_drag_begin(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, _, _| {
                let scroll = &view.imp().scroll;
                view.imp()
                    .drag_start
                    .set((scroll.hadjustment().value(), scroll.vadjustment().value()));
            }
        ));
        drag.connect_drag_update(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, dx, dy| {
                let imp = view.imp();
                if imp.zoom.get().is_none() {
                    return;
                }
                let (h, v) = imp.drag_start.get();
                imp.scroll.hadjustment().set_value(h - dx);
                imp.scroll.vadjustment().set_value(v - dy);
            }
        ));
        imp.scroll.add_controller(drag);

        let double_click = gtk::GestureClick::new();
        double_click.connect_pressed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, n_press, _, _| {
                if n_press == 2 {
                    view.fit();
                }
            }
        ));
        imp.scroll.add_controller(double_click);

        self.fit();
    }

    /// Muestra `texture` encajada.
    pub fn set_texture(&self, texture: Option<&gdk::Texture>) {
        let imp = self.imp();
        let size = texture.map_or((0.0, 0.0), |t| {
            (f64::from(t.width()), f64::from(t.height()))
        });
        imp.size.set(size);
        imp.picture.set_paintable(texture);
        self.fit();
    }

    /// Vuelve a encajar la imagen en la vista.
    fn fit(&self) {
        let imp = self.imp();
        imp.zoom.set(None);
        imp.picture.set_content_fit(gtk::ContentFit::ScaleDown);
        imp.picture.set_size_request(-1, -1);
        imp.picture.set_cursor(None);
        imp.scroll
            .set_policy(gtk::PolicyType::Never, gtk::PolicyType::Never);
    }

    fn fit_scale(&self) -> f64 {
        let imp = self.imp();
        let area = (
            f64::from(imp.scroll.width()),
            f64::from(imp.scroll.height()),
        );
        fit_scale(imp.size.get(), area)
    }

    fn zoom_by(&self, dy: f64) {
        let imp = self.imp();
        let (width, height) = imp.size.get();
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let fit = self.fit_scale();
        let old = imp.zoom.get().unwrap_or(fit);
        let Some(new) = zoom(old, fit, dy) else {
            self.fit();
            return;
        };

        // Desplazamiento actual; encajada, la imagen está centrada (offset
        // negativo respecto a la vista).
        let (view_w, view_h) = (
            f64::from(imp.scroll.width()),
            f64::from(imp.scroll.height()),
        );
        let (offset_x, offset_y) = match imp.zoom.get() {
            Some(_) => (
                imp.scroll.hadjustment().value(),
                imp.scroll.vadjustment().value(),
            ),
            None => (
                -(view_w - width * old) / 2.0,
                -(view_h - height * old) / 2.0,
            ),
        };
        let (px, py) = imp.pointer.get();
        let target_x = anchored_offset(offset_x, px, old, new);
        let target_y = anchored_offset(offset_y, py, old, new);

        imp.zoom.set(Some(new));
        imp.picture.set_content_fit(gtk::ContentFit::Fill);
        imp.picture
            .set_size_request((width * new).round() as i32, (height * new).round() as i32);
        imp.picture.set_cursor_from_name(Some("grab"));
        imp.scroll
            .set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        // Los límites de las barras cambian tras el siguiente layout.
        imp.scroll.add_tick_callback(move |scroll, _| {
            scroll.hadjustment().set_value(target_x);
            scroll.vadjustment().set_value(target_y);
            glib::ControlFlow::Break
        });
    }
}
