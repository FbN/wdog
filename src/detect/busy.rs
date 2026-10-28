//! High CPU with no output (AN-9, off by default): CPU kept rising for the threshold while
//! nothing was printed.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Busy {
    pub since: Option<Instant>,
    pub cpu_at_start: u64,
    pub fired: bool,
}

/// Returns `Some((seconds, cpu_ticks used))` when `BUSY` should fire.
pub fn on_sample(
    b: Busy,
    cpu_rising: bool,
    cpu_ticks: u64,
    threshold: Duration,
    now: Instant,
) -> (Busy, Option<(f64, u64)>) {
    if !cpu_rising {
        return (Busy { since: None, ..b }, None);
    }
    let (since, start) = match b.since {
        Some(s) => (s, b.cpu_at_start),
        None => (now, cpu_ticks),
    };
    let b = Busy {
        since: Some(since),
        cpu_at_start: start,
        ..b
    };
    let after = now.saturating_duration_since(since);
    if !b.fired && after >= threshold {
        (
            Busy { fired: true, ..b },
            Some((after.as_secs_f64(), cpu_ticks.saturating_sub(start))),
        )
    } else {
        (b, None)
    }
}

pub fn on_output(_b: Busy) -> Busy {
    Busy::default()
}

#[cfg(test)]
#[path = "busy_tests.rs"]
mod tests;
