//! Runs one job: sources (pipes, reaper, signals, timers) -> reducer -> effects.

use crate::console;
use crate::core::{self, Effect, Input, JobInfo, Mode, State};
use crate::detect::errors::ErrorMatcher;
use crate::event::{Event, render_json, render_text, unix_secs};
use crate::exec::envfile::{self, EnvChange};
use crate::exec::{guard, kill, spawn};
use crate::exit::{EndReason, Exit, WDOG_FAILED};
use crate::job::dir::{JobPaths, write_private};
use crate::job::status::{self, Status};
use crate::lines::Stream;
use crate::proc::{read, sample, tree};
use crate::rt::channel::{Sender, channel};
use crate::rt::pipe::{ReadState, read_lines};
use crate::spec::RunSpec;
use anyhow::{Context, Result};
use async_executor::{LocalExecutor, Task};
use async_io::Timer;
use futures_lite::{StreamExt, future};
use nix::sys::signal::Signal;
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

pub const DEFAULT_LOG_MAX: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sink {
    Console,
    Silent,
}

/// out.log with a byte cap (overview decision 10).
struct LogWriter {
    w: BufWriter<File>,
    bytes: u64,
    cap: u64,
    full: bool,
}

impl LogWriter {
    fn open(path: &Path) -> Result<LogWriter> {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("cannot open log {}", path.display()))?;
        let bytes = f.metadata().map(|m| m.len()).unwrap_or(0);
        let cap = std::env::var("WDOG_LOG_MAX")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_LOG_MAX);
        Ok(LogWriter {
            w: BufWriter::with_capacity(64 * 1024, f),
            bytes,
            cap,
            full: false,
        })
    }

    /// Returns true when the log just became full.
    fn append(&mut self, line: &str) -> bool {
        if self.full {
            return false;
        }
        let n = line.len() as u64 + 1;
        if self.bytes + n > self.cap {
            self.full = true;
            return true;
        }
        self.bytes += n;
        let _ = self.w.write_all(line.as_bytes());
        let _ = self.w.write_all(b"\n");
        false
    }

    fn flush(&mut self) {
        let _ = self.w.flush();
    }
}

struct Outputs {
    sink: Sink,
    json: bool,
    log: LogWriter,
    events: File,
    status_path: PathBuf,
}

impl Outputs {
    fn emit(&mut self, e: &Event) {
        let json = render_json(e, SystemTime::now());
        let _ = self.events.write_all(format!("{json}\n").as_bytes());
        if self.sink == Sink::Console {
            if self.json {
                console::out(&json);
            } else {
                console::out(&render_text(e));
            }
        }
    }

    fn write_status(&self, st: &Status) {
        let _ = status::write_atomic(&self.status_path, st);
    }
}

/// Shared between the main loop, the reaper and the drain tasks (single thread: Rc + Cell).
struct Shared {
    /// (step, attempt, pid) of the current main process.
    main: Cell<Option<(usize, u32, i32)>>,
    readers: RefCell<Vec<(usize, u32, Rc<ReadState>)>>,
    pgids: RefCell<Vec<i32>>,
    spawned: Cell<bool>,
}

