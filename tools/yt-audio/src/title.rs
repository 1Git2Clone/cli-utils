//! Pull an artist and title out of a YouTube video title.

/// Bracketed text that is promotional noise when it is exactly one of these
/// (case-insensitive). `official ...` and `mv`/`pv` prefixes are handled in
/// [`is_noise`].
const NOISE: &[&str] = &[
    "mv",
    "pv",
    "lyric video",
    "lyrics",
    "audio",
    "music video",
    "4k",
    "hd",
];

/// Bracket pairs that may hold noise tags.
const BRACKETS: &[(char, char)] = &[('【', '】'), ('[', ']'), ('(', ')')];

/// An artist and title guessed from a video title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleGuess {
    /// The guessed artist (may still hold several names; see [`split_artists`]).
    pub artist: String,
    /// The guessed track title.
    pub title: String,
    /// Name of the pattern that matched.
    pub pattern: &'static str,
}

/// Whether bracket contents are a promotional tag.
fn is_noise(inner: &str) -> bool {
    let lower = inner.trim().to_lowercase();
    NOISE.contains(&lower.as_str())
        || lower.starts_with("official ")
        || ["mv", "pv"].iter().any(|p| {
            lower
                .strip_prefix(p)
                .is_some_and(|r| r.chars().next().is_some_and(|c| !c.is_alphanumeric()))
        })
}

