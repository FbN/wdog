//! Durations on the command line: "45", "45s", "30m", "2h", "1.5m", "200ms".

use std::fmt;
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub enum DurationError {
    Empty,
    BadNumber(String),
    BadSuffix(String),
    TooLarge(String),
}

impl fmt::Display for DurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DurationError::Empty => write!(f, "empty duration"),
            DurationError::BadNumber(s) => write!(f, "bad duration '{s}' (use 45, 45s, 30m, 2h, 200ms)"),
            DurationError::BadSuffix(s) => write!(f, "unknown duration unit in '{s}' (use ms, s, m, h)"),
            DurationError::TooLarge(s) => write!(f, "duration too large: '{s}'"),
        }
    }
}

impl std::error::Error for DurationError {}

pub fn parse(s: &str) -> Result<Duration, DurationError> {
    let t = s.trim();
    if t.is_empty() {
        return Err(DurationError::Empty);
    }
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    if num.is_empty() || num.starts_with('.') && num.len() == 1 {
        return Err(DurationError::BadNumber(s.to_string()));
    }
    let value: f64 = num.parse().map_err(|_| DurationError::BadNumber(s.to_string()))?;
    let factor = match unit {
        "" | "s" => 1.0,
        "ms" => 0.001,
        "m" => 60.0,
        "h" => 3600.0,
        _ => return Err(DurationError::BadSuffix(s.to_string())),
    };
    let secs = value * factor;
    if !secs.is_finite() || secs > 1.0e9 {
        return Err(DurationError::TooLarge(s.to_string()));
    }
    Ok(Duration::from_secs_f64(secs))
}

/// Short form for event lines: "41s", "2m05s", "1h02m". Below 10 s one decimal is kept for
/// sub-second values ("0.3s").
pub fn fmt_short(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        return format!("{:.1}s", d.as_secs_f64());
    }
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{}h{:02}m", s / 3600, (s % 3600) / 60)
    }
}

pub fn fmt_secs(secs: f64) -> String {
    fmt_short(Duration::from_secs_f64(secs.max(0.0)))
}

#[cfg(test)]
#[path = "duration_tests.rs"]
mod tests;