/// Run a job to the end. The job dir exists; `spec.json` is written here if missing.
pub fn run_job(paths: &JobPaths, spec: RunSpec, mode: Mode, sink: Sink) -> Result<Exit> {
    let started = Instant::now();
    let started_wall = unix_secs(SystemTime::now());
    let log_path = spec.log.clone().unwrap_or_else(|| paths.log.clone());
    if !paths.spec.exists() {
        write_private(&paths.spec, &serde_json::to_vec(&spec)?)?;
    }
    let matcher = ErrorMatcher::new(&spec.err_patterns).context("bad error pattern")?;
    let job = JobInfo {
        id: paths.id.clone(),
        log: log_path.clone(),
        watcher_pid: std::process::id() as i32,
        watcher_start_ticks: read::self_start_ticks(),
        started_wall,
        clk_tck: read::clk_tck(),
    };
    let events = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.events)
        .with_context(|| format!("cannot open {}", paths.events.display()))?;
    let mut io = Outputs {
        sink,
        json: spec.json,
        log: LogWriter::open(&log_path)?,
        events,
        status_path: paths.status.clone(),
    };
    let spec = Rc::new(spec);
    let mut state = State::new(spec.clone(), Rc::new(matcher), Rc::new(job), mode, started);
    io.write_status(&Status::from_state(&state, started));
    io.emit(&Event::Start {
        id: paths.id.clone(),
        name: spec.name.clone(),
        replaced: spec.replaced.clone(),
        log: log_path.display().to_string(),
    });

    // 1. guard, before the subreaper is set, so it is not in our tree
    let guard_pipe = guard::start(paths)?;

    // 2. env files (the only place that waits on a child with std)
    let exe = std::env::current_exe().context("cannot find the wdog binary")?;
    let env = match envfile::load(&spec.env_files, spec.cwd.as_deref(), &exe) {
        Ok(files) => {
            let mut all = vec![];
            for (f, ch) in files {
                let keys = if spec.json {
                    envfile::key_names(&ch)
                } else {
                    vec![]
                };
                io.emit(&Event::Env {
                    file: f.display().to_string(),
                    loaded: ch.len(),
                    keys,
                });
                all.extend(ch);
            }
            envfile::merge(all, &spec.env)
        }
        Err(e) => return Ok(fail(&mut io, &mut state, &e.to_string(), guard_pipe)),
    };

    // 3. subreaper, signals, reaper; 4. steps
    // SAFETY: prctl with constant arguments.
    unsafe { nix::libc::prctl(nix::libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) };
    let signals = async_signal::Signals::new([
        async_signal::Signal::Child,
        async_signal::Signal::Int,
        async_signal::Signal::Term,
        async_signal::Signal::Hup,
    ])
    .context("cannot register signal handlers")?;
    let take_tty = spec.stdin_tty && spawn::in_tty_foreground();

    let ex: Rc<LocalExecutor<'static>> = Rc::new(LocalExecutor::new());
    let result = async_io::block_on(ex.run(main_loop(&ex, state, &mut io, env, signals, take_tty)));
    io.log.flush();
    if take_tty {
        spawn::take_back_tty();
    }
    // the final status is written: closing the pipe tells the guard all is well
    drop(guard_pipe);
    result
}

/// wdog itself failed before any step: ERROR line, final status `failed`, exit 125.
fn fail(io: &mut Outputs, s: &mut State, msg: &str, guard_pipe: std::process::ChildStdin) -> Exit {
    io.emit(&Event::Error { msg: msg.to_string() });
    s.results.push(WDOG_FAILED);
    s.ending = Some(EndReason::Completed);
    s.finished = true;
    io.write_status(&Status::from_state(s, Instant::now()));
    drop(guard_pipe);
    Exit::Code(WDOG_FAILED)
}

