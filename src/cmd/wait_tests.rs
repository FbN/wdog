use super::*;

fn st(state: &str, exit: Option<i32>) -> Status {
    let mut s = Status::initial("abc123", std::path::Path::new("/x"), 1, "x");
    s.state = state.into();
    s.exit = exit;
    s.watcher_pid = 1;
    s
}

fn stall() -> Event {
    Event::Stall {
        after: 1.0,
        cpu: 0.0,
        io: None,
        pids: 1,
        unknown: 0,
        last: String::new(),
    }
}

#[test]
fn outcomes_and_exit_codes() {
    let spec = WaitSpec::default();
    let mut ws = WaitState::default();
    let ready = decide(&spec, &mut ws, &[WaitInput::CondMet("port=1".into())]).unwrap();
    assert_eq!(wait_exit(&ready, false), 0);
    let alert = decide(&spec, &mut ws, &[WaitInput::Events(vec![stall()])]).unwrap();
    assert_eq!(alert, WaitOutcome::Alert(stall()));
    assert_eq!(wait_exit(&alert, false), 1);
    let f = decide(&spec, &mut ws, &[WaitInput::ForElapsed(2.0)]).unwrap();
    assert_eq!(wait_exit(&f, false), 124);
    assert_eq!(wait_exit(&f, true), 0);
    let done0 = Event::Done {
        exit: 0,
        reason: DoneReason::Ok,
        dur: 1.0,
    };
    assert_eq!(wait_exit(&WaitOutcome::JobExited(done0), false), 0);
    let done3 = Event::Done {
        exit: 3,
        reason: DoneReason::StepFailed,
        dur: 1.0,
    };
    assert_eq!(wait_exit(&WaitOutcome::JobExited(done3), false), 1);
    assert!(
        decide(
            &spec,
            &mut ws,
            &[WaitInput::Events(vec![Event::Resumed { after: 1.0 }])]
        )
        .is_none()
    );
}

#[test]
fn priority() {
    let spec = WaitSpec::default();
    let mut ws = WaitState::default();
    let all = vec![
        WaitInput::ForElapsed(1.0),
        WaitInput::Events(vec![
            stall(),
            Event::Done {
                exit: 1,
                reason: DoneReason::StepFailed,
                dur: 1.0,
            },
        ]),
        WaitInput::CondMet("port=1".into()),
    ];
    assert_eq!(
        decide(&spec, &mut ws, &all),
        Some(WaitOutcome::Ready("port=1".into()))
    );
    let o = decide(&spec, &mut ws, &all[..2]).unwrap();
    assert!(matches!(o, WaitOutcome::JobExited(_)));
    let o = decide(
        &spec,
        &mut ws,
        &[WaitInput::ForElapsed(1.0), WaitInput::Events(vec![stall()])],
    )
    .unwrap();
    assert!(matches!(o, WaitOutcome::Alert(_)));
}

#[test]
fn until_matches_raw_text() {
    let spec = WaitSpec {
        until: Some(Regex::new("^listening").unwrap()),
        sleep: false,
    };
    let mut ws = WaitState::default();
    let o = decide(
        &spec,
        &mut ws,
        &[WaitInput::LogLines(vec!["[1] listening on 3000".into()])],
    );
    assert!(matches!(o, Some(WaitOutcome::Ready(_))));
}

#[test]
fn finished_job_and_stopped_and_lost() {
    let spec = WaitSpec::default();
    let mut ws = WaitState::default();
    let o = decide(
        &spec,
        &mut ws,
        &[WaitInput::Status(Box::new(st("failed", Some(3))), false)],
    )
    .unwrap();
    assert_eq!(
        o,
        WaitOutcome::JobExited(Event::Done {
            exit: 3,
            reason: DoneReason::StepFailed,
            dur: 0.0
        })
    );
    let stop = Event::Stopped {
        signal: "SIGTERM".into(),
    };
    let evs = vec![
        stop.clone(),
        Event::Done {
            exit: 143,
            reason: DoneReason::Stopped,
            dur: 1.0,
        },
    ];
    assert_eq!(
        decide(&spec, &mut ws, &[WaitInput::Events(evs)]),
        Some(WaitOutcome::Stopped(stop))
    );
    let o = decide(
        &spec,
        &mut ws,
        &[WaitInput::Status(Box::new(st("running", None)), false)],
    )
    .unwrap();
    assert!(matches!(o, WaitOutcome::Lost(_)));
    assert!(
        decide(
            &spec,
            &mut ws,
            &[WaitInput::Status(Box::new(st("running", None)), true)]
        )
        .is_none()
    );
}

#[test]
fn sleep_ignores_job_exit() {
    let spec = WaitSpec {
        until: None,
        sleep: true,
    };
    let mut ws = WaitState::default();
    assert!(
        decide(
            &spec,
            &mut ws,
            &[WaitInput::Status(Box::new(st("done", Some(0))), false)]
        )
        .is_none()
    );
    assert!(matches!(
        decide(&spec, &mut ws, &[WaitInput::Events(vec![stall()])]),
        Some(WaitOutcome::Alert(_))
    ));
}

#[test]
fn url_schemes() {
    assert!(check_url("http://127.0.0.1:3000/health").is_ok());
    assert!(check_url("ftp://x/").is_err());
    assert!(check_url("localhost:3000").is_err());
    assert_eq!(check_url("https://localhost/").is_ok(), cfg!(feature = "https"));
}
