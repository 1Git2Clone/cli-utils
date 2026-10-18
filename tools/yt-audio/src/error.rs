//! Error types shared across the crate.

use thiserror::Error;

/// A failure that stops the whole run.
#[derive(Debug, Error)]
pub enum Error {
    /// The process runs with effective uid 0.
    #[error("refusing to run as root")]
    Root,
}
