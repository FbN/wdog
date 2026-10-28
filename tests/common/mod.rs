//! Shared helpers for the end-to-end tests.
#![allow(dead_code, clippy::disallowed_methods)]

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

pub const BIN: &str = env!("CARGO_BIN_EXE_wdog");

/// One test's WDOG_HOME. Dropping it kills every process recorded by its jobs (ProcGuard).
pub struct Env {
    pub home: tempfile::TempDir,
}

pub struct Out {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub took: Duration,
}

impl Out {
    pub fn has(&self, s: &str) -> bool {
        self.stdout.contains(s)
    }
    /// The last `[wdog]` line.
    pub fn last_line(&self) -> String {
        self.stdout
            .lines()
            .rev()
            .find(|l| l.starts_with("[wdog]"))
            .unwrap_or("")
            .to_string()
    }
}

impl Env {
    pub fn new() -> Env {
        let home = tempfile::Builder::new().prefix("wdog-e2e").tempdir().unwrap();
        Env { home }
    }

    pub fn path(&self) -> &Path {
        self.home.path()
    }

    pub fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args).env("WDOG_HOME", self.path()).stdin(Stdio::null());
        c
    }

    pub fn run(&self, args: &[&str]) -> Out {
        self.output(self.cmd(args))
    }

    pub fn output(&self, mut c: Command) -> Out {
        use std::os::unix::process::ExitStatusExt;
        let t0 = Instant::now();
        let o: Output = c.output().unwrap();
        Out {
            code: o.status.code(),
            signal: o.status.signal(),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            took: t0.elapsed(),
        }
    }

    /// `wdog start ...` -> job id.
    pub fn start(&self, args: &[&str]) -> String {
        let mut a = vec!["start"];
        a.extend_from_slice(args);
        let o = self.run(&a);
        assert_eq!(o.code, Some(0), "start failed: {} {}", o.stdout, o.stderr);
        id_of(&o.stdout)
    }

    pub fn job_dir(&self, id: &str) -> PathBuf {
        self.path().join(id)
    }

    pub fn status(&self, id: &str) -> serde_json::Value {
        let data = std::fs::read(self.job_dir(id).join("status.json")).unwrap();
        serde_json::from_slice(&data).unwrap()
    }

    pub fn events(&self, id: &str) -> String {
        std::fs::read_to_string(self.job_dir(id).join("events.jsonl")).unwrap_or_default()
    }

    pub fn log(&self, id: &str) -> String {
        std::fs::read_to_string(self.job_dir(id).join("out.log")).unwrap_or_default()
    }

    /// Every file of every job, concatenated (for "value never stored" checks).
    pub fn all_job_files(&self) -> String {
        let mut s = String::new();
        for d in std::fs::read_dir(self.path()).unwrap().flatten() {
            if let Ok(files) = std::fs::read_dir(d.path()) {
                for f in files.flatten() {
                    s.push_str(&String::from_utf8_lossy(
                        &std::fs::read(f.path()).unwrap_or_default(),
                    ));
                }
            }
        }
        s
    }

    /// The only job of this env (for `run` tests).
    pub fn only_job(&self) -> String {
        let ids: Vec<String> = std::fs::read_dir(self.path())
            .unwrap()
            .flatten()
            .map(|d| d.file_name().to_string_lossy().to_string())
            .filter(|n| !n.starts_with('.'))
            .collect();
        assert_eq!(ids.len(), 1, "jobs: {ids:?}");
        ids[0].clone()
    }

    pub fn wait_state(&self, id: &str, want: &str, limit: Duration) -> bool {
        let t0 = Instant::now();
        while t0.elapsed() < limit {
            if self.status(id)["state"] == want {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let Ok(rd) = std::fs::read_dir(self.path()) else {
            return;
        };
        for d in rd.flatten() {
            let Ok(data) = std::fs::read(d.path().join("status.json")) else {
                continue;
            };
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&data) else {
                continue;
            };
            let mut pids: Vec<(i64, u64)> = vec![];
            for p in v["pids"].as_array().into_iter().flatten() {
                pids.push((p[0].as_i64().unwrap_or(0), p[1].as_u64().unwrap_or(0)));
            }
            for g in v["pgids"].as_array().into_iter().flatten() {
                let (pg, leader, ticks) = (
                    g[0].as_i64().unwrap_or(0),
                    g[1].as_i64().unwrap_or(0),
                    g[2].as_u64().unwrap_or(0),
                );
                let ours = group_members(pg as i32)
                    .iter()
                    .any(|(p, t)| pids.contains(&(*p as i64, *t)) || (*p as i64 == leader && *t == ticks));
                if ours {
                    unsafe { nix::libc::killpg(pg as i32, nix::libc::SIGKILL) };
                }
            }
            for (p, t) in pids {
                if start_ticks(p as i32) == Some(t) {
                    unsafe { nix::libc::kill(p as i32, nix::libc::SIGKILL) };
                }
            }
        }
    }
}

