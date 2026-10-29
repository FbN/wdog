use super::*;

#[test]
fn golden_lines_from_spec() {
    let e = Event::Start {
        id: "a7f3".into(),
        log: "/run/user/1000/wdog/a7f3/out.log".into(),
    };
    assert_eq!(
        render_text(&e),
        "[wdog] id=a7f3 log=/run/user/1000/wdog/a7f3/out.log"
    );
    let e = Event::Step {
        n: 1,
        total: 3,
        ok: true,
        exit: 0,
        dur: 41.0,
        cmd: "npm ci".into(),
    };
    assert_eq!(render_text(&e), "[wdog] STEP 1/3 OK   41s  npm ci");
    let e = Event::Step {
        n: 3,
        total: 3,
        ok: false,
        exit: 1,
        dur: 112.0,
        cmd: "npm test".into(),
    };
    assert_eq!(render_text(&e), "[wdog] STEP 3/3 FAIL exit=1 1m52s  npm test");
    let e = Event::Stall {
        after: 120.0,
        cpu: 0.0,
        io: Some(0),
        pids: 3,
        unknown: 0,
        last: "Running 214 tests".into(),
    };
    assert_eq!(
        render_text(&e),
        "[wdog] STALL 2m00s cpu=0.0s io=0B pids=3 last=\"Running 214 tests\""
    );
    assert_eq!(
        render_text(&Event::Resumed { after: 151.0 }),
        "[wdog] RESUMED after 2m31s"
    );
    let e = Event::Errors {
        count: 2,
        items: vec![
            (842, "FAIL src/api.test.js".into()),
            (851, "Expected 200, received 500".into()),
        ],
    };
    assert_eq!(
        render_text(&e),
        "[wdog] ERRORS 2: log:842 \"FAIL src/api.test.js\" | log:851 \"Expected 200, received 500\""
    );
    let e = Event::Done {
        exit: 1,
        reason: DoneReason::StepFailed,
        dur: 241.0,
    };
    assert_eq!(render_text(&e), "[wdog] DONE exit=1 reason=step_failed 4m01s");
    let e = Event::Env {
        file: ".env.development".into(),
        loaded: 12,
        keys: vec!["A".into()],
    };
    assert_eq!(render_text(&e), "[wdog] ENV loaded=12 file=.env.development");
}

#[test]
fn other_variants() {
    let cases: Vec<(Event, &str)> = vec![
        (
            Event::Prompt {
                line: "Continue? [y/N]".into(),
                via: "text".into(),
            },
            "[wdog] PROMPT line=\"Continue? [y/N]\" via=text",
        ),
        (
            Event::Orphans {
                pids: vec![(12, "sleep 600".into()), (13, "x".into())],
            },
            "[wdog] ORPHANS pids=12,13 \"sleep 600\" | \"x\"",
        ),
        (
            Event::ErrorLive {
                log_line: 4,
                text: "FATAL boom".into(),
            },
            "[wdog] ERROR_LIVE log:4 \"FATAL boom\"",
        ),
        (
            Event::Busy {
                after: 30.0,
                cpu: 29.8,
            },
            "[wdog] BUSY 30s cpu=29.8s",
        ),
        (
            Event::Silence {
                after: 600.0,
                cpu: 3.2,
                io: Some(48 * 1024 * 1024),
                pids: 4,
                last: "Fetching layers".into(),
            },
            "[wdog] SILENCE 10m00s cpu=3.2s io=48MiB pids=4 last=\"Fetching layers\"",
        ),
        (Event::Timeout { after: 120.0 }, "[wdog] TIMEOUT after=2m00s"),
        (
            Event::Ready {
                what: "port=3000".into(),
            },
            "[wdog] READY port=3000",
        ),
        (
            Event::Stopped {
                signal: "SIGTERM".into(),
            },
            "[wdog] STOPPED sig=SIGTERM",
        ),
        (
            Event::Retry {
                n: 1,
                max: 2,
                exit: 1,
            },
            "[wdog] RETRY 1/2 exit=1",
        ),
        (Event::Lost { killed: 3 }, "[wdog] LOST killed=3"),
        (
            Event::LogFull {
                bytes: 64 * 1024 * 1024,
            },
            "[wdog] LOG_FULL at=64MiB",
        ),
        (Event::Error { msg: "bad".into() }, "[wdog] ERROR bad"),
        (Event::Skipped { lines: 7 }, "[wdog] SKIPPED 7 lines"),
        (
            Event::Tail {
                n: 2,
                lines: vec!["a".into(), "b".into()],
            },
            "[wdog] TAIL 2\na\nb",
        ),
    ];
    for (e, want) in cases {
        assert_eq!(render_text(&e), want);
    }
}

#[test]
fn stall_unknown_io() {
    let e = Event::Stall {
        after: 10.0,
        cpu: 0.0,
        io: None,
        pids: 2,
        unknown: 2,
        last: String::new(),
    };
    assert_eq!(
        render_text(&e),
        "[wdog] STALL 10s cpu=0.0s io=? pids=2 unknown=2 last=\"\""
    );
}

#[test]
fn quoting_and_truncation() {
    assert_eq!(quote("say \"hi\""), "\"say \\\"hi\\\"\"");
    let long = "x".repeat(300);
    let q = clip(&long);
    assert_eq!(q.chars().count(), 120);
    assert!(q.ends_with('…'));
}

#[test]
fn json_round_trip() {
    let e = Event::Done {
        exit: 0,
        reason: DoneReason::Ok,
        dur: 1.5,
    };
    let j = render_json(&e, SystemTime::now());
    assert!(j.starts_with("{\"v\":1,\"ts\":"));
    assert!(j.contains("\"event\":\"DONE\""));
    assert!(j.contains("\"reason\":\"ok\""));
    assert_eq!(parse_json(&j), Some(e));
    let e = Event::ErrorLive {
        log_line: 3,
        text: "x".into(),
    };
    let j = render_json(&e, SystemTime::now());
    assert!(j.contains("\"event\":\"ERROR_LIVE\""));
    assert_eq!(parse_json(&j), Some(e));
}

#[test]
fn newer_version_rejected() {
    assert_eq!(parse_json("{\"v\":2,\"event\":\"LOST\",\"killed\":1}"), None);
}

#[test]
fn alerts() {
    assert!(
        Event::Stall {
            after: 1.0,
            cpu: 0.0,
            io: None,
            pids: 0,
            unknown: 0,
            last: String::new()
        }
        .is_alert()
    );
    assert!(!Event::Resumed { after: 1.0 }.is_alert());
    assert!(!Event::Lost { killed: 0 }.is_alert());
}

#[test]
fn json_line_shape() {
    let ts = SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1500);
    let l = json_line("GC", serde_json::json!({ "removed": 2 }), ts);
    assert_eq!(l, r#"{"v":1,"ts":1.5,"event":"GC","removed":2}"#);
}
