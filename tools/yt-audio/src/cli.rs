//! Command-line interface definition.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Parser, ValueEnum};

/// Audio container/codec to extract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum AudioFormat {
    /// Opus in an Ogg container.
    #[default]
    Opus,
    /// AAC in an MP4 container.
    M4a,
    /// MP3.
    Mp3,
    /// Whatever the best source stream is, without re-encoding.
    Best,
}

/// Download YouTube audio with tidy tags and filenames.
#[derive(Debug, Parser)]
#[command(name = "yt-audio", version)]
pub struct Cli {
    /// Video, playlist or channel URLs.
    #[arg(value_name = "URL", required = true, num_args = 1..)]
    pub urls: Vec<String>,

    /// Extra arguments passed to yt-dlp verbatim (after `--`).
    #[arg(last = true, value_name = "YTDLP_ARGS")]
    pub ytdlp_args: Vec<String>,

    /// Read cookies from this browser (e.g. firefox).
    #[arg(long, value_name = "BROWSER")]
    pub cookies_from_browser: Option<String>,

    /// Do not use any cookies.
    #[arg(long, conflicts_with = "cookies_from_browser")]
    pub no_cookies: bool,

    /// Audio format to extract.
    #[arg(long, value_enum, default_value_t)]
    pub fmt: AudioFormat,

    /// Directory to write into; defaults to the current one.
    #[arg(short, long, value_name = "DIR")]
    pub output_dir: Option<PathBuf>,

    /// Override the artist tag (repeatable).
    #[arg(long)]
    pub artist: Vec<String>,

    /// Override the title tag.
    #[arg(long)]
    pub title: Option<String>,

    /// Override the album tag.
    #[arg(long)]
    pub album: Option<String>,

    /// Re-download files that already exist.
    #[arg(long)]
    pub force: bool,

    /// Print what would be done without downloading.
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Suppress progress and warnings.
    #[arg(short, long)]
    pub quiet: bool,

    /// Print extra detail.
    #[arg(short, long)]
    pub verbose: bool,

    /// LLM backend: `openai` means any OpenAI-compatible endpoint.
    #[cfg(feature = "llm")]
    #[arg(long, value_enum, env = "YT_AUDIO_LLM_BACKEND", default_value_t)]
    pub llm_backend: LlmBackend,

    /// LLM endpoint base URL.
    #[cfg(feature = "llm")]
    #[arg(long, env = "YT_AUDIO_LLM_URL")]
    pub llm_url: Option<String>,

    /// LLM model name; the LLM is used only when this is set.
    #[cfg(feature = "llm")]
    #[arg(long, env = "YT_AUDIO_LLM_MODEL")]
    pub llm_model: Option<String>,

    /// Ask the LLM for every entry, outranking title patterns.
    #[cfg(feature = "llm")]
    #[arg(long)]
    pub llm_always: bool,
}

/// Which kind of LLM endpoint to talk to.
#[cfg(feature = "llm")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum LlmBackend {
    /// A local or remote Ollama server.
    #[default]
    Ollama,
    /// Any OpenAI-compatible endpoint.
    Openai,
}

impl Cli {
    /// Parse `argv`, which must start with the program name.
    ///
    /// # Errors
    ///
    /// Returns the underlying clap error for `--help`, `--version`, or a
    /// usage mistake.
    pub fn parse_from(argv: Vec<OsString>) -> Result<Self, clap::Error> {
        <Self as Parser>::try_parse_from(argv)
    }
}
