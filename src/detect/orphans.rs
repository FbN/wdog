//! Leftover children (AN-5): after the main process exited, members still alive after the grace.

use crate::proc::sample::Member;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Orphans {
    /// Set when the main process exited and the step waits for its children.
    pub waiting_since: Option<Instant>,
    /// Already reported (pid, start_ticks), never reported twice and ignored by later steps.
    pub reported: Vec<(i32, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// No leftovers: the step is complete.
    Finish,
    Wait,
    /// Grace over with members alive: alert, then the step is complete.
    Alert(Vec<(i32, String)>),
}

/// Members that count as leftovers: not the main process, not reported before, not zombies.
pub fn leftovers<'a>(
    members: &'a [Member],
    main: Option<(i32, u64)>,
    reported: &[(i32, u64)],
) -> Vec<&'a Member> {
    members
        .iter()
        .filter(|m| m.state != 'Z')
        .filter(|m| main.is_none_or(|(p, t)| !(m.pid == p && m.start_ticks == t)))
        .filter(|m| !reported.contains(&(m.pid, m.start_ticks)))
        .collect()
}

pub fn decide(o: &Orphans, no_children: bool, left: &[&Member], grace: Duration, now: Instant) -> Decision {
    let Some(since) = o.waiting_since else {
        return Decision::Wait;
    };
    if no_children || left.is_empty() {
        return Decision::Finish;
    }
    if now.saturating_duration_since(since) >= grace {
        return Decision::Alert(left.iter().map(|m| (m.pid, m.cmdline.clone())).collect());
    }
    Decision::Wait
}

#[cfg(test)]
#[path = "orphans_tests.rs"]
mod tests;
