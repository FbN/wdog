//! Phase 5: `start`, `status`, `tail`, `stop`, `errors`, `gc` (spec BG-1..7, NF-4).
#![allow(clippy::disallowed_methods)]

mod common;
use common::*;
use std::time::Duration;

#[test]
fn start_status_done() {
    let e = Env::new();
    let o = e.run(&["start", "--", "sleep 1; echo done"]);
    assert_eq!(o.code, Some(0));
    assert!(o.took < Duration::from_secs(1), "start took {:?}", o.took);
    let id = id_of(&o.stdout);
    let st = e.run(&["status", &id]);
    assert!(st.has("running"), "{}", st.stdout);
    assert!(e.wait_state(&id, "done", Duration::from_secs(5)));
    assert!(e.run(&["status"]).has(&id));
    let j = e.run(&["status", "--json", &id]);
    let v: serde_json::Value = serde_json::from_str(j.stdout.trim()).unwrap();
    assert_eq!(v["state"], "done");
}

#[test]
fn job_outlives_the_caller() {
    let e = Env::new();
    let mut c = std::process::Command::new("sh");
    c.arg("-c")
        .arg(format!("'{BIN}' start -- 'sleep 1; echo survived'; exit 0"))
        .env("WDOG_HOME", e.path());
    let o = e.output(c);
    let id = id_of(&o.stdout);
    assert!(e.wait_state(&id, "done", Duration::from_secs(5)));
    assert!(e.log(&id).contains("survived"));
}

