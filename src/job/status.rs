//! status.json (LG-5): the job snapshot, written atomically.

use crate::core::State;
use crate::event::unix_secs;
use crate::exit::{self, EndReason};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{Instant, SystemTime};

pub const FINAL_STATES: [&str; 5] = ["done", "failed", "timeout", "killed", "stopped"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub v: u32,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// starting | running | done | failed | timeout | killed | stopped | lost
    pub state: String,
    pub watcher_pid: i32,
    pub watcher_start_ticks: u64,
    pub main_pid: Option<i32>,
    /// [pgid, leader_pid, leader_start_ticks]
    pub pgids: Vec<(i32, i32, u64)>,
    /// [pid, start_ticks]
    pub pids: Vec<(i32, u64)>,
    pub started_at: f64,
    pub last_output_at: f64,
    pub last_activity_at: f64,
    pub ended_at: Option<f64>,
    pub step: usize,
    pub total: usize,
    pub step_cmd: String,
    pub last_lines: Vec<String>,
    pub alerts: Vec<String>,
    pub exit: Option<i32>,
    pub signal: Option<String>,
    pub log: String,
    pub log_truncated: bool,
    /// null, or "watcher_died" (written by the guard)
    pub reason: Option<String>,
}

pub fn is_final(state: &str) -> bool {
    FINAL_STATES.contains(&state)
}

impl Status {
    pub fn is_final(&self) -> bool {
        is_final(&self.state)
    }
    pub fn is_live_state(&self) -> bool {
        self.state == "starting" || self.state == "running"
    }

    /// Written by `start` before the supervisor runs.
    pub fn initial(id: &str, log: &Path, total: usize, step_cmd: &str) -> Status {
        let now = unix_secs(SystemTime::now());
        Status {
            v: 1,
            id: id.to_string(),
            name: None,
            state: "starting".into(),
            watcher_pid: 0,
            watcher_start_ticks: 0,
            main_pid: None,
            pgids: vec![],
            pids: vec![],
            started_at: now,
            last_output_at: now,
            last_activity_at: now,
            ended_at: None,
            step: 0,
            total,
            step_cmd: step_cmd.to_string(),
            last_lines: vec![],
            alerts: vec![],
            exit: None,
            signal: None,
            log: log.display().to_string(),
            log_truncated: false,
            reason: None,
        }
    }

    /// Pure: the snapshot of a run.
    pub fn from_state(s: &State, now: Instant) -> Status {
        let wall = |i: Instant| s.job.started_wall + i.saturating_duration_since(s.started).as_secs_f64();
        let state = if s.finished {
            state_name(&s.results, s.ending.unwrap_or(EndReason::Completed))
        } else if s.pgids.is_empty() {
            "starting"
        } else {
            "running"
        };
        let exit = s.finished.then(|| exit::recorded_exit(&crate::core::exit_of(s)));
        let mut lines = s.out.last_lines.items();
        if let Some((p, _)) = s.out.partial.iter().flatten().max_by_key(|(_, t)| *t) {
            lines.push(p.clone());
            if lines.len() > crate::core::state::LAST_LINES {
                lines.remove(0);
            }
        }
        let step = s.step.min(s.total().saturating_sub(1));
        Status {
            v: 1,
            id: s.job.id.clone(),
            name: s.spec.name.clone(),
            state: state.into(),
            watcher_pid: s.job.watcher_pid,
            watcher_start_ticks: s.job.watcher_start_ticks,
            main_pid: s.main.map(|m| m.pid),
            pgids: s.pgids.clone(),
            pids: s.pids.clone(),
            started_at: s.job.started_wall,
            last_output_at: wall(s.out.last_output_at),
            last_activity_at: wall(s.last_activity_at),
            ended_at: s.finished.then(|| wall(now)),
            step: step + 1,
            total: s.total(),
            step_cmd: s.spec.step_name(step),
            last_lines: lines,
            alerts: s.alerts(),
            exit,
            signal: s.signal.map(|x| x.as_str().to_string()),
            log: s.job.log.display().to_string(),
            log_truncated: s.log_truncated,
            reason: None,
        }
    }
}

pub fn state_name(results: &[i32], end: EndReason) -> &'static str {
    match end {
        EndReason::Completed if results.iter().any(|c| *c != 0) => "failed",
        EndReason::Completed => "done",
        EndReason::Max => "timeout",
        EndReason::KilledOnStall => "killed",
        EndReason::Stopped(_) => "stopped",
    }
}

/// Write `status.json.tmp`, then rename.
pub fn write_atomic(path: &Path, st: &Status) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_vec(st)?;
    crate::job::dir::write_private(&tmp, &data)?;
    std::fs::rename(&tmp, path).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

pub fn read(path: &Path) -> Result<Status> {
    let data = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let v: serde_json::Value = serde_json::from_slice(&data).context("bad status.json")?;
    if v.get("v").and_then(|x| x.as_u64()).unwrap_or(1) > 1 {
        bail!("status.json was written by a newer wdog (v > 1)");
    }
    serde_json::from_value(v).context("bad status.json")
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
