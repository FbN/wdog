//! Phase 3: `wdog run` (spec EX-1..7, EX-9, LG-1..5, §5.3, NF-4).
#![allow(clippy::disallowed_methods)]

mod common;
use common::*;
use std::time::Duration;

#[test]
fn sequence_stops_at_first_failure() {
    let e = Env::new();
    let o = e.run(&["run", "--", "true", "false", "echo never"]);
    assert_eq!(o.code, Some(1), "{}", o.stdout);
    assert!(o.has("STEP 1/3 OK"));
    assert!(o.has("STEP 2/3 FAIL exit=1"));
    assert!(!o.has("STEP 3/3"));
    assert!(!e.log(&e.only_job()).contains("never"));
    assert!(o.last_line().starts_with("[wdog] DONE exit=1 reason=step_failed"));
}

#[test]
fn keep_going_runs_all() {
    let e = Env::new();
    let o = e.run(&["run", "--keep-going", "--", "true", "false", "echo three"]);
    assert_eq!(o.code, Some(1));
    assert!(o.has("STEP 3/3 OK"));
    assert!(o.has("three"));
}

#[test]
fn cwd_and_env() {
    let e = Env::new();
    let d = tempfile::tempdir().unwrap();
    let o = e.run(&[
        "run",
        "--cwd",
        d.path().to_str().unwrap(),
        "--env",
        "K=hello",
        "--",
        "pwd",
        "echo $K",
    ]);
    assert_eq!(o.code, Some(0));
    assert!(
        o.has(&d.path().canonicalize().unwrap().display().to_string())
            || o.has(&d.path().display().to_string())
    );
    assert!(o.has("hello"));
}

#[test]
fn stdin_closed_by_default() {
    let e = Env::new();
    let o = e.run(&["run", "--", "cat"]);
    assert_eq!(o.code, Some(0));
    assert!(o.took < Duration::from_secs(3));
}

#[test]
fn max_stops_the_tree() {
    let e = Env::new();
    let o = e.run(&["run", "--max", "1s", "--", "sleep 6101"]);
    assert_eq!(o.code, Some(124));
    assert!(o.took < Duration::from_secs(7));
    assert!(o.has("TIMEOUT"));
    assert!(o.last_line().contains("reason=max"));
    assert!(wait_gone("sleep 6101", Duration::from_secs(2)));
}

#[test]
fn command_not_found_and_bad_args() {
    let e = Env::new();
    assert_eq!(e.run(&["run", "--", "nosuchcmd_wdog"]).code, Some(127));
    let o = e.run(&["run", "--stall", "abc", "--", "true"]);
    assert_eq!(o.code, Some(125));
    assert!(o.last_line().starts_with("[wdog] ERROR"));
    assert_eq!(e.run(&["run", "--quiet", "--live", "--", "true"]).code, Some(125));
}

#[test]
fn log_prefixes() {
    let e = Env::new();
    e.run(&["run", "--", "echo out; echo err >&2", "echo two"]);
    let log = e.log(&e.only_job());
    assert!(log.contains("[1] out\n"));
    assert!(log.contains("[1!] err\n"));
    assert!(log.contains("[2] two\n"));
}

#[test]
fn tail_head_and_quiet() {
    let e = Env::new();
    let o = e.run(&["run", "--tail", "2", "--", "seq 10"]);
    assert!(o.has("TAIL 2\n9\n10\n"));
    assert!(!o.has("\n8\n"));
    let o = e.run(&["run", "--head", "2", "--", "seq 10"]);
    assert!(o.has("HEAD 2\n1\n2\n"));
    assert!(!o.has("\n3\n"));
    // stdout is a pipe: quiet by default, live lines only with --live
    let o = e.run(&["run", "--tail", "1", "--", "echo first; echo last"]);
    assert!(!o.has("\nfirst\n"), "{}", o.stdout);
    let o = e.run(&["run", "--live", "--tail", "1", "--", "echo first; echo last"]);
    assert!(o.has("\nfirst\n"), "{}", o.stdout);
}

#[test]
fn live_by_default_on_a_terminal() {
    let e = Env::new();
    let pty = nix::pty::openpty(None, None).unwrap();
    let mut c = e.cmd(&["run", "--tail", "1", "--", "echo first; echo last"]);
    c.stdout(std::process::Stdio::from(std::fs::File::from(pty.slave)));
    let mut child = c.spawn().unwrap();
    let status = child.wait().unwrap();
    assert!(status.success());
    let mut buf = vec![0u8; 65536];
    let n = nix::unistd::read(&pty.master, &mut buf).unwrap_or(0);
    let s = String::from_utf8_lossy(&buf[..n]);
    assert!(s.contains("first"), "{s}");
}

