//! Edad relativa de un archivo y su bucket de color (columna "Edad").
//!
//! Devuelve datos, no texto: la app formatea con su capa de strings.

use std::time::{Duration, SystemTime};

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;
const MONTH_DAYS: u64 = 30;
const YEAR_DAYS: u64 = 365;

/// Bucket de color (`docs/THEMES.md` §3): menos de 24 h, 1–6 días, 7–29
/// días, 1–11 meses y 1 año o más.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgeBucket {
    Hour,
    Day,
    Week,
    Month,
    Year,
}

impl AgeBucket {
    /// Clave en `[age]` de los temas (y en las clases CSS).
    pub fn id(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgeUnit {
    /// Menos de un minuto (o fecha en el futuro por desfase de reloj).
    Now,
    Minutes,
    Hours,
    Days,
    Months,
    Years,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Age {
    pub bucket: AgeBucket,
    pub unit: AgeUnit,
    /// Cantidad en `unit` (0 para `Now`).
    pub amount: u64,
}

impl Age {
    /// Edad de `modified` vista desde `now`. Fechas futuras cuentan como
    /// recién modificadas.
    pub fn between(modified: SystemTime, now: SystemTime) -> Self {
        Self::from_duration(now.duration_since(modified).unwrap_or(Duration::ZERO))
    }

    pub fn from_duration(elapsed: Duration) -> Self {
        let secs = elapsed.as_secs();
        let days = secs / DAY;

        let bucket = match secs {
            s if s < DAY => AgeBucket::Hour,
            s if s < WEEK => AgeBucket::Day,
            _ if days < MONTH_DAYS => AgeBucket::Week,
            _ if days < YEAR_DAYS => AgeBucket::Month,
            _ => AgeBucket::Year,
        };

        let (unit, amount) = match secs {
            s if s < MINUTE => (AgeUnit::Now, 0),
            s if s < HOUR => (AgeUnit::Minutes, s / MINUTE),
            s if s < DAY => (AgeUnit::Hours, s / HOUR),
            _ if days < MONTH_DAYS => (AgeUnit::Days, days),
            _ if days < YEAR_DAYS => (AgeUnit::Months, days / MONTH_DAYS),
            _ => (AgeUnit::Years, days / YEAR_DAYS),
        };

        Self {
            bucket,
            unit,
            amount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn age(secs: u64) -> Age {
        Age::from_duration(Duration::from_secs(secs))
    }

    #[test]
    fn bucket_ids_match_theme_keys() {
        let ids: Vec<_> = [
            AgeBucket::Hour,
            AgeBucket::Day,
            AgeBucket::Week,
            AgeBucket::Month,
            AgeBucket::Year,
        ]
        .iter()
        .map(|b| b.id())
        .collect();
        assert_eq!(ids, ["hour", "day", "week", "month", "year"]);
    }

    #[test]
    fn bucket_boundaries() {
        assert_eq!(age(0).bucket, AgeBucket::Hour);
        assert_eq!(age(DAY - 1).bucket, AgeBucket::Hour);
        assert_eq!(age(DAY).bucket, AgeBucket::Day);
        assert_eq!(age(WEEK - 1).bucket, AgeBucket::Day);
        assert_eq!(age(WEEK).bucket, AgeBucket::Week);
        assert_eq!(age(30 * DAY - 1).bucket, AgeBucket::Week);
        assert_eq!(age(30 * DAY).bucket, AgeBucket::Month);
        assert_eq!(age(365 * DAY - 1).bucket, AgeBucket::Month);
        assert_eq!(age(365 * DAY).bucket, AgeBucket::Year);
    }

    #[test]
    fn unit_and_amount() {
        let ua = |secs| {
            let a = age(secs);
            (a.unit, a.amount)
        };
        assert_eq!(ua(59), (AgeUnit::Now, 0));
        assert_eq!(ua(MINUTE), (AgeUnit::Minutes, 1));
        assert_eq!(ua(HOUR - 1), (AgeUnit::Minutes, 59));
        assert_eq!(ua(HOUR), (AgeUnit::Hours, 1));
        assert_eq!(ua(DAY - 1), (AgeUnit::Hours, 23));
        assert_eq!(ua(DAY), (AgeUnit::Days, 1));
        assert_eq!(ua(29 * DAY), (AgeUnit::Days, 29));
        assert_eq!(ua(30 * DAY), (AgeUnit::Months, 1));
        assert_eq!(ua(364 * DAY), (AgeUnit::Months, 12));
        assert_eq!(ua(365 * DAY), (AgeUnit::Years, 1));
        assert_eq!(ua(3 * 365 * DAY + 100 * DAY), (AgeUnit::Years, 3));
    }

    #[test]
    fn future_dates_count_as_now() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let future = now + Duration::from_secs(3600);
        let a = Age::between(future, now);
        assert_eq!(a.unit, AgeUnit::Now);
        assert_eq!(a.bucket, AgeBucket::Hour);
    }

    #[test]
    fn between_uses_elapsed_time() {
        let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let now = modified + Duration::from_secs(3 * DAY);
        assert_eq!(
            Age::between(modified, now),
            Age {
                bucket: AgeBucket::Day,
                unit: AgeUnit::Days,
                amount: 3
            }
        );
    }
}
