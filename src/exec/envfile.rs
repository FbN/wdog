//! `--env-file` (EX-9): source the file with /bin/sh once per run and pass the resulting
//! environment to every step. Values are never printed, logged or stored.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::Read;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const LOAD_LIMIT: Duration = Duration::from_secs(10);
/// Variables the shell sets on its own.
const SHELL_VARS: [&str; 4] = ["PWD", "OLDPWD", "SHLVL", "_"];
const SCRIPT: &str = r#"set -a; . "$1" >/dev/null 2>&1 || exit $?; set +a; exec "$2" __env0"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvChange {
    Set(OsString, OsString),
    Unset(OsString),
}

impl EnvChange {
    pub fn key(&self) -> &OsStr {
        match self {
            EnvChange::Set(k, _) | EnvChange::Unset(k) => k,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct EnvError {
    pub path: PathBuf,
    pub what: String,
}

impl fmt::Display for EnvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = self.path.display();
        write!(
            f,
            "env-file {p}: {} (check it with: sh -c 'set -a; . {p}')",
            self.what
        )
    }
}

impl std::error::Error for EnvError {}

/// `KEY=VALUE\0` pairs; values may contain `=` and newlines.
pub fn parse_env0(bytes: &[u8]) -> Vec<(OsString, OsString)> {
    bytes
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .filter_map(|kv| {
            let eq = kv.iter().position(|b| *b == b'=')?;
            if eq == 0 {
                return None;
            }
            Some((
                OsString::from_vec(kv[..eq].to_vec()),
                OsString::from_vec(kv[eq + 1..].to_vec()),
            ))
        })
        .collect()
}

pub fn diff_env(before: &[(OsString, OsString)], after: &[(OsString, OsString)]) -> Vec<EnvChange> {
    let shell = |k: &OsStr| SHELL_VARS.iter().any(|s| OsStr::new(s) == k);
    let b: BTreeMap<&OsString, &OsString> = before.iter().map(|(k, v)| (k, v)).collect();
    let a: BTreeMap<&OsString, &OsString> = after.iter().map(|(k, v)| (k, v)).collect();
    let mut out = vec![];
    for (k, v) in &a {
        if !shell(k) && b.get(k) != Some(v) {
            out.push(EnvChange::Set((*k).clone(), (*v).clone()));
        }
    }
    for k in b.keys() {
        if !shell(k) && !a.contains_key(k) {
            out.push(EnvChange::Unset((*k).clone()));
        }
    }
    out
}

/// Later changes win; `--env K=V` wins over every file.
pub fn merge(file_changes: Vec<EnvChange>, cli_env: &[(String, String)]) -> Vec<EnvChange> {
    let mut m: BTreeMap<OsString, Option<OsString>> = BTreeMap::new();
    for c in file_changes {
        match c {
            EnvChange::Set(k, v) => m.insert(k, Some(v)),
            EnvChange::Unset(k) => m.insert(k, None),
        };
    }
    for (k, v) in cli_env {
        m.insert(k.into(), Some(v.into()));
    }
    m.into_iter()
        .map(|(k, v)| match v {
            Some(v) => EnvChange::Set(k, v),
            None => EnvChange::Unset(k),
        })
        .collect()
}

pub fn apply(base: &[(OsString, OsString)], changes: &[EnvChange]) -> Vec<(OsString, OsString)> {
    let mut m: BTreeMap<OsString, OsString> = base.iter().cloned().collect();
    for c in changes {
        match c {
            EnvChange::Set(k, v) => m.insert(k.clone(), v.clone()),
            EnvChange::Unset(k) => m.remove(k),
        };
    }
    m.into_iter().collect()
}

pub fn key_names(changes: &[EnvChange]) -> Vec<String> {
    changes
        .iter()
        .map(|c| String::from_utf8_lossy(c.key().as_bytes()).into_owned())
        .collect()
}

/// Source each file in order (shell part). Runs before the reaper exists, so it may wait on
/// its own child. Returns the changes of each file.
pub fn load(
    files: &[PathBuf],
    cwd: Option<&Path>,
    exe: &Path,
) -> Result<Vec<(PathBuf, Vec<EnvChange>)>, EnvError> {
    let mut cur: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let mut out = vec![];
    for f in files {
        let err = |what: &str| EnvError {
            path: f.clone(),
            what: what.to_string(),
        };
        match std::fs::metadata(f) {
            Err(_) => return Err(err("not found")),
            Ok(m) if m.is_dir() => return Err(err("not readable")),
            Ok(_) => {}
        }
        if std::fs::File::open(f).is_err() {
            return Err(err("not readable"));
        }
        let mut c = Command::new("/bin/sh");
        c.arg("-c").arg(SCRIPT).arg("wdog-env").arg(f).arg(exe);
        c.env_clear().envs(cur.iter().map(|(k, v)| (k, v)));
        if let Some(d) = cwd {
            c.current_dir(d);
        }
        c.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = c.spawn().map_err(|e| err(&format!("cannot run /bin/sh: {e}")))?;
        let mut stdout = child.stdout.take().ok_or_else(|| err("no stdout"))?;
        let reader = std::thread::spawn(move || {
            let mut b = vec![];
            let _ = stdout.read_to_end(&mut b);
            b
        });
        let t0 = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(st)) => break st,
                Ok(None) if t0.elapsed() >= LOAD_LIMIT => {
                    let _ = child.kill();
                    // the reaper does not exist yet; this is the loader's own child
                    #[allow(clippy::disallowed_methods)]
                    let _ = child.wait();
                    return Err(err("timeout"));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => return Err(err(&format!("wait failed: {e}"))),
            }
        };
        let bytes = reader.join().unwrap_or_default();
        if !status.success() {
            use std::os::unix::process::ExitStatusExt;
            let code = status
                .code()
                .unwrap_or_else(|| 128 + status.signal().unwrap_or(0));
            return Err(err(&format!("exit={code}")));
        }
        let after = parse_env0(&bytes);
        let changes = diff_env(&cur, &after);
        cur = apply(&cur, &changes);
        out.push((f.clone(), changes));
    }
    Ok(out)
}

/// `wdog __env0`: write the environment as NUL-separated pairs.
pub fn dump_env0() -> Vec<u8> {
    let mut out = vec![];
    for (k, v) in std::env::vars_os() {
        out.extend_from_slice(k.as_bytes());
        out.push(b'=');
        out.extend_from_slice(v.as_bytes());
        out.push(0);
    }
    out
}

#[cfg(test)]
#[path = "envfile_tests.rs"]
mod tests;
