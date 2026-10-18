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

    /// yt-dlp could not be started or its output could not be read.
    #[error("cannot run yt-dlp: {0}")]
    Io(#[from] std::io::Error),

    /// yt-dlp exited unsuccessfully.
    #[error("yt-dlp failed ({status}): {stderr_tail}")]
    YtDlp {
        /// Exit status as reported by the OS.
        status: std::process::ExitStatus,
        /// Last lines of yt-dlp's stderr.
        stderr_tail: String,
    },

    /// yt-dlp did not finish within the allowed time and was killed.
    #[error("yt-dlp timed out after {0:?}")]
    Timeout(std::time::Duration),
}
