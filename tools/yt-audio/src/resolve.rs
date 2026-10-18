//! Resolve a video's tags from YouTube fields, title patterns, an optional
//! LLM and fallbacks.

use std::cell::OnceCell;
use std::fmt;

use crate::info::Video;
use crate::title::{parse_title, split_artists, strip_noise};

/// Where a tag value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Override,
    YouTube,
    /// A title pattern, by name.
    Pattern(&'static str),
    Llm,
    Fallback,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Override => f.write_str("override"),
            Self::YouTube => f.write_str("youtube"),
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

/// The tags to write for one track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tags {
    pub artists: Field<Vec<String>>,
    pub title: Field<String>,
    pub album: Field<String>,
    /// The first artist.
    pub album_artist: String,
    /// `YYYY-MM-DD` or `YYYY`.
    pub date: Option<String>,
    /// The video's URL.
    pub comment: String,
    pub track_number: Option<u32>,
}

/// User-supplied values that outrank everything else.
#[derive(Default)]
pub struct Overrides {
    pub artists: Vec<String>,
    pub title: Option<String>,
    pub album: Option<String>,
}

/// What an LLM suggested for a video.
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

/// Format `YYYYMMDD` as `YYYY-MM-DD`.
fn date_of(v: &Video) -> Option<String> {
    let full = |d: &str| {
        (d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit()))
            .then(|| format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..]))
    };
    v.release_date
        .as_deref()
        .and_then(full)
        .or_else(|| v.release_year.map(|y| y.to_string()))
        .or_else(|| v.upload_date.as_deref().and_then(full))
}

/// An LLM callback: guess tags for a video, or `None` on failure.
pub type LlmFn<'a> = dyn Fn(&Video) -> Option<Guess> + 'a;

/// Resolve tags for `v`.
///
/// Each field takes the first of: override, YouTube, LLM (only if
/// `llm_always`), title pattern, LLM (only for a field still unresolved; the
/// callback runs at most once), fallback.
pub fn resolve(v: &Video, o: &Overrides, llm: Option<&LlmFn<'_>>, llm_always: bool) -> Tags {
    let cell = OnceCell::new();
    let guess = || cell.get_or_init(|| llm.and_then(|f| f(v))).as_ref();
    let pattern = parse_title(&v.title);
    let pat = pattern.as_ref();

    let artists = names(&o.artists)
        .map(|a| (a, Source::Override))
        .or_else(|| {
            let yt = v.artists.as_deref().and_then(names);
            yt.or_else(|| names(&[v.artist.clone().unwrap_or_default()]))
                .map(|a| (a, Source::YouTube))
        })
        .or_else(|| {
            let g = || {
                guess()
                    .and_then(|g| names(&g.artists))
                    .map(|a| (a, Source::Llm))
            };
            let p = || {
                let p = pat?;
                Some((
                    names(std::slice::from_ref(&p.artist))?,
                    Source::Pattern(p.pattern),
                ))
            };
            if llm_always {
                g().or_else(p)
            } else {
                p().or_else(g)
            }
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
        .or_else(|| text(v.track.as_deref()).map(|t| (t, Source::YouTube)))
        .or_else(|| {
            let g = || {
                guess()
                    .and_then(|g| text(g.title.as_deref()))
                    .map(|t| (t, Source::Llm))
            };
            let p = || pat.map(|p| (p.title.clone(), Source::Pattern(p.pattern)));
            if llm_always {
                g().or_else(p)
            } else {
                p().or_else(g)
            }
        })
        .unwrap_or_else(|| (strip_noise(&v.title), Source::Fallback));

    let album = text(o.album.as_deref())
        .map(|a| (a, Source::Override))
        .or_else(|| text(v.album.as_deref()).map(|a| (a, Source::YouTube)))
        // Never triggers an LLM call by itself.
        .or_else(|| {
            cell.get()?
                .as_ref()
                .and_then(|g| text(g.album.as_deref()))
                .map(|a| (a, Source::Llm))
        })
        .unwrap_or_else(|| title.clone());

    Tags {
        album_artist: artists.0.first().cloned().unwrap_or_default(),
        artists: Field {
            value: artists.0,
            source: artists.1,
        },
        title: Field {
            value: title.0,
            source: title.1,
        },
        album: Field {
            value: album.0,
            source: album.1,
        },
        date: date_of(v),
        comment: v.webpage_url.clone(),
        track_number: v.track_number,
    }
}
