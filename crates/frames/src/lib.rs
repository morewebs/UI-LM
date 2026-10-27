//! Frames for the UI-LM capture rig: a styled cell grid and its wire codec.

pub mod grid;
pub mod rle;

pub use grid::{Color, Grid, Style, StyledCell};
pub use rle::{decode_grid, encode_grid, CellRun};
