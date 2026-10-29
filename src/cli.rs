//! Command line (spec §5.1) and validation into `RunSpec` (errors -> exit 125).

use crate::duration;
use crate::spec::{OutputMode, RunSpec, SPEC_VERSION, merge_patterns};
use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "wdog",
    version,
    about = "Watchdog runner for long shell commands: logs, stall/prompt/orphan/error alerts, background jobs",
    after_help = "Every line wdog writes starts with [wdog]. Read the last one: DONE exit=<n> reason=<r>, or ERROR <msg>."
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Run steps in the foreground and return when done
    Run(RunArgs),
    /// Run steps as a background job and print its id
    Start(RunArgs),
    /// Wait for a job: exit, alert, --for, or a condition
    Wait(WaitArgs),
    /// Sleep, but wake early on an alert from a job
    Sleep(SleepArgs),
    /// One line per job
    Status(StatusArgs),
    /// Show a job's log
    Tail(TailArgs),
    /// List error-pattern matches in a job's log
    Errors(ErrorsArgs),
    /// Stop a job: SIGTERM to the tree, SIGKILL after --grace
    Stop(StopArgs),
    /// Remove job directories older than 24 hours
    Gc(GcArgs),
    #[command(hide = true, name = "__supervise")]
    Supervise(IdArg),
    #[command(hide = true, name = "__env0")]
    Env0,
    #[command(hide = true, name = "__guard")]
    Guard(IdArg),
}

#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    /// Working directory for every step
    #[arg(long, value_name = "DIR")]
    pub cwd: Option<PathBuf>,
    /// Environment variable for every step (repeatable)
    #[arg(long = "env", value_name = "K=V")]
    pub env: Vec<String>,
    /// Load a .env file for every step, like `set -a; . FILE; set +a` (repeatable)
    #[arg(long = "env-file", value_name = "FILE")]
    pub env_file: Vec<PathBuf>,
    /// Log file (default: $WDOG_HOME/<id>/out.log)
    #[arg(long, value_name = "FILE")]
    pub log: Option<PathBuf>,
    /// Print the last N output lines at the end (default 20)
    #[arg(long, value_name = "N")]
    pub tail: Option<usize>,
    /// Print the first N output lines at the end
    #[arg(long, value_name = "N")]
    pub head: Option<usize>,
    /// Only [wdog] lines and the final summary (default when stdout is not a terminal)
    #[arg(long, conflicts_with = "live")]
    pub quiet: bool,
    /// Stream live output even when stdout is not a terminal
    #[arg(long)]
    pub live: bool,
    /// Seconds of no output, CPU or I/O before STALL
    #[arg(long, default_value = "120", value_name = "DURATION")]
    pub stall: String,
    /// Extra error pattern, a regex (repeatable)
    #[arg(long = "err", value_name = "REGEX")]
    pub err: Vec<String>,
    /// Drop the built-in error patterns
    #[arg(long)]
    pub no_default_err: bool,
    /// Total time limit; the tree is stopped and wdog exits 124
    #[arg(long, value_name = "DURATION")]
    pub max: Option<String>,
    /// Stop the tree on the first STALL (exit 126)
    #[arg(long)]
    pub kill_on_stall: bool,
    /// Run all steps even if one fails
    #[arg(long)]
    pub keep_going: bool,
    /// [wdog] events as JSON lines
    #[arg(long)]
    pub json: bool,
    /// Keep the terminal as stdin (humans only)
    #[arg(long)]
    pub stdin_tty: bool,
    /// Sampling interval for /proc
    #[arg(long, default_value = "2s", value_name = "DURATION")]
    pub interval: String,
    /// Alert after this long of CPU use with no output (off by default)
    #[arg(long, value_name = "DURATION")]
    pub busy: Option<String>,
    /// Rerun a failed step up to N times
    #[arg(long, default_value_t = 0, value_name = "N")]
    pub retry: u32,
    #[arg(long, default_value = "5s", hide = true)]
    pub prompt_grace: String,
    #[arg(long, default_value = "5s", hide = true)]
    pub orphan_grace: String,
    /// Read the steps from a script, `-` for stdin; `# -STEP- [label]` lines split it
    #[arg(long, value_name = "FILE", conflicts_with = "commands")]
    pub steps_file: Option<PathBuf>,
    /// Steps, each one shell command, after `--`
    #[arg(
        last = true,
        required_unless_present = "steps_file",
        num_args = 1..,
        value_name = "CMD"
    )]
    pub commands: Vec<String>,
}

