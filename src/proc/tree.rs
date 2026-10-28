//! Process trees (AN-3) and the stale-PGID check (overview decision 11).

use super::ProcEntry;
use std::collections::{HashMap, HashSet, VecDeque};

/// Members of the job: descendants of `roots` (the watcher, a subreaper) plus every live entry whose
/// pgid is in `pgids`. Roots themselves and zombies are excluded.
pub fn descendants(entries: &[ProcEntry], roots: &[i32], pgids: &[i32]) -> Vec<i32> {
    let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
    for e in entries {
        children.entry(e.ppid).or_default().push(e.pid);
    }
    let zombie: HashSet<i32> = entries.iter().filter(|e| e.state == 'Z').map(|e| e.pid).collect();
    let mut seen: HashSet<i32> = HashSet::new();
    let mut out = vec![];
    let mut q: VecDeque<i32> = roots.iter().copied().collect();
    let root_set: HashSet<i32> = roots.iter().copied().collect();
    while let Some(p) = q.pop_front() {
        if !seen.insert(p) {
            continue; // cycles in bad data
        }
        if !root_set.contains(&p) && !zombie.contains(&p) {
            out.push(p);
        }
        if let Some(c) = children.get(&p) {
            q.extend(c.iter().copied());
        }
    }
    for e in entries {
        if pgids.contains(&e.pgid) && e.state != 'Z' && !root_set.contains(&e.pid) && seen.insert(e.pid) {
            out.push(e.pid);
        }
    }
    out
}

pub fn alive(entries: &[ProcEntry], pid: i32, start_ticks: u64) -> bool {
    entries
        .iter()
        .any(|e| e.pid == pid && e.start_ticks == start_ticks && e.state != 'Z')
}

/// True only if some live entry in `pgid` matches a recorded `(pid, start_ticks)`. A group whose
/// members are all unknown reused the number and belongs to someone else.
pub fn group_is_ours(entries: &[ProcEntry], pgid: i32, known: &[(i32, u64)]) -> bool {
    entries
        .iter()
        .filter(|e| e.pgid == pgid && e.state != 'Z')
        .any(|e| known.iter().any(|(p, t)| *p == e.pid && *t == e.start_ticks))
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
