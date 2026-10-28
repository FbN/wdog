//! `wdog stop <id> [--grace 5s]` (BG-4).

use crate::cli::dur;
use crate::console;
use crate::event::json_line;
use crate::exec::kill;
use crate::exit::Exit;
use crate::job::dir::{job, wdog_home};
use crate::job::status::{self, Status};
use crate::proc::read::pid_alive;
use anyhow::Result;
use nix::sys::signal::{Signal, kill as send};
use nix::unistd::Pid;
use std::time::{Duration, Instant, SystemTime};

fn watcher_alive(st: &Status) -> bool {
    st.watcher_pid > 0 && pid_alive(st.watcher_pid, st.watcher_start_ticks)
}

pub fn run(id: &str, grace: &str, json: bool) -> Result<Exit> {
    let grace = dur("grace", grace)?;
    let home = wdog_home()?;
    let paths = job(&home, id)?;
    let mut st = status::read(&paths.status)?;

    // still starting: give the supervisor up to 2 s to reach running
    let t0 = Instant::now();
    while st.state == "starting" && t0.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(20));
        st = status::read(&paths.status)?;
    }

    let plan = kill::plan_kill(&crate::proc::read::snapshot_basic(), &st.pgids, &st.pids);
    if !st.is_live_state() && plan.members.is_empty() && plan.skipped == 0 {
        if json {
            let j = serde_json::json!({ "id": paths.id, "already": st.state });
            console::out(&json_line("STOPPED", j, SystemTime::now()));
        } else {
            console::out(&format!("[wdog] STOPPED already={}", st.state));
        }
        return Ok(Exit::Code(0));
    }

    let report = if st.is_live_state() && watcher_alive(&st) {
        // the supervisor forwards SIGTERM and records state=stopped; stop enforces the grace
        // count before signalling: the supervisor may kill everything before our first read
        let mut first_members = kill::alive_count(&st.pgids, &st.pids);
        let _ = send(Pid::from_raw(st.watcher_pid), Signal::SIGTERM);
        let t0 = Instant::now();
        loop {
            let cur = status::read(&paths.status).unwrap_or_else(|_| st.clone());
            let n = kill::alive_count(&cur.pgids, &cur.pids);
            first_members = first_members.max(n);
            if n == 0 && (!cur.pgids.is_empty() || !watcher_alive(&cur)) {
                st = cur;
                break;
            }
            if t0.elapsed() >= grace {
                kill::signal_job(&cur.pgids, &cur.pids, Signal::SIGKILL);
                kill::wait_gone(&cur.pgids, &cur.pids, Duration::from_secs(1));
                st = cur;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        // let the supervisor write the final state and exit
        let t1 = Instant::now();
        while watcher_alive(&st) && t1.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let survivors = kill::alive_count(&st.pgids, &st.pids);
        let skipped = kill::plan_kill(&crate::proc::read::snapshot_basic(), &st.pgids, &st.pids).skipped;
        kill::Report {
            killed: first_members.saturating_sub(survivors),
            skipped,
            survivors,
        }
    } else {
        // supervisor gone (lost job, or a finished `wdog run` that left orphans)
        kill::kill_tree(&st.pgids, &st.pids, grace)
    };
    if json {
        let j = serde_json::json!({
            "id": paths.id,
            "killed": report.killed,
            "skipped": report.skipped,
            "survivors": report.survivors,
        });
        console::out(&json_line("STOPPED", j, SystemTime::now()));
        return Ok(Exit::Code(if report.survivors > 0 { 1 } else { 0 }));
    }
    let skipped = if report.skipped > 0 {
        format!(" skipped={}", report.skipped)
    } else {
        String::new()
    };
    console::out(&format!(
        "[wdog] STOPPED {} killed={}{skipped}",
        paths.id, report.killed
    ));
    Ok(Exit::Code(if report.survivors > 0 { 1 } else { 0 }))
}
