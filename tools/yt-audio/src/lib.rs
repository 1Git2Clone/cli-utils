//! Download audio with yt-dlp, with tidy tags and filenames.

pub mod cli;
pub mod cookies;
pub mod error;
pub mod info;
pub mod layout;
#[cfg(feature = "llm")]
pub mod llm;
pub mod resolve;
pub mod tags;
pub mod title;
pub mod ytdlp;

use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use futures_util::{StreamExt, stream};
use url::Url;

use crate::cli::{AudioFormat, Cli};
use crate::cookies::{BrowserOrder, CookieChoice, Cookies};
use crate::error::Error;
use crate::info::{Listing, Video};
use crate::layout::{destination, sanitize};
use crate::resolve::{Guess, Overrides, Tags};

/// Program name used in diagnostics.
const PROG: &str = "yt-audio";

/// Whether the process runs with effective uid 0.
///
/// `/proc/self` is owned by the process's effective uid.
pub fn is_root() -> bool {
    std::fs::metadata("/proc/self").is_ok_and(|meta| meta.uid() == 0)
}

/// One video to fetch, and where its file goes.
struct Entry {
    url: Url,
    base: PathBuf,
    /// Straight into `base` (playlists) rather than an album folder.
    flat: bool,
}

/// An entry with its metadata and resolved tags.
struct Resolved {
    entry: Entry,
    video: Video,
    tags: Tags,
}

/// What happened to one entry.
enum Outcome {
    Downloaded(PathBuf),
    Skipped(PathBuf),
    /// `--dry-run`: the report block to print.
    Planned(String),
}

/// Per-run settings shared by every entry.
struct Ctx<'a, W> {
    cli: &'a Cli,
    cookies: CookieChoice,
    ytdlp: &'a OsStr,
    overrides: Overrides,
    /// Diagnostics; entries run concurrently on one thread and only borrow it
    /// between awaits.
    err: RefCell<&'a mut W>,
    #[cfg(feature = "llm")]
    llm: Option<(llm::Guesser, bool)>,
}

impl<W: Write> Ctx<'_, W> {
    /// Print one diagnostic line.
    fn say(&self, msg: std::fmt::Arguments) {
        let _ = writeln!(self.err.borrow_mut(), "{PROG}: {msg}");
    }

    /// A warning or progress line, silenced by `-q`.
    fn note(&self, msg: std::fmt::Arguments) {
        if !self.cli.quiet {
            self.say(msg);
        }
    }

    /// Run yt-dlp, echoing the command under `-v` (never any cookie contents).
    async fn ytdlp(&self, args: &[String]) -> Result<String, Error> {
        if self.cli.verbose {
            self.say(format_args!(
                "{} {}",
                self.ytdlp.to_string_lossy(),
                args.join(" ")
            ));
        }
        ytdlp::run(self.ytdlp, args, None).await
    }
}

/// Run the CLI, writing reports to `out` and diagnostics to `err`.
///
/// Returns the process exit code: `0` on success, `1` on a runtime failure,
/// or `2` on a usage error.
pub async fn run<I, S>(args: I, out: &mut impl Write, err: &mut impl Write) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    run_with(args, out, err, OsStr::new("yt-dlp")).await
}

/// Like [`run`], but with the yt-dlp program to execute (for tests).
pub async fn run_with<I, S, W: Write>(
    args: I,
    out: &mut impl Write,
    err: &mut W,
    ytdlp: &OsStr,
) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut command_line = vec![OsString::from(PROG)];
    command_line.extend(args.into_iter().map(Into::into));

    let cli = match Cli::parse_from(command_line) {
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

    let cookies = if cli.no_cookies {
        CookieChoice::Fixed(Cookies::None)
    } else if let Some(spec) = &cli.cookies_from_browser {
        CookieChoice::Fixed(Cookies::Browser(spec.clone()))
    } else {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let choice = match home {
            Some(home) => {
                cookies::detect(&home, ytdlp, &cli.urls[0], &BrowserOrder::default()).await
            }
            None => CookieChoice::Fixed(Cookies::None),
        };
        if (choice.is_empty() || matches!(choice, CookieChoice::Fixed(_))) && !cli.quiet {
            let _ = writeln!(
                err,
                "{PROG}: no browser has cookies for these sites; continuing without cookies"
            );
        }
        choice
    };
    #[cfg(feature = "llm")]
    let llm = llm::LlmConfig::from_cli(&cli).and_then(|c| match llm::Guesser::new(&c) {
        Ok(g) => Some((g, c.always)),
        Err(error) => {
            let _ = writeln!(err, "{PROG}: llm disabled: {error}");
            None
        }
    });
    let ctx = Ctx {
        cli: &cli,
        cookies,
        ytdlp,
        overrides: Overrides {
            artists: cli.artist.clone(),
            title: cli.title.clone(),
            album: cli.album.clone(),
        },
        err: RefCell::new(err),
        #[cfg(feature = "llm")]
        llm,
    };
    process(&ctx, out).await
}

