//! Start one step: `/bin/sh -c`, own process group, stdin /dev/null (EX-5, EX-6).

use crate::exec::envfile::EnvChange;
use crate::spec::RunSpec;
use nix::libc;
use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

pub struct Spawned {
    pub pid: i32,
    pub pgid: i32,
    pub start_ticks: u64,
    pub stdout: OwnedFd,
    pub stderr: OwnedFd,
}

/// `take_tty`: give the terminal to the step's group (`--stdin-tty` with wdog in the foreground).
pub fn spawn_step(cmd: &str, spec: &RunSpec, env: &[EnvChange], take_tty: bool) -> io::Result<Spawned> {
    let mut c = Command::new("/bin/sh");
    c.arg("-c").arg(cmd).process_group(0);
    if let Some(d) = &spec.cwd {
        c.current_dir(d);
    }
    for ch in env {
        match ch {
            EnvChange::Set(k, v) => {
                c.env(k, v);
            }
            EnvChange::Unset(k) => {
                c.env_remove(k);
            }
        }
    }
    c.stdin(if spec.stdin_tty {
        Stdio::inherit()
    } else {
        Stdio::null()
    });
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    let parent = nix::unistd::getpid().as_raw();
    // SAFETY: only async-signal-safe libc calls in the child before exec.
    unsafe {
        c.pre_exec(move || {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0);
            if libc::getppid() != parent {
                libc::_exit(1);
            }
            if take_tty {
                libc::signal(libc::SIGTTOU, libc::SIG_IGN);
                libc::tcsetpgrp(0, libc::getpid());
                libc::signal(libc::SIGTTOU, libc::SIG_DFL);
            }
            Ok(())
        });
    }
    let mut child = c.spawn()?;
    let pid = child.id() as i32;
    let stdout: OwnedFd = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("no stdout"))?
        .into();
    let stderr: OwnedFd = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("no stderr"))?
        .into();
    // The reaper owns the exit status; dropping `Child` neither waits nor kills.
    drop(child);
    if take_tty {
        set_tty_owner(pid);
    }
    Ok(Spawned {
        pid,
        pgid: pid,
        start_ticks: crate::proc::read::start_ticks(pid),
        stdout,
        stderr,
    })
}

/// `tcsetpgrp(0, pgid)` with SIGTTOU ignored.
pub fn set_tty_owner(pgid: i32) {
    // SAFETY: plain libc calls on fd 0.
    unsafe {
        let old = libc::signal(libc::SIGTTOU, libc::SIG_IGN);
        libc::tcsetpgrp(0, pgid);
        libc::signal(libc::SIGTTOU, old);
    }
}

/// wdog's stdin is a terminal and wdog's group owns it.
pub fn in_tty_foreground() -> bool {
    // SAFETY: plain libc queries on fd 0.
    unsafe { libc::isatty(0) == 1 && libc::tcgetpgrp(0) == libc::getpgrp() }
}

pub fn take_back_tty() {
    // SAFETY: getpgrp has no preconditions.
    set_tty_owner(unsafe { libc::getpgrp() });
}
