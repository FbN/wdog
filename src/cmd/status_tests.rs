use super::*;

#[test]
fn line_format() {
    let mut st = Status::initial("3fa91c", std::path::Path::new("/x/out.log"), 3, "npm test");
    st.state = "running".into();
    st.started_at = 1000.0;
    st.last_activity_at = 1239.0;
    st.step = 2;
    st.last_lines = vec!["Running 214 tests".into()];
    let l = status_line(&st, 1242.0);
    assert_eq!(
        l,
        "3fa91c running   4m02s idle=3s    step=2/3 last=\"Running 214 tests\""
    );
    st.state = "failed".into();
    st.ended_at = Some(1100.0);
    st.exit = Some(1);
    let l = status_line(&st, 5000.0);
    assert!(l.contains("failed"));
    assert!(l.contains("1m40s"));
    assert!(l.contains("idle=-"));
    assert!(l.contains("exit=1"));
}

fn steps3() -> Vec<String> {
    vec!["echo a".into(), "sleep 30".into(), "echo c".into()]
}

fn step_ev(n: usize, exit: i32) -> Event {
    Event::Step {
        n,
        total: 3,
        ok: exit == 0,
        exit,
        dur: 1.5,
        cmd: String::new(),
    }
}

fn st_at(state: &str, step: usize) -> Status {
    let mut st = Status::initial("abc123", std::path::Path::new("/x/out.log"), 3, "");
    st.state = state.into();
    st.step = step;
    st
}

fn states(rows: &[StepRow]) -> Vec<StepState> {
    rows.iter().map(|r| r.state).collect()
}

#[test]
fn steps_while_running() {
    use StepState::*;
    let rows = step_rows(&steps3(), &[step_ev(1, 0)], &st_at("running", 2));
    assert_eq!(states(&rows), [Ok, Running, Pending]);
    assert_eq!(rows[0].exit, Some(0));
    assert_eq!(rows[0].dur, Some(1.5));
    assert_eq!(rows[1].attempts, 1);
    assert_eq!(rows[1].exit, None);
    assert_eq!(rows[2].attempts, 0);
    assert_eq!(rows[2].cmd, "echo c");
}

#[test]
fn steps_before_first_step() {
    let rows = step_rows(&steps3(), &[], &st_at("starting", 0));
    assert!(rows.iter().all(|r| r.state == StepState::Pending));
}

#[test]
fn steps_after_failure_and_lost() {
    use StepState::*;
    let rows = step_rows(&steps3(), &[step_ev(1, 0), step_ev(2, 143)], &st_at("stopped", 2));
    assert_eq!(states(&rows), [Ok, Failed, NotRun]);
    assert_eq!(rows[1].exit, Some(143));
    // watcher died during step 2: no STEP event for it
    let rows = step_rows(&steps3(), &[step_ev(1, 0)], &st_at("lost", 2));
    assert_eq!(states(&rows), [Ok, Interrupted, NotRun]);
    assert_eq!(rows[1].attempts, 1);
}

#[test]
fn steps_with_retry() {
    use StepState::*;
    let retry = |n| Event::Retry { n, max: 2, exit: 3 };
    let evs = [step_ev(1, 0), step_ev(2, 3), retry(1)];
    let rows = step_rows(&steps3(), &evs, &st_at("running", 2));
    assert_eq!(states(&rows), [Ok, Running, Pending]);
    assert_eq!((rows[1].attempts, rows[1].exit), (2, None));
    let evs = [
        step_ev(1, 0),
        step_ev(2, 3),
        retry(1),
        step_ev(2, 3),
        retry(2),
        step_ev(2, 3),
    ];
    let rows = step_rows(&steps3(), &evs, &st_at("failed", 2));
    assert_eq!(states(&rows), [Ok, Failed, NotRun]);
    assert_eq!((rows[1].attempts, rows[1].exit), (3, Some(3)));
}

#[test]
fn step_line_format() {
    let rows = step_rows(&steps3(), &[step_ev(1, 0)], &st_at("running", 2));
    assert_eq!(
        step_line(&rows[0], 3),
        "   1/3 ok          exit=0       1s  echo a"
    );
    assert_eq!(
        step_line(&rows[1], 3),
        "   2/3 running                      sleep 30"
    );
    let mut r = rows[1].clone();
    r.attempts = 2;
    assert!(step_line(&r, 3).contains(" attempts=2  sleep 30"));
}
