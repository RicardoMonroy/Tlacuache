//! Portapapeles de archivos (Ctrl+C / Ctrl+X / Ctrl+V) compatible con otros
//! gestores de archivos. Los formatos están en `tlacuache_core::clipboard`.

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use tlacuache_core::clipboard::{
    ClipboardFiles, GNOME_COPIED_FILES, KDE_CUT_SELECTION, PLAIN_TEXT, URI_LIST,
};

fn bytes(text: impl Into<Vec<u8>>) -> glib::Bytes {
    glib::Bytes::from_owned(text.into())
}

/// Pone `files` en el portapapeles en todos los formatos (más las rutas
/// como texto, para pegarlas en una terminal o un editor).
pub fn set_files(
    display: &gdk::Display,
    files: &[gio::File],
    cut: bool,
) -> Result<(), glib::BoolError> {
    let content = ClipboardFiles {
        cut,
        uris: files.iter().map(|f| f.uri().to_string()).collect(),
    };
    let paths: Vec<String> = files
        .iter()
        .map(|f| {
            f.path()
                .map_or_else(|| f.uri().to_string(), |p| p.display().to_string())
        })
        .collect();
    let mut providers = vec![
        gdk::ContentProvider::for_bytes(
            GNOME_COPIED_FILES,
            &bytes(content.to_gnome_copied_files()),
        ),
        gdk::ContentProvider::for_bytes(URI_LIST, &bytes(content.to_uri_list())),
        gdk::ContentProvider::for_bytes(PLAIN_TEXT, &bytes(paths.join("\n"))),
    ];
    if cut {
        providers.push(gdk::ContentProvider::for_bytes(
            KDE_CUT_SELECTION,
            &bytes("1"),
        ));
    }
    display
        .clipboard()
        .set_content(Some(&gdk::ContentProvider::new_union(&providers)))
}

/// Vacía el portapapeles (tras pegar algo cortado).
pub fn clear(display: &gdk::Display) {
    if let Err(err) = display
        .clipboard()
        .set_content(None::<&gdk::ContentProvider>)
    {
        tracing::warn!("no se pudo vaciar el portapapeles: {err}");
    }
}

async fn read_mime(clipboard: &gdk::Clipboard, mime: &str) -> Result<Vec<u8>, glib::Error> {
    let (stream, _) = clipboard
        .read_future(&[mime], glib::Priority::DEFAULT)
        .await?;
    let out = gio::MemoryOutputStream::new_resizable();
    out.splice_future(
        &stream,
        gio::OutputStreamSpliceFlags::CLOSE_SOURCE | gio::OutputStreamSpliceFlags::CLOSE_TARGET,
        glib::Priority::DEFAULT,
    )
    .await?;
    Ok(out.steal_as_bytes().to_vec())
}

/// Archivos del portapapeles, o `None` si no contiene archivos.
pub async fn read_files(display: &gdk::Display) -> Result<Option<ClipboardFiles>, glib::Error> {
    let clipboard = display.clipboard();
    let formats = clipboard.formats();
    let mut files = if formats.contain_mime_type(GNOME_COPIED_FILES) {
        let data = read_mime(&clipboard, GNOME_COPIED_FILES).await?;
        ClipboardFiles::parse_gnome_copied_files(&String::from_utf8_lossy(&data))
    } else if formats.contain_mime_type(URI_LIST) {
        let data = read_mime(&clipboard, URI_LIST).await?;
        ClipboardFiles::parse_uri_list(&String::from_utf8_lossy(&data))
    } else {
        None
    };
    // Dolphin indica «cortar» en un formato aparte.
    if let Some(files) = files.as_mut()
        && !files.cut
        && formats.contain_mime_type(KDE_CUT_SELECTION)
    {
        files.cut = ClipboardFiles::is_kde_cut(&read_mime(&clipboard, KDE_CUT_SELECTION).await?);
    }
    Ok(files)
}
