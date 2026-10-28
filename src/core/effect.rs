use crate::event::Event;
use crate::exit::EndReason;
use nix::sys::signal::Signal;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Live stdout line to the console (not in quiet mode).
    Print(String),
    /// Live stderr line to wdog's stderr.
    PrintErr(String),
    /// Console (text or json) + events.jsonl.
    Emit(Event),
    /// One line for out.log, already prefixed.
    AppendLog(String),
    WriteStatus,
    /// To every verified job pgid and tree member.
    Signal(Signal),
    /// Schedule `GraceExpired`.
    ArmGrace(Duration),
    Spawn {
        step: usize,
        attempt: u32,
    },
    Finish(EndReason),
}