#[test]
fn exit_126_step_vs_kill_on_stall() {
    let e = Env::new();
    let o = e.run(&["run", "--", "exit 126"]);
    assert_eq!(o.code, Some(126));
    assert!(o.last_line().contains("reason=step_failed"));
    let o = e.run(&[
        "run",
        "--interval",
        "200ms",
        "--stall",
        "1",
        "--kill-on-stall",
        "--",
        "sleep 6102",
    ]);
    assert_eq!(o.code, Some(126), "{}", o.stdout);
    assert!(o.last_line().contains("reason=kill_on_stall"));
    assert!(wait_gone("sleep 6102", Duration::from_secs(2)));
}

#[test]
fn status_file_after_run() {
    let e = Env::new();
    e.run(&["run", "--", "false"]);
    let s = e.status(&e.only_job());
    assert_eq!(s["state"], "failed");
    assert_eq!(s["exit"], 1);
    assert_eq!(s["v"], 1);
}

#[test]
fn sigterm_forwarded_and_reraised() {
    let e = Env::new();
    let child = spawn_bg(&e, &["run", "--", "sleep 6103"]);
    assert!(until(Duration::from_secs(3), || !procs("sleep 6103").is_empty()));
    signal(child.id(), nix::libc::SIGTERM);
    let o = finish(child);
    assert_eq!(o.signal, Some(15), "{}", o.stdout);
    assert!(o.has("STOPPED sig=SIGTERM"));
    let s = e.status(&e.only_job());
    assert_eq!(s["exit"], 143);
    assert_eq!(s["signal"], "SIGTERM");
    assert_eq!(s["state"], "stopped");
    assert!(wait_gone("sleep 6103", Duration::from_secs(2)));
}

#[test]
fn sigint_reaches_the_job_as_sigint() {
    let e = Env::new();
    let child = spawn_bg(
        &e,
        &["run", "--", "trap 'echo got INT; exit 3' INT; sleep 6104 & wait"],
    );
    assert!(until(Duration::from_secs(3), || !procs("sleep 6104").is_empty()));
    signal(child.id(), nix::libc::SIGINT);
    let o = finish(child);
    assert_eq!(o.signal, Some(2));
    assert!(e.log(&e.only_job()).contains("got INT"));
    assert_eq!(e.status(&e.only_job())["exit"], 130);
    kill_all("sleep 6104");
}

#[test]
fn sigint_stops_a_calling_script() {
    let e = Env::new();
    let script = format!("'{BIN}' run -- 'sleep 6105'; echo after");
    let mut c = std::process::Command::new("bash");
    c.arg("-c")
        .arg(&script)
        .env("WDOG_HOME", e.path())
        .stdout(std::process::Stdio::piped());
    std::os::unix::process::CommandExt::process_group(&mut c, 0);
    let child = c.spawn().unwrap();
    assert!(until(Duration::from_secs(3), || !procs("sleep 6105").is_empty()));
    unsafe { nix::libc::killpg(child.id() as i32, nix::libc::SIGINT) };
    let o = finish(child);
    assert!(
        !o.stdout.contains("\nafter") && !o.stdout.starts_with("after"),
        "{}",
        o.stdout
    );
}

#[test]
fn child_ignoring_sigterm_gets_killed_after_grace() {
    let e = Env::new();
    let child = spawn_bg(&e, &["run", "--", "trap '' TERM; sleep 6106"]);
    assert!(until(Duration::from_secs(3), || !procs("sleep 6106").is_empty()));
    // the sleep ignores TERM too (inherited)
    signal(child.id(), nix::libc::SIGTERM);
    let o = finish(child);
    assert_eq!(o.signal, Some(15));
    assert!(o.took >= Duration::from_secs(4), "took {:?}", o.took);
    assert!(wait_gone("sleep 6106", Duration::from_secs(2)));
}

#[test]
fn double_sigint_kills_at_once() {
    let e = Env::new();
    let child = spawn_bg(&e, &["run", "--", "trap '' INT; sleep 6107"]);
    assert!(until(Duration::from_secs(3), || !procs("sleep 6107").is_empty()));
    signal(child.id(), nix::libc::SIGINT);
    std::thread::sleep(Duration::from_millis(100));
    signal(child.id(), nix::libc::SIGINT);
    let o = finish(child);
    assert_eq!(o.signal, Some(2));
    assert!(o.took < Duration::from_secs(3), "took {:?}", o.took);
    assert!(wait_gone("sleep 6107", Duration::from_secs(2)));
}

