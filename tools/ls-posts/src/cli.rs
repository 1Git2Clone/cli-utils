//! Command-line interface definition.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{CommandFactory, FromArgMatches, Parser};

use crate::error::FormatError;
use crate::record::{FormatString, Placeholder};

/// Reconstruct gallery-dl post URLs from a folder tree.
#[derive(Debug, Parser)]
#[command(name = "ls-posts", version)]
pub struct Cli {
    /// Files and/or directories (shell globs welcome); defaults to '.'.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Custom output per record, e.g. '{path}\t{url}'.
    #[arg(short, long, value_name = "FMT")]
    pub format: Option<String>,

    /// Shortcut for --format '{path}\t{url}'.
    #[arg(short, long)]
    pub long: bool,

    /// Print a JSON array of records.
    #[arg(long, conflicts_with = "jsonl")]
    pub json: bool,

    /// Print one JSON object per line.
    #[arg(long, conflicts_with = "json")]
    pub jsonl: bool,

    /// Separate records with NUL instead of newline.
    #[arg(short = '0', long = "null")]
    pub null: bool,

    /// Keep discovery order instead of sorting by path.
    #[arg(long = "no-sort")]
    pub no_sort: bool,

    /// Suppress warnings about skipped files.
    #[arg(short, long)]
    pub quiet: bool,

    /// Explain why each file was skipped.
    #[arg(short, long)]
    pub verbose: bool,
}

/// How records should be written out.
#[derive(Debug)]
pub enum OutputMode {
    /// A pretty-printed JSON array.
    Json,
    /// One compact JSON object per line.
    Jsonl,
    /// A custom `--format` template.
    Format(FormatString),
}

impl Cli {
    /// Parse `argv`, which must start with the program name.
    ///
    /// # Errors
    ///
    /// Returns the underlying clap error for `--help`, `--version`, or a
    /// usage mistake.
    pub fn parse_from(argv: Vec<OsString>) -> Result<Self, clap::Error> {
        let command = Self::command().after_help(format!("Placeholders: {}", Placeholder::names()));
        let matches = command.try_get_matches_from(argv)?;
        Self::from_arg_matches(&matches)
    }

    /// Resolve the output mode from the flags, last-resort `--format` winning
    /// over the `--long` shortcut.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] when `--format` names an unknown placeholder.
    pub fn output_mode(&self) -> Result<OutputMode, FormatError> {
        if self.json {
            return Ok(OutputMode::Json);
        }
        if self.jsonl {
            return Ok(OutputMode::Jsonl);
        }
        let template = match (&self.format, self.long) {
            (Some(format), _) => format.clone(),
            (None, true) => "{path}\\t{url}".to_owned(),
            (None, false) => "{url}".to_owned(),
        };
        Ok(OutputMode::Format(FormatString::parse(&template)?))
    }
}
