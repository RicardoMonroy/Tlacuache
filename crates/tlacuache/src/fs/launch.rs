//! Abrir archivos con la aplicación predeterminada del sistema.

use gtk::prelude::*;
use gtk::{gio, glib};

use crate::{strings, window};

/// Abre `file` con su app predeterminada sin bloquear la UI. El contexto de
/// lanzamiento del display aporta el token de activación (Wayland) para que
/// la app abierta reciba el foco.
pub async fn open_default(
    file: &gio::File,
    widget: &impl IsA<gtk::Widget>,
) -> Result<(), glib::Error> {
    let context = widget.display().app_launch_context();
    gio::AppInfo::launch_default_for_uri_future(&file.uri(), Some(&context)).await
}

/// Lanza `open_default` en segundo plano y avisa con un toast si falla.
pub fn open_file(widget: &impl IsA<gtk::Widget>, file: gio::File, name: String) {
    let widget = widget.clone().upcast::<gtk::Widget>();
    glib::spawn_future_local(async move {
        if let Err(err) = open_default(&file, &widget).await {
            tracing::warn!("no se pudo abrir {}: {err}", file.uri());
            window::show_toast_from(&widget, &strings::open_failed(&name, &err));
        }
    });
}
