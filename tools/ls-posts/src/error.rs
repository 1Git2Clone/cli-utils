//! Error types shared across the crate.

use thiserror::Error;

/// Why a file was skipped instead of being turned into a record.
#[derive(Debug, Error)]
pub enum Skip {
    /// No path component names a supported site.
    #[error("no known site in path")]
    UnknownSite,
    /// The site folder is known, but the file name does not fit its scheme.
    #[error("unrecognized {site} filename {name:?}")]
    UnrecognizedName {
        /// Canonical name of the site the folder resolved to.
        site: &'static str,
        /// The offending file name.
        name: String,
    },
}

/// A `--format` template that could not be parsed.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FormatError {
    /// `{name}` did not match any known placeholder.
    #[error("unknown placeholder {{{0}}} in --format (valid: {1})")]
    UnknownPlaceholder(String, String),
    /// A `{` was never closed by a `}`.
    #[error("unclosed '{{' in --format")]
    UnclosedBrace,
}
