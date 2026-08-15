//! `hop` — smart directory jumper.
//!
//! A fuzzy `cd` replacement that learns your directory history and supports
//! bookmarks.
//!
//! The same crate powers the `hop` binary and is published as a library so
//! integrations (wrappers, plugins, tests) can reuse the dispatch logic and
//! storage layer without shelling out.
//!
//! # Entry points
//! - [`cli::run`] — dispatches a raw `argv`, the same way the binary does.
//! - [`cli::find_best`] — resolve a query to the best path using bookmarks
//!   and history.
//! - [`Database`] — SQLite-backed store. [`Database::open`] handles XDG
//!   paths, WAL setup, migrations, and the `fuzzy-cd` → `hop` legacy copy.
//! - [`init::script_for`] — shell integration scripts for bash/zsh/fish.
//! - [`completions::script_for`] — tab-completion scripts for bash/zsh/fish.
//!
//! # Quick example
//! ```no_run
//! use hop::Database;
//! let db = Database::in_memory().unwrap();
//! db.record_visit("/tmp").unwrap();
//! ```

pub mod cli;
pub mod completions;
pub mod config;
pub mod db;
pub mod doctor;
pub mod import;
pub mod init;
pub mod picker;
pub mod score;

pub use db::Database;
