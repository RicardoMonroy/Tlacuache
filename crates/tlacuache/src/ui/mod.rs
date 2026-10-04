//! Widgets propios (un archivo por widget).

pub mod context_menu;
pub mod dnd;
pub mod drive_row;
pub mod favorites_section;
pub mod filter_indicator;
pub mod list_view;
pub mod miller_view;
pub mod ops_indicator;
pub mod pane;
pub mod pane_actions;
pub mod path_bar;
pub mod properties_dialog;
pub mod sidebar;
pub mod status_bar;
pub mod tab_page;
pub mod terminal;

use gtk::prelude::*;

/// El foco de teclado está en `widget` o dentro de él.
pub fn has_focus_within(widget: &impl IsA<gtk::Widget>) -> bool {
    let widget = widget.upcast_ref::<gtk::Widget>();
    widget
        .root()
        .and_then(|root| root.focus())
        .is_some_and(|focus| focus == *widget || focus.is_ancestor(widget))
}
