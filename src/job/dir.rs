//! WDOG_HOME, job ids and paths (BG-7, overview decision 5).

use anyhow::{Context, Result, bail};
use std::fs;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct JobPaths {
    pub id: String,
    pub dir: PathBuf,
    pub spec: PathBuf,
    pub log: PathBuf,
    pub events: PathBuf,
    pub status: PathBuf,
    pub guard_pid: PathBuf,
}

impl JobPaths {
    pub fn new(home: &Path, id: &str) -> JobPaths {
        let dir = home.join(id);
        JobPaths {
            id: id.to_string(),
            spec: dir.join("spec.json"),
            log: dir.join("out.log"),
            events: dir.join("events.jsonl"),
            status: dir.join("status.json"),
            guard_pid: dir.join("guard.pid"),
            dir,
        }
    }
}

/// Pure: which directory to use, from the environment values.
pub fn home_path(wdog_home: Option<&str>, xdg_runtime: Option<&str>, uid: u32) -> PathBuf {
    match (
        wdog_home.filter(|s| !s.is_empty()),
        xdg_runtime.filter(|s| !s.is_empty()),
    ) {
        (Some(h), _) => PathBuf::from(h),
        (None, Some(x)) => Path::new(x).join("wdog"),
        (None, None) => PathBuf::from(format!("/tmp/wdog-{uid}")),
    }
}

/// Pure: the safety rule for an existing directory.
pub fn check_home(is_symlink: bool, is_dir: bool, owner: u32, mode: u32, uid: u32) -> Result<(), String> {
    if is_symlink {
        return Err("is a symlink".into());
    }
    if !is_dir {
        return Err("is not a directory".into());
    }
    if owner != uid {
        return Err(format!("is owned by uid {owner}, not {uid}"));
    }
    if mode & 0o022 != 0 {
        return Err(format!(
            "is writable by group or others (mode {:o})",
            mode & 0o777
        ));
    }
    Ok(())
}

/// The job root, created with mode 0700 if missing; refused if unsafe (exit 125).
pub fn wdog_home() -> Result<PathBuf> {
    let uid = nix::unistd::geteuid().as_raw();
    let home = home_path(
        std::env::var("WDOG_HOME").ok().as_deref(),
        std::env::var("XDG_RUNTIME_DIR").ok().as_deref(),
        uid,
    );
    if fs::symlink_metadata(&home).is_err() {
        let r = fs::DirBuilder::new().recursive(true).mode(0o700).create(&home);
        r.with_context(|| format!("cannot create WDOG_HOME {}", home.display()))?;
    }
    let md = fs::symlink_metadata(&home).with_context(|| format!("cannot read {}", home.display()))?;
    if let Err(why) = check_home(md.file_type().is_symlink(), md.is_dir(), md.uid(), md.mode(), uid) {
        bail!("unsafe WDOG_HOME {}: {why}", home.display());
    }
    Ok(home)
}

pub fn new_id() -> String {
    format!("{:06x}", fastrand::u32(..0x100_0000))
}

/// Allocate a job dir with an exclusive mkdir (collision -> new id).
pub fn new_job(home: &Path) -> Result<JobPaths> {
    for _ in 0..100 {
        let id = new_id();
        let p = JobPaths::new(home, &id);
        match fs::DirBuilder::new().mode(0o700).create(&p.dir) {
            Ok(()) => return Ok(p),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).with_context(|| format!("cannot create {}", p.dir.display())),
        }
    }
    bail!("cannot allocate a job id")
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 16 && id.chars().all(|c| c.is_ascii_hexdigit())
}

/// An existing job.
pub fn job(home: &Path, id: &str) -> Result<JobPaths> {
    if !valid_id(id) {
        bail!("bad job id '{id}'");
    }
    let p = JobPaths::new(home, id);
    if !p.dir.is_dir() {
        bail!("no such job '{id}'");
    }
    Ok(p)
}

/// Every job id in WDOG_HOME with its status.json mtime, newest first.
pub fn list_jobs(home: &Path) -> Vec<(String, std::time::SystemTime)> {
    let mut out = vec![];
    if let Ok(rd) = fs::read_dir(home) {
        for d in rd.flatten() {
            let name = d.file_name().to_string_lossy().to_string();
            if !valid_id(&name) {
                continue;
            }
            let p = JobPaths::new(home, &name);
            let t = fs::metadata(&p.status)
                .or_else(|_| fs::metadata(&p.dir))
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            out.push((name, t));
        }
    }
    out.sort_by_key(|j| std::cmp::Reverse(j.1));
    out
}

/// Write a private file (mode 0600).
pub fn write_private(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    f.write_all(data)?;
    Ok(())
}

#[cfg(test)]
#[path = "dir_tests.rs"]
mod tests;
