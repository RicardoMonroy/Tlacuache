//! Escapado seguro de rutas para enviarlas a un shell (terminal integrada).
//!
//! Nunca se concatena una ruta sin escapar: todo va entre comillas simples,
//! donde el shell no expande nada (`$`, `` ` ``, `*`, `~`, espacios…).
//!
//! - POSIX (bash, zsh, sh, dash, ksh): dentro de comillas simples nada es
//!   especial; una `'` se escribe cerrando, escapando y reabriendo: `'\''`.
//! - fish: dentro de comillas simples `\\` y `\'` sí son escapes, así que
//!   se escapan `\` y `'` con barra invertida.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShellKind {
    #[default]
    Posix,
    Fish,
}

impl ShellKind {
    /// Deduce el tipo por el nombre del ejecutable (`/usr/bin/fish` →
    /// `Fish`); cualquier otro se trata como POSIX.
    pub fn from_shell_path(shell: &str) -> Self {
        let name = Path::new(shell.trim())
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name == "fish" {
            Self::Fish
        } else {
            Self::Posix
        }
    }
}

/// `text` entre comillas simples, escapado para `shell`.
pub fn quote(text: &str, shell: ShellKind) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        match (shell, c) {
            (ShellKind::Posix, '\'') => out.push_str("'\\''"),
            (ShellKind::Fish, '\'') => out.push_str("\\'"),
            (ShellKind::Fish, '\\') => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// Línea para cambiar de carpeta: `cd -- '<ruta>'` y salto de línea. El `--`
/// evita que una ruta que empieza con `-` se tome como opción.
pub fn cd_command(path: &str, shell: ShellKind) -> String {
    format!("cd -- {}\n", quote(path, shell))
}

#[cfg(test)]
mod tests {
    use super::*;

    const POSIX: ShellKind = ShellKind::Posix;
    const FISH: ShellKind = ShellKind::Fish;

    #[test]
    fn plain_and_spaces() {
        assert_eq!(quote("/home/ana/dev", POSIX), "'/home/ana/dev'");
        assert_eq!(quote("/tmp/con espacio", POSIX), "'/tmp/con espacio'");
        assert_eq!(quote("/tmp/con espacio", FISH), "'/tmp/con espacio'");
    }

    #[test]
    fn single_quotes() {
        assert_eq!(quote("it's", POSIX), r"'it'\''s'");
        assert_eq!(quote("''", POSIX), r"''\'''\'''");
        assert_eq!(quote("it's", FISH), r"'it\'s'");
    }

    #[test]
    fn backslashes_only_matter_in_fish() {
        assert_eq!(quote(r"a\b", POSIX), r"'a\b'");
        assert_eq!(quote(r"a\b", FISH), r"'a\\b'");
        assert_eq!(quote(r"fin\", FISH), r"'fin\\'");
    }

    #[test]
    fn expansions_stay_literal() {
        // Entre comillas simples nada se expande.
        for text in [
            "$HOME",
            "`id`",
            "$(rm -rf ~)",
            "*.txt",
            "~",
            "a;b",
            "a&&b",
            "!!",
        ] {
            assert_eq!(quote(text, POSIX), format!("'{text}'"));
        }
    }

    #[test]
    fn newlines_and_unicode() {
        assert_eq!(quote("línea1\nlínea2", POSIX), "'línea1\nlínea2'");
        assert_eq!(quote("Canción ñandú 🎵", POSIX), "'Canción ñandú 🎵'");
    }

    #[test]
    fn empty_string_is_quoted() {
        assert_eq!(quote("", POSIX), "''");
        assert_eq!(quote("", FISH), "''");
    }

    #[test]
    fn cd_command_protects_dash_prefix() {
        assert_eq!(cd_command("-rf", POSIX), "cd -- '-rf'\n");
        assert_eq!(
            cd_command("/home/ana/it's", POSIX),
            "cd -- '/home/ana/it'\\''s'\n"
        );
        assert_eq!(cd_command(r"/x\y", FISH), "cd -- '/x\\\\y'\n");
    }

    /// Pasa cada cadena por un shell real (`printf %s <escapada>`) y
    /// comprueba que sale idéntica. Omite los shells no instalados.
    #[test]
    fn real_shells_round_trip() {
        use std::process::Command;

        let tricky = [
            "/tmp/con espacio",
            "it's",
            "''",
            r"a\b\'c",
            "$HOME `id` $(echo x)",
            "línea1\nlínea2",
            "-rf",
            "*.txt ~ ; && | > < !",
            "\"dobles\"",
            "Canción ñandú 🎵",
            "",
        ];
        let mut tested = 0;
        for shell in ["bash", "sh", "zsh", "dash", "fish"] {
            let kind = ShellKind::from_shell_path(shell);
            for text in tricky {
                let script = format!("printf %s {}", quote(text, kind));
                let Ok(output) = Command::new(shell).arg("-c").arg(&script).output() else {
                    break; // shell no instalado
                };
                assert!(output.status.success(), "{shell}: {script}");
                assert_eq!(
                    String::from_utf8_lossy(&output.stdout),
                    text,
                    "{shell}: {script}"
                );
                tested += 1;
            }
        }
        assert!(tested > 0, "no hay ningún shell para probar");
    }

    #[test]
    fn shell_kind_detection() {
        assert_eq!(ShellKind::from_shell_path("/usr/bin/fish"), FISH);
        assert_eq!(ShellKind::from_shell_path("fish"), FISH);
        assert_eq!(ShellKind::from_shell_path("/bin/bash"), POSIX);
        assert_eq!(ShellKind::from_shell_path("/usr/bin/zsh"), POSIX);
        assert_eq!(ShellKind::from_shell_path(""), POSIX);
        assert_eq!(ShellKind::from_shell_path("/opt/fishy"), POSIX);
    }
}