#[test]
fn step_killed_from_outside() {
    let e = Env::new();
    let child = spawn_bg(&e, &["run", "--", "sleep 6108"]);
    assert!(until(Duration::from_secs(3), || !procs("sleep 6108").is_empty()));
    kill_all("sleep 6108");
    let o = finish(child);
    assert_eq!(o.code, Some(137));
    assert!(o.has("STEP 1/1 FAIL exit=137"));
}

#[test]
fn output_order_kept() {
    let e = Env::new();
    e.run(&["run", "--", "seq 2000"]);
    let log = e.log(&e.only_job());
    let nums: Vec<u32> = log
        .lines()
        .filter_map(|l| l.strip_prefix("[1] "))
        .map(|n| n.parse().unwrap())
        .collect();
    assert_eq!(nums, (1..=2000).collect::<Vec<_>>());
}

#[test]
fn env_file_values_and_precedence() {
    let e = Env::new();
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join(".env.test"),
        "A=1\nB=\"x y\"\nC=$A-2\nS=topsecret\nunset HOME_X\n",
    )
    .unwrap();
    std::fs::write(d.path().join(".env2"), "A=5\n").unwrap();
    let cwd = d.path().to_str().unwrap();
    let mut c = e.cmd(&[
        "run",
        "--cwd",
        cwd,
        "--env-file",
        ".env.test",
        "--",
        "echo \"A=$A B=$B C=$C\"",
        "echo \"HOME_X=${HOME_X-unset}\"",
        "test \"${#S}\" = 9",
    ]);
    c.env("HOME_X", "1");
    let o = e.output(c);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(o.has("ENV loaded=5"));
    assert!(o.has("A=1 B=x y C=1-2"));
    assert!(o.has("HOME_X=unset"));
    let o = e.run(&[
        "run",
        "--cwd",
        cwd,
        "--env-file",
        ".env.test",
        "--env-file",
        ".env2",
        "--env",
        "B=cli",
        "--",
        "echo \"A=$A B=$B\"",
    ]);
    assert!(o.has("A=5 B=cli"), "{}", o.stdout);
    assert!(!o.stdout.contains("topsecret") && !o.stderr.contains("topsecret"));
    assert!(!e.all_job_files().contains("topsecret"));
}

#[test]
fn env_file_errors_leak_nothing() {
    let e = Env::new();
    let d = tempfile::tempdir().unwrap();
    let bad = d.path().join("bad.env");
    std::fs::write(&bad, "X=1\ntopsecret here(\n").unwrap();
    let o = e.run(&["run", "--env-file", bad.to_str().unwrap(), "--", "echo never"]);
    assert_eq!(o.code, Some(125));
    assert!(o.has("ERROR env-file"));
    assert!(o.has("exit=2"));
    assert!(!o.stdout.contains("topsecret") && !o.stderr.contains("topsecret"));
    assert!(!e.all_job_files().contains("topsecret"));
    assert!(!e.log(&e.only_job()).contains("never"));
    let o = e.run(&["run", "--env-file", "/no/such/file.env", "--", "true"]);
    assert_eq!(o.code, Some(125));
}

#[test]
fn caller_environment_unchanged() {
    let e = Env::new();
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join(".env");
    std::fs::write(&f, "FOO=bar\n").unwrap();
    let script = format!(
        "'{BIN}' run --env-file '{}' -- 'test \"$FOO\" = bar' >/dev/null; echo \"rc=$? FOO=${{FOO-unset}}\"",
        f.display()
    );
    let mut c = std::process::Command::new("sh");
    c.arg("-c").arg(script).env("WDOG_HOME", e.path());
    let o = e.output(c);
    assert_eq!(o.stdout.trim(), "rc=0 FOO=unset");
}

#[test]
fn last_line_never_lost() {
    let e = Env::new();
    for _ in 0..50 {
        let o = e.run(&["run", "--", "printf 'a\\nLAST ERROR: boom FATAL'; exit 1"]);
        assert!(o.has("LAST ERROR: boom"), "{}", o.stdout);
        assert!(o.has("ERRORS 1"), "{}", o.stdout);
    }
}

#[test]
fn sigkill_of_wdog_guard_cleans_up() {
    let e = Env::new();
    let child = spawn_bg(
        &e,
        &[
            "run",
            "--interval",
            "200ms",
            "--",
            "sh -c 'sleep 6109 & sleep 6109'",
        ],
    );
    assert!(until(Duration::from_secs(3), || procs("sleep 6109").len() == 2));
    std::thread::sleep(Duration::from_millis(500));
    signal(child.id(), nix::libc::SIGKILL);
    let _ = finish(child);
    assert!(wait_gone("sleep 6109", Duration::from_secs(3)), "sleeps survive");
    let id = e.only_job();
    assert!(until(Duration::from_secs(3), || e.status(&id)["state"] == "lost"));
    assert_eq!(e.status(&id)["reason"], "watcher_died");
    assert!(e.events(&id).contains("\"event\":\"LOST\""));
}

