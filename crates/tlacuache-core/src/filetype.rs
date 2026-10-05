//! Tipo de archivo → categoría de ícono (`docs/BRAND.md` §5). Cada
//! categoría tiene un ícono simbólico `tl-*` y una clase CSS `tl-ft-*` que
//! el tema colorea (`[filetypes]` en `docs/THEMES.md`).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileCategory {
    Folder,
    Code,
    Image,
    Video,
    Audio,
    Archive,
    Document,
    Pdf,
    Spreadsheet,
    Text,
    Executable,
    Other,
}

impl FileCategory {
    pub const ALL: [Self; 12] = [
        Self::Folder,
        Self::Code,
        Self::Image,
        Self::Video,
        Self::Audio,
        Self::Archive,
        Self::Document,
        Self::Pdf,
        Self::Spreadsheet,
        Self::Text,
        Self::Executable,
        Self::Other,
    ];

    /// Nombre de la categoría (clave de `[filetypes]` en los temas).
    pub fn id(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Code => "code",
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Archive => "archive",
            Self::Document => "document",
            Self::Pdf => "pdf",
            Self::Spreadsheet => "spreadsheet",
            Self::Text => "text",
            Self::Executable => "executable",
            Self::Other => "other",
        }
    }

    /// Ícono simbólico de la app (gresource).
    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Folder => "tl-folder-symbolic",
            Self::Code => "tl-file-code-symbolic",
            Self::Image => "tl-file-image-symbolic",
            Self::Video => "tl-file-video-symbolic",
            Self::Audio => "tl-file-audio-symbolic",
            Self::Archive => "tl-file-archive-symbolic",
            Self::Document => "tl-file-document-symbolic",
            Self::Pdf => "tl-file-pdf-symbolic",
            Self::Spreadsheet => "tl-file-spreadsheet-symbolic",
            Self::Text => "tl-file-text-symbolic",
            Self::Executable => "tl-file-executable-symbolic",
            Self::Other => "tl-file-other-symbolic",
        }
    }

    /// Clase CSS que aplica el color del tema.
    pub fn css_class(self) -> &'static str {
        match self {
            Self::Folder => "tl-ft-folder",
            Self::Code => "tl-ft-code",
            Self::Image => "tl-ft-image",
            Self::Video => "tl-ft-video",
            Self::Audio => "tl-ft-audio",
            Self::Archive => "tl-ft-archive",
            Self::Document => "tl-ft-document",
            Self::Pdf => "tl-ft-pdf",
            Self::Spreadsheet => "tl-ft-spreadsheet",
            Self::Text => "tl-ft-text",
            Self::Executable => "tl-ft-executable",
            Self::Other => "tl-ft-other",
        }
    }
}

const SPREADSHEET_TYPES: &[&str] = &[
    "text/csv",
    "text/tab-separated-values",
    "application/vnd.ms-excel",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.oasis.opendocument.spreadsheet",
];

const DOCUMENT_TYPES: &[&str] = &[
    "application/msword",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.oasis.opendocument.text",
    "application/vnd.ms-powerpoint",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/vnd.oasis.opendocument.presentation",
    "application/rtf",
    "text/rtf",
    "application/epub+zip",
    "application/vnd.amazon.mobi8-ebook",
];

const ARCHIVE_TYPES: &[&str] = &[
    "application/zip",
    "application/x-tar",
    "application/gzip",
    "application/x-gzip",
    "application/x-bzip2",
    "application/x-bzip2-compressed-tar",
    "application/x-xz",
    "application/x-xz-compressed-tar",
    "application/x-compressed-tar",
    "application/zstd",
    "application/x-zstd-compressed-tar",
    "application/x-7z-compressed",
    "application/vnd.rar",
    "application/x-rar",
    "application/x-rar-compressed",
    "application/x-cpio",
    "application/x-iso9660-image",
    "application/vnd.debian.binary-package",
    "application/x-rpm",
    "application/x-lzma",
];

const CODE_TYPES: &[&str] = &[
    "application/json",
    "application/xml",
    "application/javascript",
    "application/x-javascript",
    "application/x-shellscript",
    "application/x-php",
    "application/x-ruby",
    "application/x-perl",
    "application/sql",
    "application/toml",
    "application/x-yaml",
    "application/yaml",
    "application/x-sh",
    "text/html",
    "text/css",
    "text/javascript",
    "text/xml",
    // Algunas versiones de shared-mime-info usan nombres sin `x-`.
    "text/rust",
];

const EXECUTABLE_TYPES: &[&str] = &[
    "application/x-executable",
    "application/x-pie-executable",
    "application/x-sharedlib",
    "application/x-appimage",
    "application/vnd.appimage",
    "application/x-msdownload",
];

