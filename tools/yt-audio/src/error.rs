//! Error types shared across the crate.

use thiserror::Error;

/// How many trailing stderr lines an [`Error::YtDlp`] message shows.
const SHOWN_STDERR_LINES: usize = 20;

/// A failure that stops the whole run.
#[derive(Debug, Error)]
pub enum Error {
    /// The process runs with effective uid 0.
    #[error("refusing to run as root")]
    Root,

    /// yt-dlp's JSON output could not be parsed.
    #[error("unreadable yt-dlp JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// yt-dlp could not be started.
    #[error("cannot run yt-dlp: {0}")]
    Spawn(#[source] std::io::Error),

    /// Any other I/O failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// yt-dlp exited unsuccessfully.
    #[error("yt-dlp failed ({status}): {}", last_lines(stderr, SHOWN_STDERR_LINES))]
    YtDlp {
        /// Exit status as reported by the OS.
        status: std::process::ExitStatus,
        /// Everything yt-dlp wrote to stderr.
        stderr: String,
    },

    /// yt-dlp did not finish within the allowed time and was killed.
    #[error("yt-dlp timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// A download finished without yt-dlp reporting where it wrote the file.
    #[error("yt-dlp printed no output path")]
    NoOutputPath,

    /// The audio file's tags could not be read or written.
    #[error("cannot write tags: {0}")]
    Tag(#[from] lofty::error::LoftyError),

    /// The LLM client could not be set up.
    #[cfg(feature = "llm")]
    #[error("cannot set up the LLM client: {0}")]
    Llm(String),
}

/// The last `n` lines of `text`.
pub fn last_lines(text: &str, n: usize) -> &str {
    let text = text.trim_end();
    let start = text
        .rmatch_indices('\n')
        .nth(n.saturating_sub(1))
        .map_or(0, |(i, _)| i + 1);
    &text[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_lines_counts_from_the_end() {
        assert_eq!(last_lines("a\nb\nc\n", 2), "b\nc");
        assert_eq!(last_lines("a\nb", 5), "a\nb");
        assert_eq!(last_lines("", 3), "");
    }
}
