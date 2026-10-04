//! Arrastrar y soltar compartido por las vistas y la barra lateral.
//!
//! - Arrastrar una fila ofrece sus archivos como `gdk::FileList` (lo
//!   entienden el otro panel, la barra lateral y otras apps). Si la fila es
//!   parte de una selección, se arrastra toda la selección.
//! - Soltar sobre una carpeta copia dentro de ella; sobre un archivo o una
//!   zona vacía, en la carpeta mostrada. Shift al soltar mueve.

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use tlacuache_core::ops::OpKind;

use crate::fs::file_item::FileItem;
use crate::window;

/// Prefijo del texto interno con que se arrastra un favorito para
/// reordenarlo: `tlacuache-favorite:<grupo>:<índice>`.
pub const FAVORITE_PREFIX: &str = "tlacuache-favorite:";

/// Archivos que arrastra `list_item`: toda la selección si la fila está
/// seleccionada, si no solo la fila.
fn dragged_files(list_item: &gtk::ListItem, selection: &gtk::SelectionModel) -> Vec<gio::File> {
    if list_item.is_selected() {
        let bitset = selection.selection();
        let files: Vec<gio::File> = gtk::BitsetIter::init_first(&bitset)
            .map(|(iter, first)| std::iter::once(first).chain(iter))
            .into_iter()
            .flatten()
            .filter_map(|position| selection.item(position).and_downcast::<FileItem>())
            .filter_map(|item| item.file())
            .collect();
        if !files.is_empty() {
            return files;
        }
    }
    list_item
        .item()
        .and_downcast::<FileItem>()
        .and_then(|item| item.file())
        .into_iter()
        .collect()
}

/// Hace arrastrable la fila de `list_item` (con su selección).
pub fn attach_file_drag(
    widget: &impl IsA<gtk::Widget>,
    list_item: &gtk::ListItem,
    selection: &gtk::SelectionModel,
) {
    let source = gtk::DragSource::new();
    source.set_actions(gdk::DragAction::COPY | gdk::DragAction::MOVE);
    source.connect_prepare(glib::clone!(
        #[weak]
        list_item,
        #[weak]
        selection,
        #[upgrade_or]
        None,
        move |_, _, _| {
            let files = dragged_files(&list_item, &selection);
            if files.is_empty() {
                return None;
            }
            let files = gdk::FileList::from_array(&files);
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

/// Acepta archivos soltados sobre la fila de `list_item`: dentro de ella si
/// es carpeta, si no en su misma carpeta.
pub fn attach_row_drop(widget: &impl IsA<gtk::Widget>, list_item: &gtk::ListItem) {
    let target = file_drop_target();
    target.connect_drop(glib::clone!(
        #[weak]
        list_item,
        #[upgrade_or]
        false,
        move |target, value, _, _| {
            let Some(item) = list_item.item().and_downcast::<FileItem>() else {
                return false;
            };
            let Some(file) = item.file() else {
                return false;
            };
            let dest = if item.entry().is_dir {
                Some(file)
            } else {
                file.parent()
            };
            dest.is_some_and(|dest| drop_files(target, value, &dest))
        }
    ));
    widget.add_controller(target);
}

/// Acepta archivos soltados en la zona vacía de una vista: van a la
/// carpeta que devuelva `dest` en ese momento.
pub fn attach_dir_drop<F>(widget: &impl IsA<gtk::Widget>, dest: F)
where
    F: Fn() -> Option<gio::File> + 'static,
{
    let target = file_drop_target();
    target.connect_drop(move |target, value, _, _| {
        dest().is_some_and(|dest| drop_files(target, value, &dest))
    });
    widget.add_controller(target);
}

fn file_drop_target() -> gtk::DropTarget {
    gtk::DropTarget::new(
        gdk::FileList::static_type(),
        gdk::DragAction::COPY | gdk::DragAction::MOVE,
    )
}

/// Encola la copia (o movimiento, si se negoció MOVE con Shift) hacia
/// `dest`. Ignora lo que ya está en `dest` o es `dest` mismo.
fn drop_files(target: &gtk::DropTarget, value: &glib::Value, dest: &gio::File) -> bool {
    let Ok(files) = value.get::<gdk::FileList>() else {
        return false;
    };
    let sources: Vec<gio::File> = files
        .files()
        .into_iter()
        .filter(|f| !f.equal(dest) && !f.parent().is_some_and(|p| p.equal(dest)))
        .collect();
    if sources.is_empty() {
        return false;
    }
    let action = target
        .current_drop()
        .map_or(gdk::DragAction::COPY, |drop| drop.actions());
    let kind = if action == gdk::DragAction::MOVE {
        OpKind::Move
    } else {
        OpKind::Copy
    };
    let Some(widget) = target.widget() else {
        return false;
    };
    window::enqueue_from(&widget, kind, &sources, dest)
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