/// Remove noise tags such as `【MV】` or `(Official Video)` and trim.
///
/// Brackets with any other content are kept.
pub fn strip_noise(title: &str) -> String {
    let mut out = String::new();
    let mut rest = title;
    while let Some(c) = rest.chars().next() {
        let group = BRACKETS
            .iter()
            .find(|(open, _)| *open == c)
            .and_then(|&(open, close)| {
                let inner = &rest[open.len_utf8()..];
                let end = inner.find(close)?;
                is_noise(&inner[..end]).then_some(open.len_utf8() + end + close.len_utf8())
            });
        match group {
            Some(len) => rest = &rest[len..],
            None => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Strip noise, then try each title pattern in order.
pub fn parse_title(title: &str) -> Option<TitleGuess> {
    let title = strip_noise(title);
    let patterns: [(&'static str, fn(&str) -> Option<(String, String)>); 3] = [
        ("artist-dash", artist_dash),
        ("bracket-quote", bracket_quote),
        ("title-slash-artist", title_slash_artist),
    ];
    patterns.iter().find_map(|(pattern, f)| {
        f(&title).map(|(artist, title)| TitleGuess {
            artist,
            title,
            pattern,
        })
    })
}

/// Trim both sides and require them to be non-empty.
fn pair(artist: &str, title: &str) -> Option<(String, String)> {
    let (artist, title) = (artist.trim(), title.trim());
    (!artist.is_empty() && !title.is_empty()).then(|| (artist.into(), title.into()))
}

/// Split `「Title」tail` / `『Title』tail` into the title and a featured-artist
/// suffix (`" feat. X"`, or empty). Other trailing text is dropped.
fn quoted(s: &str) -> Option<(&str, String)> {
    let first = s.chars().next()?;
    let close = match first {
        '「' => '」',
        '『' => '』',
        _ => return None,
    };
    let (title, tail) = s[first.len_utf8()..].split_once(close)?;
    let tail = tail.trim();
    let (word, rest) = tail.split_once(char::is_whitespace).unwrap_or((tail, ""));
    let feat = ["feat.", "ft.", "feat", "ft", "featuring"].contains(&word.to_lowercase().as_str());
    let suffix = if feat && !rest.trim().is_empty() {
        format!(" feat. {}", rest.trim())
    } else {
        String::new()
    };
    Some((title, suffix))
}

/// `Artist - Title`, splitting on the first spaced dash.
fn artist_dash(s: &str) -> Option<(String, String)> {
    let (at, len) = [" - ", " – ", " — "]
        .iter()
        .filter_map(|d| s.find(d).map(|i| (i, d.len())))
        .min()?;
    let (artist, title) = (s[..at].trim(), s[at + len..].trim());
    match quoted(title) {
        Some((t, suffix)) => pair(&format!("{artist}{suffix}"), t),
        None => pair(artist, title),
    }
}

/// `Artist「Title」…`; trailing text is dropped except a `feat.` credit.
fn bracket_quote(s: &str) -> Option<(String, String)> {
    let at = s.find(['「', '『'])?;
    let (title, suffix) = quoted(&s[at..])?;
    pair(&format!("{}{suffix}", s[..at].trim()), title)
}

/// `Title / Artist`, splitting on the last spaced slash.
fn title_slash_artist(s: &str) -> Option<(String, String)> {
    let (title, artist) = s.rsplit_once(" / ")?;
    pair(artist, title)
}

/// Split an artist string on `feat`/`ft`/`featuring` and spaced `/`.
///
/// Splits only on whole words (a `.` ends the word, and a bracketed
/// `(feat. X)` group counts), so `Daft Punk` stays whole; `&`, `x` and commas
/// are left alone because they appear in real band names.
pub fn split_artists(s: &str) -> Vec<String> {
    let mut names = vec![Vec::new()];
    let mut in_group = false;
    for word in s.split_whitespace() {
        let bare = word.trim_start_matches(['(', '[', '（', '【']);
        let opened = bare.len() != word.len();
        let lower = bare.to_lowercase();
        let keyword = ["featuring", "feat", "ft"].iter().find_map(|k| {
            let r = lower.strip_prefix(k)?;
            (r.is_empty() || r.starts_with('.')).then(|| &bare[k.len()..])
        });
        let mut piece = word;
        if word == "/" || keyword.is_some() {
            names.push(Vec::new());
            piece = keyword.map_or("", |r| r.strip_prefix('.').unwrap_or(r));
            in_group = opened;
        }
        if in_group && piece.ends_with([')', ']', '）', '】']) {
            let last = piece.chars().next_back().map_or(0, char::len_utf8);
            piece = &piece[..piece.len() - last];
            in_group = false;
        }
        if !piece.is_empty() {
            names.last_mut().expect("never empty").push(piece);
        }
    }
    names
        .into_iter()
        .filter(|w| !w.is_empty())
        .map(|w| w.join(" "))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guess(t: &str) -> (String, String, &'static str) {
        let g = parse_title(t).unwrap();
        (g.artist, g.title, g.pattern)
    }

    #[test]
    fn strips_noise_only() {
        assert_eq!(strip_noise("アリバイ 【MV】"), "アリバイ");
        assert_eq!(strip_noise("Song (Official Video) [4K]"), "Song");
        assert_eq!(strip_noise("Song (Remix)"), "Song (Remix)");
    }

    #[test]
    fn dash() {
        assert_eq!(
            guess("Halozy - Lucky 7"),
            ("Halozy".into(), "Lucky 7".into(), "artist-dash")
        );
        assert_eq!(guess("かめりあ - 「アリバイ」").1, "アリバイ");
    }

    #[test]
    fn bracket_quote_drops_tail() {
        assert_eq!(
            guess("かめりあ「アリバイ」feat. 初音ミク"),
            (
                "かめりあ feat. 初音ミク".into(),
                "アリバイ".into(),
                "bracket-quote"
            )
        );
    }

    #[test]
    fn slash() {
        assert_eq!(
            guess("ロキ / みきとP"),
            ("みきとP".into(), "ロキ".into(), "title-slash-artist")
        );
    }

    #[test]
    fn no_match() {
        assert_eq!(parse_title("AC/DC Back In Black"), None);
        assert_eq!(parse_title("just a title"), None);
    }

    #[test]
    fn artists() {
        assert_eq!(
            split_artists("かめりあ feat. 初音ミク"),
            ["かめりあ", "初音ミク"]
        );
        assert_eq!(split_artists("A ft B"), ["A", "B"]);
        assert_eq!(split_artists("A Featuring B / C"), ["A", "B", "C"]);
        assert_eq!(split_artists("Simon & Garfunkel"), ["Simon & Garfunkel"]);
        assert_eq!(split_artists("A x B, C"), ["A x B, C"]);
        assert_eq!(split_artists("Daft Punk"), ["Daft Punk"]);
    }

    #[test]
    fn dash_needs_spaces() {
        assert_eq!(guess("Jay-Z - Empire").0, "Jay-Z");
        assert_eq!(guess("Jay-Z - Empire").1, "Empire");
        assert_eq!(parse_title("Back-to-Black"), None);
    }

    #[test]
    fn quoted_title_keeps_feat_tail() {
        assert_eq!(
            guess("かめりあ - 「アリバイ」 feat. 初音ミク"),
            (
                "かめりあ feat. 初音ミク".into(),
                "アリバイ".into(),
                "artist-dash"
            )
        );
        assert_eq!(guess("かめりあ - 「アリバイ」 remix").0, "かめりあ");
    }

    #[test]
    fn bracketed_and_tight_feat() {
        assert_eq!(split_artists("A (feat. B)"), ["A", "B"]);
        assert_eq!(split_artists("A feat.B"), ["A", "B"]);
        assert_eq!(split_artists("A FEAT. B"), ["A", "B"]);
    }

    #[test]
    fn noise_needs_word_boundary() {
        assert_eq!(strip_noise("Song (HDR edit)"), "Song (HDR edit)");
        assert_eq!(strip_noise("Song (Audio Drama)"), "Song (Audio Drama)");
        assert_eq!(strip_noise("Song 【MV/Full】"), "Song");
    }
}
