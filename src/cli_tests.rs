use super::*;

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("wdog").chain(args.iter().copied()))
}

fn run_args(args: &[&str]) -> RunArgs {
    match parse(args).unwrap().cmd {
        Cmd::Run(a) | Cmd::Start(a) => a,
        _ => panic!("not run"),
    }
}

#[test]
fn spec_examples_parse() {
    for line in [
        "run -- cmd1 cmd2",
        "start --max 30m -- cmd1",
        "wait abc123 --for 2m --until RE --port 3000 --url http://x --file /tmp/x",
        "sleep 30s",
        "sleep 30s abc123",
        "status",
        "status abc123",
        "tail abc123 -n 50 -f",
        "errors abc123",
        "stop abc123 --grace 5s",
        "gc",
    ] {
        let v: Vec<&str> = line.split(' ').collect();
        assert!(parse(&v).is_ok(), "{line}");
    }
}

#[test]
fn missing_double_dash_and_conflicts() {
    assert!(parse(&["run", "cmd"]).is_err());
    assert!(parse(&["run", "--"]).is_err());
    assert!(parse(&["run", "--quiet", "--live", "--", "x"]).is_err());
}

#[test]
fn validation() {
    let ok = to_spec(&run_args(&["run", "--env", "A=1=2", "--", "x"]), false).unwrap();
    assert_eq!(ok.env, vec![("A".to_string(), "1=2".to_string())]);
    assert!(to_spec(&run_args(&["run", "--env", "A", "--", "x"]), false).is_err());
    assert!(to_spec(&run_args(&["run", "--env", "1A=2", "--", "x"]), false).is_err());
    assert!(to_spec(&run_args(&["run", "--err", "(", "--", "x"]), false).is_err());
    assert!(to_spec(&run_args(&["run", "--stall", "abc", "--", "x"]), false).is_err());
    assert!(to_spec(&run_args(&["run", "--cwd", "/no/such/dir", "--", "x"]), false).is_err());
    assert!(
        to_spec(
            &run_args(&["run", "--env-file", "/no/such/.env", "--", "x"]),
            false
        )
        .is_err()
    );
    assert!(to_spec(&run_args(&["run", "--interval", "10ms", "--", "x"]), false).is_err());
}

#[test]
fn quiet_resolution() {
    assert!(to_spec(&run_args(&["run", "--", "x"]), false).unwrap().quiet);
    assert!(!to_spec(&run_args(&["run", "--", "x"]), true).unwrap().quiet);
    assert!(
        !to_spec(&run_args(&["run", "--live", "--", "x"]), false)
            .unwrap()
            .quiet
    );
    assert!(
        to_spec(&run_args(&["run", "--quiet", "--", "x"]), true)
            .unwrap()
            .quiet
    );
}

#[test]
fn output_modes_and_defaults() {
    let s = to_spec(&run_args(&["run", "--", "x"]), false).unwrap();
    assert_eq!(s.output, OutputMode { head: 0, tail: 20 });
    assert_eq!(s.err_patterns.len(), 7);
    let s = to_spec(&run_args(&["run", "--head", "3", "--", "x"]), false).unwrap();
    assert_eq!(s.output, OutputMode { head: 3, tail: 0 });
    let s = to_spec(
        &run_args(&[
            "run",
            "--no-default-err",
            "--err",
            "x",
            "--interval",
            "200ms",
            "--",
            "x",
        ]),
        false,
    )
    .unwrap();
    assert_eq!(s.err_patterns, vec!["x".to_string()]);
    assert_eq!(s.interval, std::time::Duration::from_millis(200));
}

#[test]
fn env_file_resolved_against_cwd() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join(".env"), "A=1\n").unwrap();
    let cwd = d.path().to_str().unwrap();
    let s = to_spec(
        &run_args(&["run", "--cwd", cwd, "--env-file", ".env", "--", "x"]),
        false,
    )
    .unwrap();
    assert_eq!(s.env_files, vec![d.path().join(".env")]);
    assert!(s.env_files[0].is_absolute());
}
