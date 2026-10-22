//! Serde model for `yt-dlp -J` output.

use std::fmt;

use jiff::civil::Date;
use serde::{Deserialize, Deserializer};
use url::Url;

use crate::error::Error;

/// One video's metadata; unknown fields are ignored.
#[derive(Clone, Debug, Deserialize)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub webpage_url: Url,
    pub channel: Option<String>,
    pub uploader: Option<String>,
    pub description: Option<String>,
    pub artists: Option<Vec<String>>,
    pub artist: Option<String>,
    pub track: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<u32>,
    #[serde(default, deserialize_with = "ymd")]
    pub release_date: Option<Date>,
    pub release_year: Option<i16>,
    #[serde(default, deserialize_with = "ymd")]
    pub upload_date: Option<Date>,
}

/// A recording date as precise as the source knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingDate {
    Day(Date),
    Year(i16),
}

impl fmt::Display for RecordingDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Day(d) => write!(f, "{d}"),
            Self::Year(y) => write!(f, "{y:04}"),
        }
    }
}

impl Video {
    /// Release date, else release year, else upload date.
    pub fn recording_date(&self) -> Option<RecordingDate> {
        self.release_date
            .map(RecordingDate::Day)
            .or_else(|| self.release_year.map(RecordingDate::Year))
            .or_else(|| self.upload_date.map(RecordingDate::Day))
    }
}

/// yt-dlp's `YYYYMMDD` dates; anything unparseable reads as absent rather
/// than failing the whole video.
fn ymd<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Date>, D::Error> {
    let s = Option::<String>::deserialize(d)?;
    Ok(s.and_then(|s| jiff::fmt::strtime::parse("%Y%m%d", &s).ok()?.to_date().ok()))
}

/// What a URL resolved to under `--flat-playlist`.
#[derive(Debug, PartialEq, Eq)]
pub enum Listing {
    /// A playlist, its entries' URLs, and how many entries had no URL.
    Playlist {
        title: String,
        entries: Vec<Url>,
        missing: usize,
    },
    /// A single video's canonical URL, when yt-dlp reported one.
    Single(Option<Url>),
}

/// Parse `yt-dlp -J --flat-playlist` output.
///
/// # Errors
///
/// Fails on malformed JSON or URLs.
pub fn parse_listing(json: &str) -> Result<Listing, Error> {
    #[derive(Deserialize)]
    struct Raw {
        #[serde(rename = "_type")]
        kind: Option<String>,
        title: Option<String>,
        webpage_url: Option<Url>,
        #[serde(default)]
        entries: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Entry {
        url: Option<Url>,
        webpage_url: Option<Url>,
    }

    let raw: Raw = serde_json::from_str(json)?;
    if raw.kind.as_deref() != Some("playlist") {
        return Ok(Listing::Single(raw.webpage_url));
    }
    let total = raw.entries.len();
    let entries: Vec<Url> = raw
        .entries
        .into_iter()
        .filter_map(|e| e.url.or(e.webpage_url))
        .collect();
    Ok(Listing::Playlist {
        title: raw.title.unwrap_or_default(),
        missing: total - entries.len(),
        entries,
    })
}

/// Parse `yt-dlp -J` output for one video.
///
/// # Errors
///
/// Fails on malformed JSON, a malformed URL, or a missing required field.
pub fn parse_video(json: &str) -> Result<Video, Error> {
    Ok(serde_json::from_str(json)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    const PLAYLIST: &str = include_str!("../tests/fixtures/playlist_flat.json");
    const TOPIC: &str = include_str!("../tests/fixtures/topic.json");

    #[test]
    fn playlist_listing() {
        let Listing::Playlist {
            title,
            entries,
            missing,
        } = parse_listing(PLAYLIST).unwrap()
        else {
            panic!("not a playlist");
        };
        assert_eq!(title, "My Mix");
        assert_eq!(entries.len(), 2);
        assert_eq!(missing, 0);
        assert!(entries[1].as_str().contains("bad"));
    }

    #[test]
    fn video_is_single() {
        assert_eq!(
            parse_listing(TOPIC).unwrap(),
            Listing::Single(Some(
                Url::parse("https://www.youtube.com/watch?v=topic1").unwrap()
            ))
        );
    }

    #[test]
    fn entries_without_a_url_are_counted() {
        let json = r#"{"_type":"playlist","title":"t","entries":[
            {"id":"x1"},{"id":"x2","webpage_url":"https://example.com/v/x2"}]}"#;
        assert_eq!(
            parse_listing(json).unwrap(),
            Listing::Playlist {
                title: "t".into(),
                entries: vec![Url::parse("https://example.com/v/x2").unwrap()],
                missing: 1,
            }
        );
    }

    #[test]
    fn topic_video_fields() {
        let v = parse_video(TOPIC).unwrap();
        assert_eq!(v.artists, Some(vec!["Halozy".into(), "Nanahira".into()]));
        assert_eq!(v.track_number, Some(3));
        assert_eq!(v.album.as_deref(), Some("Lucky 7"));
        assert_eq!(v.release_date, Some(date(2016, 8, 12)));
    }

    #[test]
    fn recording_date_precedence_and_format() {
        let v = parse_video(TOPIC).unwrap();
        assert_eq!(v.recording_date().unwrap().to_string(), "2016-08-12");
        let json = r#"{"id":"x","title":"t","webpage_url":"https://e.com/x",
            "release_year":2016,"upload_date":"20200101"}"#;
        assert_eq!(
            parse_video(json).unwrap().recording_date(),
            Some(RecordingDate::Year(2016))
        );
    }

    #[test]
    fn malformed_dates_read_as_absent() {
        let json = r#"{"id":"x","title":"t","webpage_url":"https://e.com/x",
            "release_date":"aéééb","upload_date":"20201399"}"#;
        let v = parse_video(json).unwrap();
        assert_eq!((v.release_date, v.upload_date), (None, None));
    }

    #[test]
    fn bad_json_is_error() {
        assert!(parse_video("{").is_err());
        assert!(parse_video(r#"{"id":"x","title":"t","webpage_url":"not a url"}"#).is_err());
    }
}
