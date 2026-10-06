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
    [[ -n "${TLACUACHE_STATE_FILE-}" ]] && printf prompt 2>/dev/null >| "$TLACUACHE_STATE_FILE"
    return $status
}

# Solo dentro de Flatpak (ADR-016): avisa que se ejecuta una orden.
if [[ -n "${TLACUACHE_STATE_FILE-}" ]]; then
    __tlacuache_busy() { printf busy 2>/dev/null >| "$TLACUACHE_STATE_FILE"; }
    PS0='$(__tlacuache_busy)'"${PS0-}"
fi

if [[ "$(declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
    PROMPT_COMMAND=(__tlacuache_osc7 "${PROMPT_COMMAND[@]}")
else
    PROMPT_COMMAND="__tlacuache_osc7${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
"#;

/// Variable con el `ZDOTDIR` original del usuario (vacía = `$HOME`).
pub const ZSH_USER_ZDOTDIR_VAR: &str = "TLACUACHE_USER_ZDOTDIR";

/// `.zshenv` de integración para zsh (ADR-008). Tlacuache lanza zsh con
/// `ZDOTDIR` apuntando a la carpeta de este archivo: aquí se restaura el
/// `ZDOTDIR` del usuario y se carga su `.zshenv`, así que zsh sigue con su
/// `.zprofile`/`.zshrc` de siempre. En shells interactivos se añade un
/// gancho `precmd` que emite OSC 7 (conserva `$?`).
pub const ZSH_INTEGRATION_ZSHENV: &str = r#"# Generado por Tlacuache. Restaura tu ZDOTDIR, carga tu configuración de
# zsh y añade OSC 7 (avisa a la terminal la carpeta actual).
if [[ -n "${TLACUACHE_USER_ZDOTDIR-}" ]]; then
    ZDOTDIR="$TLACUACHE_USER_ZDOTDIR"
else
    unset ZDOTDIR
fi
unset TLACUACHE_USER_ZDOTDIR
[[ -f "${ZDOTDIR:-$HOME}/.zshenv" ]] && source "${ZDOTDIR:-$HOME}/.zshenv"

if [[ -o interactive ]]; then
    __tlacuache_osc7() {
        local ret=$?
        emulate -L zsh
        setopt no_multibyte
        local LC_ALL=C out="" c
        for c in ${(s::)PWD}; do
            if [[ "$c" == [a-zA-Z0-9/._~-] ]]; then
                out+="$c"
            else
                out+=$(printf '%%%02X' "'$c")
            fi
        done
        printf '\e]7;file://%s%s\e\\' "${HOST:-localhost}" "$out"
        return $ret
    }
    autoload -Uz add-zsh-hook
    add-zsh-hook precmd __tlacuache_osc7

    # Solo dentro de Flatpak (ADR-016): prompt mostrado / orden en curso.
    if [[ -n "${TLACUACHE_STATE_FILE-}" ]]; then
        __tlacuache_prompt() {
            local ret=$?
            print -n prompt 2>/dev/null >| "$TLACUACHE_STATE_FILE"
            return $ret
        }
        __tlacuache_busy() { print -n busy 2>/dev/null >| "$TLACUACHE_STATE_FILE" }
        add-zsh-hook precmd __tlacuache_prompt
        add-zsh-hook preexec __tlacuache_busy
    fi
fi
"#;

/// Variable con la ruta del archivo de estado del shell. Solo se define
/// dentro de Flatpak (ADR-016), donde `tcgetpgrp` no ve los procesos del
/// sistema: los ganchos de integración escriben `prompt` al mostrar el
/// prompt y `busy` al empezar a ejecutar una orden.
pub const STATE_FILE_VAR: &str = "TLACUACHE_STATE_FILE";

/// Lo último que escribió la integración en el archivo de estado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellState {
    /// El shell espera órdenes.
    Prompt,
    /// Hay una orden en curso.
    Busy,
}

impl ShellState {
    /// `None` si el archivo está vacío o tiene otra cosa (el shell aún no
    /// ha mostrado su primer prompt).
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "prompt" => Some(Self::Prompt),
            "busy" => Some(Self::Busy),
            _ => None,
        }
    }
}

