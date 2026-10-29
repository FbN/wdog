use crate::detect::{
    busy::Busy, errors::ErrorMatcher, errors::Errors, orphans::Orphans, prompt::Prompt, silence::Silence,
    stall::Stall,
};
use crate::exit::EndReason;
use crate::lines::ring::{Head, Tail};
use crate::proc::sample::Sample;
use crate::spec::RunSpec;
use nix::sys::signal::Signal;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub const LAST_LINES: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `wdog run`
    Foreground,
    /// `wdog __supervise` (background job)
    Supervised,
}

impl Mode {
    pub fn grace(self) -> Duration {
        match self {
            Mode::Foreground => Duration::from_secs(5),
            Mode::Supervised => Duration::from_secs(30),
        }
    }
}

/// Fixed facts about the job, used to build `status.json`.
#[derive(Debug, Clone)]
pub struct JobInfo {
    pub id: String,
    pub log: PathBuf,
    pub watcher_pid: i32,
    pub watcher_start_ticks: u64,
    /// Wall clock (Unix seconds) at `State::started`.
    pub started_wall: f64,
    pub clk_tck: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MainProc {
    pub pid: i32,
    pub start_ticks: u64,
}

#[derive(Debug, Clone)]
pub struct OutputState {
    pub log_line_no: u64,
    pub tail: Tail<String>,
    pub head: Head<String>,
    pub last_lines: Tail<String>,
    pub last_output_at: Instant,
    /// Unterminated tail per stream (stdout, stderr) of the current step, with when it changed.
    pub partial: [Option<(String, Instant)>; 2],
}

#[derive(Debug, Clone, Default)]
pub struct Detectors {
    pub stall: Stall,
    pub prompt: Prompt,
    pub orphans: Orphans,
    pub errors: Errors,
    pub busy: Busy,
    pub silence: Silence,
}

#[derive(Debug, Clone)]
pub struct State {
    pub spec: Rc<RunSpec>,
    pub matcher: Rc<ErrorMatcher>,
    pub job: Rc<JobInfo>,
    pub mode: Mode,
    pub started: Instant,
    /// Current step, 0-based.
    pub step: usize,
    pub attempt: u32,
    pub step_started: Instant,
    pub main: Option<MainProc>,
    /// Main process exited; the step waits for its children.
    pub main_code: Option<i32>,
    pub no_children: bool,
    /// (pgid, leader pid, leader start_ticks)
    pub pgids: Vec<(i32, i32, u64)>,
    /// Known tree members (pid, start_ticks): latest sample plus reported orphans.
    pub pids: Vec<(i32, u64)>,
    /// Exit code of each finished step.
    pub results: Vec<i32>,
    pub out: OutputState,
    pub last_activity_at: Instant,
    pub last_sample: Option<Sample>,
    pub ending: Option<EndReason>,
    pub killing: bool,
    pub killed_at: Option<Instant>,
    pub finished: bool,
    pub signal: Option<Signal>,
    pub log_truncated: bool,
    pub status_dirty: bool,
    pub detectors: Detectors,
}

impl State {
    pub fn new(
        spec: Rc<RunSpec>,
        matcher: Rc<ErrorMatcher>,
        job: Rc<JobInfo>,
        mode: Mode,
        now: Instant,
    ) -> State {
        let out = OutputState {
            log_line_no: 0,
            tail: Tail::new(spec.output.tail.max(1)),
            head: Head::new(spec.output.head),
            last_lines: Tail::new(LAST_LINES),
            last_output_at: now,
            partial: [None, None],
        };
        State {
            spec,
            matcher,
            job,
            mode,
            started: now,
            step: 0,
            attempt: 0,
            step_started: now,
            main: None,
            main_code: None,
            no_children: false,
            pgids: vec![],
            pids: vec![],
            results: vec![],
            out,
            last_activity_at: now,
            last_sample: None,
            ending: None,
            killing: false,
            killed_at: None,
            finished: false,
            signal: None,
            log_truncated: false,
            status_dirty: false,
            detectors: Detectors::default(),
        }
    }

    pub fn total(&self) -> usize {
        self.spec.steps.len()
    }

    /// Last output line, or the current unterminated tail.
    pub fn last_text(&self) -> String {
        let newest = self.out.partial.iter().flatten().max_by_key(|(_, t)| *t);
        match newest {
            Some((p, _)) => p.clone(),
            None => self.out.last_lines.last().cloned().unwrap_or_default(),
        }
    }

    /// Active alerts, mirrored into status.json.
    pub fn alerts(&self) -> Vec<String> {
        let mut a = vec![];
        if self.detectors.stall.active() {
            a.push("STALL".to_string());
        }
        if self.detectors.prompt.fired {
            a.push("PROMPT".to_string());
        }
        if !self.detectors.orphans.reported.is_empty() {
            a.push("ORPHANS".to_string());
        }
        if self.detectors.busy.fired {
            a.push("BUSY".to_string());
        }
        if self.detectors.silence.fired {
            a.push("SILENCE".to_string());
        }
        a
    }
}
