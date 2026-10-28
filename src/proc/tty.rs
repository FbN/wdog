//! Is a process blocked reading a terminal? (AN-4, tty path)

use super::TtyRead;
use std::path::Path;

pub fn is_tty_path(p: &Path) -> bool {
    let s = p.to_string_lossy();
    s.starts_with("/dev/pts/") || s.starts_with("/dev/tty") || s == "/dev/console"
}

/// `fd0_target` is the target of `/proc/<pid>/fd/0`; None when unreadable.
pub fn classify(state: char, wchan: Option<&str>, fd0_target: Option<&Path>) -> TtyRead {
    let Some(fd0) = fd0_target else {
        return TtyRead::Unknown;
    };
    if !is_tty_path(fd0) {
        return TtyRead::No;
    }
    if state == 'T' {
        return TtyRead::Yes;
    }
    match wchan {
        Some("n_tty_read") | Some("wait_woken") => TtyRead::Yes,
        Some(_) => TtyRead::No,
        None => TtyRead::Unknown,
    }
}

#[cfg(test)]
#[path = "tty_tests.rs"]
mod tests;
