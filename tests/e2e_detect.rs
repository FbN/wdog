//! Phase 4: one deliberately broken command per anomaly (spec §7, NF-7).
#![allow(clippy::disallowed_methods)]

mod common;
use common::*;
use std::time::Duration;

/// Seconds from the start of the run to the first line containing `word`, using event timestamps.
fn event_after(e: &Env, id: &str, word: &str) -> Option<f64> {
    let ev = e.events(id);
    let mut t0 = None;
    for l in ev.lines() {
        let v: serde_json::Value = serde_json::from_str(l).ok()?;
        let ts = v["ts"].as_f64()?;
        if v["event"] == "START" {
            t0 = Some(ts);
        }
        if v["event"] == word {
            return Some(ts - t0?);
        }
    }
    None
}

#[test]
fn stall_acceptance() {
    let e = Env::new();
    let o = e.run(&["run", "--stall", "10", "--max", "13s", "--", "sleep 6401"]);
    assert!(o.has("STALL"), "{}", o.stdout);
    let t = event_after(&e, &e.only_job(), "STALL").unwrap();
    assert!(t < 12.0, "STALL after {t}s");
}

#[test]
fn busy_loop_is_not_a_stall() {
    let e = Env::new();
    let o = e.run(&[
        "run",
        "--interval",
        "500ms",
        "--stall",
        "3",
        "--max",
        "6s",
        "--",
        "while :; do :; done",
    ]);
    assert_eq!(o.code, Some(124));
    assert!(!o.has("STALL"), "{}", o.stdout);
}

#[test]
fn orphans_acceptance() {
    let e = Env::new();
    let o = e.run(&["run", "--", "sh -c 'sleep 6402 &'"]);
    assert!(o.has("ORPHANS"));
    assert!(o.took < Duration::from_secs(8), "{:?}", o.took);
    let id = e.only_job();
    e.run(&["stop", &id]);
    assert!(wait_gone("sleep 6402", Duration::from_secs(2)));
}

#[test]
fn error_live_acceptance() {
    let e = Env::new();
    let o = e.run(&["run", "--max", "4s", "--", "sh -c 'echo FATAL boom; sleep 6403'"]);
    assert!(o.has("ERROR_LIVE"));
    let t = event_after(&e, &e.only_job(), "ERROR_LIVE").unwrap();
    assert!(t < 3.0, "{t}");
}

#[test]
fn kill_on_stall_leaves_nothing() {
    let e = Env::new();
    let o = e.run(&[
        "run",
        "--interval",
        "200ms",
        "--stall",
        "2",
        "--kill-on-stall",
        "--",
        "sleep 6404",
    ]);
    assert_eq!(o.code, Some(126));
    assert!(wait_gone("sleep 6404", Duration::from_secs(1)));
}

#[test]
fn stall_then_resumed() {
    let e = Env::new();
    let o = e.run(&[
        "run",
        "--interval",
        "200ms",
        "--stall",
        "1",
        "--",
        "sleep 2; echo hi",
    ]);
    assert!(o.has("STALL"), "{}", o.stdout);
    assert!(o.has("RESUMED"), "{}", o.stdout);
}

#[test]
fn text_prompt_without_tty() {
    let e = Env::new();
    let o = e.run(&[
        "run",
        "--max",
        "8s",
        "--",
        "printf 'Overwrite? [y/N] '; sleep 6405",
    ]);
    assert!(o.has("PROMPT line=\"Overwrite? [y/N]\" via=text"), "{}", o.stdout);
    let t = event_after(&e, &e.only_job(), "PROMPT").unwrap();
    assert!(t < 7.0, "{t}");
}

#[test]
fn twenty_quick_steps() {
    let e = Env::new();
    let steps = vec!["true"; 20];
    let mut args = vec!["run", "--"];
    args.extend(steps);
    let o = e.run(&args);
    assert_eq!(o.code, Some(0));
    assert!(o.took < Duration::from_secs(1), "{:?}", o.took);
}

#[test]
fn progress_bar_is_activity() {
    let e = Env::new();
    let o = e.run(&[
        "run",
        "--interval",
        "200ms",
        "--stall",
        "2",
        "--",
        "for i in $(seq 8); do printf '\\r%d%%' $i; sleep 0.5; done",
    ]);
    assert_eq!(o.code, Some(0));
    assert!(!o.has("STALL"), "{}", o.stdout);
}

#[test]
fn busy_alert() {
    let e = Env::new();
    let o = e.run(&[
        "run",
        "--interval",
        "500ms",
        "--busy",
        "2",
        "--max",
        "4s",
        "--",
        "while :; do :; done",
    ]);
    assert!(o.has("BUSY"), "{}", o.stdout);
}

/// `--stdin-tty` with a pty that is wdog's controlling terminal (setsid + TIOCSCTTY).
#[test]
fn tty_prompt_acceptance() {
    use std::io::{Read, Write};
    use std::os::unix::process::CommandExt;
    if !has("bash") {
        return;
    }
    let e = Env::new();
    let pty = nix::pty::openpty(None, None).unwrap();
    let slave = std::fs::File::from(pty.slave);
    let mut c = e.cmd(&[
        "run",
        "--stdin-tty",
        "--interval",
        "500ms",
        "--max",
        "20s",
        "--",
        "bash -c 'read -p \"Continue? (y/N) \" x; echo got=$x'",
    ]);
    c.stdin(slave.try_clone().unwrap())
        .stdout(slave.try_clone().unwrap())
        .stderr(slave.try_clone().unwrap());
    unsafe {
        c.pre_exec(|| {
            nix::libc::setsid();
            nix::libc::ioctl(0, nix::libc::TIOCSCTTY as _, 0);
            Ok(())
        });
    }
    let mut child = c.spawn().unwrap();
    drop(slave);
    let mut master = std::fs::File::from(pty.master);
    // read until PROMPT shows up on the terminal
    let mut acc = String::new();
    let t0 = std::time::Instant::now();
    let mut buf = [0u8; 4096];
    let reader = master.try_clone().unwrap();
    let fd = std::os::fd::AsRawFd::as_raw_fd(&reader);
    unsafe { nix::libc::fcntl(fd, nix::libc::F_SETFL, nix::libc::O_NONBLOCK) };
    let mut r = reader;
    while !acc.contains("[wdog] PROMPT") && t0.elapsed() < Duration::from_secs(10) {
        match r.read(&mut buf) {
            Ok(n) if n > 0 => acc.push_str(&String::from_utf8_lossy(&buf[..n])),
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    assert!(acc.contains("[wdog] PROMPT"), "{acc}");
    assert!(
        t0.elapsed() < Duration::from_secs(8),
        "PROMPT after {:?}",
        t0.elapsed()
    );
    // the reader is blocked, not stopped by SIGTTIN
    let id = e.only_job();
    let pids = e.status(&id)["pids"].clone();
    for p in pids.as_array().unwrap() {
        let pid = p[0].as_i64().unwrap();
        if let Ok(s) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            let state = s[s.rfind(')').unwrap() + 2..].chars().next().unwrap();
            assert_ne!(state, 'T', "pid {pid} stopped");
        }
    }
    master.write_all(b"y\n").unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
    assert!(e.log(&id).contains("got=y"));
}