pub fn id_of(stdout: &str) -> String {
    let l = stdout
        .lines()
        .find(|l| l.contains("id="))
        .unwrap_or_else(|| panic!("no id in {stdout:?}"));
    l.split("id=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

fn stat_fields(pid: i32) -> Option<Vec<String>> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &s[s.rfind(')')? + 2..];
    Some(rest.split(' ').map(|x| x.to_string()).collect())
}

pub fn start_ticks(pid: i32) -> Option<u64> {
    stat_fields(pid)?.get(19)?.parse().ok()
}

fn group_members(pg: i32) -> Vec<(i32, u64)> {
    let mut out = vec![];
    for d in std::fs::read_dir("/proc").unwrap().flatten() {
        let Some(pid) = d.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) else {
            continue;
        };
        if let Some(f) = stat_fields(pid) {
            if f.get(2).and_then(|x| x.parse::<i32>().ok()) == Some(pg) && f[0] != "Z" {
                out.push((pid, f[19].parse().unwrap_or(0)));
            }
        }
    }
    out
}

/// Live (non-zombie) processes whose full command line is exactly `cmdline`.
pub fn procs(cmdline: &str) -> Vec<i32> {
    let mut out = vec![];
    for d in std::fs::read_dir("/proc").unwrap().flatten() {
        let Some(pid) = d.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) else {
            continue;
        };
        let Ok(c) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
            continue;
        };
        let c = String::from_utf8_lossy(&c).replace('\0', " ").trim().to_string();
        if c == cmdline && stat_fields(pid).is_some_and(|f| f[0] != "Z") {
            out.push(pid);
        }
    }
    out
}

pub fn wait_gone(cmdline: &str, limit: Duration) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < limit {
        if procs(cmdline).is_empty() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    procs(cmdline).is_empty()
}

pub fn kill_all(cmdline: &str) {
    for p in procs(cmdline) {
        unsafe { nix::libc::kill(p, nix::libc::SIGKILL) };
    }
}

/// Spawn wdog in its own process group (so the test can signal it alone).
pub fn spawn_bg(env: &Env, args: &[&str]) -> std::process::Child {
    let mut c = env.cmd(args);
    c.stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
    c.spawn().unwrap()
}

pub fn signal(pid: u32, sig: i32) {
    unsafe { nix::libc::kill(pid as i32, sig) };
}

pub fn finish(child: std::process::Child) -> Out {
    use std::os::unix::process::ExitStatusExt;
    let t0 = Instant::now();
    let o = child.wait_with_output().unwrap();
    Out {
        code: o.status.code(),
        signal: o.status.signal(),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        took: t0.elapsed(),
    }
}

/// Wait until `f` is true, up to `limit`.
pub fn until(limit: Duration, mut f: impl FnMut() -> bool) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < limit {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    f()
}

pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

pub fn has(cmd: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {cmd}"))
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
