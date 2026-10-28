use super::*;
use crate::core::{JobInfo, Mode, State};
use crate::detect::errors::ErrorMatcher;
use crate::spec::RunSpec;
use std::rc::Rc;
use std::time::Duration;

fn st(t0: Instant) -> State {
    let spec = RunSpec::new(vec!["true".into(), "false".into()]);
    let m = ErrorMatcher::new(&spec.err_patterns).unwrap();
    let job = JobInfo {
        id: "abc123".into(),
        log: "/x/out.log".into(),
        watcher_pid: 42,
        watcher_start_ticks: 7,
        started_wall: 1000.0,
        clk_tck: 100.0,
    };
    State::new(Rc::new(spec), Rc::new(m), Rc::new(job), Mode::Foreground, t0)
}

#[test]
fn from_state_running_and_final() {
    let t0 = Instant::now();
    let mut s = st(t0);
    assert_eq!(Status::from_state(&s, t0).state, "starting");
    s.pgids.push((10, 10, 5));
    s.main = Some(crate::core::MainProc {
        pid: 10,
        start_ticks: 5,
    });
    s.out.last_output_at = t0 + Duration::from_secs(3);
    let x = Status::from_state(&s, t0 + Duration::from_secs(4));
    assert_eq!(x.state, "running");
    assert_eq!(x.v, 1);
    assert_eq!(x.watcher_pid, 42);
    assert_eq!(x.main_pid, Some(10));
    assert_eq!(x.last_output_at, 1003.0);
    assert_eq!(x.step, 1);
    assert_eq!(x.exit, None);
    s.results = vec![0, 1];
    s.ending = Some(EndReason::Completed);
    s.finished = true;
    let x = Status::from_state(&s, t0 + Duration::from_secs(5));
    assert_eq!(x.state, "failed");
    assert_eq!(x.exit, Some(1));
    assert_eq!(x.ended_at, Some(1005.0));
    s.ending = Some(EndReason::Stopped(nix::sys::signal::Signal::SIGINT));
    s.signal = Some(nix::sys::signal::Signal::SIGINT);
    let x = Status::from_state(&s, t0);
    assert_eq!(
        (x.state.as_str(), x.exit, x.signal.as_deref()),
        ("stopped", Some(130), Some("SIGINT"))
    );
}

#[test]
fn write_read_round_trip_and_version() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("status.json");
    let s = Status::initial("abc123", std::path::Path::new("/x/out.log"), 2, "true");
    write_atomic(&p, &s).unwrap();
    assert_eq!(read(&p).unwrap(), s);
    std::fs::write(&p, r#"{"v":2}"#).unwrap();
    assert!(read(&p).unwrap_err().to_string().contains("newer"));
}

#[test]
fn final_states() {
    for s in ["done", "failed", "timeout", "killed", "stopped"] {
        assert!(is_final(s));
    }
    assert!(!is_final("running") && !is_final("lost") && !is_final("starting"));
}
