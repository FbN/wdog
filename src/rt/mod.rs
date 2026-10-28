//! Single-threaded async runtime: `LocalExecutor` + `async-io` (no Tokio, no Arc).
//!
//! Callers own the executor and pass `&ex` to their tasks:
//! `let ex = LocalExecutor::new(); async_io::block_on(ex.run(main(&ex)))`.

pub mod channel;
pub mod pipe;
