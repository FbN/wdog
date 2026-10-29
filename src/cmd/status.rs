//! `wdog status [<id>]` (BG-2).

use crate::console;
use crate::duration::fmt_secs;
use crate::event::{Event, clip, parse_json, quote, unix_secs};
use crate::exit::Exit;
use crate::job::dir::{JobPaths, job, list_jobs, wdog_home};
use crate::job::status::{self, Status};
use crate::spec::RunSpec;
use anyhow::Result;
use serde::Serialize;
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
    let name = st.name.as_ref().map(|n| format!(" name={n}")).unwrap_or_default();
    let exit = st.exit.map(|e| format!(" exit={e}")).unwrap_or_default();
    let alerts = if st.alerts.is_empty() {
        String::new()
    } else {
        format!(" alerts={}", st.alerts.join(","))
    };
    format!(
        "{:<6} {:<8} {:>6} idle={:<5} step={}/{}{name}{exit}{alerts} last={}",
        st.id,
        st.state,
        runtime,
        idle,
        st.step,
        st.total,
        quote(&last)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Ok,
    Failed,
    Running,
    /// Not started yet; the job is still live.
    Pending,
    /// The job ended before this step started.
    NotRun,
    /// Started, but the job ended without a result for it (watcher lost).
    Interrupted,
}

impl StepState {
    pub fn as_str(self) -> &'static str {
        match self {
            StepState::Ok => "ok",
            StepState::Failed => "failed",
            StepState::Running => "running",
            StepState::Pending => "pending",
            StepState::NotRun => "not_run",
            StepState::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepRow {
    pub n: usize,
    pub state: StepState,
    pub exit: Option<i32>,
    pub dur: Option<f64>,
    /// Runs of this step so far (more than 1 with `--retry`).
    pub attempts: u32,
    pub cmd: String,
}

/// Pure: one row per step, from the spec's steps, the job's events and its status.
/// Each attempt ends with a STEP event; a RETRY right after it means that step runs again.
pub fn step_rows(steps: &[String], events: &[Event], st: &Status) -> Vec<StepRow> {
    let mut rows: Vec<StepRow> = steps
        .iter()
        .enumerate()
        .map(|(i, cmd)| StepRow {
            n: i + 1,
            state: StepState::Pending,
            exit: None,
            dur: None,
            attempts: 0,
            cmd: cmd.clone(),
        })
        .collect();
    let mut last = None;
    for ev in events {
        match ev {
            Event::Step { n, ok, exit, dur, .. } => {
                if let Some(r) = rows.get_mut(n.wrapping_sub(1)) {
                    r.state = if *ok { StepState::Ok } else { StepState::Failed };
                    r.exit = Some(*exit);
                    r.dur = Some(*dur);
                    r.attempts += 1;
                    last = Some(n - 1);
                }
            }
            Event::Retry { .. } => {
                if let Some(r) = last.and_then(|i| rows.get_mut(i)) {
                    r.state = StepState::Running;
                }
            }
            _ => {}
        }
    }
    let live = st.is_live_state();
    for r in &mut rows {
        let current = r.n == st.step;
        r.state = match r.state {
            StepState::Running if live => StepState::Running,
            StepState::Running => StepState::Interrupted,
            StepState::Pending if current => {
                if live {
                    StepState::Running
                } else {
                    StepState::Interrupted
                }
            }
            StepState::Pending if live => StepState::Pending,
            StepState::Pending => StepState::NotRun,
            done => done,
        };
        if matches!(r.state, StepState::Running | StepState::Interrupted) {
            // the attempt that started but has no STEP event yet
            r.attempts += 1;
            r.exit = None;
            r.dur = None;
        }
    }
    rows
}

/// Pure: the text line for one step row.
pub fn step_line(r: &StepRow, total: usize) -> String {
    let exit = r.exit.map(|e| format!("exit={e}")).unwrap_or_default();
    let dur = r.dur.map(fmt_secs).unwrap_or_default();
    let tries = if r.attempts > 1 {
        format!(" attempts={}", r.attempts)
    } else {
        String::new()
    };
    format!(
        "  {:>2}/{total} {:<11} {:<8} {:>6}{tries}  {}",
        r.n,
        r.state.as_str(),
        exit,
        dur,
        clip(&r.cmd)
    )
}

fn read_rows(p: &JobPaths, st: &Status) -> Option<Vec<StepRow>> {
    let spec: RunSpec = serde_json::from_slice(&std::fs::read(&p.spec).ok()?).ok()?;
    let text = std::fs::read_to_string(&p.events).unwrap_or_default();
    let events: Vec<Event> = text.lines().filter_map(parse_json).collect();
    let names: Vec<String> = (0..spec.steps.len()).map(|i| spec.step_name(i)).collect();
    Some(step_rows(&names, &events, st))
}

pub fn run(id: Option<&str>, steps: bool, json: bool) -> Result<Exit> {
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
        let rows = if steps { read_rows(&p, &st) } else { None };
        if json {
            let mut v = serde_json::to_value(&st)?;
            if let (Some(rows), Some(obj)) = (&rows, v.as_object_mut()) {
                obj.insert("steps".into(), serde_json::to_value(rows)?);
            }
            console::out(&serde_json::to_string(&v)?);
        } else {
            console::out(&status_line(&st, now));
            for r in rows.iter().flatten() {
                console::out(&step_line(r, st.total));
            }
        }
    }
    Ok(Exit::Code(0))
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