async fn main_loop(
    ex: &Rc<LocalExecutor<'static>>,
    mut s: State,
    io: &mut Outputs,
    env: Vec<EnvChange>,
    signals: async_signal::Signals,
    take_tty: bool,
) -> Result<Exit> {
    let (tx, rx) = channel::<Input>();
    let shared = Rc::new(Shared {
        main: Cell::new(None),
        readers: RefCell::new(vec![]),
        pgids: RefCell::new(vec![]),
        spawned: Cell::new(false),
    });
    let spec = s.spec.clone();
    let mut tasks: Vec<Task<()>> = vec![];
    tasks.push(ex.spawn(reaper(
        ex.clone(),
        signals,
        tx.clone(),
        shared.clone(),
        spec.stdin_tty,
    )));
    tasks.push(ex.spawn(sampler(tx.clone(), shared.clone(), spec.interval, spec.stdin_tty)));
    tasks.push(ex.spawn(ticker(tx.clone(), spec.interval.min(Duration::from_millis(500)))));
    if let Some(max) = spec.max {
        let t = tx.clone();
        tasks.push(ex.spawn(async move {
            Timer::after(max).await;
            t.send(Input::MaxReached);
        }));
    }

    let mut queue: VecDeque<Effect> = VecDeque::from([Effect::Spawn { step: 0, attempt: 0 }]);
    let mut finished = false;
    loop {
        while let Some(e) = queue.pop_front() {
            match run_effect(ex, e, &s, io, &env, &tx, &shared, &mut tasks, take_tty) {
                Ok(true) => finished = true,
                Ok(false) => {}
                Err(msg) => {
                    io.emit(&Event::Error { msg });
                    s.results.push(WDOG_FAILED);
                    s.ending = Some(EndReason::Completed);
                    s.finished = true;
                    kill::signal_job(&s.pgids, &s.pids, Signal::SIGKILL);
                    io.write_status(&Status::from_state(&s, Instant::now()));
                    return Ok(Exit::Code(WDOG_FAILED));
                }
            }
        }
        io.log.flush();
        if finished {
            break;
        }
        let Some(input) = rx.recv().await else { break };
        let (ns, fx) = core::step(s, input, Instant::now());
        s = ns;
        queue.extend(fx);
        // take everything already queued before flushing the log
        while let Some(input) = rx.try_recv() {
            let (ns, fx) = core::step(s, input, Instant::now());
            s = ns;
            queue.extend(fx);
            if queue.len() > 256 {
                break;
            }
        }
    }

    // final drain: output already in the pipes (orphans included), 50 ms idle, 200 ms cap
    let t0 = Instant::now();
    loop {
        let left = Duration::from_millis(200).saturating_sub(t0.elapsed());
        if left.is_zero() {
            break;
        }
        let next = future::or(async { rx.recv().await }, async {
            Timer::after(left.min(Duration::from_millis(50))).await;
            None
        })
        .await;
        let Some(input) = next else { break };
        if !matches!(
            input,
            Input::Line { .. } | Input::Partial { .. } | Input::LogFull { .. }
        ) {
            continue;
        }
        let (ns, fx) = core::step(s, input, Instant::now());
        s = ns;
        for e in fx {
            let _ = run_effect(ex, e, &s, io, &env, &tx, &shared, &mut tasks, take_tty);
        }
    }
    for e in core::summary(&s, Instant::now()) {
        let _ = run_effect(ex, e, &s, io, &env, &tx, &shared, &mut tasks, take_tty);
    }
    io.log.flush();
    // dropping the reader tasks closes the pipes; orphans that write later get SIGPIPE
    drop(tasks);
    Ok(core::exit_of(&s))
}

/// Ok(true) when the run is finished.
#[allow(clippy::too_many_arguments)]
fn run_effect(
    ex: &Rc<LocalExecutor<'static>>,
    e: Effect,
    s: &State,
    io: &mut Outputs,
    env: &[EnvChange],
    tx: &Sender<Input>,
    shared: &Rc<Shared>,
    tasks: &mut Vec<Task<()>>,
    take_tty: bool,
) -> Result<bool, String> {
    match e {
        Effect::Print(t) => {
            if io.sink == Sink::Console {
                console::out(&t);
            }
        }
        Effect::PrintErr(t) => {
            if io.sink == Sink::Console {
                console::err(&t);
            }
        }
        Effect::Emit(ev) => io.emit(&ev),
        Effect::AppendLog(line) => {
            if io.log.append(&line) {
                tx.send(Input::LogFull { bytes: io.log.cap });
            }
        }
        Effect::WriteStatus => io.write_status(&Status::from_state(s, Instant::now())),
        Effect::Signal(sig) => {
            kill::signal_job(&s.pgids, &s.pids, sig);
        }
        Effect::ArmGrace(d) => {
            let t = tx.clone();
            ex.spawn(async move {
                Timer::after(d).await;
                t.send(Input::GraceExpired);
            })
            .detach();
        }
        Effect::Spawn { step, attempt } => {
            let cmd = &s.spec.steps[step];
            let sp = spawn::spawn_step(cmd, &s.spec, env, take_tty)
                .map_err(|e| format!("cannot spawn /bin/sh: {e}"))?;
            shared.main.set(Some((step, attempt, sp.pid)));
            shared.spawned.set(true);
            shared.pgids.borrow_mut().push(sp.pgid);
            let rs = Rc::new(ReadState {
                open: Cell::new(2),
                last_read: Cell::new(Instant::now()),
            });
            shared.readers.borrow_mut().push((step, attempt, rs.clone()));
            tasks.push(ex.spawn(read_lines(
                step,
                attempt,
                sp.stdout,
                Stream::Out,
                tx.clone(),
                rs.clone(),
            )));
            tasks.push(ex.spawn(read_lines(step, attempt, sp.stderr, Stream::Err, tx.clone(), rs)));
            tx.send(Input::Spawned {
                step,
                attempt,
                pid: sp.pid,
                pgid: sp.pgid,
                start_ticks: sp.start_ticks,
            });
        }
        Effect::Finish(_) => return Ok(true),
    }
    Ok(false)
}