#[test]
fn stop_kills_three_processes() {
    let e = Env::new();
    let id = e.start(&["--", "sh -c 'sleep 6201 & sleep 6201 & sleep 6201'"]);
    assert!(until(Duration::from_secs(3), || procs("sleep 6201").len() == 3));
    let o = e.run(&["stop", &id]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(o.has(&format!("STOPPED {id} killed=")));
    assert!(o.took < Duration::from_secs(5));
    assert!(wait_gone("sleep 6201", Duration::from_secs(1)));
    let st = e.status(&id);
    assert_eq!(st["state"], "stopped");
    assert_eq!(st["signal"], "SIGTERM");
    assert_eq!(st["exit"], 143);
}

#[test]
fn stop_cleans_orphans_of_a_finished_run() {
    let e = Env::new();
    let o = e.run(&[
        "run",
        "--interval",
        "200ms",
        "--orphan-grace",
        "500ms",
        "--",
        "sh -c 'sleep 6202 &'",
    ]);
    assert!(o.has("ORPHANS"));
    assert_eq!(procs("sleep 6202").len(), 1);
    let id = e.only_job();
    let o = e.run(&["stop", &id, "--grace", "1s"]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(o.has("killed=1"), "{}", o.stdout);
    assert!(wait_gone("sleep 6202", Duration::from_secs(1)));
}

#[test]
fn plain_sigterm_to_supervisor_same_as_stop() {
    let e = Env::new();
    let id = e.start(&["--", "sleep 6203"]);
    assert!(e.wait_state(&id, "running", Duration::from_secs(3)));
    let pid = e.status(&id)["watcher_pid"].as_i64().unwrap();
    signal(pid as u32, nix::libc::SIGTERM);
    assert!(e.wait_state(&id, "stopped", Duration::from_secs(5)));
    assert_eq!(e.status(&id)["exit"], 143);
    assert!(wait_gone("sleep 6203", Duration::from_secs(1)));
}

#[test]
fn tail_and_follow() {
    let e = Env::new();
    let id = e.start(&["--", "seq 10"]);
    assert!(e.wait_state(&id, "done", Duration::from_secs(5)));
    let o = e.run(&["tail", &id, "-n", "3"]);
    assert_eq!(o.stdout, "8\n9\n10\n");
    let o = e.run(&["tail", &id, "-n", "1", "--raw"]);
    assert_eq!(o.stdout, "[1] 10\n");
    let id = e.start(&["--", "for i in 1 2 3; do echo line$i; sleep 0.4; done"]);
    let o = e.run(&["tail", &id, "-n", "0", "-f"]);
    assert_eq!(o.code, Some(0));
    assert!(o.has("line3"), "{}", o.stdout);
}

#[test]
fn errors_match_the_summary() {
    let e = Env::new();
    let o = e.run(&[
        "run",
        "--",
        "echo ok; echo 'panic: boom'; echo fine; echo Traceback",
    ]);
    assert!(
        o.has("ERRORS 2: log:2 \"panic: boom\" | log:4 \"Traceback\""),
        "{}",
        o.stdout
    );
    let o = e.run(&["errors", &e.only_job()]);
    assert_eq!(
        o.stdout,
        "log:2 \"panic: boom\"\nlog:4 \"Traceback\"\n[wdog] ERRORS 2\n"
    );
}

#[test]
fn gc_removes_old_dead_jobs_only() {
    let e = Env::new();
    e.run(&["run", "--", "true"]);
    let old = e.only_job();
    let live = e.start(&["--", "sleep 6204"]);
    assert!(e.wait_state(&live, "running", Duration::from_secs(3)));
    let past = std::time::SystemTime::now() - Duration::from_secs(25 * 3600);
    for id in [&old, &live] {
        let f = std::fs::File::options()
            .write(true)
            .open(e.job_dir(id).join("status.json"))
            .unwrap();
        f.set_modified(past).unwrap();
    }
    let o = e.run(&["gc"]);
    assert!(o.has("GC removed=1"), "{}", o.stdout);
    assert!(!e.job_dir(&old).exists());
    assert!(e.job_dir(&live).exists());
    e.run(&["stop", &live]);
}

#[test]
fn start_rejects_stdin_tty() {
    let e = Env::new();
    assert_eq!(e.run(&["start", "--stdin-tty", "--", "true"]).code, Some(125));
}

#[test]
fn stale_pgid_is_skipped() {
    let e = Env::new();
    // an unrelated process group that is not ours
    let mut other = std::process::Command::new("setsid");
    other.arg("sleep").arg("6205");
    let mut other = other.spawn().unwrap();
    assert!(until(Duration::from_secs(2), || !procs("sleep 6205").is_empty()));
    let pg = procs("sleep 6205")[0];
    // a finished job that recorded that pgid with another leader start time
    e.run(&["run", "--", "true"]);
    let id = e.only_job();
    let p = e.job_dir(&id).join("status.json");
    let mut st = e.status(&id);
    st["pgids"] = serde_json::json!([[pg, pg, 1]]);
    st["pids"] = serde_json::json!([]);
    std::fs::write(&p, st.to_string()).unwrap();
    let o = e.run(&["stop", &id, "--grace", "1s"]);
    assert!(o.has("skipped=1"), "{}", o.stdout);
    assert_eq!(procs("sleep 6205").len(), 1, "unrelated group left alone");
    other.kill().unwrap();
    other.wait().unwrap();
}

#[test]
fn parallel_stops_keep_the_grace() {
    let e = Env::new();
    let id = e.start(&["--", "trap '' TERM; sleep 6206"]);
    assert!(until(Duration::from_secs(3), || !procs("sleep 6206").is_empty()));
    let a = e.cmd(&["stop", &id, "--grace", "3s"]).spawn().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let t0 = std::time::Instant::now();
    let b = e.output(e.cmd(&["stop", &id, "--grace", "3s"]));
    let _ = finish(a);
    assert!(
        !procs("sleep 6206").is_empty() || t0.elapsed() >= Duration::from_millis(2500),
        "killed too early"
    );
    assert!(b.code.is_some());
    assert!(wait_gone("sleep 6206", Duration::from_secs(2)));
}

#[test]
fn stop_grace_short_and_long() {
    let e = Env::new();
    let id = e.start(&["--", "trap '' TERM; sleep 6207"]);
    assert!(until(Duration::from_secs(3), || !procs("sleep 6207").is_empty()));
    let o = e.run(&["stop", &id, "--grace", "1s"]);
    assert!(
        o.took >= Duration::from_millis(900) && o.took < Duration::from_secs(5),
        "{:?}",
        o.took
    );
    assert!(wait_gone("sleep 6207", Duration::from_secs(1)));
    assert!(e.wait_state(&id, "stopped", Duration::from_secs(3)));

    let id = e.start(&["--", "sleep 6208"]);
    assert!(until(Duration::from_secs(3), || !procs("sleep 6208").is_empty()));
    let o = e.run(&["stop", &id, "--grace", "10s"]);
    assert!(o.took < Duration::from_secs(3), "returned after {:?}", o.took);
}

#[test]
fn start_with_slow_env_file() {
    let e = Env::new();
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("slow.env");
    std::fs::write(&f, "sleep 3\nA=1\n").unwrap();
    let o = e.run(&["start", "--env-file", f.to_str().unwrap(), "--", "echo $A"]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    let id = id_of(&o.stdout);
    assert!(e.wait_state(&id, "done", Duration::from_secs(5)));
    assert!(e.log(&id).contains("[1] 1"));
}

#[test]
fn stop_right_after_start() {
    let e = Env::new();
    let id = e.start(&["--", "sleep 6209"]);
    let o = e.run(&["stop", &id]);
    assert!(o.code.is_some());
    assert!(wait_gone("sleep 6209", Duration::from_secs(3)));
    let st = e.status(&id)["state"].as_str().unwrap().to_string();
    assert!(st == "stopped" || st == "lost", "{st}");
}

#[test]
fn kill_9_supervisor_guard_cleans_up() {
    let e = Env::new();
    let id = e.start(&["--", "sleep 6210"]);
    assert!(e.wait_state(&id, "running", Duration::from_secs(3)));
    let pid = e.status(&id)["watcher_pid"].as_i64().unwrap();
    signal(pid as u32, nix::libc::SIGKILL);
    assert!(wait_gone("sleep 6210", Duration::from_secs(3)));
    assert!(until(Duration::from_secs(3), || e
        .run(&["status", &id])
        .has("lost")));
}

#[test]
fn unsafe_wdog_home_refused() {
    let d = tempfile::tempdir().unwrap();
    let real = d.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let link = d.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let mut c = std::process::Command::new(BIN);
    c.args(["run", "--", "true"]).env("WDOG_HOME", &link);
    let o = c.output().unwrap();
    assert_eq!(o.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&o.stdout).contains("symlink"));
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o777)).unwrap();
    let mut c = std::process::Command::new(BIN);
    c.args(["run", "--", "true"]).env("WDOG_HOME", &real);
    assert_eq!(c.output().unwrap().status.code(), Some(125));
}

