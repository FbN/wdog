//! The guard (NF-4, overview decision 9): a detached process per run that kills the job tree
//! when the watcher dies without cleaning up. It holds the read end of a pipe; EOF means the
//! watcher exited; status.json says whether that was normal and what to kill.

use crate::event::{Event, render_json, unix_secs};
use crate::exec::kill;
use crate::job::dir::JobPaths;
use crate::job::status::{self, Status};
use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardAction {
    Nothing,
    Kill {
        pgids: Vec<(i32, i32, u64)>,
        pids: Vec<(i32, u64)>,
    },
}

/// Pure: final state -> the watcher finished normally; otherwise kill what it recorded.
pub fn guard_action(st: &Status) -> GuardAction {
    if st.is_final() || st.state == "lost" {
        GuardAction::Nothing
    } else {
        GuardAction::Kill {
            pgids: st.pgids.clone(),
            pids: st.pids.clone(),
        }
    }
}

/// Start `wdog __guard <id>` and keep the write end of its stdin (O_CLOEXEC: steps never get it).
pub fn start(paths: &JobPaths) -> Result<ChildStdin> {
    let exe = std::env::current_exe().context("cannot find the wdog binary")?;
    let mut child = Command::new(exe)
        .arg("__guard")
        .arg(&paths.id)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("cannot start the guard")?;
    let stdin = child.stdin.take().context("no guard pipe")?;
    // The guard forks and its first process exits at once. Waiting is allowed here: the reaper
    // does not run yet.
    #[allow(clippy::disallowed_methods)]
    let _ = child.wait();
    Ok(stdin)
}

/// Body of the detached guard process.
pub fn run(paths: &JobPaths) -> Result<()> {
    let mut b = [0u8; 64];
    let mut stdin = std::io::stdin().lock();
    loop {
        match stdin.read(&mut b) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    let Ok(mut st) = status::read(&paths.status) else {
        return Ok(());
    };
    let GuardAction::Kill { pgids, pids } = guard_action(&st) else {
        return Ok(());
    };
    // EOF arrives while the watcher exits; give it a moment to be gone for sure.
    let t0 = Instant::now();
    while crate::proc::read::pid_alive(st.watcher_pid, st.watcher_start_ticks)
        && t0.elapsed() < Duration::from_secs(2)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    let report = kill::kill_tree(&pgids, &pids, Duration::from_secs(2));
    st.state = "lost".into();
    st.reason = Some("watcher_died".into());
    st.ended_at = Some(unix_secs(SystemTime::now()));
    st.main_pid = None;
    status::write_atomic(&paths.status, &st)?;
    let line = render_json(
        &Event::Lost {
            killed: report.killed,
        },
        SystemTime::now(),
    );
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&paths.events)?;
    f.write_all(format!("{line}\n").as_bytes())?;
    Ok(())
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
