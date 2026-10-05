//! Miniaturas según la especificación freedesktop (8.5): caché compartida
//! `$XDG_CACHE_HOME/thumbnails/<tamaño>/<md5 de la URI>.png`, válida si sus
//! metadatos `Thumb::URI` y `Thumb::MTime` coinciden con el archivo. Así se
//! reutilizan las que ya generaron otras apps (Nautilus, etc.).

use crate::filetype::FileCategory;

/// Tamaños de la especificación, de menor a mayor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThumbSize {
    Normal,
    Large,
    XLarge,
    XXLarge,
}

impl ThumbSize {
    pub const ALL: [Self; 4] = [Self::Normal, Self::Large, Self::XLarge, Self::XXLarge];

    /// Carpeta dentro de la caché.
    pub fn dir(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Large => "large",
            Self::XLarge => "x-large",
            Self::XXLarge => "xx-large",
        }
    }

    /// Lado máximo en píxeles.
    pub fn pixels(self) -> u32 {
        match self {
            Self::Normal => 128,
            Self::Large => 256,
            Self::XLarge => 512,
            Self::XXLarge => 1024,
        }
    }

    /// Tamaños que sirven para mostrar `pixels` px, del menor al mayor.
    pub fn sufficient_for(pixels: u32) -> impl Iterator<Item = Self> {
        Self::ALL.into_iter().filter(move |s| s.pixels() >= pixels)
    }
}

/// Tamaño que genera Tlacuache cuando no hay ninguno válido.
pub const GENERATED: ThumbSize = ThumbSize::Large;

/// Subcarpeta de `fail/` para los archivos que no se pudieron miniaturizar.
pub const FAIL_DIR: &str = "tlacuache";

/// Nombre del archivo de miniatura a partir del MD5 (hex) de la URI.
pub fn file_name(md5_hex: &str) -> String {
    format!("{}.png", md5_hex.to_ascii_lowercase())
}

/// Una miniatura sirve si fue hecha para esta URI y esta fecha de
/// modificación (segundos, como en `Thumb::MTime`).
pub fn is_valid(
    stored_uri: Option<&str>,
    stored_mtime: Option<&str>,
    uri: &str,
    mtime: u64,
) -> bool {
    stored_uri == Some(uri)
        && stored_mtime
            .and_then(|m| m.trim().parse::<u64>().ok())
            .is_some_and(|m| m == mtime)
}

/// Categorías que Tlacuache sabe miniaturizar.
pub fn can_thumbnail(category: FileCategory) -> bool {
    matches!(category, FileCategory::Image | FileCategory::Pdf)
}

/// Tamaño de una miniatura de `width`×`height` dentro de `side` px, sin
/// agrandar imágenes pequeñas.
pub fn fit(width: u32, height: u32, side: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= side || longest == 0 {
        return (width.max(1), height.max(1));
    }
    let scale = f64::from(side) / f64::from(longest);
    let px = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
    (px(width), px(height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_directories() {
        assert_eq!(ThumbSize::Normal.dir(), "normal");
        assert_eq!(ThumbSize::XXLarge.pixels(), 1024);
        let for_96: Vec<_> = ThumbSize::sufficient_for(96).collect();
        assert_eq!(for_96, ThumbSize::ALL);
        let for_192: Vec<_> = ThumbSize::sufficient_for(192).collect();
        assert_eq!(
            for_192,
            [ThumbSize::Large, ThumbSize::XLarge, ThumbSize::XXLarge]
        );
        assert_eq!(ThumbSize::sufficient_for(2000).count(), 0);
        assert!(GENERATED.pixels() >= 192, "sirve también en pantallas ×2");
    }

    #[test]
    fn validity_requires_same_uri_and_mtime() {
        let uri = "file:///home/ana/foto%20uno.png";
        assert!(is_valid(Some(uri), Some("1791063303"), uri, 1_791_063_303));
        assert!(!is_valid(Some(uri), Some("1791063303"), uri, 1_791_063_304));
        assert!(!is_valid(Some("file:///otra.png"), Some("1"), uri, 1));
        assert!(!is_valid(None, Some("1"), uri, 1));
        assert!(!is_valid(Some(uri), None, uri, 1));
        assert!(!is_valid(Some(uri), Some("no-numero"), uri, 1));
    }

    #[test]
    fn names_and_fitting() {
        assert_eq!(
            file_name("0C8C1E75F67BD63BFC03BA981E567912"),
            "0c8c1e75f67bd63bfc03ba981e567912.png"
        );
        assert_eq!(fit(1000, 500, 256), (256, 128));
        assert_eq!(fit(100, 50, 256), (100, 50));
        assert_eq!(fit(1, 4000, 256), (1, 256));
        assert_eq!(fit(0, 0, 256), (1, 1));
    }

    #[test]
    fn only_images_and_pdfs() {
        assert!(can_thumbnail(FileCategory::Image));
        assert!(can_thumbnail(FileCategory::Pdf));
        assert!(!can_thumbnail(FileCategory::Video));
        assert!(!can_thumbnail(FileCategory::Folder));
    }
}
