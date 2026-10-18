//! Serde model for `yt-dlp -J` output.

use serde::Deserialize;

use crate::error::Error;

/// URL prefix for a video id.
const WATCH: &str = "https://www.youtube.com/watch?v=";

/// One video's metadata; unknown fields are ignored.
#[derive(Debug, Deserialize)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub webpage_url: String,
    pub channel: Option<String>,
    pub uploader: Option<String>,
    pub description: Option<String>,
    pub artists: Option<Vec<String>>,
    pub artist: Option<String>,
    pub track: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<u32>,
    /// `YYYYMMDD`.
    pub release_date: Option<String>,
    pub release_year: Option<u32>,
    /// `YYYYMMDD`.
    pub upload_date: Option<String>,
}

/// What a URL resolved to under `--flat-playlist`.
#[derive(Debug, PartialEq, Eq)]
pub enum Listing {
    /// A playlist and its entries' URLs.
    Playlist { title: String, entries: Vec<String> },
    /// A single video's URL.
    Single(String),
}

/// Parse `yt-dlp -J --flat-playlist` output.
pub fn parse_listing(json: &str) -> Result<Listing, Error> {
    #[derive(Deserialize)]
    struct Raw {
        #[serde(rename = "_type")]
        kind: Option<String>,
        title: Option<String>,
        webpage_url: Option<String>,
        #[serde(default)]
        entries: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Entry {
        id: Option<String>,
        url: Option<String>,
    }

    let raw: Raw = serde_json::from_str(json)?;
    if raw.kind.as_deref() == Some("playlist") {
        let entries = raw
            .entries
            .into_iter()
            .filter_map(|e| e.url.or_else(|| e.id.map(|id| format!("{WATCH}{id}"))))
            .collect();
        Ok(Listing::Playlist {
            title: raw.title.unwrap_or_default(),
            entries,
        })
    } else {
        Ok(Listing::Single(raw.webpage_url.unwrap_or_default()))
    }
}

/// Parse `yt-dlp -J` output for one video.
pub fn parse_video(json: &str) -> Result<Video, Error> {
    Ok(serde_json::from_str(json)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYLIST: &str = include_str!("../tests/fixtures/playlist_flat.json");
    const TOPIC: &str = include_str!("../tests/fixtures/topic.json");

    #[test]
    fn playlist_listing() {
        let Listing::Playlist { title, entries } = parse_listing(PLAYLIST).unwrap() else {
            panic!("not a playlist");
        };
        assert_eq!(title, "My Mix");
        assert_eq!(entries.len(), 2);
        assert!(entries[1].contains("bad"));
    }

    #[test]
    fn video_is_single() {
        assert_eq!(
            parse_listing(TOPIC).unwrap(),
            Listing::Single("https://www.youtube.com/watch?v=topic1".into())
        );
    }

    #[test]
    fn entry_url_falls_back_to_id() {
        let json = r#"{"_type":"playlist","title":"t","entries":[{"id":"x1"}]}"#;
        assert_eq!(
            parse_listing(json).unwrap(),
            Listing::Playlist {
                title: "t".into(),
                entries: vec!["https://www.youtube.com/watch?v=x1".into()]
            }
        );
    }

    #[test]
    fn topic_video_fields() {
        let v = parse_video(TOPIC).unwrap();
        assert_eq!(v.artists, Some(vec!["Halozy".into(), "Nanahira".into()]));
        assert_eq!(v.track_number, Some(3));
        assert_eq!(v.album.as_deref(), Some("Lucky 7"));
    }

    #[test]
    fn bad_json_is_error() {
        assert!(parse_video("{").is_err());
    }
}
