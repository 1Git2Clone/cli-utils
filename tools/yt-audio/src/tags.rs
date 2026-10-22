//! Writing resolved tags into the downloaded audio file.

use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::{ItemKey, ItemValue, Tag, TagExt, TagItem};

use crate::error::Error;
use crate::info::Video;
use crate::resolve::Tags;

/// Keys this module owns; cleared before writing so stale values never linger.
const OWNED: [ItemKey; 8] = [
    ItemKey::TrackArtist,
    ItemKey::TrackArtists,
    ItemKey::TrackTitle,
    ItemKey::AlbumTitle,
    ItemKey::AlbumArtist,
    ItemKey::RecordingDate,
    ItemKey::Comment,
    ItemKey::TrackNumber,
];

/// Write `t`, plus the date, track number and source URL of `v`, into the
/// audio file at `path`, one `ItemKey::TrackArtist` per artist.
///
/// Embedded pictures and any other existing tag items are left untouched.
///
/// # Errors
///
/// Fails if the file cannot be read, parsed or written.
pub fn write_tags(path: &Path, t: &Tags, v: &Video) -> Result<(), Error> {
    let file = Probe::open(path)?.read()?;
    let mut tag = file
        .primary_tag()
        .cloned()
        .unwrap_or_else(|| Tag::new(file.primary_tag_type()));

    for key in OWNED {
        tag.remove_key(&key);
    }
    for artist in &t.artists.value {
        tag.push(TagItem::new(
            ItemKey::TrackArtist,
            ItemValue::Text(artist.clone()),
        ));
    }
    tag.insert_text(ItemKey::TrackTitle, t.title.value.clone());
    tag.insert_text(ItemKey::AlbumTitle, t.album.value.clone());
    if let Some(artist) = t.album_artist() {
        tag.insert_text(ItemKey::AlbumArtist, artist.to_owned());
    }
    if let Some(date) = v.recording_date() {
        tag.insert_text(ItemKey::RecordingDate, date.to_string());
    }
    tag.insert_text(ItemKey::Comment, v.webpage_url.to_string());
    if let Some(n) = v.track_number {
        tag.insert_text(ItemKey::TrackNumber, n.to_string());
    }

    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::info::parse_video;
    use crate::resolve::{Field, Source};
    use lofty::file::AudioFile;
    use lofty::picture::{Picture, PictureType};
    use std::process::{Command, Stdio};

    fn ffmpeg(args: &[&str], out: &Path) -> bool {
        let _g = crate::ytdlp::SPAWN_LOCK.blocking_read();
        Command::new("ffmpeg")
            .args(["-v", "error", "-y"])
            .args(args)
            .arg(out)
            .stdin(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    fn sample() -> (Tags, Video) {
        let f = |v: &str| Field {
            value: v.to_string(),
            source: Source::Fallback,
        };
        let tags = Tags {
            artists: Field {
                value: vec!["かめりあ".into(), "初音ミク".into()],
                source: Source::Fallback,
            },
            title: f("曲"),
            album: f("Album"),
        };
        let video = parse_video(
            r#"{"id":"x","title":"t","webpage_url":"https://example.com/v",
                "release_year":2024,"track_number":3}"#,
        )
        .unwrap();
        (tags, video)
    }

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn check(file: &Path) {
        let (tags, video) = sample();
        write_tags(file, &tags, &video).unwrap();
        let read = Probe::open(file).unwrap().read().unwrap();
        let tag = read.primary_tag().unwrap();
        let artists: Vec<_> = tag.get_strings(&ItemKey::TrackArtist).collect();
        assert_eq!(artists, ["かめりあ", "初音ミク"]);
        assert_eq!(tag.get_string(&ItemKey::TrackTitle), Some("曲"));
        assert_eq!(tag.get_string(&ItemKey::AlbumTitle), Some("Album"));
        assert_eq!(tag.get_string(&ItemKey::AlbumArtist), Some("かめりあ"));
        assert_eq!(tag.get_string(&ItemKey::TrackNumber), Some("3"));
        assert_eq!(
            tag.get_string(&ItemKey::Comment),
            Some("https://example.com/v")
        );
        assert_eq!(tag.get_string(&ItemKey::RecordingDate), Some("2024"));
    }

    #[test]
    fn writes_multiple_artists() {
        let d = dir();
        let (opus, png) = (d.path().join("x.opus"), d.path().join("c.png"));
        if !ffmpeg(
            &[
                "-f", "lavfi", "-i", "anullsrc", "-t", "1", "-c:a", "libopus",
            ],
            &opus,
        ) || !ffmpeg(
            &["-f", "lavfi", "-i", "color=c=red:s=8x8", "-frames:v", "1"],
            &png,
        ) {
            eprintln!("skipping: ffmpeg unavailable");
            return;
        }
        // Embed a picture first, as yt-dlp would.
        let mut pic = Picture::from_reader(&mut std::fs::File::open(&png).unwrap()).unwrap();
        pic.set_pic_type(PictureType::CoverFront);
        let mut file = Probe::open(&opus).unwrap().read().unwrap();
        let mut tag = Tag::new(file.primary_tag_type());
        tag.push_picture(pic);
        file.insert_tag(tag);
        file.save_to_path(&opus, WriteOptions::default()).unwrap();

        check(&opus);
        let read = Probe::open(&opus).unwrap().read().unwrap();
        assert_eq!(read.primary_tag().unwrap().picture_count(), 1);
    }

    #[test]
    fn writes_tags_to_m4a() {
        let d = dir();
        let m4a = d.path().join("x.m4a");
        if !ffmpeg(
            &["-f", "lavfi", "-i", "anullsrc", "-t", "1", "-c:a", "aac"],
            &m4a,
        ) {
            eprintln!("skipping: ffmpeg unavailable");
            return;
        }
        check(&m4a);
    }
}
