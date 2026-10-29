//! Frames for the UI-LM capture rig: a styled cell grid and its wire codec.

pub mod frame;
pub mod grid;
pub mod rle;
pub mod session;

pub use frame::{changed_rows, frame_from_chars, Frame, MaskReason, Span};
pub use grid::{grid_from_screen, Color, Grid, Style, StyledCell};
pub use rle::{decode_grid, encode_grid, CellRun};
pub use session::{
    read_frames, read_orphaned_frames, record_exit, timestamp, Cost, SessionRecord, SessionWriter,
};
