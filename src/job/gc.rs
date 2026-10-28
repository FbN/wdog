//! Remove job dirs older than 24 h (BG-6).

use super::dir::{JobPaths, list_jobs};
use super::status;
use std::path::Path;
use std::time::{Duration, SystemTime};

pub const MAX_AGE: Duration = Duration::from_secs(24 * 3600);
const EVERY: Duration = Duration::from_secs(600);

/// Pure: `(id, watcher alive, status mtime)` -> ids to remove.
pub fn to_remove(jobs: &[(String, bool, SystemTime)], now: SystemTime) -> Vec<String> {
    jobs.iter()
        .filter(|(_, live, t)| !live && now.duration_since(*t).unwrap_or_default() > MAX_AGE)
        .map(|(id, _, _)| id.clone())
        .collect()
}

pub fn watcher_alive(p: &JobPaths) -> bool {
    match status::read(&p.status) {
        Ok(s) => s.is_live_state() && crate::proc::read::pid_alive(s.watcher_pid, s.watcher_start_ticks),
        Err(_) => false,
    }
}

pub fn run(home: &Path) -> usize {
    let jobs: Vec<(String, bool, SystemTime)> = list_jobs(home)
        .into_iter()
        .map(|(id, t)| {
            let live = watcher_alive(&JobPaths::new(home, &id));
            (id, live, t)
        })
        .collect();
    let ids = to_remove(&jobs, SystemTime::now());
    let mut n = 0;
    for id in ids {
        if std::fs::remove_dir_all(home.join(&id)).is_ok() {
            n += 1;
        }
    }
    n
}

/// At most once per 10 minutes, guarded by `WDOG_HOME/.gc`; errors are ignored.
pub fn maybe_run(home: &Path) {
    let stamp = home.join(".gc");
    let recent = std::fs::metadata(&stamp)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < EVERY);
    if recent {
        return;
    }
    let _ = std::fs::write(&stamp, b"");
    run(home);
}

#[cfg(test)]
#[path = "gc_tests.rs"]
mod tests;
