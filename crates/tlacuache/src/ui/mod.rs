//! Widgets propios (un archivo por widget).

pub mod bulk_rename_dialog;
pub mod context_menu;
pub mod dnd;
pub mod drive_row;
pub mod favorites_section;
pub mod file_name_cell;
pub mod filter_indicator;
pub mod icon_view;
pub mod image_preview;
pub mod list_view;
pub mod markdown_view;
pub mod media_preview;
pub mod miller_view;
pub mod notes_view;
pub mod ops_indicator;
pub mod pane;
pub mod pane_actions;
pub mod path_bar;
pub mod pdf_preview;
pub mod preferences_dialog;
pub mod preview;
pub mod properties_dialog;
pub mod search_page;
pub mod sidebar;
pub mod status_bar;
pub mod tab_button;
pub mod tab_page;
pub mod tab_strip;
pub mod terminal;
pub mod text_preview;
pub mod theme_manager;
pub mod theme_thumbnail;

use gtk::prelude::*;

/// Repinta la celda de `list_item` cuando su `FileItem` se actualiza en el
/// sitio (la carpeta se resincronizó; ADR-017). Se llama en `setup` de la
/// fábrica con el mismo código que `bind`: cada vez que GTK le asigna otro
/// elemento, se suelta el aviso del anterior y se escucha el del nuevo.
pub fn refresh_on_item_change(
    list_item: &gtk::ListItem,
    refresh: impl Fn(&gtk::ListItem) + 'static,
) {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::fs::file_item::FileItem;

    let current: Rc<RefCell<Option<(FileItem, gtk::glib::SignalHandlerId)>>> = Rc::default();
    let refresh = Rc::new(refresh);
    list_item.connect_item_notify(move |list_item| {
        if let Some((item, handler)) = current.take() {
            item.disconnect(handler);
        }
        let Some(item) = list_item.item().and_downcast::<FileItem>() else {
            return;
        };
        let weak = list_item.downgrade();
        let refresh = refresh.clone();
        let handler = item.connect_changed(move |_| {
            if let Some(list_item) = weak.upgrade() {
                refresh(&list_item);
            }
        });
        current.replace(Some((item, handler)));
    });
}

/// El foco de teclado está en `widget` o dentro de él.
pub fn has_focus_within(widget: &impl IsA<gtk::Widget>) -> bool {
    let widget = widget.upcast_ref::<gtk::Widget>();
    widget
        .root()
        .and_then(|root| root.focus())
        .is_some_and(|focus| focus == *widget || focus.is_ancestor(widget))
}

/// Atajo de teclado configurable (`[keys]`), ya interpretado.
#[derive(Clone, Copy, Debug)]
pub struct Accel {
    key: gtk::gdk::Key,
    mods: gtk::gdk::ModifierType,
}

impl Accel {
    /// El de la config o, si no se puede interpretar, el predeterminado.
    pub fn for_action(
        action: tlacuache_core::keymap::Action,
        keys: &std::collections::BTreeMap<String, String>,
    ) -> Self {
        let text = tlacuache_core::keymap::accelerator(action, keys);
        let parsed = gtk::accelerator_parse(text).or_else(|| {
            tracing::warn!("atajo no válido para {}: {text:?}", action.id());
            gtk::accelerator_parse(action.default_accelerator())
        });
        let (key, mods) =
            parsed.unwrap_or((gtk::gdk::Key::VoidSymbol, gtk::gdk::ModifierType::empty()));
        Self {
            key: normalize(key),
            mods,
        }
    }

    pub fn matches(&self, key: gtk::gdk::Key, mods: gtk::gdk::ModifierType) -> bool {
        let relevant = gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::SHIFT_MASK
            | gtk::gdk::ModifierType::ALT_MASK
            | gtk::gdk::ModifierType::SUPER_MASK;
        normalize(key) == self.key && (mods & relevant) == self.mods
    }
}

/// Con Shift, Tab llega como `ISO_Left_Tab` y las letras en mayúscula.
fn normalize(key: gtk::gdk::Key) -> gtk::gdk::Key {
    if key == gtk::gdk::Key::ISO_Left_Tab {
        gtk::gdk::Key::Tab
    } else {
        key.to_lower()
    }
}

/// Ícono `tl-*` de `category`, coloreado por el tema con su clase.
pub fn set_category_icon(image: &gtk::Image, category: tlacuache_core::filetype::FileCategory) {
    use tlacuache_core::filetype::FileCategory;

    for other in FileCategory::ALL {
        image.remove_css_class(other.css_class());
    }
    image.set_icon_name(Some(category.icon_name()));
    image.add_css_class(category.css_class());
}

/// Color del tema como `gdk::RGBA` (terminal, Markdown).
pub fn rgba(color: tlacuache_core::theme::Color) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        1.0,
    )
}
