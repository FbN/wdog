#![allow(clippy::disallowed_methods)]
use super::*;
use crate::proc::tree::descendants;

#[test]
fn snapshot_has_self_and_child() {
    let me = std::process::id() as i32;
    let mut child = std::process::Command::new("sleep").arg("5").spawn().unwrap();
    let es = snapshot(false);
    let mine = es.iter().find(|e| e.pid == me).expect("self in snapshot");
    assert_eq!(mine.ppid, nix::unistd::getppid().as_raw());
    let d = descendants(&es, &[me], &[]);
    assert!(d.contains(&(child.id() as i32)));
    assert!(pid_alive(me, self_start_ticks()));
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(!pid_alive(child.id() as i32, 0));
}
