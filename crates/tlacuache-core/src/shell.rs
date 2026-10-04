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

/// Texto a escribir en la terminal al soltar archivos: cada ruta escapada,
/// separadas por espacio y con un espacio final para seguir escribiendo.
/// Sin salto de línea: no se ejecuta nada.
pub fn paste_paths<'a, I>(paths: I, shell: ShellKind) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    paths
        .into_iter()
        .map(|path| format!("{} ", quote(path, shell)))
        .collect()
}

/// Archivo de inicio para bash (`bash --rcfile`): carga la configuración
/// habitual del usuario y añade un gancho que emite OSC 7 (carpeta actual)
/// en cada prompt, para que el panel siga a la terminal.
///
/// No se usa `/etc/profile.d/vte.sh`: con `PROMPT_COMMAND` como cadena lo
/// sobrescribe (rompería mise, zoxide…) y además modifica `PS1`. Aquí el
/// gancho se antepone sin pisar nada y conserva `$?` (starship lo usa).
pub const BASH_INTEGRATION_RC: &str = r#"# Generado por Tlacuache. Carga tu configuración de bash y añade OSC 7
# (avisa a la terminal la carpeta actual) sin modificar tu prompt.
[ -f /etc/bash.bashrc ] && . /etc/bash.bashrc
[ -f "$HOME/.bashrc" ] && . "$HOME/.bashrc"

__tlacuache_urlencode() {
    local LC_ALL=C text="$1" out="" char i
    for (( i = 0; i < ${#text}; i++ )); do
        char="${text:i:1}"
        case "$char" in
            [a-zA-Z0-9/._~-]) out+="$char" ;;
            *) printf -v char '%%%02X' "'$char"; out+="$char" ;;
        esac
    done
    printf '%s' "$out"
}

__tlacuache_osc7() {
    local status=$?
    printf '\e]7;file://%s%s\e\\' "${HOSTNAME:-localhost}" "$(__tlacuache_urlencode "$PWD")"
    return $status
}

if [[ "$(declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
    PROMPT_COMMAND=(__tlacuache_osc7 "${PROMPT_COMMAND[@]}")
else
    PROMPT_COMMAND="__tlacuache_osc7${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
"#;

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

    /// El rcfile con bash real: emite OSC 7 con la carpeta (codificada) en
    /// cada prompt y conserva el `PROMPT_COMMAND` del usuario.
    #[test]
    fn bash_integration_emits_osc7_and_keeps_user_hooks() {
        use std::process::{Command, Stdio};

        let Ok(home) = tempfile::tempdir() else {
            return;
        };
        let rc = home.path().join("tlacuache.rc");
        std::fs::write(&rc, BASH_INTEGRATION_RC).unwrap();
        std::fs::write(
            home.path().join(".bashrc"),
            "PROMPT_COMMAND='echo GANCHO-USUARIO'\n",
        )
        .unwrap();
        let target = home.path().join("con espacio ñ");
        std::fs::create_dir(&target).unwrap();

        let script = format!("cd {}\nexit\n", quote(&target.to_string_lossy(), POSIX));
        let child = Command::new("bash")
            .args(["--rcfile", &rc.to_string_lossy(), "-i"])
            .env("HOME", home.path())
            .env("HOSTNAME", "equipo")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let Ok(mut child) = child else {
            return; // sin bash
        };
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(script.as_bytes()).unwrap();
        }
        let output = child.wait_with_output().unwrap();
        // bash interactivo sin TTY escribe el prompt y los ganchos en stderr/stdout.
        let all = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let encoded = target
            .to_string_lossy()
            .replace(' ', "%20")
            .replace('ñ', "%C3%B1");
        assert!(
            all.contains(&format!("\u{1b}]7;file://equipo{encoded}\u{1b}\\")),
            "sin OSC 7 de la carpeta: {all:?}"
        );
        assert!(
            all.contains("GANCHO-USUARIO"),
            "se perdió el gancho del usuario: {all:?}"
        );
    }

    #[test]
    fn paste_paths_quotes_each_and_never_executes() {
        let text = paste_paths(["/tmp/a b", "/tmp/it's"], POSIX);
        assert_eq!(text, "'/tmp/a b' '/tmp/it'\\''s' ");
        assert!(!text.contains('\n'));
        assert_eq!(paste_paths([], POSIX), "");
        assert_eq!(paste_paths([r"C:\x"], FISH), r"'C:\\x' ");
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
