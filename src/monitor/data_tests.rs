use super::*;

#[test]
fn clean_drops_escapes_and_controls() {
    assert_eq!(clean("\x1b[1;31merror\x1b[0m: x"), "error: x");
    assert_eq!(clean("\x1b]0;title\x07after"), "after");
    assert_eq!(clean("\x1b]8;;http://x\x1b\\link"), "link");
    assert_eq!(clean("a\tb"), "a       b");
    assert_eq!(clean("10%\r50%\r100%"), "100%");
    assert_eq!(clean("done\r"), "done");
    assert_eq!(clean("bell\x07 nul\x00 ok"), "bell nul ok");
    assert_eq!(clean("plain ünïcode"), "plain ünïcode");
}

#[test]
fn log_line_splits_prefix() {
    assert_eq!(
        log_line("[2!] \x1b[31mboom\x1b[0m"),
        LogLine {
            tag: "2".into(),
            err: true,
            text: "boom".into()
        }
    );
    assert_eq!(log_line("[1.1] retry").tag, "1.1");
    assert_eq!(
        log_line("no prefix"),
        LogLine {
            tag: String::new(),
            err: false,
            text: "no prefix".into()
        }
    );
}

#[test]
fn read_log_from_a_fixed_end() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("out.log");
    std::fs::write(&p, "[1] a\n[1] b\n[2!] c\n").unwrap();
    let all = read_log(&p, log_len(&p), 10);
    assert!(all.at_start);
    assert_eq!(all.lines.len(), 3);
    assert_eq!(all.end, 19);
    let frozen = read_log(&p, 12, 1);
    assert_eq!(frozen.lines[0].text, "b");
    assert!(!frozen.at_start);
    let missing = read_log(&d.path().join("nope"), 0, 5);
    assert!(missing.lines.is_empty());
}

#[test]
fn step_details_show_state_and_script() {
    let mut spec = RunSpec::new(vec!["set -e\ncd x\nnpm ci".into(), "npm test".into()]);
    spec.names = vec!["install".into(), String::new()];
    spec.cwd = Some("/work".into());
    let mut st = Status::initial("abc123", Path::new("/x/out.log"), 2, "");
    st.state = "running".into();
    st.step = 2;
    let ev = Event::Step {
        n: 1,
        total: 2,
        ok: true,
        exit: 0,
        dur: 3.0,
        cmd: "install".into(),
    };
    let d = step_details(&spec, &[ev], &st);
    let text: Vec<&str> = d.iter().map(|x| x.text.as_str()).collect();
    assert_eq!(text[0], "cwd: /work");
    assert_eq!(text[1], "");
    assert!(text[2].starts_with("1/2 ok"), "{}", text[2]);
    assert!(text[2].ends_with("install"));
    assert_eq!(text[3], "    cd x");
    assert_eq!(text[4], "    npm ci");
    assert!(text[5].starts_with("2/2 running"));
    assert_eq!(text[6], "    npm test");
    assert_eq!(d[2].kind, Kind::Step(StepState::Ok));
    assert_eq!(d[3].kind, Kind::Code);
}
