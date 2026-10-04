//! Indicador flotante del filtro rápido («texto» · N elementos) y traducción
//! de teclas a `FilterKey`. Lo comparten la lista y la vista Miller.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, glib};
use tlacuache_core::filter::FilterKey;

use crate::strings;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FilterIndicator {
        pub revealer: gtk::Revealer,
        pub pill: gtk::Box,
        pub label: gtk::Label,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FilterIndicator {
        const NAME: &'static str = "TlacuacheFilterIndicator";
        type Type = super::FilterIndicator;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for FilterIndicator {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            self.pill.set_orientation(gtk::Orientation::Horizontal);
            self.pill.set_spacing(6);
            self.pill.add_css_class("filter-indicator");
            self.pill
                .append(&gtk::Image::from_icon_name("edit-find-symbolic"));
            self.pill.append(&self.label);

            self.revealer.set_child(Some(&self.pill));
            self.revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);

            obj.set_child(Some(&self.revealer));
            obj.set_halign(gtk::Align::End);
            obj.set_valign(gtk::Align::End);
            obj.set_can_target(false);
        }
    }

    impl WidgetImpl for FilterIndicator {}
    impl BinImpl for FilterIndicator {}
}

glib::wrapper! {
    pub struct FilterIndicator(ObjectSubclass<imp::FilterIndicator>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for FilterIndicator {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl FilterIndicator {
    /// Muestra el filtro con su número de coincidencias, u oculta si `query`
    /// está vacío. Sin coincidencias se marca en rojo.
    pub fn update(&self, query: &str, count: u32) {
        let imp = self.imp();
        let active = !query.is_empty();
        imp.revealer.set_reveal_child(active);
        if !active {
            return;
        }
        imp.label.set_text(&strings::filter_indicator(query, count));
        if count == 0 {
            imp.pill.add_css_class("no-match");
        } else {
            imp.pill.remove_css_class("no-match");
        }
    }
}

/// Traduce una tecla al filtro rápido; `None` si no le corresponde (atajos
/// con Ctrl/Alt/Super o teclas sin texto).
pub fn filter_key(key: gdk::Key, mods: gdk::ModifierType) -> Option<FilterKey> {
    let shortcut_mods = gdk::ModifierType::CONTROL_MASK
        | gdk::ModifierType::ALT_MASK
        | gdk::ModifierType::SUPER_MASK;
    if mods.intersects(shortcut_mods) {
        return None;
    }
    match key {
        gdk::Key::Escape => Some(FilterKey::Escape),
        gdk::Key::BackSpace => Some(FilterKey::Backspace),
        _ => key.to_unicode().map(FilterKey::Char),
    }
}
