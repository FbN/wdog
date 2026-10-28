//! `wdog wait` and `wdog sleep` (WT-1..4): read the job dir, never signal the watcher.

use crate::cli::dur;
use crate::console;
use crate::event::{DoneReason, Event, parse_json, render_json, render_text};
use crate::exit::Exit;
use crate::job::dir::{JobPaths, job, list_jobs, wdog_home};
use crate::job::status::{self, Status};
use crate::job::tailer::Tailer;
use crate::lines::logfmt;
use crate::rt::channel::{Sender, channel};
use anyhow::{Result, bail};
use async_io::{Async, Timer};
use futures_lite::future;
use regex::Regex;
use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

const POLL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, PartialEq)]
pub enum WaitOutcome {
    /// Condition met -> exit 0, "READY ..."
    Ready(String),
    /// The job's DONE event (or one rebuilt from status.json).
    JobExited(Event),
    /// Job stopped (wdog stop or a signal) -> 1, the job's STOPPED line.
    Stopped(Event),
    /// STALL / PROMPT / ORPHANS / ERROR_LIVE / BUSY -> 1
    Alert(Event),
    /// --for elapsed -> 124 (wait) or 0 (sleep)
    For(f64),
    /// Watcher gone, or LOST from the guard -> 1
    Lost(Event),
}

