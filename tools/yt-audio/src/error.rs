//! Error types shared across the crate.

use thiserror::Error;

/// A failure that stops the whole run.
#[derive(Debug, Error)]
pub enum Error {
    /// The process runs with effective uid 0.
    #[error("refusing to run as root")]
    Root,

    /// yt-dlp's JSON output could not be parsed.
    #[error("unreadable yt-dlp JSON: {0}")]
    Json(#[from] serde_json::Error),
}
