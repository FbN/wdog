//! `wdog on HOST <wdog command>`: run a wdog command on another host over ssh, installing this
//! binary into `/tmp/wdog-<uid>` there first if needed (RM-1..RM-8).

use crate::event::{Event, render_json, render_text};
use crate::exit::Exit;
use crate::remote::{self, DEFAULT_BASE, Elf, INSTALL_SCRIPT, MISSING_CODE, Marker, RUN_SCRIPT, StdinMode};
use anyhow::{Context, Result, bail};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::Pid;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::SystemTime;

pub struct OnOpts {
    pub host: String,
    pub dir: Option<String>,
    pub agent: Option<PathBuf>,
    pub args: Vec<String>,
}

/// Escape hatch for agents built against glibc, when the remote is known to match (tests use it).
const ALLOW_DYNAMIC_ENV: &str = "WDOG_ON_ALLOW_DYNAMIC";
/// The ssh program, with optional extra words, e.g. `ssh -p 2222` (tests use a fake).
const SSH_ENV: &str = "WDOG_SSH";

pub fn run(o: &OnOpts) -> Result<Exit> {
    if let Err(why) = remote::check_host(&o.host) {
        bail!("on {}: host {why}", o.host);
    }
    let base = o.dir.clone().unwrap_or_else(|| DEFAULT_BASE.to_string());
    if !base.starts_with('/') {
        bail!("--dir {base}: must be an absolute path on the remote host");
    }
    let agent_path = match &o.agent {
        Some(p) => p.clone(),
        None => std::env::current_exe().context("cannot find the wdog binary")?,
    };
    let agent = std::fs::read(&agent_path)
        .with_context(|| format!("cannot read the agent binary {}", agent_path.display()))?;
    let name = remote::agent_name(env!("CARGO_PKG_VERSION"), &agent);
    // SAFETY: isatty has no preconditions.
    let stdin_tty = unsafe { nix::libc::isatty(0) == 1 };
    let mode = remote::stdin_mode(&o.args, stdin_tty);
    let input = match mode {
        StdinMode::Buffer => {
            let mut b = vec![];
            std::io::stdin()
                .read_to_end(&mut b)
                .context("on: cannot read stdin")?;
            Some(b)
        }
        _ => None,
    };
    let mut run_args = vec![base.clone(), name.clone()];
    run_args.extend(o.args.iter().cloned());
    let run_cmd = remote::remote_command(RUN_SCRIPT, &run_args);

    for attempt in 0..2 {
        let (code, markers) = ssh(&o.host, &run_cmd, mode, input.as_deref())?;
        if code == MISSING_CODE {
            if let Some(Marker::Unsupported(u)) = markers.first() {
                bail!("on {}: remote is '{u}'; only Linux x86_64 is supported", o.host);
            }
            if markers.contains(&Marker::Missing) && attempt == 0 {
                check_agent(&agent, &agent_path, &o.host)?;
                install(o, &base, &name, &agent)?;
                continue;
            }
        }
        return Ok(Exit::Code(code));
    }
    bail!("on {}: the agent is still missing after installing it", o.host)
}

/// Only a static x86_64 binary is sure to run on another host.
fn check_agent(agent: &[u8], path: &std::path::Path, host: &str) -> Result<()> {
    match remote::check_elf(agent) {
        Elf::Static => Ok(()),
        Elf::Dynamic if std::env::var_os(ALLOW_DYNAMIC_ENV).is_some() => Ok(()),
        Elf::Dynamic => bail!(
            "on: {} is not a static binary, so it may not run on {host}; use the release build \
             (cargo binstall wdog, or the musl tarball) or pass --agent PATH",
            path.display()
        ),
        Elf::NotX86_64 | Elf::Unknown => {
            bail!("on: {} is not an x86_64 Linux binary", path.display())
        }
    }
}

fn install(o: &OnOpts, base: &str, name: &str, agent: &[u8]) -> Result<()> {
    let args = vec![base.to_string(), name.to_string(), agent.len().to_string()];
    let cmd = remote::remote_command(INSTALL_SCRIPT, &args);
    let (code, markers) = ssh(&o.host, &cmd, StdinMode::Buffer, Some(agent))?;
    let host = &o.host;
    for m in &markers {
        match m {
            Marker::Unsafe(d) => {
                bail!("on {host}: {d} is a symlink or owned by another user; remove it or use --dir DIR")
            }
            Marker::UploadFailed => bail!("on {host}: upload failed (disk full?)"),
            Marker::Noexec(d) => {
                bail!("on {host}: cannot run programs from {d} (noexec mount?); use --dir DIR")
            }
            Marker::Installed(h) if code == 0 => {
                let e = Event::On {
                    host: host.clone(),
                    installed: env!("CARGO_PKG_VERSION").to_string(),
                    path: format!("{h}/wdog"),
                };
                if remote::wants_json(&o.args) {
                    crate::console::out(&render_json(&e, SystemTime::now()));
                } else {
                    crate::console::out(&render_text(&e));
                }
                return Ok(());
            }
            _ => {}
        }
    }
    bail!("on {host}: installing the agent failed (ssh exit {code})")
}

/// Run one ssh command. stdout goes straight through; stderr is forwarded line by line except
/// `WDOG_ON` marker lines, which are returned. Exit code: the remote's, or 128 + signal.
fn ssh(host: &str, cmd: &str, mode: StdinMode, input: Option<&[u8]>) -> Result<(i32, Vec<Marker>)> {
    let ssh = std::env::var(SSH_ENV).unwrap_or_else(|_| "ssh".to_string());
    let mut words = ssh.split_whitespace();
    let prog = words.next().unwrap_or("ssh");
    let mut c = Command::new(prog);
    c.args(words)
        .args(["-o", "BatchMode=yes"])
        .arg(if mode == StdinMode::Tty { "-t" } else { "-T" })
        .arg("--")
        .arg(host)
        .arg(cmd);
    c.stdin(match (mode, input) {
        (StdinMode::Tty, _) => Stdio::inherit(),
        (_, Some(_)) => Stdio::piped(),
        _ => Stdio::null(),
    })
    .stdout(Stdio::inherit())
    .stderr(Stdio::piped());
    let mut child = c.spawn().with_context(|| format!("cannot run {prog}"))?;
    let feeder = match (child.stdin.take(), input) {
        (Some(mut w), Some(data)) => {
            let data = data.to_vec();
            Some(std::thread::spawn(move || {
                let _ = w.write_all(&data);
            }))
        }
        _ => None,
    };
    let mut markers = vec![];
    if let Some(err) = child.stderr.take() {
        let mut r = BufReader::new(err);
        let mut line = Vec::new();
        while r.read_until(b'\n', &mut line).unwrap_or(0) > 0 {
            let text = String::from_utf8_lossy(&line);
            let text = text.trim_end_matches(['\n', '\r']);
            match remote::parse_marker(text) {
                Some(m) => markers.push(m),
                None => crate::console::err(text),
            }
            line.clear();
        }
    }
    if let Some(f) = feeder {
        let _ = f.join();
    }
    let pid = Pid::from_raw(child.id() as i32);
    let code = loop {
        match waitpid(pid, None) {
            Ok(WaitStatus::Exited(_, c)) => break c,
            Ok(WaitStatus::Signaled(_, s, _)) => break 128 + s as i32,
            Ok(_) => continue,
            Err(nix::errno::Errno::EINTR) => continue,
            Err(e) => return Err(e).context("waiting for ssh"),
        }
    };
    Ok((code, markers))
}
