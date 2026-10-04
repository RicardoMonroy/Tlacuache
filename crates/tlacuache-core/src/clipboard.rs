//! Formatos de portapapeles para archivos que entienden los gestores de
//! archivos de Linux y otras apps.
//!
//! - `x-special/gnome-copied-files` (Nautilus, Nemo, Thunar…): primera
//!   línea `copy` o `cut`, luego una URI por línea.
//! - `text/uri-list` (RFC 2483): URIs separadas por CRLF; `#` comenta.
//! - `application/x-kde-cutselection`: `1` si es cortar (Dolphin).

pub const GNOME_COPIED_FILES: &str = "x-special/gnome-copied-files";
pub const URI_LIST: &str = "text/uri-list";
pub const KDE_CUT_SELECTION: &str = "application/x-kde-cutselection";
pub const PLAIN_TEXT: &str = "text/plain;charset=utf-8";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardFiles {
    /// Cortar (mover al pegar) en vez de copiar.
    pub cut: bool,
    pub uris: Vec<String>,
}

impl ClipboardFiles {
    pub fn to_gnome_copied_files(&self) -> String {
        let action = if self.cut { "cut" } else { "copy" };
        std::iter::once(action)
            .chain(self.uris.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn to_uri_list(&self) -> String {
        self.uris.iter().map(|uri| format!("{uri}\r\n")).collect()
    }

    /// `None` si la primera línea no es `copy` ni `cut` o no hay URIs.
    pub fn parse_gnome_copied_files(text: &str) -> Option<Self> {
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
        let cut = match lines.next()? {
            "copy" => false,
            "cut" => true,
            _ => return None,
        };
        let uris: Vec<String> = lines.map(str::to_owned).collect();
        (!uris.is_empty()).then_some(Self { cut, uris })
    }

    /// `None` si no hay URIs. Siempre es copiar (el formato no lo indica).
    pub fn parse_uri_list(text: &str) -> Option<Self> {
        let uris: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect();
        (!uris.is_empty()).then_some(Self { cut: false, uris })
    }

    /// Valor de `application/x-kde-cutselection` que indica cortar.
    pub fn is_kde_cut(value: &[u8]) -> bool {
        value.first() == Some(&b'1')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(cut: bool) -> ClipboardFiles {
        ClipboardFiles {
            cut,
            uris: vec![
                "file:///home/ana/a.txt".into(),
                "file:///home/ana/con%20espacio".into(),
            ],
        }
    }

    #[test]
    fn gnome_round_trip() {
        for cut in [false, true] {
            let text = files(cut).to_gnome_copied_files();
            assert!(text.starts_with(if cut { "cut\n" } else { "copy\n" }));
            assert_eq!(
                ClipboardFiles::parse_gnome_copied_files(&text),
                Some(files(cut))
            );
        }
    }

    #[test]
    fn gnome_parsing_is_lenient_with_whitespace_and_crlf() {
        let parsed =
            ClipboardFiles::parse_gnome_copied_files("cut\r\nfile:///a\r\n\r\nfile:///b\n")
                .unwrap();
        assert!(parsed.cut);
        assert_eq!(parsed.uris, ["file:///a", "file:///b"]);
    }

    #[test]
    fn gnome_rejects_unknown_action_or_empty() {
        assert_eq!(
            ClipboardFiles::parse_gnome_copied_files("move\nfile:///a"),
            None
        );
        assert_eq!(ClipboardFiles::parse_gnome_copied_files("copy\n"), None);
        assert_eq!(ClipboardFiles::parse_gnome_copied_files(""), None);
    }

    #[test]
    fn uri_list_round_trip_and_comments() {
        let text = files(false).to_uri_list();
        assert_eq!(
            text,
            "file:///home/ana/a.txt\r\nfile:///home/ana/con%20espacio\r\n"
        );
        assert_eq!(ClipboardFiles::parse_uri_list(&text), Some(files(false)));

        let commented = "# comentario\r\nfile:///x\r\n";
        assert_eq!(
            ClipboardFiles::parse_uri_list(commented).unwrap().uris,
            ["file:///x"]
        );
        assert_eq!(ClipboardFiles::parse_uri_list("# solo\n"), None);
    }

    #[test]
    fn kde_cut_flag() {
        assert!(ClipboardFiles::is_kde_cut(b"1"));
        assert!(!ClipboardFiles::is_kde_cut(b"0"));
        assert!(!ClipboardFiles::is_kde_cut(b""));
    }
}