#[test]
fn sigkill_of_whole_group_guard_survives() {
    let e = Env::new();
    let child = spawn_bg(&e, &["run", "--interval", "200ms", "--", "sleep 6110"]);
    assert!(until(Duration::from_secs(3), || !procs("sleep 6110").is_empty()));
    unsafe { nix::libc::killpg(child.id() as i32, nix::libc::SIGKILL) };
    let _ = finish(child);
    assert!(wait_gone("sleep 6110", Duration::from_secs(3)));
    let id = e.only_job();
    assert!(until(Duration::from_secs(3), || e.status(&id)["state"] == "lost"));
}

#[test]
fn normal_run_leaves_orphan_alive() {
    let e = Env::new();
    let o = e.run(&["run", "--interval", "200ms", "--", "sh -c 'sleep 6111 &'"]);
    assert_eq!(o.code, Some(0));
    assert!(o.has("ORPHANS"));
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(procs("sleep 6111").len(), 1, "orphan not killed (AN-8)");
    let st = e.status(&e.only_job());
    assert_eq!(st["state"], "done");
    kill_all("sleep 6111");
}

#[test]
fn closed_stdout_does_not_break_the_run() {
    let e = Env::new();
    let script = format!("'{BIN}' run --live -- 'seq 200000' | head -1");
    let mut c = std::process::Command::new("sh");
    c.arg("-c").arg(script).env("WDOG_HOME", e.path());
    let o = e.output(c);
    assert_eq!(
        o.stdout.trim(),
        "[wdog] id=".to_string()
            + &e.only_job()
            + " log="
            + &e.job_dir(&e.only_job()).join("out.log").display().to_string()
    );
    assert!(until(Duration::from_secs(5), || e.status(&e.only_job())["state"]
        == "done"));
    assert!(e.log(&e.only_job()).contains("[1] 200000"));
}

#[test]
fn orphan_output_logged_during_next_step() {
    let e = Env::new();
    let o = e.run(&[
        "run",
        "--interval",
        "200ms",
        "--orphan-grace",
        "300ms",
        "--",
        "sh -c 'for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do echo tick$i; sleep 0.1; done &'",
        "sleep 1.5",
    ]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    let log = e.log(&e.only_job());
    assert!(log.contains("[1] tick10"), "{log}");
}

#[test]
fn log_cap() {
    let e = Env::new();
    let mut c = e.cmd(&["run", "--tail", "1", "--", "seq 1000000"]);
    c.env("WDOG_LOG_MAX", "100000");
    let o = e.output(c);
    assert_eq!(o.code, Some(0));
    let id = e.only_job();
    let len = std::fs::metadata(e.job_dir(&id).join("out.log")).unwrap().len();
    assert!(len <= 100_000, "log is {len} bytes");
    assert_eq!(e.events(&id).matches("LOG_FULL").count(), 1);
    assert_eq!(e.status(&id)["log_truncated"], true);
    assert!(o.has("TAIL 1\n1000000\n"));
}

#[test]
fn stdin_tty_needs_terminal_foreground() {
    let e = Env::new();
    let o = e.run(&["run", "--stdin-tty", "--", "true"]);
    assert_eq!(o.code, Some(125));
    assert!(o.has("terminal foreground"));
}

#[test]
fn json_events() {
    let e = Env::new();
    let o = e.run(&["run", "--json", "--", "echo hi"]);
    let lines: Vec<&str> = o.stdout.lines().filter(|l| l.starts_with('{')).collect();
    assert!(lines.len() >= 3);
    for l in &lines {
        let v: serde_json::Value = serde_json::from_str(l).unwrap();
        assert_eq!(v["v"], 1);
    }
    assert!(lines.last().unwrap().contains("\"event\":\"DONE\""));
}

#[test]
fn retry_flaky_step() {
    let e = Env::new();
    let d = tempfile::tempdir().unwrap();
    let counter = d.path().join("n");
    let step = format!(
        "n=$(cat '{0}' 2>/dev/null || echo 0); n=$((n+1)); echo $n > '{0}'; [ $n -ge 3 ]",
        counter.display()
    );
    let o = e.run(&["run", "--retry", "2", "--", &step]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert_eq!(o.stdout.matches("RETRY").count(), 2);
}
