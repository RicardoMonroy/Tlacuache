//! Una pestaña de `TabStrip`: título alineado a la izquierda (recortado con
//! «…»), botón de cerrar, clic medio para cerrar, menú contextual y
//! arrastre para reordenar o pasarla al otro panel.

use std::cell::OnceCell;

use adw::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib, pango};

use crate::strings;

/// Ancho mínimo de una pestaña al encogerse (px).
const MIN_WIDTH: i32 = 96;
/// Ancho natural (px): todas iguales mientras quepan.
const NATURAL_WIDTH: i32 = 200;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TabButton {
        pub page: OnceCell<adw::TabPage>,
        pub view: glib::WeakRef<adw::TabView>,
        pub title: gtk::Label,
        pub close: gtk::Button,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TabButton {
        const NAME: &'static str = "TlacuacheTabButton";
        type Type = super::TabButton;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for TabButton {}
    impl WidgetImpl for TabButton {
        /// Ancho fijo natural y mínimo pequeño: la tira las encoge
        /// cuando no caben.
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let measured = self.parent_measure(orientation, for_size);
            if orientation == gtk::Orientation::Horizontal {
                (MIN_WIDTH, NATURAL_WIDTH.max(MIN_WIDTH), -1, -1)
            } else {
                measured
            }
        }
    }
    impl BoxImpl for TabButton {}
}

glib::wrapper! {
    pub struct TabButton(ObjectSubclass<imp::TabButton>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl TabButton {
    pub fn new(view: &adw::TabView, page: &adw::TabPage) -> Self {
        let button: Self = glib::Object::new();
        let imp = button.imp();
        let _ = imp.page.set(page.clone());
        imp.view.set(Some(view));
        button.build();
        button
    }

    pub fn page(&self) -> Option<adw::TabPage> {
        self.imp().page.get().cloned()
    }

    /// La vista a la que pertenecía la pestaña al crear el botón.
    pub fn view(&self) -> Option<adw::TabView> {
        self.imp().view.upgrade()
    }

    pub fn set_selected(&self, selected: bool) {
        if selected {
            self.add_css_class("selected");
        } else {
            self.remove_css_class("selected");
        }
    }

    fn build(&self) {
        let imp = self.imp();
        let Some(page) = self.page() else {
            return;
        };
        self.set_orientation(gtk::Orientation::Horizontal);
        self.add_css_class("tl-tab");

        imp.title.add_css_class("tl-tab-title");
        imp.title.set_xalign(0.0);
        imp.title.set_hexpand(true);
        imp.title.set_ellipsize(pango::EllipsizeMode::End);
        page.bind_property("title", &imp.title, "label")
            .sync_create()
            .build();
        page.bind_property("tooltip", self, "tooltip-markup")
            .sync_create()
            .build();

        imp.close.set_icon_name("window-close-symbolic");
        imp.close.add_css_class("flat");
        imp.close.add_css_class("tl-tab-close");
        imp.close.set_focusable(false);
        imp.close.set_valign(gtk::Align::Center);
        imp.close.set_tooltip_text(Some(strings::TAB_CLOSE));
        imp.close.connect_clicked(glib::clone!(
            #[weak(rename_to = button)]
            self,
            move |_| button.close()
        ));
        self.append(&imp.title);
        self.append(&imp.close);

        // Primario: seleccionar; medio: cerrar; secundario: menú.
        let click = gtk::GestureClick::new();
        click.set_button(0);
        click.connect_pressed(glib::clone!(
            #[weak(rename_to = button)]
            self,
            move |gesture, _, x, y| match gesture.current_button() {
                gdk::BUTTON_PRIMARY => button.select(),
                gdk::BUTTON_MIDDLE => button.close(),
                gdk::BUTTON_SECONDARY => button.show_menu(x, y),
                _ => {}
            }
        ));
        self.add_controller(click);

        let drag = gtk::DragSource::new();
        drag.set_actions(gdk::DragAction::MOVE);
        drag.connect_prepare(glib::clone!(
            #[weak(rename_to = button)]
            self,
            #[upgrade_or]
            None,
            move |_, _, _| Some(gdk::ContentProvider::for_value(&button.to_value()))
        ));
        drag.connect_drag_begin(glib::clone!(
            #[weak(rename_to = button)]
            self,
            move |source, _| {
                let icon = gtk::WidgetPaintable::new(Some(&button));
                source.set_icon(Some(&icon), 0, 0);
            }
        ));
        self.add_controller(drag);

        self.install_menu_actions();
    }

    fn select(&self) {
        if let (Some(view), Some(page)) = (self.view(), self.page()) {
            view.set_selected_page(&page);
        }
    }

    fn close(&self) {
        if let (Some(view), Some(page)) = (self.view(), self.page()) {
            view.close_page(&page);
        }
    }

    /// Acciones del menú contextual (`tab.*`).
    fn install_menu_actions(&self) {
        let group = gio::SimpleActionGroup::new();
        let close = gio::SimpleAction::new("close", None);
        close.connect_activate(glib::clone!(
            #[weak(rename_to = button)]
            self,
            move |_, _| button.close()
        ));
        let close_others = gio::SimpleAction::new("close-others", None);
        close_others.connect_activate(glib::clone!(
            #[weak(rename_to = button)]
            self,
            move |_, _| {
                if let (Some(view), Some(page)) = (button.view(), button.page()) {
                    view.close_other_pages(&page);
                }
            }
        ));
        // Duplicar = nueva pestaña en la carpeta de esta (acción del panel).
        let duplicate = gio::SimpleAction::new("duplicate", None);
        duplicate.connect_activate(glib::clone!(
            #[weak(rename_to = button)]
            self,
            move |_, _| {
                button.select();
                let _ = button.activate_action("pane.new-tab", None);
            }
        ));
        group.add_action(&close);
        group.add_action(&close_others);
        group.add_action(&duplicate);
        self.insert_action_group("tab", Some(&group));
    }

    fn show_menu(&self, x: f64, y: f64) {
        let menu = gio::Menu::new();
        menu.append(Some(strings::TAB_DUPLICATE), Some("tab.duplicate"));
        let closing = gio::Menu::new();
        closing.append(Some(strings::TAB_CLOSE), Some("tab.close"));
        closing.append(Some(strings::TAB_CLOSE_OTHERS), Some("tab.close-others"));
        menu.append_section(None, &closing);

        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(self);
        popover.set_has_arrow(false);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.connect_closed(|popover| {
            // Se suelta después de que el menú active la acción.
            let popover = popover.clone();
            glib::idle_add_local_once(move || popover.unparent());
        });
        popover.popup();
    }
}
