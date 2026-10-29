//! Job names (`--name`): `WDOG_HOME/names/<name>` holds the job id and the
//! directory it was started in. A finished job's name can be taken by a new job
//! started in the same directory; the old job is then removed.

use super::dir::{JobPaths, valid_id, write_private};
use super::status;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

const MAX_LEN: usize = 32;
/// A `starting` job whose supervisor never wrote its pid is given up after this long.
const START_LIMIT: f64 = 60.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub id: String,
    pub dir: PathBuf,
}

/// What became of the job a name pointed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OldJob {
    Missing,
    Live,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    Free,
    /// The name's finished job, to remove.
    Replace(String),
    Running(String),
    /// Used by a job started in another directory.
    Elsewhere(String, PathBuf),
}

/// Pure: letters, digits, `.` `_` `-`, at most 32, not starting with `.` or `-`,
/// and not only hex digits (that would look like a job id).
pub fn check_name(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > MAX_LEN {
        return Err(format!("must be 1 to {MAX_LEN} characters"));
    }
    if s.starts_with(['.', '-']) {
        return Err("cannot start with '.' or '-'".into());
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err("only letters, digits, '.', '_' and '-' are allowed".into());
    }
    if valid_id(s) {
        return Err("only hex digits, so it looks like a job id; add another letter".into());
    }
    Ok(())
}

pub fn valid_name(s: &str) -> bool {
    check_name(s).is_ok()
}

/// Pure: may a new job started in `dir` take a name that points to `link`?
pub fn decide(link: Option<&Link>, old: OldJob, dir: &Path) -> Claim {
    match link {
        None => Claim::Free,
        Some(_) if old == OldJob::Missing => Claim::Free,
        Some(l) if l.dir != dir => Claim::Elsewhere(l.id.clone(), l.dir.clone()),
        Some(l) if old == OldJob::Live => Claim::Running(l.id.clone()),
        Some(l) => Claim::Replace(l.id.clone()),
    }
}

fn names_dir(home: &Path) -> PathBuf {
    home.join("names")
}

fn read_link(path: &Path) -> Option<Link> {
    let l: Link = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    valid_id(&l.id).then_some(l)
}

/// The job id a name points to.
pub fn resolve(home: &Path, name: &str) -> Option<String> {
    if !valid_name(name) {
        return None;
    }
    read_link(&names_dir(home).join(name)).map(|l| l.id)
}

fn old_job(home: &Path, id: &str) -> OldJob {
    let p = JobPaths::new(home, id);
    if !p.dir.is_dir() {
        return OldJob::Missing;
    }
    let Ok(s) = status::read(&p.status) else {
        // being created right now
        return OldJob::Live;
    };
    if !s.is_live_state() {
        return OldJob::Ended;
    }
    if s.watcher_pid == 0 {
        let age = crate::event::unix_secs(std::time::SystemTime::now()) - s.started_at;
        return if age > START_LIMIT {
            OldJob::Ended
        } else {
            OldJob::Live
        };
    }
    if crate::proc::read::pid_alive(s.watcher_pid, s.watcher_start_ticks) {
        OldJob::Live
    } else {
        OldJob::Ended
    }
}

/// Hold `names/.lock` while the closure runs.
fn locked<T>(home: &Path, f: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    let dir = names_dir(home);
    if !dir.is_dir() {
        let r = fs::DirBuilder::new().mode(0o700).create(&dir);
        if let Err(e) = r
            && e.kind() != std::io::ErrorKind::AlreadyExists
        {
            return Err(e).with_context(|| format!("cannot create {}", dir.display()));
        }
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(".lock"))
        .context("cannot open the names lock")?;
    let _lock = nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusive)
        .map_err(|(_, e)| e)
        .context("cannot lock the names")?;
    f(&dir)
}

/// Give `name` to job `id`, started in `dir`. Returns the id of the finished job
/// it replaced, which is removed.
pub fn claim(home: &Path, name: &str, dir: &Path, id: &str) -> Result<Option<String>> {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    locked(home, |names| {
        let path = names.join(name);
        let link = read_link(&path);
        let old = link.as_ref().map_or(OldJob::Missing, |l| old_job(home, &l.id));
        let replaced = match decide(link.as_ref(), old, &dir) {
            Claim::Free => None,
            Claim::Replace(old) => Some(old),
            Claim::Running(old) => {
                bail!(
                    "name '{name}' is used by running job {old}; `wdog stop {name}` first, or pick another name"
                )
            }
            Claim::Elsewhere(old, d) => {
                bail!(
                    "name '{name}' is used by job {old}, started in {}; pick another name",
                    d.display()
                )
            }
        };
        let tmp = names.join(format!(".tmp-{id}"));
        let data = serde_json::to_vec(&Link {
            id: id.to_string(),
            dir: dir.clone(),
        })?;
        write_private(&tmp, &data)?;
        fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))?;
        if let Some(old) = &replaced
            && old != id
        {
            let _ = fs::remove_dir_all(home.join(old));
        }
        Ok(replaced)
    })
}

/// Name job `paths` if the spec asks for it: claim the name and record what it
/// replaced. On failure the new job dir is removed.
pub fn apply(home: &Path, paths: &JobPaths, spec: &mut crate::spec::RunSpec) -> Result<()> {
    let Some(name) = spec.name.clone() else {
        return Ok(());
    };
    let dir = match &spec.cwd {
        Some(d) => d.clone(),
        None => std::env::current_dir().context("cannot read the current directory")?,
    };
    match claim(home, &name, &dir, &paths.id) {
        Ok(r) => {
            spec.replaced = r;
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&paths.dir);
            Err(e)
        }
    }
}

/// Drop names whose job is gone (after gc). Errors are ignored.
pub fn prune(home: &Path) {
    if !names_dir(home).is_dir() {
        return;
    }
    let _ = locked(home, |names| {
        for d in fs::read_dir(names)?.flatten() {
            let name = d.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let gone = read_link(&d.path()).is_none_or(|l| !home.join(&l.id).is_dir());
            if gone {
                let _ = fs::remove_file(d.path());
            }
        }
        Ok(())
    });
}

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;
