//! Clic derecho en las vistas: selecciona lo necesario y pide a `TabPage`
//! (acción `tab.context-menu`) que muestre el menú en la posición del
//! puntero, en coordenadas de la ventana.

use gtk::prelude::*;
use gtk::{gdk, glib, graphene};

/// Pide el menú: `on_item` = menú de elementos; si no, el de la carpeta.
fn request_menu(widget: &gtk::Widget, x: f64, y: f64, on_item: bool) {
    let Some(root) = widget.root() else {
        return;
    };
    // Píxeles de puntero a coordenadas de la ventana.
    let point = widget
        .compute_point(&root, &graphene::Point::new(x as f32, y as f32))
        .unwrap_or_else(|| graphene::Point::new(x as f32, y as f32));
    let target = (f64::from(point.x()), f64::from(point.y()), on_item).to_variant();
    if let Err(err) = widget.activate_action("tab.context-menu", Some(&target)) {
        tracing::debug!("sin menú contextual: {err}");
    }
}

/// La lista (ListView/ColumnView) que contiene `widget`: darle el foco
/// convierte su carpeta en la actual (columnas Miller).
fn focus_list(widget: &gtk::Widget) {
    let list = widget
        .ancestor(gtk::ColumnView::static_type())
        .or_else(|| widget.ancestor(gtk::ListView::static_type()));
    if let Some(list) = list {
        list.grab_focus();
    }
}

/// Clic derecho sobre una fila: si no estaba seleccionada, se selecciona
/// solo ella; luego, menú de elementos.
pub fn attach_row_menu(
    widget: &impl IsA<gtk::Widget>,
    list_item: &gtk::ListItem,
    selection: &gtk::SelectionModel,
) {
    let click = gtk::GestureClick::new();
    click.set_button(gdk::BUTTON_SECONDARY);
    click.connect_pressed(glib::clone!(
        #[weak]
        list_item,
        #[weak]
        selection,
        move |gesture, _, x, y| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let Some(widget) = gesture.widget() else {
                return;
            };
            focus_list(&widget);
            if !list_item.is_selected() {
                selection.select_item(list_item.position(), true);
            }
            request_menu(&widget, x, y, true);
        }
    ));
    widget.add_controller(click);
}

/// Clic derecho en la zona vacía de una vista: `before` (p. ej. quitar la
/// selección) y menú de la carpeta.
pub fn attach_background_menu<F>(widget: &impl IsA<gtk::Widget>, before: F)
where
    F: Fn() + 'static,
{
    let click = gtk::GestureClick::new();
    click.set_button(gdk::BUTTON_SECONDARY);
    click.connect_pressed(move |gesture, _, x, y| {
        let Some(widget) = gesture.widget() else {
            return;
        };
        focus_list(&widget);
        before();
        request_menu(&widget, x, y, false);
    });
    widget.add_controller(click);
}