/// SIGCHLD -> waitpid(-1) loop; INT/TERM/HUP -> Input::Signal. A 1 s timer is a safety net.
async fn reaper(
    ex: Rc<LocalExecutor<'static>>,
    mut sigs: async_signal::Signals,
    tx: Sender<Input>,
    shared: Rc<Shared>,
    want_tty: bool,
) {
    loop {
        let next = future::or(async { sigs.next().await.and_then(|r| r.ok()) }, async {
            Timer::after(Duration::from_secs(1)).await;
            None
        })
        .await;
        let forwarded = match next {
            Some(async_signal::Signal::Int) => Some(Signal::SIGINT),
            Some(async_signal::Signal::Term) => Some(Signal::SIGTERM),
            Some(async_signal::Signal::Hup) => Some(Signal::SIGHUP),
            _ => None,
        };
        match forwarded {
            Some(sig) => tx.send(Input::Signal(sig)),
            None => reap(&ex, &tx, &shared, want_tty),
        }
    }
}

fn reap(ex: &Rc<LocalExecutor<'static>>, tx: &Sender<Input>, shared: &Rc<Shared>, want_tty: bool) {
    loop {
        match waitpid(None, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => break,
            Ok(st) => {
                let Some(pid) = st.pid() else { continue };
                if let Some((step, attempt, main)) = shared.main.get() {
                    if main == pid.as_raw() {
                        shared.main.set(None);
                        let code = crate::exit::status_code(st);
                        ex.spawn(drain(step, attempt, code, tx.clone(), shared.clone(), want_tty))
                            .detach();
                    }
                }
            }
            Err(nix::errno::Errno::EINTR) => continue,
            Err(nix::errno::Errno::ECHILD) => {
                if shared.spawned.get() {
                    tx.send(Input::NoChildren);
                }
                break;
            }
            Err(_) => break,
        }
    }
}

/// After the main process exited: let the readers catch up (both at EOF, or 50 ms without a
/// read since the drain began, 200 ms cap), take a fresh sample, then send `MainExited`.
async fn drain(step: usize, attempt: u32, code: i32, tx: Sender<Input>, shared: Rc<Shared>, want_tty: bool) {
    let rs = shared
        .readers
        .borrow()
        .iter()
        .find(|(s, a, _)| *s == step && *a == attempt)
        .map(|(_, _, r)| r.clone());
    let t0 = Instant::now();
    if let Some(rs) = rs {
        loop {
            let idle_from = rs.last_read.get().max(t0);
            if rs.open.get() == 0
                || idle_from.elapsed() >= Duration::from_millis(50)
                || t0.elapsed() >= Duration::from_millis(200)
            {
                break;
            }
            Timer::after(Duration::from_millis(5)).await;
        }
    }
    let sample = take_sample(&shared, want_tty);
    tx.send(Input::MainExited {
        step,
        code,
        sample: Some(sample),
    });
}

fn take_sample(shared: &Shared, want_tty: bool) -> sample::Sample {
    let mut entries = read::snapshot_basic();
    let me = std::process::id() as i32;
    let members = tree::descendants(&entries, &[me], &shared.pgids.borrow());
    for e in entries.iter_mut().filter(|e| members.contains(&e.pid)) {
        read::enrich(e, want_tty);
    }
    sample::aggregate(&entries, &members, Instant::now())
}

async fn sampler(tx: Sender<Input>, shared: Rc<Shared>, every: Duration, want_tty: bool) {
    loop {
        Timer::after(every).await;
        if shared.spawned.get() {
            tx.send(Input::Sample(take_sample(&shared, want_tty)));
        }
    }
}

async fn ticker(tx: Sender<Input>, every: Duration) {
    loop {
        Timer::after(every).await;
        tx.send(Input::Tick);
    }
}