pub fn wait_exit(o: &WaitOutcome, sleep: bool) -> i32 {
    match o {
        WaitOutcome::Ready(_) => 0,
        WaitOutcome::JobExited(Event::Done { exit: 0, .. }) => 0,
        WaitOutcome::JobExited(_)
        | WaitOutcome::Stopped(_)
        | WaitOutcome::Alert(_)
        | WaitOutcome::Lost(_) => 1,
        WaitOutcome::For(_) => {
            if sleep {
                0
            } else {
                crate::exit::LIMIT
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WaitInput {
    Events(Vec<Event>),
    LogLines(Vec<String>),
    /// status.json, and whether its watcher is alive
    Status(Box<Status>, bool),
    CondMet(String),
    ForElapsed(f64),
}

#[derive(Debug, Default)]
pub struct WaitSpec {
    pub until: Option<Regex>,
    /// `wdog sleep`: only alerts and LOST wake it; job exit does not.
    pub sleep: bool,
}

#[derive(Debug, Default)]
pub struct WaitState {
    stopped: Option<Event>,
}

/// Rebuild the DONE event from a final status.json.
pub fn done_from_status(st: &Status) -> Event {
    let reason = match st.state.as_str() {
        "done" => DoneReason::Ok,
        "timeout" => DoneReason::Max,
        "killed" => DoneReason::KillOnStall,
        "stopped" => DoneReason::Stopped,
        _ => DoneReason::StepFailed,
    };
    let dur = st.ended_at.map(|e| e - st.started_at).unwrap_or(0.0);
    Event::Done {
        exit: st.exit.unwrap_or(1),
        reason,
        dur,
    }
}

/// Pure: everything seen in one poll -> outcome. Priority: Ready > job end > Alert > For.
pub fn decide(spec: &WaitSpec, ws: &mut WaitState, inputs: &[WaitInput]) -> Option<WaitOutcome> {
    let mut ready = None;
    let mut end = None;
    let mut alert = None;
    let mut timeout = None;
    for i in inputs {
        match i {
            WaitInput::CondMet(what) => ready = ready.or(Some(what.clone())),
            WaitInput::LogLines(lines) => {
                if let Some(re) = &spec.until {
                    if lines.iter().any(|l| re.is_match(logfmt::raw(l))) {
                        ready = ready.or(Some(format!("until={}", crate::event::quote(re.as_str()))));
                    }
                }
            }
            WaitInput::Events(evs) => {
                for e in evs {
                    match e {
                        Event::Stopped { .. } => ws.stopped = Some(e.clone()),
                        Event::Done { reason, .. } if !spec.sleep => {
                            let out = if *reason == DoneReason::Stopped {
                                WaitOutcome::Stopped(ws.stopped.clone().unwrap_or_else(|| e.clone()))
                            } else {
                                WaitOutcome::JobExited(e.clone())
                            };
                            end = end.or(Some(out));
                        }
                        Event::Lost { .. } => end = end.or(Some(WaitOutcome::Lost(e.clone()))),
                        e if e.is_alert() => alert = alert.or(Some(e.clone())),
                        _ => {}
                    }
                }
            }
            WaitInput::Status(st, watcher_alive) => {
                if st.state == "lost" || (st.is_live_state() && st.watcher_pid > 0 && !watcher_alive) {
                    end = end.or(Some(WaitOutcome::Lost(Event::Lost { killed: 0 })));
                } else if st.is_final() && !spec.sleep {
                    let done = done_from_status(st);
                    let out = if st.state == "stopped" {
                        let sig = st.signal.clone().unwrap_or_else(|| "SIGTERM".into());
                        WaitOutcome::Stopped(ws.stopped.clone().unwrap_or(Event::Stopped { signal: sig }))
                    } else {
                        WaitOutcome::JobExited(done)
                    };
                    end = end.or(Some(out));
                }
            }
            WaitInput::ForElapsed(after) => timeout = Some(*after),
        }
    }
    if let Some(r) = ready {
        return Some(WaitOutcome::Ready(r));
    }
    if let Some(e) = end {
        return Some(e);
    }
    if let Some(a) = alert {
        return Some(WaitOutcome::Alert(a));
    }
    timeout.map(WaitOutcome::For)
}

fn print(o: &WaitOutcome, json: bool, suffix: &str) {
    let e = match o {
        WaitOutcome::Ready(what) => Event::Ready { what: what.clone() },
        WaitOutcome::JobExited(e)
        | WaitOutcome::Stopped(e)
        | WaitOutcome::Alert(e)
        | WaitOutcome::Lost(e) => e.clone(),
        WaitOutcome::For(after) => Event::Timeout { after: *after },
    };
    if json {
        console::out(&render_json(&e, SystemTime::now()));
    } else {
        console::out(&format!("{}{suffix}", render_text(&e)));
    }
}

async fn port_task(port: u16, tx: Sender<String>) {
    let addrs: [SocketAddr; 2] = [
        ([127, 0, 0, 1], port).into(),
        (std::net::Ipv6Addr::LOCALHOST, port).into(),
    ];
    loop {
        for a in addrs {
            let ok = future::or(async { Async::<TcpStream>::connect(a).await.is_ok() }, async {
                Timer::after(Duration::from_millis(500)).await;
                false
            })
            .await;
            if ok {
                tx.send(format!("port={port}"));
                return;
            }
        }
        Timer::after(Duration::from_millis(250)).await;
    }
}

fn url_ok(url: &str) -> bool {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .http_status_as_error(false)
        .build()
        .into();
    match agent.get(url).call() {
        Ok(r) => r.status().as_u16() < 500,
        Err(_) => false,
    }
}

/// ureq blocks: each attempt runs on a plain thread; polling `is_finished` needs no Arc.
async fn url_task(url: String, tx: Sender<String>) {
    loop {
        let u = url.clone();
        let h = std::thread::spawn(move || url_ok(&u));
        while !h.is_finished() {
            Timer::after(Duration::from_millis(50)).await;
        }
        if h.join().unwrap_or(false) {
            tx.send(format!("url={url}"));
            return;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
}

fn watcher_alive(st: &Status) -> bool {
    st.watcher_pid > 0 && crate::proc::read::pid_alive(st.watcher_pid, st.watcher_start_ticks)
}

pub struct WaitOpts {
    pub for_: Option<String>,
    pub until: Option<String>,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub file: Option<PathBuf>,
    pub json: bool,
}

/// `http://` always; `https://` only in a build with the `https` feature.
pub fn check_url(u: &str) -> Result<()> {
    if u.starts_with("http://") || (cfg!(feature = "https") && u.starts_with("https://")) {
        return Ok(());
    }
    if u.starts_with("https://") {
        bail!("--url {u}: https needs a wdog built with the 'https' feature; use http");
    }
    bail!("--url {u}: expected http://...")
}

pub fn wait(id: &str, o: WaitOpts) -> Result<Exit> {
    let for_ = o.for_.as_deref().map(|d| dur("for", d)).transpose()?;
    let until = o
        .until
        .as_deref()
        .map(Regex::new)
        .transpose()
        .map_err(|e| anyhow::anyhow!("--until: bad regex: {e}"))?;
    if let Some(u) = &o.url {
        check_url(u)?;
    }
    let home = wdog_home()?;
    let paths = job(&home, id)?;
    let spec = WaitSpec { until, sleep: false };
    let out = run_loop(&paths, &spec, for_, o.port, o.url, o.file)?;
    print(&out, o.json, "");
    Ok(Exit::Code(wait_exit(&out, false)))
}

fn run_loop(
    paths: &JobPaths,
    spec: &WaitSpec,
    for_: Option<Duration>,
    port: Option<u16>,
    url: Option<String>,
    file: Option<PathBuf>,
) -> Result<WaitOutcome> {
    let log: PathBuf = status::read(&paths.status)
        .map(|s| s.log.into())
        .unwrap_or(paths.log.clone());
    let ex = async_executor::LocalExecutor::new();
    let out = async_io::block_on(ex.run(async {
        let (tx, rx) = channel::<String>();
        let mut tasks = vec![];
        if let Some(p) = port {
            tasks.push(ex.spawn(port_task(p, tx.clone())));
        }
        if let Some(u) = url {
            tasks.push(ex.spawn(url_task(u, tx.clone())));
        }
        let mut events = Tailer::from_end(&paths.events);
        let mut logt = Tailer::from_start(&log);
        let t0 = Instant::now();
        let mut ws = WaitState::default();
        let mut pending: Vec<String> = vec![];
        loop {
            let mut inputs = vec![];
            pending.extend(std::iter::from_fn(|| rx.try_recv()));
            for c in pending.drain(..) {
                inputs.push(WaitInput::CondMet(c));
            }
            if let Some(f) = &file {
                if f.exists() {
                    inputs.push(WaitInput::CondMet(format!("file={}", f.display())));
                }
            }
            if spec.until.is_some() {
                inputs.push(WaitInput::LogLines(logt.read_lines()));
            }
            let evs: Vec<Event> = events.read_lines().iter().filter_map(|l| parse_json(l)).collect();
            inputs.push(WaitInput::Events(evs));
            if let Ok(st) = status::read(&paths.status) {
                let alive = watcher_alive(&st);
                inputs.push(WaitInput::Status(Box::new(st), alive));
            }
            if let Some(f) = for_ {
                if t0.elapsed() >= f {
                    inputs.push(WaitInput::ForElapsed(f.as_secs_f64()));
                }
            }
            if let Some(o) = decide(spec, &mut ws, &inputs) {
                return o;
            }
            let got = future::or(async { rx.recv().await }, async {
                Timer::after(POLL).await;
                None
            })
            .await;
            pending.extend(got);
        }
    }));
    Ok(out)
}

pub fn sleep(duration: &str, id: Option<&str>, json: bool) -> Result<Exit> {
    let d = dur("duration", duration)?;
    let home = wdog_home()?;
    let spec = WaitSpec {
        until: None,
        sleep: true,
    };
    let out = match id {
        Some(i) => {
            let paths = job(&home, i)?;
            run_loop(&paths, &spec, Some(d), None, None, None)?
        }
        None => {
            let (out, from) = sleep_all(&home, d);
            if let Some(job_id) = from {
                print(&out, json, &format!(" job={job_id}"));
                return Ok(Exit::Code(wait_exit(&out, true)));
            }
            out
        }
    };
    if !matches!(out, WaitOutcome::For(_)) || json {
        print(&out, json, "");
    }
    Ok(Exit::Code(wait_exit(&out, true)))
}

/// `wdog sleep D` without an id: wake on an alert from any running job.
fn sleep_all(home: &std::path::Path, d: Duration) -> (WaitOutcome, Option<String>) {
    let t0 = Instant::now();
    let mut tails: HashMap<String, Tailer> = HashMap::new();
    for (id, _) in list_jobs(home) {
        let p = JobPaths::new(home, &id);
        tails.insert(id, Tailer::from_end(&p.events));
    }
    let mut last_scan = Instant::now();
    loop {
        if last_scan.elapsed() >= Duration::from_secs(1) {
            last_scan = Instant::now();
            for (id, _) in list_jobs(home) {
                // jobs started during the sleep: every alert they raised is new
                tails
                    .entry(id.clone())
                    .or_insert_with(|| Tailer::from_start(&JobPaths::new(home, &id).events));
            }
        }
        for (id, t) in tails.iter_mut() {
            for l in t.read_lines() {
                if let Some(e) = parse_json(&l) {
                    if e.is_alert() {
                        return (WaitOutcome::Alert(e), Some(id.clone()));
                    }
                }
            }
        }
        if t0.elapsed() >= d {
            return (WaitOutcome::For(d.as_secs_f64()), None);
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
#[path = "wait_tests.rs"]
mod tests;
