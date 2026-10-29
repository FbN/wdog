//! `wdog start`: write the job dir, start `__supervise` detached, print the id (BG-1).

use crate::cli::{RunArgs, to_spec};
use crate::console;
use crate::event::{Event, json_line, parse_json, render_json, render_text};
use crate::exit::{Exit, WDOG_FAILED};
use crate::job::dir::{new_job, wdog_home, write_private};
use crate::job::status::{self, Status};
use anyhow::{Context, Result, bail};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

pub fn run(a: &RunArgs) -> Result<Exit> {
    if a.stdin_tty {
        bail!("--stdin-tty is not available for start: a background job has no terminal");
    }
    let spec = to_spec(a, false)?;
    let home = wdog_home()?;
    crate::job::gc::maybe_run(&home);
    let paths = new_job(&home)?;
    write_private(&paths.spec, &serde_json::to_vec(&spec)?)?;
    let log = spec.log.clone().unwrap_or_else(|| paths.log.clone());
    status::write_atomic(
        &paths.status,
        &Status::initial(&paths.id, &log, spec.steps.len(), &spec.step_name(0)),
    )?;
    let exe = std::env::current_exe().context("cannot find the wdog binary")?;
    let mut c = Command::new(exe);
    c.arg("__supervise")
        .arg(&paths.id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe.
    unsafe {
        c.pre_exec(|| {
            nix::libc::setsid();
            Ok(())
        });
    }
    let mut child = c.spawn().context("cannot start the supervisor")?;
    let limit = Duration::from_secs(2) + Duration::from_secs(10) * spec.env_files.len() as u32;
    let t0 = Instant::now();
    let id_line = |starting: bool| {
        if a.json {
            let mut j = serde_json::json!({ "id": paths.id, "log": log.display().to_string() });
            if starting {
                j["state"] = "starting".into();
            }
            json_line("START", j, SystemTime::now())
        } else if starting {
            format!("[wdog] id={} log={} state=starting", paths.id, log.display())
        } else {
            format!("[wdog] id={} log={}", paths.id, log.display())
        }
    };
    let show = |e: &Event| {
        if a.json {
            render_json(e, SystemTime::now())
        } else {
            render_text(e)
        }
    };
    loop {
        let st = status::read(&paths.status).ok();
        let state = st.as_ref().map(|s| s.state.clone()).unwrap_or_default();
        if let Some(err) = error_event(&paths.events) {
            if state != "running" {
                console::out(&show(&err));
                return Ok(Exit::Code(WDOG_FAILED));
            }
        }
        if state != "starting" && !state.is_empty() {
            console::out(&id_line(false));
            return Ok(Exit::Code(0));
        }
        if let Ok(Some(_)) = child.try_wait() {
            // the supervisor died while starting
            std::thread::sleep(Duration::from_millis(20));
            if let Some(err) = error_event(&paths.events) {
                console::out(&show(&err));
            } else if let Ok(st) = status::read(&paths.status) {
                if st.state != "starting" {
                    console::out(&id_line(false));
                    return Ok(Exit::Code(0));
                }
                let msg = "the background job did not start".to_string();
                console::out(&show(&Event::Error { msg }));
            }
            return Ok(Exit::Code(WDOG_FAILED));
        }
        if t0.elapsed() >= limit {
            console::out(&id_line(true));
            return Ok(Exit::Code(0));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn error_event(events: &std::path::Path) -> Option<Event> {
    let data = std::fs::read_to_string(events).ok()?;
    data.lines()
        .filter_map(parse_json)
        .find(|e| matches!(e, Event::Error { .. }))
}
