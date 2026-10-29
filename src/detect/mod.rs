//! Anomaly detectors (spec §4.3). Plain structs and pure functions; the reducer calls them.

pub mod busy;
pub mod errors;
pub mod orphans;
pub mod prompt;
pub mod silence;
pub mod stall;
