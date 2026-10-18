//! Download YouTube audio with tidy tags and filenames.

pub mod cli;
pub mod cookies;
pub mod error;
pub mod info;
pub mod layout;
pub mod resolve;
pub mod title;
pub mod ytdlp;

use std::ffi::OsString;
use std::io::Write;
use std::os::unix::fs::MetadataExt;

use crate::cli::Cli;
use crate::error::Error;

/// Program name used in diagnostics.
const PROG: &str = "yt-audio";

/// Whether the process runs with effective uid 0.
///
/// `/proc/self` is owned by the process's effective uid.
pub fn is_root() -> bool {
    std::fs::metadata("/proc/self").is_ok_and(|meta| meta.uid() == 0)
}

/// Run the CLI, writing reports to `out` and diagnostics to `err`.
///
/// Returns the process exit code: `0` on success, `1` on a runtime failure,
/// or `2` on a usage error.
pub fn run<I, S>(args: I, out: &mut impl Write, err: &mut impl Write) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut argv = vec![OsString::from(PROG)];
    argv.extend(args.into_iter().map(Into::into));

    let _command = match Cli::parse_from(argv) {
        Ok(command) => command,
        Err(error) => {
            let code = error.exit_code();
            // Help/version go to `out`, usage mistakes to `err`.
            let _ = if error.use_stderr() {
                write!(err, "{}", error.render())
            } else {
                write!(out, "{}", error.render())
            };
            return code;
        }
    };

    if is_root() {
        let _ = writeln!(err, "{PROG}: {}", Error::Root);
        return 1;
    }

    0
}
