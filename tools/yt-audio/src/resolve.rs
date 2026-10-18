//! Resolve a video's tags from the site's music fields, title patterns, an
//! optional LLM guess and fallbacks.

use std::fmt;

use crate::info::Video;
use crate::title::{TitleGuess, parse_title, split_artists, strip_noise};

/// Where a tag value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Override,
    /// The site's own music metadata (e.g. `YouTube` Music fields).
    Site,
    /// A title pattern, by name.
    Pattern(&'static str),
    Llm,
    Fallback,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Override => f.write_str("override"),
            Self::Site => f.write_str("site"),
            Self::Pattern(name) => write!(f, "pattern:{name}"),
            Self::Llm => f.write_str("llm"),
            Self::Fallback => f.write_str("fallback"),
        }
    }
}

/// A resolved value and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field<T> {
    pub value: T,
    pub source: Source,
}

impl<T> Field<T> {
    fn new((value, source): (T, Source)) -> Self {
        Self { value, source }
    }
}

/// The tag values resolution decides; everything else is read off the
/// [`Video`] when writing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tags {
    pub artists: Field<Vec<String>>,
    pub title: Field<String>,
    pub album: Field<String>,
}

impl Tags {
    /// The album artist: the first artist.
    pub fn album_artist(&self) -> Option<&str> {
        self.artists.value.first().map(String::as_str)
    }
}

/// User-supplied values that outrank everything else.
#[derive(Default)]
pub struct Overrides {
    pub artists: Vec<String>,
    pub title: Option<String>,
    pub album: Option<String>,
}

/// What an LLM suggested for a video.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Guess {
    pub artists: Vec<String>,
    pub title: Option<String>,
    pub album: Option<String>,
}

/// Split every element on featuring markers and drop duplicates, keeping order.
fn normalize(names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in names.iter().flat_map(|n| split_artists(n)) {
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// `Some` unless `s` is empty or blank.
fn text(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// `Some` unless the list is empty after normalising.
fn names(list: &[String]) -> Option<Vec<String>> {
    Some(normalize(list)).filter(|n| !n.is_empty())
}

fn site_artists(v: &Video) -> Option<Vec<String>> {
    v.artists
        .as_deref()
        .and_then(names)
        .or_else(|| names(&[v.artist.clone()?]))
}

fn pattern_artists(p: Option<&TitleGuess>) -> Option<(Vec<String>, Source)> {
    let p = p?;
    Some((
        names(std::slice::from_ref(&p.artist))?,
        Source::Pattern(p.pattern),
    ))
}

/// The LLM's value before the pattern's when `llm_first`, else after.
fn prefer<T>(llm_first: bool, llm: Option<T>, pattern: Option<T>) -> Option<T> {
    if llm_first {
        llm.or(pattern)
    } else {
        pattern.or(llm)
    }
}

/// Whether [`resolve`] would consult an LLM guess for `v`: some field the
/// overrides and site leave open that patterns cannot fill (or `llm_always`).
pub fn needs_llm(v: &Video, o: &Overrides, llm_always: bool) -> bool {
    let pattern = parse_title(&v.title);
    let artists_open = names(&o.artists).is_none() && site_artists(v).is_none();
    let title_open = text(o.title.as_deref()).is_none() && text(v.track.as_deref()).is_none();
    let patterns_fill = |open: bool, filled: bool| open && (llm_always || !filled);
    patterns_fill(artists_open, pattern_artists(pattern.as_ref()).is_some())
        || patterns_fill(title_open, pattern.is_some())
}

/// Resolve tags for `v`.
///
/// Each field takes the first of: override, site, LLM guess (only if
/// `llm_always`), title pattern, LLM guess, fallback. Get `guess` only when
/// [`needs_llm`] says so; an album guess is used only after site metadata.
pub fn resolve(v: &Video, o: &Overrides, guess: Option<&Guess>, llm_always: bool) -> Tags {
    let pattern = parse_title(&v.title);

    let artists = names(&o.artists)
        .map(|a| (a, Source::Override))
        .or_else(|| site_artists(v).map(|a| (a, Source::Site)))
        .or_else(|| {
            prefer(
                llm_always,
                guess
                    .and_then(|g| names(&g.artists))
                    .map(|a| (a, Source::Llm)),
                pattern_artists(pattern.as_ref()),
            )
        })
        .unwrap_or_else(|| {
            let ch = v.channel.as_deref().or(v.uploader.as_deref()).unwrap_or("");
            let ch = ch.strip_suffix(" - Topic").unwrap_or(ch);
            (
                names(&[ch.to_string()]).unwrap_or_default(),
                Source::Fallback,
            )
        });

    let title = text(o.title.as_deref())
        .map(|t| (t, Source::Override))
        .or_else(|| text(v.track.as_deref()).map(|t| (t, Source::Site)))
        .or_else(|| {
            prefer(
                llm_always,
                guess
                    .and_then(|g| text(g.title.as_deref()))
                    .map(|t| (t, Source::Llm)),
                pattern
                    .as_ref()
                    .map(|p| (p.title.clone(), Source::Pattern(p.pattern))),
            )
        })
        .unwrap_or_else(|| (strip_noise(&v.title), Source::Fallback));

    let album = text(o.album.as_deref())
        .map(|a| (a, Source::Override))
        .or_else(|| text(v.album.as_deref()).map(|a| (a, Source::Site)))
        .or_else(|| {
            guess
                .and_then(|g| text(g.album.as_deref()))
                .map(|a| (a, Source::Llm))
        })
        .unwrap_or_else(|| title.clone());

    Tags {
        artists: Field::new(artists),
        title: Field::new(title),
        album: Field::new(album),
    }
}
