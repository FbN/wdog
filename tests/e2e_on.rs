//! `wdog on HOST` (spec RM-1..RM-8), with a fake ssh that runs the remote side locally.
#![allow(clippy::disallowed_methods)]

mod common;
use common::*;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Drops `-o X`, `-t`/`-T` and `--`, then the host, and runs the command like sshd: `sh -c`.
const FAKE_SSH: &str = r#"#!/bin/sh
while [ $# -gt 0 ]; do case "$1" in -o) shift 2;; -t|-T) shift;; --) shift; break;; *) break;; esac; done
shift
exec sh -c "$*"
"#;

struct Remote {
    e: Env,
    ssh: PathBuf,
    /// `--dir`: the "remote" base directory.
    base: PathBuf,
}

impl Remote {
    fn new() -> Remote {
        let e = Env::new();
        let ssh = e.path().join("fake-ssh");
        std::fs::write(&ssh, FAKE_SSH).unwrap();
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let base = e.path().join("remote");
        std::fs::create_dir(&base).unwrap();
        Remote { e, ssh, base }
    }

    fn cmd(&self, args: &[&str]) -> std::process::Command {
        let mut a = vec!["on", "--dir", self.base.to_str().unwrap(), "db01"];
        a.extend_from_slice(args);
        let mut c = self.e.cmd(&a);
        c.env("WDOG_SSH", &self.ssh).env("WDOG_ON_ALLOW_DYNAMIC", "1");
        c
    }

    fn run(&self, args: &[&str]) -> Out {
        self.e.output(self.cmd(args))
    }

    fn piped(&self, args: &[&str], input: &str) -> Out {
        let mut c = self.cmd(args);
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = c.spawn().unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        finish(child)
    }

    /// `<base>/wdog-<uid>`, read from the ON line.
    fn home(&self, on_stdout: &str) -> PathBuf {
        let l = on_stdout
            .lines()
            .find(|l| l.starts_with("[wdog] ON "))
            .unwrap_or_else(|| panic!("no ON line: {on_stdout}"));
        let path = l.split("path=").nth(1).unwrap();
        Path::new(path).parent().unwrap().to_path_buf()
    }
}

#[test]
fn installs_once_then_runs_directly() {
    let r = Remote::new();
    let o = r.run(&["status"]);
    assert_eq!(o.code, Some(0), "{} {}", o.stdout, o.stderr);
    assert!(o.has("[wdog] ON host=db01 installed="), "{}", o.stdout);
    let home = r.home(&o.stdout);
    assert!(home.starts_with(&r.base));
    let mode = std::fs::metadata(&home).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    let bins: Vec<_> = std::fs::read_dir(home.join("bin")).unwrap().flatten().collect();
    assert_eq!(bins.len(), 1, "one agent binary");

    let o = r.run(&["status"]);
    assert_eq!(o.code, Some(0), "{} {}", o.stdout, o.stderr);
    assert!(!o.has("[wdog] ON "), "no second install: {}", o.stdout);
    assert!(!o.stderr.contains("WDOG_ON"), "markers are hidden: {}", o.stderr);
}

#[test]
fn passes_args_and_exit_code_through() {
    let r = Remote::new();
    let o = r.run(&["run", "--", r#"echo "a  b" 'it'\''s' $0x"#, "exit 3"]);
    assert_eq!(o.code, Some(3), "{} {}", o.stdout, o.stderr);
    assert!(o.has("a  b it's"), "{}", o.stdout);
    assert!(
        o.last_line().contains("DONE exit=3 reason=step_failed"),
        "{}",
        o.stdout
    );
}

#[test]
fn exit_197_from_the_command_is_not_rerun() {
    let r = Remote::new();
    assert_eq!(r.run(&["status"]).code, Some(0));
    let count = r.e.path().join("count");
    let step = format!("echo x >> {}; exit 197", count.display());
    let o = r.run(&["run", "--", &step]);
    assert_eq!(o.code, Some(197), "{} {}", o.stdout, o.stderr);
    assert_eq!(std::fs::read_to_string(&count).unwrap(), "x\n");
}

#[test]
fn steps_from_stdin_survive_the_install_retry() {
    let r = Remote::new();
    let script = "# -STEP- hello\necho hello-remote\n# -STEP- bye\necho bye\n";
    let o = r.piped(&["run", "--steps-file", "-"], script);
    assert_eq!(o.code, Some(0), "{} {}", o.stdout, o.stderr);
    assert!(o.has("[wdog] ON "), "installed on this call: {}", o.stdout);
    assert!(o.has("STEP 1/2 OK") && o.has("hello"), "{}", o.stdout);
    assert!(o.has("STEP 2/2 OK"), "{}", o.stdout);
}

#[test]
fn background_job_by_name_and_wrapper() {
    let r = Remote::new();
    let o = r.run(&["start", "--name", "mig", "--", "echo migrated"]);
    assert_eq!(o.code, Some(0), "{} {}", o.stdout, o.stderr);
    let home = r.home(&o.stdout);
    let o = r.run(&["wait", "mig", "--for", "10s"]);
    assert_eq!(o.code, Some(0), "{} {}", o.stdout, o.stderr);
    let o = r.run(&["tail", "mig"]);
    assert!(o.has("migrated"), "{}", o.stdout);
    // The wrapper works with plain ssh: no env vars needed.
    let mut c = std::process::Command::new(home.join("wdog"));
    c.args(["status", "mig"]).env_remove("WDOG_HOME");
    let o = r.e.output(c);
    assert_eq!(o.code, Some(0), "{} {}", o.stdout, o.stderr);
    assert!(o.has("name=mig"), "{}", o.stdout);
}

#[test]
fn json_on_line() {
    let r = Remote::new();
    let o = r.run(&["status", "--json"]);
    assert_eq!(o.code, Some(0), "{} {}", o.stdout, o.stderr);
    let first = o.stdout.lines().next().unwrap();
    let v: serde_json::Value = serde_json::from_str(first).unwrap();
    assert_eq!(v["event"], "ON");
    assert_eq!(v["host"], "db01");
}

#[test]
fn refuses_a_dynamic_binary() {
    let r = Remote::new();
    let mut c = r.cmd(&["status"]);
    c.env_remove("WDOG_ON_ALLOW_DYNAMIC");
    let o = r.e.output(c);
    assert_eq!(o.code, Some(125));
    assert!(o.last_line().contains("not a static binary"), "{}", o.stdout);
}

#[test]
fn refuses_an_unsafe_remote_dir() {
    let r = Remote::new();
    let elsewhere = r.e.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    // Every wdog-<uid> name is a symlink, whatever the uid is.
    let uid = std::fs::metadata(r.e.path()).map(|m| {
        use std::os::unix::fs::MetadataExt;
        m.uid()
    });
    std::os::unix::fs::symlink(&elsewhere, r.base.join(format!("wdog-{}", uid.unwrap()))).unwrap();
    let o = r.run(&["status"]);
    assert_eq!(o.code, Some(125), "{} {}", o.stdout, o.stderr);
    assert!(
        o.last_line().contains("symlink or owned by another user"),
        "{}",
        o.stdout
    );
}

#[test]
fn bad_host_and_relative_dir() {
    let e = Env::new();
    let o = e.run(&["on", "-oProxyCommand=x", "status"]);
    assert_eq!(o.code, Some(125));
    let o = e.run(&["on", "--dir", "rel", "db01", "status"]);
    assert_eq!(o.code, Some(125));
    assert!(o.last_line().contains("absolute"), "{}", o.stdout);
}
