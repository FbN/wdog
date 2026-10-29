//! No output for a while, whatever CPU and I/O do (`--silence`, off by default). Fires once
//! per quiet stretch; any output, or the next step, re-arms it.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Silence {
    /// CPU ticks and I/O bytes at the first sample of the quiet stretch.
    pub base: Option<(u64, u64)>,
    pub fired: bool,
}

/// What `SILENCE` reports: quiet seconds, CPU ticks and I/O bytes used meanwhile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fire {
    pub after: f64,
    pub cpu_ticks: u64,
    pub io_bytes: u64,
}

/// `quiet_since`: the last output, or the step's start if it printed nothing yet.
pub fn on_sample(
    s: Silence,
    quiet_since: Instant,
    cpu_ticks: u64,
    io_bytes: u64,
    threshold: Duration,
    now: Instant,
) -> (Silence, Option<Fire>) {
    let base = s.base.unwrap_or((cpu_ticks, io_bytes));
    let s = Silence {
        base: Some(base),
        ..s
    };
    let quiet = now.saturating_duration_since(quiet_since);
    if s.fired || quiet < threshold {
        return (s, None);
    }
    let fire = Fire {
        after: quiet.as_secs_f64(),
        cpu_ticks: cpu_ticks.saturating_sub(base.0),
        io_bytes: io_bytes.saturating_sub(base.1),
    };
    (Silence { fired: true, ..s }, Some(fire))
}

pub fn on_output(_s: Silence) -> Silence {
    Silence::default()
}

#[cfg(test)]
#[path = "silence_tests.rs"]
mod tests;