/// Extensiones de código para cuando el tipo MIME no es concreto
/// (`text/plain`, `application/octet-stream`…).
const CODE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "mjs", "cjs", "ts", "tsx", "jsx", "go", "c", "h", "cc", "cpp", "hpp", "cxx",
    "java", "kt", "kts", "rb", "php", "lua", "sh", "bash", "zsh", "fish", "toml", "yaml", "yml",
    "json", "jsonc", "xml", "html", "htm", "css", "scss", "sass", "less", "sql", "swift", "zig",
    "nix", "vim", "cs", "dart", "ex", "exs", "erl", "hs", "ml", "scala", "clj", "r", "pl", "ps1",
    "vue", "svelte", "astro", "gradle", "cmake", "mk", "ini", "conf",
];

/// Nombres sin extensión que son código o configuración.
const CODE_NAMES: &[&str] = &[
    "Makefile",
    "Dockerfile",
    "Containerfile",
    "PKGBUILD",
    "CMakeLists.txt",
    "Justfile",
];

fn extension(name: &str) -> Option<String> {
    let stem_start = usize::from(name.starts_with('.'));
    let dot = name[stem_start..].rfind('.')? + stem_start;
    Some(name[dot + 1..].to_ascii_lowercase()).filter(|e| !e.is_empty())
}

/// `text/x-rust`, `text/x-python`, `text/x-csrc`… (los `text/x-*` salvo
/// los registros).
fn is_code_text(mime: &str) -> bool {
    mime.starts_with("text/")
        && (mime.contains("src")
            || mime.contains("script")
            || (mime.starts_with("text/x-") && mime != "text/x-log"))
}

fn is_code_by_name(name: &str) -> bool {
    CODE_NAMES.contains(&name)
        || extension(name).is_some_and(|ext| CODE_EXTENSIONS.contains(&ext.as_str()))
}

/// Categoría de un archivo. `content_type` es el tipo MIME de gio (puede
/// faltar); el nombre sirve de respaldo para el código; los ejecutables sin
/// un tipo más concreto (binarios, `octet-stream` con permiso de ejecución)
/// van a `Executable`.
pub fn classify(
    content_type: Option<&str>,
    name: &str,
    is_dir: bool,
    is_executable: bool,
) -> FileCategory {
    if is_dir || content_type == Some("inode/directory") {
        return FileCategory::Folder;
    }
    let mime = content_type.unwrap_or("").to_ascii_lowercase();
    let mime = mime.as_str();
    let family = mime.split('/').next().unwrap_or("");

    if mime == "application/pdf" {
        return FileCategory::Pdf;
    }
    if SPREADSHEET_TYPES.contains(&mime) {
        return FileCategory::Spreadsheet;
    }
    if DOCUMENT_TYPES.contains(&mime) {
        return FileCategory::Document;
    }
    if ARCHIVE_TYPES.contains(&mime) {
        return FileCategory::Archive;
    }
    if EXECUTABLE_TYPES.contains(&mime) {
        return FileCategory::Executable;
    }
    match family {
        "image" => return FileCategory::Image,
        "video" => return FileCategory::Video,
        "audio" => return FileCategory::Audio,
        _ => {}
    }
    // `text/x-rust`, `text/x-python`, `text/x-csrc`… y los de la lista.
    if CODE_TYPES.contains(&mime) || is_code_text(mime) || is_code_by_name(name) {
        return FileCategory::Code;
    }
    if family == "text" {
        return FileCategory::Text;
    }
    if is_executable {
        return FileCategory::Executable;
    }
    FileCategory::Other
}

#[cfg(test)]
mod tests {
    use super::*;
    use FileCategory::*;

    fn file(mime: &str, name: &str) -> FileCategory {
        classify(Some(mime), name, false, false)
    }

    #[test]
    fn folders() {
        assert_eq!(classify(Some("inode/directory"), "src", true, true), Folder);
        assert_eq!(classify(None, "x", true, false), Folder);
        assert_eq!(classify(Some("inode/directory"), "x", false, false), Folder);
    }

