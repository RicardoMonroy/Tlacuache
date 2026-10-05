//! Widgets propios (un archivo por widget).

pub mod context_menu;
pub mod dnd;
pub mod drive_row;
pub mod favorites_section;
pub mod file_name_cell;
pub mod filter_indicator;
pub mod image_preview;
pub mod list_view;
pub mod miller_view;
pub mod ops_indicator;
pub mod pane;
pub mod pane_actions;
pub mod path_bar;
pub mod preferences_dialog;
pub mod preview;
pub mod properties_dialog;
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
