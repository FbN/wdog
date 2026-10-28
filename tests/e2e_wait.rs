//! Phase 6: `wait` and `sleep` (spec WT-1..4).
#![allow(clippy::disallowed_methods)]

mod common;
use common::*;
use std::time::Duration;

#[test]
fn port_acceptance_8123() {
    if !has("python3") {
        return;
    }
    let e = Env::new();
    let id = e.start(&["--", "exec python3 -m http.server 8123 --bind 127.0.0.1"]);
    let o = e.run(&["wait", &id, "--port", "8123", "--for", "10s"]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(o.has("READY port=8123"));
    assert!(o.took < Duration::from_secs(5));
    let o = e.run(&["wait", &id, "--url", "http://127.0.0.1:8123/", "--for", "5s"]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    e.run(&["stop", &id]);
}

#[test]
fn port_free_and_https_rejected() {
    let e = Env::new();
    let port = free_port();
    let id = e.start(&[
        "--",
        &format!("sleep 1; exec python3 -m http.server {port} --bind 127.0.0.1 || sleep 600"),
    ]);
    if has("python3") {
        let o = e.run(&["wait", &id, "--port", &port.to_string(), "--for", "10s"]);
        assert_eq!(o.code, Some(0), "{}", o.stdout);
    }
    let o = e.run(&["wait", &id, "--url", "https://localhost/", "--for", "1s"]);
    assert_eq!(o.code, Some(125));
    e.run(&["stop", &id]);
}

#[test]
fn until_new_and_old_lines() {
    let e = Env::new();
    let id = e.start(&["--", "sleep 1; echo listening; sleep 6301"]);
    let o = e.run(&["wait", &id, "--until", "listening", "--for", "5s"]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(
        o.took >= Duration::from_millis(500) && o.took < Duration::from_secs(4),
        "{:?}",
        o.took
    );
    // already printed before this wait started
    let o = e.run(&["wait", &id, "--until", "^listen", "--for", "5s"]);
    assert_eq!(o.code, Some(0));
    assert!(o.took < Duration::from_secs(1));
    e.run(&["stop", &id]);
}

#[test]
fn file_condition() {
    let e = Env::new();
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("ready");
    let id = e.start(&["--", &format!("sleep 1; touch '{}'; sleep 6302", f.display())]);
    let o = e.run(&["wait", &id, "--file", f.to_str().unwrap(), "--for", "5s"]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(o.has("READY file="));
    e.run(&["stop", &id]);
}

#[test]
fn for_limit() {
    let e = Env::new();
    let id = e.start(&["--", "sleep 6303"]);
    let o = e.run(&["wait", &id, "--for", "1s"]);
    assert_eq!(o.code, Some(124));
    assert!(o.has("TIMEOUT"));
    assert!(!procs("sleep 6303").is_empty(), "the job keeps running");
    e.run(&["stop", &id]);
}

#[test]
fn job_exit_while_waiting() {
    let e = Env::new();
    let id = e.start(&["--", "sleep 1; exit 3"]);
    let o = e.run(&["wait", &id, "--for", "10s"]);
    assert_eq!(o.code, Some(1));
    assert!(o.has("DONE exit=3"), "{}", o.stdout);
    let id = e.start(&["--", "true"]);
    assert!(e.wait_state(&id, "done", Duration::from_secs(3)));
    let o = e.run(&["wait", &id]);
    assert_eq!(o.code, Some(0), "already finished: {}", o.stdout);
}

#[test]
fn stop_while_waiting() {
    let e = Env::new();
    let id = e.start(&["--", "sleep 6304"]);
    assert!(e.wait_state(&id, "running", Duration::from_secs(3)));
    let w = e
        .cmd(&["wait", &id, "--for", "10s"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    e.run(&["stop", &id]);
    let o = finish(w);
    assert_eq!(o.code, Some(1));
    assert!(o.has("STOPPED sig=SIGTERM"), "{}", o.stdout);
}

#[test]
fn alerts_wake_wait() {
    let e = Env::new();
    let id = e.start(&["--", "sleep 0.5; echo FATAL x; sleep 6305"]);
    let o = e.run(&["wait", &id, "--for", "10s"]);
    assert_eq!(o.code, Some(1));
    assert!(o.has("ERROR_LIVE"), "{}", o.stdout);
    e.run(&["stop", &id]);
    let id = e.start(&["--interval", "200ms", "--stall", "2", "--", "sleep 6306"]);
    let o = e.run(&["wait", &id, "--for", "10s"]);
    assert_eq!(o.code, Some(1));
    assert!(o.has("STALL"), "{}", o.stdout);
    e.run(&["stop", &id]);
}

#[test]
fn sleep_with_and_without_alert() {
    let e = Env::new();
    let id = e.start(&["--interval", "200ms", "--stall", "2", "--", "sleep 6307"]);
    let o = e.run(&["sleep", "30s", &id]);
    assert_eq!(o.code, Some(1));
    assert!(o.has("STALL"));
    assert!(o.took < Duration::from_secs(5));
    e.run(&["stop", &id]);
    let o = e.run(&["sleep", "1s", &id]);
    assert_eq!(o.code, Some(0));
    assert!(o.took >= Duration::from_millis(900));
}

#[test]
fn sleep_without_id_wakes_on_any_job() {
    let e = Env::new();
    let sl = e
        .cmd(&["sleep", "30s"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let id = e.start(&["--interval", "200ms", "--stall", "1", "--", "sleep 6308"]);
    let o = finish(sl);
    assert_eq!(o.code, Some(1));
    assert!(o.has("STALL") && o.has(&format!("job={id}")), "{}", o.stdout);
    e.run(&["stop", &id]);
}

#[test]
fn guard_lost_wakes_wait() {
    let e = Env::new();
    let id = e.start(&["--", "sleep 6309"]);
    assert!(e.wait_state(&id, "running", Duration::from_secs(3)));
    let w = e
        .cmd(&["wait", &id, "--for", "10s"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let pid = e.status(&id)["watcher_pid"].as_i64().unwrap();
    signal(pid as u32, nix::libc::SIGKILL);
    let o = finish(w);
    assert_eq!(o.code, Some(1));
    assert!(o.has("LOST"), "{}", o.stdout);
    assert!(wait_gone("sleep 6309", Duration::from_secs(3)));
}
