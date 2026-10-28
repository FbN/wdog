//! `wdog status [<id>]` (BG-2).

use crate::console;
use crate::duration::fmt_secs;
use crate::event::{quote, unix_secs};
use crate::exit::Exit;
use crate::job::dir::{JobPaths, job, list_jobs, wdog_home};
use crate::job::status::{self, Status};
use anyhow::Result;
use std::time::SystemTime;

/// Status as shown: a live state whose watcher is gone becomes `lost`.
pub fn effective(mut st: Status) -> Status {
    if st.is_live_state()
        && st.watcher_pid > 0
        && !crate::proc::read::pid_alive(st.watcher_pid, st.watcher_start_ticks)
    {
        st.state = "lost".into();
    }
    st
}

/// Pure: one line per job.
pub fn status_line(st: &Status, now: f64) -> String {
    let end = st.ended_at.unwrap_or(now);
    let runtime = fmt_secs(end - st.started_at);
    let idle = if st.is_live_state() {
        format!("{:.0}s", (now - st.last_activity_at).max(0.0))
    } else {
        "-".into()
    };
    let last = st.last_lines.last().cloned().unwrap_or_default();
    let exit = st.exit.map(|e| format!(" exit={e}")).unwrap_or_default();
    let alerts = if st.alerts.is_empty() {
        String::new()
    } else {
        format!(" alerts={}", st.alerts.join(","))
    };
    format!(
        "{:<6} {:<8} {:>6} idle={:<5} step={}/{}{exit}{alerts} last={}",
        st.id,
        st.state,
        runtime,
        idle,
        st.step,
        st.total,
        quote(&last)
    )
}

pub fn run(id: Option<&str>, json: bool) -> Result<Exit> {
    let home = wdog_home()?;
    let ids: Vec<String> = match id {
        Some(i) => vec![job(&home, i)?.id],
        None => list_jobs(&home).into_iter().map(|(i, _)| i).collect(),
    };
    let now = unix_secs(SystemTime::now());
    for i in ids {
        let p = JobPaths::new(&home, &i);
        let Ok(st) = status::read(&p.status) else { continue };
        let st = effective(st);
        if json {
            console::out(&serde_json::to_string(&st)?);
        } else {
            console::out(&status_line(&st, now));
        }
    }
    Ok(Exit::Code(0))
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
