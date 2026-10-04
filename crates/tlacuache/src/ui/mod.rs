//! Widgets propios (un archivo por widget).

pub mod filter_indicator;
// Sin uso desde que Miller es la vista por defecto (2.1); vuelve a usarse al
// alternar vistas en la tarea 2.2, que quita este `allow`.
#[allow(dead_code)]
pub mod list_view;
pub mod miller_view;
pub mod path_bar;
pub mod tab_page;
