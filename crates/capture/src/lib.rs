//! `ui-lm-capture` internals: recording a child program's terminal output.
//!
//! The binary in `main.rs` is a thin shell around this library; the types a
//! recording session is built from live here so the integration tests can reach
//! them. The command line arrives in a later task.

pub mod driver;
pub mod mask;
pub mod quiesce;

pub use driver::{Driver, SessionConfig};
pub use mask::{classify, Semantics};
pub use quiesce::Quiescer;
