//! Download YouTube audio with tidy tags and filenames.

pub mod cli;
pub mod cookies;
pub mod error;
pub mod info;
pub mod layout;
pub mod resolve;
pub mod tags;
pub mod title;
pub mod ytdlp;

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::cli::{AudioFormat, Cli};
use crate::cookies::Cookies;
use crate::error::Error;
use crate::info::Listing;
use crate::layout::{destination, sanitize};
use crate::resolve::{Overrides, Tags};

/// Program name used in diagnostics.
const PROG: &str = "yt-audio";

/// Whether the process runs with effective uid 0.
///
/// `/proc/self` is owned by the process's effective uid.
pub fn is_root() -> bool {
    std::fs::metadata("/proc/self").is_ok_and(|meta| meta.uid() == 0)
}

/// What happened to one entry.
enum Outcome {
    Downloaded(PathBuf),
    Skipped(PathBuf),
    /// `--dry-run`: the report block to print.
    Planned(String),
}

/// Per-run settings shared by every entry.
struct Ctx<'a> {
    cli: &'a Cli,
    cookies: Cookies,
    ytdlp: &'a OsStr,
    overrides: Overrides,
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
    run_with(args, out, err, OsStr::new("yt-dlp"))
}

/// Like [`run`], but with the yt-dlp program to execute (for tests).
pub fn run_with<I, S>(args: I, out: &mut impl Write, err: &mut impl Write, ytdlp: &OsStr) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut argv = vec![OsString::from(PROG)];
    argv.extend(args.into_iter().map(Into::into));

    let cli = match Cli::parse_from(argv) {
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

    let cookies = if cli.no_cookies {
        Cookies::None
    } else if let Some(spec) = &cli.cookies_from_browser {
        Cookies::Browser(spec.clone())
    } else {
        let found = std::env::var_os("HOME").map_or(Cookies::None, |home| {
            cookies::detect(Path::new(&home), ytdlp)
        });
        if found == Cookies::None && !cli.quiet {
            let _ = writeln!(
                err,
                "{PROG}: no browser with a YouTube login found; continuing without cookies"
            );
        }
        found
    };
    let ctx = Ctx {
        cli: &cli,
        cookies,
        ytdlp,
        overrides: Overrides {
            artists: cli.artist.clone(),
            title: cli.title.clone(),
            album: cli.album.clone(),
        },
    };

    let (mut total, mut failed, mut planned) = (0, 0, 0);
    for url in &cli.urls {
        let entries = match list(url, &ctx) {
            Ok(entries) => entries,
            Err(error) => {
                let _ = writeln!(err, "{PROG}: {url}: {error}");
                total += 1;
                failed += 1;
                continue;
            }
        };
        for (entry, base, flat) in entries {
            total += 1;
            match process_entry(&entry, &base, flat, &ctx) {
                Ok(Outcome::Planned(block)) => {
                    if planned > 0 {
                        let _ = writeln!(out);
                    }
                    planned += 1;
                    let _ = write!(out, "{block}");
                }
                Ok(Outcome::Downloaded(path)) if !cli.quiet => {
                    let _ = writeln!(err, "{PROG}: wrote {}", path.display());
                }
                Ok(Outcome::Skipped(path)) if !cli.quiet => {
                    let _ = writeln!(err, "{PROG}: skipped (exists): {}", path.display());
                }
                Ok(_) => {}
                Err(error) => {
                    let _ = writeln!(err, "{PROG}: {entry}: {error}");
                    failed += 1;
                }
            }
        }
    }

    if failed > 0 {
        let _ = writeln!(err, "{PROG}: {failed} of {total} entries failed");
        return 1;
    }
    0
}

/// Expand a CLI URL into `(entry url, base dir, flat)` triples.
///
/// `flat` entries are written straight into the base dir (playlists);
/// otherwise they get an album folder (single videos).
fn list(url: &str, ctx: &Ctx) -> Result<Vec<(String, PathBuf, bool)>, Error> {
    let json = ytdlp_run(ctx, &ytdlp::listing_args(url, &ctx.cookies))?;
    Ok(match info::parse_listing(&json)? {
        Listing::Playlist { title, entries } => {
            let base = ctx
                .cli
                .output_dir
                .clone()
                .unwrap_or_else(|| Path::new(".").join(sanitize(&title)));
            entries
                .into_iter()
                .map(|e| (e, base.clone(), true))
                .collect()
        }
        Listing::Single(video) => {
            let base = ctx.cli.output_dir.clone().unwrap_or_else(|| ".".into());
            // The listing gave the canonical URL; fall back to what was typed.
            let video = if video.is_empty() {
                url.to_string()
            } else {
                video
            };
            vec![(video, base, false)]
        }
    })
}

/// Run yt-dlp, echoing the command under `-v` (never any cookie contents).
fn ytdlp_run(ctx: &Ctx, args: &[String]) -> Result<String, Error> {
    if ctx.cli.verbose {
        eprintln!("{PROG}: {} {}", ctx.ytdlp.to_string_lossy(), args.join(" "));
    }
    ytdlp::run(ctx.ytdlp, args)
}

/// Resolve, then plan, skip or download and tag one entry.
fn process_entry(url: &str, base: &Path, flat: bool, ctx: &Ctx) -> Result<Outcome, Error> {
    let cli = ctx.cli;
    let video = info::parse_video(&ytdlp_run(ctx, &ytdlp::info_args(url, &ctx.cookies))?)?;
    // ponytail: no LLM guesser yet; the `llm` feature passes one here.
    let tags = resolve::resolve(&video, &ctx.overrides, None, false);
    let dest = if flat {
        base.join(sanitize(&tags.title.value))
    } else {
        destination(base, &tags.album.value, &tags.title.value)
    };

    if !cli.force
        && let Some(existing) = existing(&dest)
    {
        return Ok(Outcome::Skipped(existing));
    }
    if cli.dry_run {
        return Ok(Outcome::Planned(report(&dest, cli.fmt, &tags)));
    }

    let args = ytdlp::download_args(
        url,
        &dest,
        cli.fmt,
        cli.force,
        &ctx.cookies,
        &cli.ytdlp_args,
    );
    let stdout = ytdlp_run(ctx, &args)?;
    let path = stdout
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| Error::Io(std::io::Error::other("yt-dlp printed no output path")))?;
    let path = PathBuf::from(path.trim());
    tags::write_tags(&path, &tags)?;
    Ok(Outcome::Downloaded(path))
}

/// A file named `dest.<anything>`, if one exists.
fn existing(dest: &Path) -> Option<PathBuf> {
    let stem = format!("{}.", dest.file_name()?.to_string_lossy());
    std::fs::read_dir(dest.parent()?)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with(&stem))
        .map(|e| e.path())
}

/// The `--dry-run` block for one entry.
fn report(dest: &Path, fmt: AudioFormat, t: &Tags) -> String {
    let ext = match fmt {
        AudioFormat::Opus => "opus",
        AudioFormat::M4a => "m4a",
        AudioFormat::Mp3 => "mp3",
        AudioFormat::Best => "*",
    };
    format!(
        "{}.{ext}\n  title:        {}  ({})\n  artists:      {}  ({})\n  album:        {}  ({})\n  album_artist: {}\n",
        dest.display(),
        t.title.value,
        t.title.source,
        t.artists.value.join("; "),
        t.artists.source,
        t.album.value,
        t.album.source,
        t.album_artist,
    )
}
