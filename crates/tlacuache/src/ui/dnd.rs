//! Arrastrar y soltar compartido por las vistas y la barra lateral.

use gtk::prelude::*;
use gtk::{gdk, glib};

use crate::fs::file_item::FileItem;

/// Prefijo del texto interno con que se arrastra un favorito para
/// reordenarlo: `tlacuache-favorite:<grupo>:<índice>`.
pub const FAVORITE_PREFIX: &str = "tlacuache-favorite:";

/// Hace arrastrable la fila de `list_item`: ofrece su archivo como
/// `gdk::FileList` (lo entienden la barra lateral, otras apps y, más
/// adelante, el otro panel y la terminal).
pub fn attach_file_drag(widget: &impl IsA<gtk::Widget>, list_item: &gtk::ListItem) {
    let source = gtk::DragSource::new();
    source.set_actions(gdk::DragAction::COPY | gdk::DragAction::MOVE | gdk::DragAction::LINK);
    source.connect_prepare(glib::clone!(
        #[weak]
        list_item,
        #[upgrade_or]
        None,
        move |_, _, _| {
            let file = list_item.item().and_downcast::<FileItem>()?.file()?;
            let files = gdk::FileList::from_array(&[file]);
            Some(gdk::ContentProvider::for_value(&files.to_value()))
        }
    ));
    source.connect_drag_begin(|source, _| {
        if let Some(widget) = source.widget() {
            let icon = gtk::WidgetPaintable::new(Some(&widget));
            source.set_icon(Some(&icon), 0, 0);
        }
    });
    widget.add_controller(source);
}

/// Codifica la posición de un favorito para arrastrarlo.
pub fn favorite_token(group: usize, index: usize) -> String {
    format!("{FAVORITE_PREFIX}{group}:{index}")
}

/// Decodifica `favorite_token`.
pub fn parse_favorite_token(text: &str) -> Option<(usize, usize)> {
    let (group, index) = text.strip_prefix(FAVORITE_PREFIX)?.split_once(':')?;
    Some((group.parse().ok()?, index.parse().ok()?))
}