/// `--json` for `errors`, `stop` and `gc`: one JSON line each (spec §7.2).
#[test]
fn json_for_errors_stop_gc() {
    let parse = |o: &Out| -> serde_json::Value {
        let lines: Vec<&str> = o.stdout.lines().collect();
        assert_eq!(lines.len(), 1, "{}", o.stdout);
        serde_json::from_str(lines[0]).unwrap()
    };
    let e = Env::new();
    e.run(&["run", "--", "echo 'panic: boom'; echo ok; echo Traceback"]);
    let id = e.only_job();
    let v = parse(&e.run(&["errors", "--json", &id]));
    assert_eq!(v["v"], 1);
    assert_eq!(v["event"], "ERRORS");
    assert_eq!(v["count"], 2);
    assert_eq!(
        v["items"],
        serde_json::json!([[1, "panic: boom"], [3, "Traceback"]])
    );

    let v = parse(&e.run(&["stop", "--json", &id]));
    assert_eq!(v["event"], "STOPPED");
    assert_eq!(v["already"], "done");

    let live = e.start(&["--", "sleep 6211"]);
    assert!(e.wait_state(&live, "running", Duration::from_secs(3)));
    let o = e.run(&["stop", "--json", &live]);
    assert_eq!(o.code, Some(0));
    let v = parse(&o);
    assert_eq!(v["id"], live.as_str());
    assert_eq!(v["survivors"], 0);
    assert!(v["killed"].as_u64().unwrap() >= 1, "{v}");

    let v = parse(&e.run(&["gc", "--json"]));
    assert_eq!(v["event"], "GC");
    assert_eq!(v["removed"], 0);
}

#[test]
fn status_steps() {
    let e = Env::new();
    let id = e.start(&["--", "echo a", "sleep 6511", "echo c"]);
    assert!(e.wait_state(&id, "running", Duration::from_secs(3)));
    let t0 = std::time::Instant::now();
    while e.status(&id)["step"] != 2 && t0.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let o = e.run(&["status", &id, "--steps"]);
    let lines: Vec<&str> = o.stdout.lines().collect();
    assert_eq!(lines.len(), 4, "{}", o.stdout);
    assert!(lines[1].contains("1/3 ok") && lines[1].contains("exit=0") && lines[1].ends_with("echo a"));
    assert!(lines[2].contains("2/3 running") && lines[2].ends_with("sleep 6511"));
    assert!(lines[3].contains("3/3 pending") && lines[3].ends_with("echo c"));

    let o = e.run(&["status", &id, "--steps", "--json"]);
    let v: serde_json::Value = serde_json::from_str(o.stdout.trim()).unwrap();
    let st: Vec<&str> = v["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["state"].as_str().unwrap())
        .collect();
    assert_eq!(st, ["ok", "running", "pending"]);
    assert_eq!(v["steps"][0]["cmd"], "echo a");

    e.run(&["stop", &id]);
    let o = e.run(&["status", &id, "--steps", "--json"]);
    let v: serde_json::Value = serde_json::from_str(o.stdout.trim()).unwrap();
    assert_eq!(v["steps"][1]["state"], "failed");
    assert_eq!(v["steps"][2]["state"], "not_run");
    // without --steps the JSON has no steps field
    let o = e.run(&["status", &id, "--json"]);
    assert!(!o.stdout.contains("\"steps\""), "{}", o.stdout);
}
