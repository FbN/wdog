//! `--steps-file` (spec EX-10): a script split at `# -STEP-` lines, from a file or stdin.
#![allow(clippy::disallowed_methods)]

mod common;
use common::*;
use std::io::Write;
use std::process::Stdio;
use std::time::Duration;

/// Run wdog with `script` on stdin.
fn piped(e: &Env, args: &[&str], script: &str) -> Out {
    let mut c = e.cmd(args);
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    child.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    finish(child)
}

const SCRIPT: &str = r#"GREETING="it's \"quoted\" \$HOME-free"
# -STEP- greet
echo "$GREETING"
read -r line || echo no-stdin
# -STEP-
false
echo after-false
# -STEP- never
echo never
"#;

#[test]
fn run_from_stdin() {
    let e = Env::new();
    let o = piped(&e, &["run", "--steps-file", "-"], SCRIPT);
    assert_eq!(o.code, Some(1), "{}", o.stdout);
    assert!(o.has("STEP 1/3 OK"), "{}", o.stdout);
    assert!(o.has("  greet"), "label is the name: {}", o.stdout);
    assert!(
        o.has("STEP 2/3 FAIL exit=1") && o.has("  false …"),
        "{}",
        o.stdout
    );
    assert!(!o.has("STEP 3/3"));
    let log = e.log(&e.only_job());
    assert!(
        log.contains(r#"it's "quoted" $HOME-free"#),
        "prelude in every step: {log}"
    );
    assert!(log.contains("no-stdin"), "steps get /dev/null: {log}");
    assert!(!log.contains("after-false"), "set -e: {log}");
    assert!(!log.contains("never"));
}

#[test]
fn run_from_file_and_errors() {
    let e = Env::new();
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("s.sh");
    std::fs::write(&f, "set +e\n# -STEP- tolerant\nfalse\necho kept\n").unwrap();
    let o = e.run(&["run", "--steps-file", f.to_str().unwrap()]);
    assert_eq!(o.code, Some(0), "set +e in the prelude: {}", o.stdout);
    assert!(e.log(&e.only_job()).contains("kept"));

    let o = piped(&e, &["run", "--steps-file", "-"], "# nothing\n");
    assert_eq!(o.code, Some(125));
    assert!(o.last_line().contains("no commands"), "{}", o.stdout);
    let o = piped(
        &e,
        &["run", "--steps-file", "-"],
        "# -STEP- a\ntrue\n# -STEP- b\n",
    );
    assert_eq!(o.code, Some(125));
    assert!(o.last_line().contains("step 2 is empty"), "{}", o.stdout);
    let o = e.run(&["run", "--steps-file", "-", "--", "true"]);
    assert_eq!(o.code, Some(125));
}

#[test]
fn start_from_stdin_shows_names() {
    let e = Env::new();
    let script = "# -STEP- quick\ntrue\n# -STEP- slow\nsleep 6521\n";
    let o = piped(&e, &["start", "--steps-file", "-"], script);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    let id = id_of(&o.stdout);
    assert!(until(Duration::from_secs(5), || e.status(&id)["step"] == 2));
    assert_eq!(e.status(&id)["step_cmd"], "slow");
    let o = e.run(&["status", &id, "--steps"]);
    let lines: Vec<&str> = o.stdout.lines().collect();
    assert!(
        lines[1].contains("ok") && lines[1].ends_with("  quick"),
        "{}",
        o.stdout
    );
    assert!(
        lines[2].contains("running") && lines[2].ends_with("  slow"),
        "{}",
        o.stdout
    );
    assert!(e.events(&id).contains(r#""cmd":"quick""#), "{}", e.events(&id));
    e.run(&["stop", &id]);
}
