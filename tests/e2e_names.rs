//! `--name`: a job name usable in place of the id; a finished job's name can be reused.
#![allow(clippy::disallowed_methods)]

mod common;
use common::*;
use std::time::Duration;

#[test]
fn name_works_in_place_of_the_id() {
    let e = Env::new();
    let o = e.run(&["start", "--name", "test", "--", "echo hello-name"]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(o.has(" name=test "), "{}", o.stdout);
    let id = id_of(&o.stdout);
    let w = e.run(&["wait", "test", "--for", "5s"]);
    assert!(w.last_line().contains("DONE exit=0"), "{}", w.stdout);
    assert!(e.run(&["tail", "test"]).has("hello-name"));
    let st = e.run(&["status", "test"]);
    assert!(st.has(&id) && st.has("name=test"), "{}", st.stdout);
    assert_eq!(e.status(&id)["name"], "test");
    let j = e.run(&["status", "--json", "test"]);
    assert!(j.stdout.contains("\"name\":\"test\""), "{}", j.stdout);
    let o = e.run(&["status", "nope"]);
    assert_eq!(o.code, Some(125));
    assert!(o.last_line().contains("no job named 'nope'"), "{}", o.stdout);
}

#[test]
fn a_finished_job_is_replaced() {
    let e = Env::new();
    let o = e.run(&["run", "--name", "t", "--", "echo first"]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    let old = e.only_job();
    let o = e.run(&["start", "--name", "t", "--", "echo second"]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(o.has(&format!("name=t replaced={old} ")), "{}", o.stdout);
    let new = id_of(&o.stdout);
    assert!(!e.job_dir(&old).exists(), "old job still there");
    assert!(e.wait_state(&new, "done", Duration::from_secs(5)));
    assert!(e.run(&["tail", "t"]).has("second"));
}

#[test]
fn a_running_job_keeps_its_name() {
    let e = Env::new();
    let id = e.start(&["--name", "srv", "--", "sleep 6301"]);
    let o = e.run(&["start", "--name", "srv", "--", "true"]);
    assert_eq!(o.code, Some(125));
    assert!(
        o.last_line().contains(&format!("used by running job {id}")),
        "{}",
        o.stdout
    );
    let o = e.run(&["stop", "srv"]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    let o = e.run(&["start", "--name", "srv", "--", "true"]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(o.has(&format!("replaced={id}")));
}

#[test]
fn another_directory_cannot_take_the_name() {
    let e = Env::new();
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let mut c = e.cmd(&["run", "--name", "t", "--", "true"]);
    c.current_dir(a.path());
    assert_eq!(e.output(c).code, Some(0));
    let mut c = e.cmd(&["run", "--name", "t", "--", "true"]);
    c.current_dir(b.path());
    let o = e.output(c);
    assert_eq!(o.code, Some(125));
    assert!(o.last_line().contains("pick another name"), "{}", o.stdout);
    // --cwd counts as the directory
    let dir = a.path().to_str().unwrap();
    let mut c = e.cmd(&["run", "--name", "t", "--cwd", dir, "--", "true"]);
    c.current_dir(b.path());
    assert_eq!(e.output(c).code, Some(0));
}

#[test]
fn bad_names_are_refused() {
    let e = Env::new();
    for bad in ["cafe", "-x", "a b", "a/b"] {
        let o = e.run(&["run", &format!("--name={bad}"), "--", "true"]);
        assert_eq!(o.code, Some(125), "{bad}: {}", o.stdout);
        assert!(o.last_line().contains("--name"), "{}", o.stdout);
    }
}