/// List, resolve, place and download every entry; returns the exit code.
async fn process<W: Write>(ctx: &Ctx<'_, W>, out: &mut impl Write) -> i32 {
    let jobs = ctx.cli.jobs.get();
    let (mut total, mut failed) = (0, 0);

    let mut entries = Vec::new();
    let mut listings = stream::iter(&ctx.cli.urls)
        .map(|url| async move { (url, list(url, ctx).await) })
        .buffered(jobs);
    while let Some((url, listed)) = listings.next().await {
        match listed {
            Ok((found, missing)) => {
                if missing > 0 {
                    ctx.say(format_args!("{url}: {missing} entries have no URL"));
                }
                total += missing;
                failed += missing;
                entries.extend(found);
            }
            Err(error) => {
                ctx.say(format_args!("{url}: {error}"));
                total += 1;
                failed += 1;
            }
        }
    }
    total += entries.len();

    // Resolve concurrently, then place in order so " (2)" numbering is stable.
    let mut resolved = Vec::new();
    let mut fetches = stream::iter(entries)
        .map(|e| resolve_entry(e, ctx))
        .buffered(jobs);
    while let Some(r) = fetches.next().await {
        match r {
            Ok(r) => resolved.push(r),
            Err(entry_error) => {
                let (url, error) = *entry_error;
                ctx.say(format_args!("{url}: {error}"));
                failed += 1;
            }
        }
    }
    let mut seen = HashSet::new();
    let placed: Vec<(PathBuf, Resolved)> = resolved
        .into_iter()
        .map(|r| (place(&r, &mut seen), r))
        .collect();

    let mut planned = 0;
    let mut finishes = stream::iter(placed)
        .map(|(dest, r)| async move {
            let url = r.entry.url.clone();
            (url, finish(dest, r, ctx).await)
        })
        .buffered(jobs);
    while let Some((url, outcome)) = finishes.next().await {
        match outcome {
            Ok(Outcome::Planned(block)) => {
                if planned > 0 {
                    let _ = writeln!(out);
                }
                planned += 1;
                let _ = write!(out, "{block}");
            }
            Ok(Outcome::Downloaded(path)) => {
                ctx.note(format_args!("wrote {}", path.display()));
            }
            Ok(Outcome::Skipped(path)) => {
                ctx.note(format_args!("skipped (exists): {}", path.display()));
            }
            Err(error) => {
                ctx.say(format_args!("{url}: {error}"));
                failed += 1;
            }
        }
    }

    if failed > 0 {
        ctx.say(format_args!("{failed} of {total} entries failed"));
        return 1;
    }
    0
}

/// Expand a CLI URL into its entries, plus how many had no URL.
async fn list<W: Write>(url: &Url, ctx: &Ctx<'_, W>) -> Result<(Vec<Entry>, usize), Error> {
    let args = ytdlp::listing_args(url, &ctx.cookies.for_url(url));
    let output_dir = ctx.cli.output_dir.clone();
    let root = ctx.cli.root.clone().unwrap_or_else(|| ".".into());
    Ok(match info::parse_listing(&ctx.ytdlp(&args).await?)? {
        Listing::Playlist {
            title,
            entries,
            missing,
        } => {
            let base = output_dir.unwrap_or_else(|| root.join(sanitize(&title)));
            let entries = entries
                .into_iter()
                .map(|url| Entry {
                    url,
                    base: base.clone(),
                    flat: true,
                })
                .collect();
            (entries, missing)
        }
        Listing::Single(canonical) => {
            let entry = Entry {
                url: canonical.unwrap_or_else(|| url.clone()),
                base: output_dir.unwrap_or(root),
                flat: false,
            };
            (vec![entry], 0)
        }
    })
}

