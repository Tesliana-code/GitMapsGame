//! GitMapsGame real mode: derive a dependency map from a GitHub repository snapshot.
//!
//! Native Rust port of the Python reference in `gitmaps/`. Output must stay byte-identical
//! to the reference; see `tests/parity.rs`.

pub mod canonical;
pub mod derive;
pub mod fetch;
pub mod github;
pub mod projection;
pub mod pyrepr;
mod pyrepr_table;
mod queries;
pub mod render;
pub mod requires;

pub const SNAPSHOT_SCHEMA: &str = "gitmaps.snapshot/v1";
pub const MAP_SCHEMA: &str = "gitmaps.map/v1";
pub const DEFAULT_LABEL: &str = "gitmaps:crossing";
pub const MAX_CROSSINGS: u64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// GitHub query failed, replay diverged, or a limit was exceeded (reference: GitHubError).
    GitHub,
    /// Malformed or non-canonical input (reference: ValueError).
    Value,
    /// Publication refused (reference: PermissionError).
    Permission,
    Io,
}

#[derive(Debug, Clone)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl Error {
    pub fn github(message: impl Into<String>) -> Self {
        Error {
            kind: ErrorKind::GitHub,
            message: message.into(),
        }
    }
    pub fn value(message: impl Into<String>) -> Self {
        Error {
            kind: ErrorKind::Value,
            message: message.into(),
        }
    }
    pub fn permission(message: impl Into<String>) -> Self {
        Error {
            kind: ErrorKind::Permission,
            message: message.into(),
        }
    }
    pub fn io(message: impl Into<String>) -> Self {
        Error {
            kind: ErrorKind::Io,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}
