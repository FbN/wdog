//! Signal a job's verified groups and members (overview decision 11).

use crate::proc::{ProcEntry, read::snapshot_basic, tree};
use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::Pid;
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// Groups with a live member we recorded.
    pub groups: Vec<i32>,
    /// Recorded pids still alive with the same start time.
    pub pids: Vec<i32>,
    /// Groups with live members, none of them ours: never signalled.
    pub skipped: usize,
    /// Every live process the plan reaches.
    pub members: Vec<i32>,
}

/// Pure: what to signal.
pub fn plan_kill(entries: &[ProcEntry], pgids: &[(i32, i32, u64)], pids: &[(i32, u64)]) -> Plan {
    let mut known: Vec<(i32, u64)> = pgids.iter().map(|(_, p, t)| (*p, *t)).collect();
    known.extend_from_slice(pids);
    let mut plan = Plan::default();
    let mut members: HashSet<i32> = HashSet::new();
    for (g, _, _) in pgids {
        if plan.groups.contains(g) {
            continue;
        }
        let live: Vec<i32> = entries
            .iter()
            .filter(|e| e.pgid == *g && e.state != 'Z')
            .map(|e| e.pid)
            .collect();
        if live.is_empty() {
            continue;
        }
        if tree::group_is_ours(entries, *g, &known) {
            plan.groups.push(*g);
            members.extend(live);
        } else {
            plan.skipped += 1;
        }
    }
    for (p, t) in pids {
        if tree::alive(entries, *p, *t) && !plan.pids.contains(p) {
            plan.pids.push(*p);
            members.insert(*p);
        }
    }
    let mut m: Vec<i32> = members.into_iter().collect();
    m.sort();
    plan.members = m;
    plan
}

pub fn send(plan: &Plan, sig: Signal) {
    for g in &plan.groups {
        let _ = killpg(Pid::from_raw(*g), sig);
    }
    for p in &plan.pids {
        let _ = kill(Pid::from_raw(*p), sig);
    }
}

/// Signal the job now; returns the plan used.
pub fn signal_job(pgids: &[(i32, i32, u64)], pids: &[(i32, u64)], sig: Signal) -> Plan {
    let plan = plan_kill(&snapshot_basic(), pgids, pids);
    send(&plan, sig);
    plan
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub killed: usize,
    pub skipped: usize,
    pub survivors: usize,
}

/// Live processes of ours (re-planned from a fresh snapshot).
pub fn alive_count(pgids: &[(i32, i32, u64)], pids: &[(i32, u64)]) -> usize {
    plan_kill(&snapshot_basic(), pgids, pids).members.len()
}

/// Wait until nothing of ours is alive, up to `limit`. Returns true when all are gone.
pub fn wait_gone(pgids: &[(i32, i32, u64)], pids: &[(i32, u64)], limit: Duration) -> bool {
    let t0 = Instant::now();
    loop {
        if alive_count(pgids, pids) == 0 {
            return true;
        }
        if t0.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// TERM, wait up to `grace`, then KILL; only verified groups and pids.
pub fn kill_tree(pgids: &[(i32, i32, u64)], pids: &[(i32, u64)], grace: Duration) -> Report {
    let first = signal_job(pgids, pids, Signal::SIGTERM);
    // pids seen at the start stay "ours" even after their group leader is gone
    let mut all_pids: Vec<(i32, u64)> = pids.to_vec();
    let entries = snapshot_basic();
    for m in &first.members {
        if let Some(e) = entries.iter().find(|e| e.pid == *m) {
            all_pids.push((e.pid, e.start_ticks));
        }
    }
    if !wait_gone(pgids, &all_pids, grace) {
        signal_job(pgids, &all_pids, Signal::SIGKILL);
        wait_gone(pgids, &all_pids, Duration::from_secs(1));
    }
    let survivors = alive_count(pgids, &all_pids);
    Report {
        killed: first.members.len().saturating_sub(survivors),
        skipped: first.skipped,
        survivors,
    }
}

#[cfg(test)]
#[path = "kill_tests.rs"]
mod tests;