/// Fetch an entry's metadata and resolve its tags.
async fn resolve_entry<W: Write>(
    entry: Entry,
    ctx: &Ctx<'_, W>,
) -> Result<Resolved, Box<(Url, Error)>> {
    let args = ytdlp::info_args(&entry.url, &ctx.cookies.for_url(&entry.url));
    let video = match ctx.ytdlp(&args).await.and_then(|j| info::parse_video(&j)) {
        Ok(video) => video,
        Err(error) => return Err(Box::new((entry.url, error))),
    };
    let (guess, always) = guess(&video, ctx).await;
    let tags = resolve::resolve(&video, &ctx.overrides, guess.as_ref(), always);
    Ok(Resolved { entry, video, tags })
}

/// The LLM's guess when one is configured and needed, and whether it
/// outranks title patterns. A failed call is a warning, not an entry failure.
#[cfg(feature = "llm")]
async fn guess<W: Write>(v: &Video, ctx: &Ctx<'_, W>) -> (Option<Guess>, bool) {
    let Some((guesser, always)) = &ctx.llm else {
        return (None, false);
    };
    if !resolve::needs_llm(v, &ctx.overrides, *always) {
        return (None, *always);
    }
    match guesser.guess(v).await {
        Ok(g) => (Some(g), *always),
        Err(error) => {
            ctx.note(format_args!("{}: llm: {error}", v.webpage_url));
            (None, *always)
        }
    }
}

#[cfg(not(feature = "llm"))]
async fn guess<W: Write>(_: &Video, _: &Ctx<'_, W>) -> (Option<Guess>, bool) {
    (None, false)
}

/// The destination (without extension) for `r`; entries sharing one within a
/// run get " (2)", " (3)", ... in listing order.
fn place(r: &Resolved, seen: &mut HashSet<PathBuf>) -> PathBuf {
    let mut title = r.tags.title.value.clone();
    let mut n = 1;
    loop {
        let dest = if r.entry.flat {
            r.entry.base.join(sanitize(&title))
        } else {
            destination(&r.entry.base, &r.tags.album.value, &title)
        };
        if seen.insert(dest.clone()) {
            return dest;
        }
        n += 1;
        title = format!("{} ({n})", r.tags.title.value);
    }
}

/// Skip, plan, or download and tag one placed entry.
async fn finish<W: Write>(dest: PathBuf, r: Resolved, ctx: &Ctx<'_, W>) -> Result<Outcome, Error> {
    let cli = ctx.cli;
    if !cli.force
        && let Some(existing) = existing(&dest, cli.fmt)
    {
        return Ok(Outcome::Skipped(existing));
    }
    if cli.dry_run {
        return Ok(Outcome::Planned(report(&dest, cli.fmt, &r.tags)));
    }

    let cookies = ctx.cookies.for_url(&r.entry.url);
    let args = ytdlp::download_args(
        &r.entry.url,
        &dest,
        cli.fmt,
        cli.force,
        &cookies,
        &cli.ytdlp_args,
    );
    let stdout = ctx.ytdlp(&args).await?;
    let path = stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(PathBuf::from)
        .ok_or(Error::NoOutputPath)?;
    let written = path.clone();
    tokio::task::spawn_blocking(move || tags::write_tags(&written, &r.tags, &r.video))
        .await
        .map_err(std::io::Error::other)??;
    Ok(Outcome::Downloaded(path))
}

/// A finished audio file `dest.<ext>`, if one exists.
///
/// Only exact final names count, so `.part`, `.webp` and `Song.live.opus`
/// leftovers do not make an entry look done.
fn existing(dest: &Path, fmt: AudioFormat) -> Option<PathBuf> {
    let exts: &[&str] = match fmt {
        AudioFormat::Opus => &["opus"],
        AudioFormat::M4a => &["m4a"],
        AudioFormat::Mp3 => &["mp3"],
        AudioFormat::Best => &[
            "opus", "m4a", "mp3", "webm", "ogg", "aac", "flac", "wav", "mka",
        ],
    };
    exts.iter().find_map(|ext| {
        let mut name = dest.as_os_str().to_owned();
        name.push(".");
        name.push(ext);
        let path = PathBuf::from(name);
        path.is_file().then_some(path)
    })
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
        t.album_artist().unwrap_or_default(),
    )
}