    #[test]
    fn common_types() {
        let cases = [
            ("application/pdf", "a.pdf", Pdf),
            ("image/png", "a.png", Image),
            ("image/jpeg", "a.jpg", Image),
            ("image/svg+xml", "a.svg", Image),
            ("image/webp", "a.webp", Image),
            ("video/mp4", "a.mp4", Video),
            ("video/x-matroska", "a.mkv", Video),
            ("video/webm", "a.webm", Video),
            ("audio/mpeg", "a.mp3", Audio),
            ("audio/flac", "a.flac", Audio),
            ("audio/x-wav", "a.wav", Audio),
            ("application/zip", "a.zip", Archive),
            ("application/x-compressed-tar", "a.tar.gz", Archive),
            ("application/x-xz-compressed-tar", "a.tar.xz", Archive),
            ("application/x-zstd-compressed-tar", "a.tar.zst", Archive),
            ("application/x-7z-compressed", "a.7z", Archive),
            ("application/vnd.rar", "a.rar", Archive),
            ("application/x-iso9660-image", "a.iso", Archive),
            ("text/csv", "a.csv", Spreadsheet),
            (
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                "a.xlsx",
                Spreadsheet,
            ),
            (
                "application/vnd.oasis.opendocument.spreadsheet",
                "a.ods",
                Spreadsheet,
            ),
            (
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "a.docx",
                Document,
            ),
            ("application/vnd.oasis.opendocument.text", "a.odt", Document),
            (
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                "a.pptx",
                Document,
            ),
            ("application/epub+zip", "a.epub", Document),
            ("text/x-rust", "main.rs", Code),
            ("text/x-python", "a.py", Code),
            ("text/x-csrc", "a.c", Code),
            ("text/x-c++src", "a.cpp", Code),
            ("application/javascript", "a.js", Code),
            ("application/json", "a.json", Code),
            ("text/html", "index.html", Code),
            ("text/css", "a.css", Code),
            ("application/x-shellscript", "a.sh", Code),
            ("application/toml", "Cargo.toml", Code),
            ("application/x-yaml", "a.yml", Code),
            ("text/plain", "notas.txt", Text),
            ("text/markdown", "README.md", Text),
            ("application/x-executable", "programa", Executable),
            ("application/x-pie-executable", "tlacuache", Executable),
            ("application/x-sharedlib", "libfoo.so", Executable),
            ("application/octet-stream", "datos.bin", Other),
        ];
        assert!(cases.len() >= 30);
        for (mime, name, expected) in cases {
            assert_eq!(file(mime, name), expected, "{mime} ({name})");
        }
    }

    #[test]
    fn code_detected_by_name_when_mime_is_generic() {
        assert_eq!(file("text/plain", "main.zig"), Code);
        assert_eq!(file("application/octet-stream", "build.gradle"), Code);
        assert_eq!(file("text/plain", "Dockerfile"), Code);
        assert_eq!(file("text/plain", "PKGBUILD"), Code);
        assert_eq!(classify(None, "lib.RS", false, false), Code);
    }

    /// Tipos que reportó gio en Arch para archivos reales (incluido
    /// `text/rust`, sin `x-`), con nombres que no ayudan al respaldo.
    #[test]
    fn types_reported_by_gio_on_arch() {
        assert_eq!(file("text/rust", "sin_extension"), Code);
        assert_eq!(file("audio/x-vorbis+ogg", "sonido.oga"), Audio);
        assert_eq!(
            file("application/x-compressed-tar", "archivo.tar.gz"),
            Archive
        );
        assert_eq!(file("application/x-executable", "programa"), Executable);
    }

    #[test]
    fn log_files_are_text() {
        assert_eq!(file("text/x-log", "app.log"), Text);
    }

    #[test]
    fn executable_flag_only_for_unspecific_types() {
        assert_eq!(
            classify(Some("application/octet-stream"), "run", false, true),
            Executable
        );
        // Un script con +x sigue siendo código; una imagen con +x, imagen.
        assert_eq!(
            classify(Some("application/x-shellscript"), "deploy.sh", false, true),
            Code
        );
        assert_eq!(classify(Some("image/png"), "a.png", false, true), Image);
    }

    #[test]
    fn unknown_without_type() {
        assert_eq!(classify(None, "sin-extension", false, false), Other);
        assert_eq!(classify(Some(""), ".hidden", false, false), Other);
    }

    /// Cada ícono nombrado existe en los recursos de la app.
    #[test]
    fn icon_files_exist() {
        let icons = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tlacuache/resources/icons/scalable/mimetypes");
        for category in FileCategory::ALL {
            let file = icons.join(format!("{}.svg", category.icon_name()));
            assert!(file.is_file(), "falta {}", file.display());
        }
    }

    #[test]
    fn every_category_has_distinct_ids_icons_and_classes() {
        use std::collections::HashSet;
        let ids: HashSet<_> = FileCategory::ALL.iter().map(|c| c.id()).collect();
        let icons: HashSet<_> = FileCategory::ALL.iter().map(|c| c.icon_name()).collect();
        let classes: HashSet<_> = FileCategory::ALL.iter().map(|c| c.css_class()).collect();
        assert_eq!(ids.len(), 12);
        assert_eq!(icons.len(), 12);
        assert_eq!(classes.len(), 12);
        for c in FileCategory::ALL {
            assert_eq!(c.css_class(), format!("tl-ft-{}", c.id()));
        }
    }
}
