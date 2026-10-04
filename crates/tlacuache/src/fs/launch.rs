//! Abrir archivos con la aplicación predeterminada del sistema.

use gtk::prelude::*;
use gtk::{gio, glib};

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
