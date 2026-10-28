//! wdog: watchdog runner for long shell commands, built for coding agents.

mod cli;
mod cmd;
mod console;
mod core;
mod detect;
mod duration;
mod engine;
mod event;
mod exec;
mod exit;
mod job;
mod lines;
mod proc;
mod rt;
mod spec;

use clap::Parser;
use cli::{Cli, Cmd};
use exit::Exit;

fn main() {
    std::panic::set_hook(Box::new(|info| {
        // Cleanup is the guard's job: the abort closes its pipe.
        console::err(&format!("[wdog] ERROR internal: {info}"));
        std::process::abort();
    }));
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            use clap::error::ErrorKind;
            if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                let _ = e.print();
                exit::apply(Exit::Code(0));
            }
            let msg = e.to_string();
            let first = msg
                .lines()
                .next()
                .unwrap_or("bad arguments")
                .trim_start_matches("error: ");
            console::err(&msg);
            console::out(&format!("[wdog] ERROR {first}"));
            exit::apply(Exit::Code(exit::WDOG_FAILED));
        }
    };
    let result = dispatch(cli.cmd);
    let code = match result {
        Ok(e) => e,
        Err(e) => {
            console::out(&format!("[wdog] ERROR {e:#}"));
            Exit::Code(exit::WDOG_FAILED)
        }
    };
    exit::apply(code)
}

fn dispatch(c: Cmd) -> anyhow::Result<Exit> {
    match c {
        Cmd::Run(a) => cmd::run::run(&a),
        Cmd::Start(a) => cmd::start::run(&a),
        Cmd::Wait(a) => cmd::wait::wait(
            &a.id,
            cmd::wait::WaitOpts {
                for_: a.for_,
                until: a.until,
                port: a.port,
                url: a.url,
                file: a.file,
                json: a.json,
            },
        ),
        Cmd::Sleep(a) => cmd::wait::sleep(&a.duration, a.id.as_deref(), a.json),
        Cmd::Status(a) => cmd::status::run(a.id.as_deref(), a.json),
        Cmd::Tail(a) => cmd::tail::run(&a.id, a.n, a.follow, a.raw),
        Cmd::Errors(a) => cmd::errors::run(&a.id, a.json),
        Cmd::Stop(a) => cmd::stop::run(&a.id, &a.grace, a.json),
        Cmd::Gc(a) => cmd::gc::run(a.json),
        Cmd::Supervise(a) => cmd::supervise::run(&a.id),
        Cmd::Guard(a) => cmd::guard::run(&a.id),
        Cmd::Env0 => {
            use std::io::Write;
            let _ = std::io::stdout().lock().write_all(&exec::envfile::dump_env0());
            Ok(Exit::Code(0))
        }
    }
}
