//! Permisos Unix legibles para el diálogo de propiedades.

/// `rwxr-xr-x` a partir de `unix::mode`, incluidos setuid/setgid (`s`/`S`)
/// y sticky (`t`/`T`).
pub fn mode_string(mode: u32) -> String {
    let bit = |mask: u32, c: char| if mode & mask != 0 { c } else { '-' };
    let special = |exec: u32, special: u32, lower: char, upper: char| match (
        mode & exec != 0,
        mode & special != 0,
    ) {
        (true, true) => lower,
        (false, true) => upper,
        (true, false) => 'x',
        (false, false) => '-',
    };
    [
        bit(0o400, 'r'),
        bit(0o200, 'w'),
        special(0o100, 0o4000, 's', 'S'),
        bit(0o040, 'r'),
        bit(0o020, 'w'),
        special(0o010, 0o2000, 's', 'S'),
        bit(0o004, 'r'),
        bit(0o002, 'w'),
        special(0o001, 0o1000, 't', 'T'),
    ]
    .into_iter()
    .collect()
}

/// Permisos en octal (`755`, `4755` con bits especiales).
pub fn mode_octal(mode: u32) -> String {
    let perms = mode & 0o7777;
    if perms > 0o777 {
        format!("{perms:04o}")
    } else {
        format!("{perms:03o}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_modes() {
        assert_eq!(mode_string(0o755), "rwxr-xr-x");
        assert_eq!(mode_string(0o644), "rw-r--r--");
        assert_eq!(mode_string(0o600), "rw-------");
        assert_eq!(mode_string(0o000), "---------");
        // El tipo de archivo (bits altos) se ignora.
        assert_eq!(mode_string(0o100644), "rw-r--r--");
    }

    #[test]
    fn special_bits() {
        assert_eq!(mode_string(0o4755), "rwsr-xr-x");
        assert_eq!(mode_string(0o4644), "rwSr--r--");
        assert_eq!(mode_string(0o2755), "rwxr-sr-x");
        assert_eq!(mode_string(0o1777), "rwxrwxrwt");
        assert_eq!(mode_string(0o1776), "rwxrwxrwT");
    }

    #[test]
    fn octal() {
        assert_eq!(mode_octal(0o100644), "644");
        assert_eq!(mode_octal(0o40755), "755");
        assert_eq!(mode_octal(0o4755), "4755");
        assert_eq!(mode_octal(0o1777), "1777");
    }
}