#[derive(Args, Debug)]
pub struct WaitArgs {
    pub id: String,
    /// Give up after this long (exit 124; the job keeps running)
    #[arg(long = "for", value_name = "DURATION")]
    pub for_: Option<String>,
    /// Ready when a log line matches
    #[arg(long, value_name = "REGEX")]
    pub until: Option<String>,
    /// Ready when a TCP port on localhost accepts connections
    #[arg(long, value_name = "N")]
    pub port: Option<u16>,
    /// Ready when an HTTP GET returns a status below 500
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,
    /// Ready when a file exists
    #[arg(long, value_name = "PATH")]
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct SleepArgs {
    pub duration: String,
    pub id: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct StatusArgs {
    pub id: Option<String>,
    /// Also list every step: done (exit, duration), running, pending or not run
    #[arg(long)]
    pub steps: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct TailArgs {
    pub id: String,
    #[arg(short = 'n', default_value_t = 20)]
    pub n: usize,
    /// Follow the log until the job ends
    #[arg(short = 'f')]
    pub follow: bool,
    /// Keep the [n] / [n!] prefixes
    #[arg(long)]
    pub raw: bool,
}

#[derive(Args, Debug)]
pub struct StopArgs {
    pub id: String,
    #[arg(long, default_value = "5s", value_name = "DURATION")]
    pub grace: String,
    /// Print the result as one JSON line
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ErrorsArgs {
    pub id: String,
    /// Print the matches as one JSON line
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct GcArgs {
    /// Print the result as one JSON line
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct IdArg {
    pub id: String,
}

pub fn dur(name: &str, s: &str) -> Result<std::time::Duration> {
    duration::parse(s).with_context(|| format!("--{name}"))
}

fn valid_env_key(k: &str) -> bool {
    let mut c = k.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn absolute(base: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

/// Validate and resolve. `stdout_tty` decides the default for quiet (LG-4); `start` passes false.
pub fn to_spec(a: &RunArgs, stdout_tty: bool) -> Result<RunSpec> {
    let here = std::env::current_dir().context("cannot read the current directory")?;
    let cwd = match &a.cwd {
        Some(d) => {
            let d = absolute(&here, d);
            if !d.is_dir() {
                bail!("--cwd {}: not a directory", d.display());
            }
            Some(d)
        }
        None => None,
    };
    let mut env = vec![];
    for kv in &a.env {
        let Some((k, v)) = kv.split_once('=') else {
            bail!("--env {kv}: expected K=V")
        };
        if !valid_env_key(k) {
            bail!("--env {kv}: bad variable name");
        }
        env.push((k.to_string(), v.to_string()));
    }
    let base = cwd.clone().unwrap_or_else(|| here.clone());
    let mut env_files = vec![];
    for f in &a.env_file {
        let p = absolute(&base, f);
        match std::fs::metadata(&p) {
            Err(_) => bail!("env-file {}: not found", p.display()),
            Ok(m) if m.is_dir() => bail!("env-file {}: not readable", p.display()),
            Ok(_) => {}
        }
        if std::fs::File::open(&p).is_err() {
            bail!("env-file {}: not readable", p.display());
        }
        env_files.push(p);
    }
    for r in &a.err {
        regex::Regex::new(r).with_context(|| format!("--err {r}: bad regex"))?;
    }
    let output = match (a.head, a.tail) {
        (None, None) => OutputMode { head: 0, tail: 20 },
        (Some(h), None) => OutputMode { head: h, tail: 0 },
        (None, Some(t)) => OutputMode { head: 0, tail: t },
        (Some(h), Some(t)) => OutputMode { head: h, tail: t },
    };
    let interval = dur("interval", &a.interval)?;
    if interval < std::time::Duration::from_millis(50) {
        bail!("--interval must be at least 50ms");
    }
    let (steps, names) = match &a.steps_file {
        None => (a.commands.clone(), vec![]),
        Some(f) => read_steps_file(&here, f, a.stdin_tty)?,
    };
    let quiet = if a.quiet {
        true
    } else if a.live {
        false
    } else {
        !stdout_tty
    };
    Ok(RunSpec {
        v: SPEC_VERSION,
        steps,
        names,
        cwd,
        env,
        env_files,
        log: a.log.as_ref().map(|l| absolute(&here, l)),
        output,
        quiet,
        json: a.json,
        stall: dur("stall", &a.stall)?,
        interval,
        prompt_grace: dur("prompt-grace", &a.prompt_grace)?,
        orphan_grace: dur("orphan-grace", &a.orphan_grace)?,
        busy: a.busy.as_deref().map(|b| dur("busy", b)).transpose()?,
        err_patterns: merge_patterns(&a.err, a.no_default_err),
        max: a.max.as_deref().map(|m| dur("max", m)).transpose()?,
        kill_on_stall: a.kill_on_stall,
        keep_going: a.keep_going,
        stdin_tty: a.stdin_tty,
        retry: a.retry,
    })
}

/// `--steps-file FILE|-`: (scripts, names). With `-` the steps get /dev/null as stdin.
fn read_steps_file(here: &Path, f: &Path, stdin_tty: bool) -> Result<(Vec<String>, Vec<String>)> {
    use std::io::Read;
    let text = if f == Path::new("-") {
        if stdin_tty {
            bail!("--steps-file - reads stdin: it cannot be used with --stdin-tty");
        }
        // SAFETY: isatty has no preconditions.
        if unsafe { nix::libc::isatty(0) == 1 } {
            bail!("--steps-file -: stdin is a terminal; pipe the script in, e.g. <<'EOF'");
        }
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .context("--steps-file -: cannot read stdin")?;
        s
    } else {
        let p = absolute(here, f);
        std::fs::read_to_string(&p).with_context(|| format!("--steps-file {}: cannot read", p.display()))?
    };
    let steps = crate::stepfile::parse(&text)?;
    Ok(steps.into_iter().map(|s| (s.script, s.name)).unzip())
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
