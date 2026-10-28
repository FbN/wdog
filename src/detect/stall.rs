//! Stall (AN-1, AN-7): no output, no CPU and no I/O change for the threshold. Re-alerts at
//! doubling intervals while the episode lasts; `RESUMED` on the first activity after it.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stall {
    pub episode: Option<Episode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Episode {
    /// Last activity before the stall.
    pub since: Instant,
    pub alerts: u32,
}

impl Stall {
    pub fn active(&self) -> bool {
        self.episode.is_some()
    }
}

/// Returns `Some(seconds since last activity)` when a `STALL` should fire now.
pub fn check(s: Stall, last_activity: Instant, threshold: Duration, now: Instant) -> (Stall, Option<f64>) {
    let idle = now.saturating_duration_since(last_activity);
    match s.episode {
        None if idle >= threshold => (
            Stall {
                episode: Some(Episode {
                    since: last_activity,
                    alerts: 1,
                }),
            },
            Some(idle.as_secs_f64()),
        ),
        Some(ep) => {
            let factor = 1u32.checked_shl(ep.alerts).unwrap_or(u32::MAX);
            let next = ep.since + threshold.saturating_mul(factor);
            if now >= next {
                let ep = Episode {
                    alerts: ep.alerts + 1,
                    ..ep
                };
                (Stall { episode: Some(ep) }, Some(idle.as_secs_f64()))
            } else {
                (s, None)
            }
        }
        None => (s, None),
    }
}

/// Activity seen at `now`: ends the episode. Returns the episode length for `RESUMED`.
pub fn on_activity(s: Stall, now: Instant) -> (Stall, Option<f64>) {
    match s.episode {
        Some(ep) => (
            Stall::default(),
            Some(now.saturating_duration_since(ep.since).as_secs_f64()),
        ),
        None => (s, None),
    }
}

#[cfg(test)]
#[path = "stall_tests.rs"]
mod tests;
