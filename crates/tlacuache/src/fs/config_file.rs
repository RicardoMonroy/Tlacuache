//! Reescribir config.toml desde la app (favoritos, tema): lee, transforma
//! el texto y lo guarda de forma atómica, todo asíncrono.

use std::path::Path;

use gtk::gio;
use gtk::prelude::*;
use tlacuache_core::config;

/// Aplica `edit` al texto actual de `path` (la plantilla si no existe) y lo
/// guarda. Si `edit` falla (TOML inválido) el archivo no se toca.
pub async fn update<F, E>(path: &Path, edit: F) -> Result<(), String>
where
    F: FnOnce(&str) -> Result<String, E>,
    E: std::fmt::Display,
{
    let file = gio::File::for_path(path);
    let text = match file.load_contents_future().await {
        Ok((bytes, _)) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(err) if err.matches(gio::IOErrorEnum::NotFound) => config::DEFAULT_TEMPLATE.to_owned(),
        Err(err) => return Err(err.to_string()),
    };
    let updated = edit(&text).map_err(|err| err.to_string())?;
    // La carpeta de config ya existe (se crea al cargar la config).
    // `replace_contents` escribe en un temporal y renombra (atómico).
    file.replace_contents_future(
        updated.into_bytes(),
        None,
        false,
        gio::FileCreateFlags::NONE,
    )
    .await
    .map(|_| ())
    .map_err(|(_, err)| err.to_string())
}