/// Integración para fish dentro de Flatpak (ADR-016). fish ya emite OSC 7
/// por sí mismo; esto solo añade el archivo de estado. Se carga con
/// `fish --init-command 'source <archivo>'`.
pub const FISH_INTEGRATION: &str = r#"# Generado por Tlacuache. Avisa a Tlacuache si fish espera órdenes o
# ejecuta una (solo dentro de Flatpak).
if set -q TLACUACHE_STATE_FILE
    function __tlacuache_prompt --on-event fish_prompt
        printf prompt 2>/dev/null >"$TLACUACHE_STATE_FILE"
    end
    function __tlacuache_busy --on-event fish_preexec
        printf busy 2>/dev/null >"$TLACUACHE_STATE_FILE"
    end
end
"#;

/// Shell de inicio de sesión de una línea de `getent passwd` (séptimo
/// campo), si es una ruta absoluta.
pub fn login_shell(passwd_line: &str) -> Option<&str> {
    let shell = passwd_line.lines().next()?.split(':').nth(6)?;
    shell.starts_with('/').then_some(shell)
}

/// Orden que lanza `argv` en el sistema desde el sandbox de Flatpak con
/// `flatpak-spawn --host` (ADR-016), en la carpeta `dir` y con las variables
/// `env` (`NOMBRE=valor`). `--watch-bus` termina el shell si Tlacuache se
/// cierra. flatpak-spawn solo acepta las opciones en la forma `--opción=valor`
/// y toma como orden el primer argumento que no empieza por `-`.
pub fn host_command(argv: &[String], dir: &str, env: &[String]) -> Vec<String> {
    let mut command = vec![
        "flatpak-spawn".to_owned(),
        "--host".to_owned(),
        "--watch-bus".to_owned(),
        format!("--directory={dir}"),
    ];
    command.extend(env.iter().map(|var| format!("--env={var}")));
    command.extend(argv.iter().cloned());
    command
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

    /// `cd_command` con shells reales: entra en carpetas con nombres
    /// difíciles y `pwd` confirma que llegó (fish incluido). Omite los
    /// shells no instalados.
    #[test]
    fn real_shells_cd_into_tricky_folders() {
        use std::process::Command;

        let Ok(root) = tempfile::tempdir() else {
            return;
        };
        let names = [
            "con espacio",
            "it's",
            "$HOME `id`",
            "-rf",
            "*.txt ; && |",
            "\"dobles\" \\ barra",
            "Canción ñandú 🎵",
        ];
        let mut tested = 0;
        for shell in ["bash", "sh", "zsh", "dash", "fish"] {
            let kind = ShellKind::from_shell_path(shell);
            for name in names {
                let dir = root.path().join(name);
                std::fs::create_dir_all(&dir).unwrap();
                let script = format!("{}pwd", cd_command(&dir.to_string_lossy(), kind));
                let Ok(output) = Command::new(shell).arg("-c").arg(&script).output() else {
                    break; // shell no instalado
                };
                assert!(output.status.success(), "{shell}: {script:?}");
                assert_eq!(
                    String::from_utf8_lossy(&output.stdout).trim_end_matches('\n'),
                    dir.to_string_lossy(),
                    "{shell}: {script:?}"
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
    fn zsh_integration_emits_osc7_and_loads_user_config() {
        use std::process::{Command, Stdio};

        let (Ok(home), Ok(ours)) = (tempfile::tempdir(), tempfile::tempdir()) else {
            return;
        };
        std::fs::write(ours.path().join(".zshenv"), ZSH_INTEGRATION_ZSHENV).unwrap();
        // La config del usuario vive en su propio ZDOTDIR.
        let user_dir = home.path().join("zdot");
        std::fs::create_dir(&user_dir).unwrap();
        std::fs::write(user_dir.join(".zshenv"), "echo ENV-USUARIO\n").unwrap();
        std::fs::write(
            user_dir.join(".zshrc"),
            "echo RC-USUARIO\nautoload -Uz add-zsh-hook\n__mio() { echo GANCHO-USUARIO }\nadd-zsh-hook precmd __mio\n",
        )
        .unwrap();
        let target = home.path().join("con espacio ñ");
        std::fs::create_dir(&target).unwrap();

        let script = format!("cd {}\nexit\n", quote(&target.to_string_lossy(), POSIX));
        let child = Command::new("zsh")
            .args(["-i"])
            .env("HOME", home.path())
            .env("HOST", "equipo")
            .env("ZDOTDIR", ours.path())
            .env(ZSH_USER_ZDOTDIR_VAR, &user_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let Ok(mut child) = child else {
            return; // sin zsh
        };
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(script.as_bytes()).unwrap();
        }
        let output = child.wait_with_output().unwrap();
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
        for expected in ["ENV-USUARIO", "RC-USUARIO", "GANCHO-USUARIO"] {
            assert!(all.contains(expected), "falta {expected}: {all:?}");
        }
    }

    /// Lanza `shell` interactivo con el archivo de estado definido y le
    /// pasa `script` por la entrada estándar. Devuelve `None` si el shell no
    /// está instalado.
    fn run_with_state(
        shell: &str,
        args: &[&str],
        env: &[(&str, &std::path::Path)],
        script: &str,
    ) -> Option<std::process::Output> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut command = Command::new(shell);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in env {
            command.env(key, value);
        }
        let mut child = command.spawn().ok()?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        Some(child.wait_with_output().unwrap())
    }

    /// bash: `busy` mientras corre una orden y `prompt` al volver al prompt
    /// (el `PROMPT_COMMAND` del usuario corre después del nuestro y lo ve).
    #[test]
    fn bash_integration_reports_prompt_and_busy() {
        let Ok(home) = tempfile::tempdir() else {
            return;
        };
        let rc = home.path().join("tlacuache.rc");
        let state = home.path().join("state");
        let log = home.path().join("log");
        std::fs::write(&rc, BASH_INTEGRATION_RC).unwrap();
        std::fs::write(
            home.path().join(".bashrc"),
            "PROMPT_COMMAND='cat \"$TLACUACHE_STATE_FILE\" >> ~/log; echo >> ~/log'\n",
        )
        .unwrap();
        let script = "cat \"$TLACUACHE_STATE_FILE\" >> ~/log; echo >> ~/log\nexit\n";
        let Some(output) = run_with_state(
            "bash",
            &["--rcfile", &rc.to_string_lossy(), "-i"],
            &[("HOME", home.path()), (STATE_FILE_VAR, &state)],
            script,
        ) else {
            return; // sin bash
        };
        assert!(output.status.success());
        // Prompt inicial, la orden, el prompt siguiente.
        let log = std::fs::read_to_string(&log).unwrap();
        assert_eq!(log, "prompt\nbusy\nprompt\n", "{output:?}");
    }

    #[test]
    fn zsh_integration_reports_prompt_and_busy() {
        let (Ok(home), Ok(ours)) = (tempfile::tempdir(), tempfile::tempdir()) else {
            return;
        };
        std::fs::write(ours.path().join(".zshenv"), ZSH_INTEGRATION_ZSHENV).unwrap();
        let state = home.path().join("state");
        let log = home.path().join("log");
        // El gancho del usuario se añade después del nuestro.
        std::fs::write(
            home.path().join(".zshrc"),
            "__mio() { cat \"$TLACUACHE_STATE_FILE\" >> ~/log; echo >> ~/log }\nadd-zsh-hook precmd __mio\n",
        )
        .unwrap();
        let script = "cat \"$TLACUACHE_STATE_FILE\" >> ~/log; echo >> ~/log\nexit\n";
        let Some(output) = run_with_state(
            "zsh",
            &["-i"],
            &[
                ("HOME", home.path()),
                ("ZDOTDIR", ours.path()),
                (STATE_FILE_VAR, &state),
            ],
            script,
        ) else {
            return; // sin zsh
        };
        assert!(output.status.success());
        let log = std::fs::read_to_string(&log).unwrap();
        assert_eq!(log, "prompt\nbusy\nprompt\n", "{output:?}");
    }

    /// fish sin terminal no muestra prompts: se emiten sus eventos a mano.
    #[test]
    fn fish_integration_reports_prompt_and_busy() {
        use std::process::Command;

        let Ok(dir) = tempfile::tempdir() else {
            return;
        };
        let file = dir.path().join("tlacuache.fish");
        let state = dir.path().join("state");
        std::fs::write(&file, FISH_INTEGRATION).unwrap();
        let script = format!(
            "source {}; emit fish_prompt; cat $TLACUACHE_STATE_FILE; echo; emit fish_preexec; cat $TLACUACHE_STATE_FILE",
            quote(&file.to_string_lossy(), FISH)
        );
        let Ok(output) = Command::new("fish")
            .args(["--no-config", "-c", &script])
            .env(STATE_FILE_VAR, &state)
            .output()
        else {
            return; // sin fish
        };
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "prompt\nbusy",
            "{output:?}"
        );
    }

    /// Sin la variable, la integración no escribe ningún archivo de estado
    /// (fuera de Flatpak nada cambia).
    #[test]
    fn integration_without_state_var_writes_nothing() {
        let Ok(home) = tempfile::tempdir() else {
            return;
        };
        let rc = home.path().join("tlacuache.rc");
        std::fs::write(&rc, BASH_INTEGRATION_RC).unwrap();
        let Some(output) = run_with_state(
            "bash",
            &["--rcfile", &rc.to_string_lossy(), "-i"],
            &[("HOME", home.path())],
            "echo \"PS0=[${PS0-}]\"\nexit\n",
        ) else {
            return;
        };
        assert!(String::from_utf8_lossy(&output.stdout).contains("PS0=[]"));
        // Solo el rcfile y el historial de bash.
        for entry in std::fs::read_dir(home.path()).unwrap() {
            let name = entry.unwrap().file_name();
            assert!(
                name == "tlacuache.rc" || name == ".bash_history",
                "archivo inesperado: {name:?}"
            );
        }
    }

    #[test]
    fn login_shell_from_passwd() {
        assert_eq!(
            login_shell("ana:x:1000:1000:Ana:/home/ana:/usr/bin/zsh\n"),
            Some("/usr/bin/zsh")
        );
        assert_eq!(login_shell("ana:x:1000:1000::/home/ana:"), None);
        assert_eq!(login_shell("ana:x:1000"), None);
        assert_eq!(login_shell(""), None);
    }

    #[test]
    fn shell_state_parse() {
        assert_eq!(ShellState::parse("prompt"), Some(ShellState::Prompt));
        assert_eq!(ShellState::parse("busy\n"), Some(ShellState::Busy));
        assert_eq!(ShellState::parse(""), None);
        assert_eq!(ShellState::parse("otro"), None);
    }

    #[test]
    fn host_command_puts_options_before_the_shell() {
        let argv = ["/usr/bin/zsh".to_owned(), "-l".to_owned()];
        let env = [
            "TERM=xterm-256color".to_owned(),
            "ZDOTDIR=/run/a b".to_owned(),
        ];
        assert_eq!(
            host_command(&argv, "/home/ana/con espacio", &env),
            [
                "flatpak-spawn",
                "--host",
                "--watch-bus",
                "--directory=/home/ana/con espacio",
                "--env=TERM=xterm-256color",
                "--env=ZDOTDIR=/run/a b",
                "/usr/bin/zsh",
                "-l",
            ]
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
