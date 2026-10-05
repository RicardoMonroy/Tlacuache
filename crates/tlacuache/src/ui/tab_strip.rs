//! Tira de pestañas propia sobre un `adw::TabView` (reemplaza a
//! `adw::TabBar`, que centra el título; ver P.6 del ROADMAP). Pestañas de
//! ancho fijo a la izquierda que se encogen al abrir más y, al no caber,
//! se desplazan; arrastrar reordena o pasa la pestaña al otro panel.

use std::cell::RefCell;

use adw::prelude::*;
use gtk::glib;
use gtk::subclass::prelude::*;

use crate::ui::tab_button::TabButton;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TabStrip {
        pub view: RefCell<Option<adw::TabView>>,
        /// Modelo de páginas de la vista: hay que conservarlo, si no se
        /// destruye y deja de avisar de los cambios.
        pub pages: RefCell<Option<gtk::SelectionModel>>,
        pub tabs: gtk::Box,
        pub scroll: gtk::ScrolledWindow,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TabStrip {
        const NAME: &'static str = "TlacuacheTabStrip";
        type Type = super::TabStrip;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for TabStrip {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }
    }
    impl WidgetImpl for TabStrip {}
    impl BoxImpl for TabStrip {}
}

glib::wrapper! {
    pub struct TabStrip(ObjectSubclass<imp::TabStrip>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for TabStrip {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl TabStrip {
    fn build(&self) {
        let imp = self.imp();
        self.set_orientation(gtk::Orientation::Horizontal);
        self.add_css_class("tl-tab-strip");

        imp.tabs.set_orientation(gtk::Orientation::Horizontal);
        imp.tabs.set_halign(gtk::Align::Start);
        imp.tabs.add_css_class("tl-tabs");
        // Con la política por omisión el contenido recibe al menos su ancho
        // mínimo: las pestañas se encogen hasta él y luego se desplazan.
        imp.scroll
            .set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
        imp.scroll.set_hexpand(true);
        imp.scroll.set_child(Some(&imp.tabs));
        self.append(&imp.scroll);

        let drop = gtk::DropTarget::new(TabButton::static_type(), gdk_move());
        drop.connect_drop(glib::clone!(
            #[weak(rename_to = strip)]
            self,
            #[upgrade_or]
            false,
            move |_, value, x, _| {
                value
                    .get::<TabButton>()
                    .is_ok_and(|source| strip.drop_tab(&source, x))
            }
        ));
        // En toda la tira: soltar en el espacio libre la deja al final.
        self.add_controller(drop);
    }

    /// Widget fijo al final (botón de nueva pestaña).
    pub fn set_end_widget(&self, widget: &impl IsA<gtk::Widget>) {
        self.append(widget);
    }

    pub fn set_view(&self, view: &adw::TabView) {
        self.imp().view.replace(Some(view.clone()));
        let pages = view.pages();
        pages.connect_items_changed(glib::clone!(
            #[weak(rename_to = strip)]
            self,
            move |_, _, _, _| strip.rebuild()
        ));
        self.imp().pages.replace(Some(pages));
        view.connect_selected_page_notify(glib::clone!(
            #[weak(rename_to = strip)]
            self,
            move |_| strip.update_selected()
        ));
        self.rebuild();
    }

    fn buttons(&self) -> Vec<TabButton> {
        let mut buttons = Vec::new();
        let mut child = self.imp().tabs.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(button) = widget.downcast::<TabButton>() {
                buttons.push(button);
            }
        }
        buttons
    }

    /// Una pestaña por página, en orden (pocas: se recrean todas).
    fn rebuild(&self) {
        let imp = self.imp();
        for button in self.buttons() {
            imp.tabs.remove(&button);
        }
        let Some(view) = imp.view.borrow().clone() else {
            return;
        };
        for i in 0..view.n_pages() {
            imp.tabs.append(&TabButton::new(&view, &view.nth_page(i)));
        }
        self.update_selected();
    }

    /// Marca la pestaña seleccionada y la desplaza a la vista.
    fn update_selected(&self) {
        let imp = self.imp();
        let selected = imp
            .view
            .borrow()
            .as_ref()
            .and_then(adw::TabView::selected_page);
        for button in self.buttons() {
            let is_selected = button.page().is_some() && button.page() == selected;
            button.set_selected(is_selected);
            if is_selected
                && let Some(viewport) = imp.scroll.child().and_downcast::<gtk::Viewport>()
            {
                viewport.scroll_to(&button, None);
            }
        }
    }

    /// Posición de inserción para una pestaña soltada en `x` (coordenadas
    /// de la tira): delante de la primera cuyo centro queda a la derecha;
    /// al final si no hay ninguna.
    fn drop_position(&self, x: f64) -> i32 {
        let mut position = 0;
        for button in self.buttons() {
            // Relativo a la tira: ya incluye el desplazamiento del scroll.
            let center = button
                .compute_bounds(self)
                .map_or(f64::MAX, |b| f64::from(b.x() + b.width() / 2.0));
            if x < center {
                break;
            }
            position += 1;
        }
        position
    }

    /// Reordena (mismo panel) o pasa la pestaña a este panel. El panel de
    /// origen nunca se queda sin pestañas.
    fn drop_tab(&self, source: &TabButton, x: f64) -> bool {
        let (Some(view), Some(page), Some(source_view)) = (
            self.imp().view.borrow().clone(),
            source.page(),
            source.view(),
        ) else {
            return false;
        };
        let position = self.drop_position(x);
        if source_view == view {
            let current = view.page_position(&page);
            let target = if position > current {
                position - 1
            } else {
                position
            };
            view.reorder_page(&page, target.clamp(0, view.n_pages() - 1))
        } else if source_view.n_pages() > 1 {
            source_view.transfer_page(&page, &view, position);
            true
        } else {
            false
        }
    }
}

fn gdk_move() -> gtk::gdk::DragAction {
    gtk::gdk::DragAction::MOVE
}
