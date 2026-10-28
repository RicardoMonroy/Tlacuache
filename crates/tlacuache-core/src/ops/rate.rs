//! Velocidad y tiempo restante de una operación a partir de su progreso.

/// Cuánto tarda la velocidad en acercarse a la mitad de un cambio: con menos
/// salta con cada archivo pequeño; con más tarda en notar una memoria lenta.
const HALF_LIFE_MS: f64 = 3_000.0;
/// Muestras más juntas que esto no cuentan (el tramo se mide desde la última
/// que contó): con intervalos muy cortos la velocidad es solo ruido.
const MIN_INTERVAL_MS: u64 = 200;
/// Sin estimación durante el primer segundo medido.
const WARMUP_MS: u64 = 1_000;

/// Medidor de velocidad de una operación: promedio móvil exponencial de los
/// bytes por segundo entre muestras (instante, bytes hechos). El instante lo
/// da la app (un reloj monótono en ms), así que se prueba sin reloj.
#[derive(Debug, Clone, Default)]
pub struct RateMeter {
    last: Option<(u64, u64)>,
    rate: Option<f64>,
    measured_ms: u64,
}

impl RateMeter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Añade una muestra. Si el tiempo o los bytes retroceden (otra
    /// operación, o el runner reinició la cuenta), empieza de nuevo.
    pub fn sample(&mut self, now_ms: u64, bytes_done: u64) {
        let Some((t, b)) = self.last else {
            self.last = Some((now_ms, bytes_done));
            return;
        };
        if now_ms < t || bytes_done < b {
            *self = Self {
                last: Some((now_ms, bytes_done)),
                ..Self::default()
            };
            return;
        }
        let dt = now_ms - t;
        if dt < MIN_INTERVAL_MS {
            return;
        }
        let instant = (bytes_done - b) as f64 * 1_000.0 / dt as f64;
        let alpha = 1.0 - 0.5_f64.powf(dt as f64 / HALF_LIFE_MS);
        self.rate = Some(match self.rate {
            None => instant,
            Some(r) => r + alpha * (instant - r),
        });
        self.measured_ms += dt;
        self.last = Some((now_ms, bytes_done));
    }

    /// Bytes por segundo, o `None` mientras no hay un segundo medido.
    pub fn bytes_per_sec(&self) -> Option<u64> {
        if self.measured_ms < WARMUP_MS {
            return None;
        }
        self.rate.map(|r| r.round() as u64)
    }

    /// Segundos que faltan para llegar a `bytes_total` (redondeado hacia
    /// arriba), o `None` sin velocidad o si está parado.
    pub fn eta_secs(&self, bytes_total: u64) -> Option<u64> {
        let rate = self.bytes_per_sec().filter(|&r| r > 0)?;
        let (_, done) = self.last?;
        Some(bytes_total.saturating_sub(done).div_ceil(rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1_000_000;

    /// Muestras cada 250 ms a `mb_per_sec` desde `start` (ms, bytes).
    fn feed(m: &mut RateMeter, start: (u64, u64), mb_per_sec: u64, samples: u64) -> (u64, u64) {
        let (mut t, mut b) = start;
        for _ in 0..samples {
            t += 250;
            b += mb_per_sec * MB / 4;
            m.sample(t, b);
        }
        (t, b)
    }

    #[test]
    fn steady_rate_and_eta() {
        let mut m = RateMeter::new();
        m.sample(0, 0);
        assert_eq!(m.bytes_per_sec(), None);
        let (_, done) = feed(&mut m, (0, 0), 10, 3);
        assert_eq!(m.bytes_per_sec(), None, "aún no hay un segundo medido");
        feed(&mut m, (750, done), 10, 1);
        assert_eq!(m.bytes_per_sec(), Some(10 * MB));
        // 10 MB hechos de 100 MB: 90 MB a 10 MB/s.
        assert_eq!(m.eta_secs(100 * MB), Some(9));
        assert_eq!(m.eta_secs(5 * MB), Some(0));
    }

    #[test]
    fn follows_a_slowdown_gradually() {
        let mut m = RateMeter::new();
        m.sample(0, 0);
        let at = feed(&mut m, (0, 0), 20, 8);
        assert_eq!(m.bytes_per_sec(), Some(20 * MB));
        let at = feed(&mut m, at, 5, 1);
        let r = m.bytes_per_sec().unwrap();
        assert!(r > 5 * MB && r < 20 * MB, "{r}");
        feed(&mut m, at, 5, 120); // 30 s: diez vidas medias
        assert!(m.bytes_per_sec().unwrap().abs_diff(5 * MB) < MB / 10);
    }

    #[test]
    fn ignores_close_samples_and_restarts_when_going_back() {
        let mut m = RateMeter::new();
        m.sample(0, 0);
        m.sample(50, 4 * MB); // demasiado cerca: no cuenta
        m.sample(1_000, 4 * MB);
        assert_eq!(m.bytes_per_sec(), Some(4 * MB));
        m.sample(1_100, MB); // retrocede: empieza de nuevo
        assert_eq!(m.bytes_per_sec(), None);
        assert_eq!(m.eta_secs(10 * MB), None);
    }

    #[test]
    fn stalled_has_no_eta() {
        let mut m = RateMeter::new();
        m.sample(0, 0);
        m.sample(2_000, 0);
        assert_eq!(m.bytes_per_sec(), Some(0));
        assert_eq!(m.eta_secs(MB), None);
    }
}
