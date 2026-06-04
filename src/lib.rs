//! Solstice — A local-first TUI calendar application
//!
//! This library crate contains all application modules, making them
//! accessible to integration tests in `tests/`.

#![allow(
    clippy::collapsible_if,
    clippy::type_complexity,
    clippy::too_many_arguments
)]

pub mod app;
pub mod calendar;
pub mod config;
pub mod events;
pub mod gcal;
pub mod ui;
pub mod utils;
