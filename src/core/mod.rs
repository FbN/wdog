//! Functional core: `step(State, Input, now) -> (State, Vec<Effect>)`.

pub mod effect;
pub mod input;
pub mod state;
pub mod step;

pub use effect::Effect;
pub use input::Input;
pub use state::{JobInfo, MainProc, Mode, State};
pub use step::{exit_of, step, summary};
