//! Pull an artist and title out of a YouTube video title.

/// Bracketed text that is promotional noise, matched case-insensitively as a
/// prefix of the bracket's contents.
const NOISE: &[&str] = &[
    "mv",
    "pv",
    "official video",
    "official music video",
    "official audio",
    "official lyric video",
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
                let lower = inner[..end].trim().to_lowercase();
                let noisy = NOISE.iter().any(|n| lower.starts_with(n));
                noisy.then_some(open.len_utf8() + end + close.len_utf8())
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

/// `Artist - Title`, splitting on the first dash.
fn artist_dash(s: &str) -> Option<(String, String)> {
    let (artist, title) = s.split_once(['-', '–', '—'])?;
    let (artist, title) = (artist.trim(), title.trim());
    let title = ["「」", "『』"]
        .iter()
        .find_map(|b| {
            let mut c = b.chars();
            title.strip_prefix(c.next()?)?.strip_suffix(c.next()?)
        })
        .unwrap_or(title);
    (!artist.is_empty() && !title.is_empty()).then(|| (artist.into(), title.into()))
}

/// `Artist「Title」…`; anything after the closing bracket is dropped.
fn bracket_quote(s: &str) -> Option<(String, String)> {
    let (open, close) = [('「', '」'), ('『', '』')]
        .into_iter()
        .filter_map(|(o, c)| s.find(o).map(|i| (i, (o, c))))
        .min_by_key(|(i, _)| *i)
        .map(|(_, p)| p)?;
    let (artist, rest) = s.split_once(open)?;
    let (title, _) = rest.split_once(close)?;
    let (artist, title) = (artist.trim(), title.trim());
    (!artist.is_empty() && !title.is_empty()).then(|| (artist.into(), title.into()))
}

/// `Title / Artist`, splitting on the last spaced slash.
fn title_slash_artist(s: &str) -> Option<(String, String)> {
    let (title, artist) = s.rsplit_once(" / ")?;
    let (artist, title) = (artist.trim(), title.trim());
    (!artist.is_empty() && !title.is_empty()).then(|| (artist.into(), title.into()))
}

/// Split an artist string on `feat`/`ft`/`featuring` and spaced `/`.
///
/// Splits only on whole words, so `Daft Punk` stays whole; `&`, `x` and
/// commas are left alone because they appear in real band names.
pub fn split_artists(s: &str) -> Vec<String> {
    let mut names = vec![Vec::new()];
    for word in s.split_whitespace() {
        let lower = word.to_lowercase();
        if word == "/" || matches!(lower.trim_end_matches('.'), "feat" | "ft" | "featuring") {
            names.push(Vec::new());
        } else {
            names.last_mut().expect("never empty").push(word);
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
            ("かめりあ".into(), "アリバイ".into(), "bracket-quote")
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
}
