//! Uso de disco de una unidad (barra de la barra lateral).

/// Nivel de ocupación para colorear la barra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageLevel {
    Normal,
    /// ≥ 80 % usado.
    Warning,
    /// ≥ 90 % usado.
    Critical,
}

/// Fracción usada (0.0–1.0). Un tamaño 0 cuenta como vacío.
pub fn used_fraction(free: u64, size: u64) -> f64 {
    if size == 0 {
        return 0.0;
    }
    let used = size.saturating_sub(free);
    let fraction = used as f64 / size as f64;
    fraction.clamp(0.0, 1.0)
}

pub fn usage_level(fraction: f64) -> UsageLevel {
    if fraction >= 0.9 {
        UsageLevel::Critical
    } else if fraction >= 0.8 {
        UsageLevel::Warning
    } else {
        UsageLevel::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_cases() {
        assert_eq!(used_fraction(0, 0), 0.0);
        assert_eq!(used_fraction(100, 100), 0.0);
        assert_eq!(used_fraction(0, 100), 1.0);
        assert_eq!(used_fraction(25, 100), 0.75);
        // free > size (datos raros de algunos FS): sin negativos.
        assert_eq!(used_fraction(200, 100), 0.0);
    }

    #[test]
    fn level_thresholds() {
        assert_eq!(usage_level(0.79), UsageLevel::Normal);
        assert_eq!(usage_level(0.8), UsageLevel::Warning);
        assert_eq!(usage_level(0.89), UsageLevel::Warning);
        assert_eq!(usage_level(0.9), UsageLevel::Critical);
        assert_eq!(usage_level(1.0), UsageLevel::Critical);
    }
}
