//! Colores del tema (paleta Nord) que no viven en el CSS: la terminal VTE
//! usa la misma paleta de 16 colores ANSI.

/// Texto y fondo de la terminal.
pub const TERMINAL_FOREGROUND: &str = "#D8DEE9";
pub const TERMINAL_BACKGROUND: &str = "#2E3440";

/// Paleta ANSI Nord: 8 normales y 8 brillantes (negro, rojo, verde,
/// amarillo, azul, magenta, cian, blanco).
pub const TERMINAL_PALETTE: [&str; 16] = [
    "#3B4252", "#BF616A", "#A3BE8C", "#EBCB8B", "#81A1C1", "#B48EAD", "#88C0D0", "#E5E9F0",
    "#4C566A", "#BF616A", "#A3BE8C", "#EBCB8B", "#81A1C1", "#B48EAD", "#8FBCBB", "#ECEFF4",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn is_hex_color(text: &str) -> bool {
        text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
    }

    #[test]
    fn all_terminal_colors_are_valid_hex() {
        assert!(is_hex_color(TERMINAL_FOREGROUND));
        assert!(is_hex_color(TERMINAL_BACKGROUND));
        assert!(TERMINAL_PALETTE.iter().all(|c| is_hex_color(c)));
    }

    #[test]
    fn background_differs_from_black_for_contrast() {
        assert_ne!(TERMINAL_BACKGROUND, TERMINAL_PALETTE[0]);
    }
}
