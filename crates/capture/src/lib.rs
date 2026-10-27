//! `ui-lm-capture` internals: recording a child program's terminal output.
//!
//! The binary in `main.rs` is a thin shell around this library; the types a
//! recording session is built from live here so the integration tests can reach
//! them. What the shell decides is *when* a screen is a frame; what a frame is, and
//! how it is written to disk, lives in `ui-lm-frames`.

pub mod driver;
pub mod events;
pub mod mask;
pub mod quiesce;
pub mod screen;
pub mod summary;

pub use driver::{added_environment, child_environment, Driver, SessionConfig, DEFAULT_ENV};
pub use events::EventTap;
pub use mask::{classify, Semantics};
pub use quiesce::Quiescer;
pub use screen::frame_from_screen;
pub use summary::{summarize, Summary};
