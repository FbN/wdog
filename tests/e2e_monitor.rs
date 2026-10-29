//! `wdog monitor`: refuses without a terminal; in a pty it draws the jobs, `o` shows the
//! steps, `q` quits.
#![allow(clippy::disallowed_methods)]

mod common;
use common::*;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::process::Stdio;
use std::time::{Duration, Instant};

#[test]
fn needs_a_terminal() {
    let e = Env::new();
    let o = e.run(&["monitor"]);
    assert_eq!(o.code, Some(125));
    assert!(
        o.last_line().starts_with("[wdog] ERROR monitor needs a terminal"),
        "{}",
        o.stdout
    );
}

#[test]
fn draws_jobs_and_quits_on_q() {
    let e = Env::new();
    let id = e.start(&["--", "echo hello-from-job; sleep 30"]);
    let size = nix::pty::Winsize {
        ws_row: 24,
        ws_col: 110,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = nix::pty::openpty(Some(&size), None).unwrap();
    let slave = |fd: &OwnedFd| Stdio::from(fd.try_clone().unwrap());
    let mut c = e.cmd(&["monitor"]);
    c.stdin(slave(&pty.slave))
        .stdout(slave(&pty.slave))
        .stderr(slave(&pty.slave));
    let mut child = c.spawn().unwrap();
    // the Command holds copies of the slave end: close all of them
    drop(c);
    drop(pty.slave);
    let mut master = std::fs::File::from(pty.master);
    let mut reader = master.try_clone().unwrap();
    let screen = std::thread::spawn(move || {
        let mut all = vec![];
        let mut buf = [0u8; 4096];
        // EIO once the child closes the pty
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            all.extend_from_slice(&buf[..n]);
        }
        String::from_utf8_lossy(&all).into_owned()
    });
    std::thread::sleep(Duration::from_millis(1500));
    master.write_all(b"o").unwrap();
    std::thread::sleep(Duration::from_millis(700));
    master.write_all(b"q").unwrap();
    let t0 = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        assert!(t0.elapsed() < Duration::from_secs(5), "monitor did not quit on q");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success());
    drop(master);
    let out = screen.join().unwrap();
    assert!(out.contains("STATUS") && out.contains("following"), "{out}");
    assert!(out.contains(&id), "{out}");
    assert!(out.contains("hello-from-job"), "{out}");
    // `o`: the steps pane shows the script; nothing else on screen says "sleep"
    assert!(out.contains("sleep"), "{out}");
    e.run(&["stop", &id]);
}
