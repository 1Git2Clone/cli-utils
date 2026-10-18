//! Writing resolved tags into the downloaded audio file.

use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::{ItemKey, Tag, TagExt};

use crate::error::Error;
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

/// Write `t` into the audio file at `path`, one `TrackArtist` per artist.
///
/// Embedded pictures and any other existing tag items are left untouched.
pub fn write_tags(path: &Path, t: &Tags) -> Result<(), Error> {
    let mut file = Probe::open(path)?.read()?;
    if file.primary_tag().is_none() {
        file.insert_tag(Tag::new(file.primary_tag_type()));
    }
    let tag = file.primary_tag_mut().expect("tag inserted above");

    for key in OWNED {
        tag.remove_key(&key);
    }
    for artist in &t.artists.value {
        tag.push(lofty::tag::TagItem::new(
            ItemKey::TrackArtist,
            lofty::tag::ItemValue::Text(artist.clone()),
        ));
    }
    tag.insert_text(ItemKey::TrackTitle, t.title.value.clone());
    tag.insert_text(ItemKey::AlbumTitle, t.album.value.clone());
    tag.insert_text(ItemKey::AlbumArtist, t.album_artist.clone());
    if let Some(date) = &t.date {
        tag.insert_text(ItemKey::RecordingDate, date.clone());
    }
    tag.insert_text(ItemKey::Comment, t.comment.clone());
    if let Some(n) = t.track_number {
        tag.insert_text(ItemKey::TrackNumber, n.to_string());
    }

    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::{Field, Source};
    use lofty::file::AudioFile;
    use lofty::picture::{Picture, PictureType};
    use std::process::{Command, Stdio};

    fn ffmpeg(args: &[&str], out: &Path) -> bool {
        Command::new("ffmpeg")
            .args(["-v", "error", "-y"])
            .args(args)
            .arg(out)
            .stdin(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    fn sample() -> Tags {
        let f = |v: &str| Field {
            value: v.to_string(),
            source: Source::Fallback,
        };
        Tags {
            artists: Field {
                value: vec!["かめりあ".into(), "初音ミク".into()],
                source: Source::Fallback,
            },
            title: f("曲"),
            album: f("Album"),
            album_artist: "かめりあ".into(),
            date: Some("2024".into()),
            comment: "https://example.com".into(),
            track_number: Some(3),
        }
    }

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn check(file: &Path) {
        write_tags(file, &sample()).unwrap();
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
            Some("https://example.com")
        );
        assert!(
            tag.get_string(&ItemKey::RecordingDate)
                .is_some_and(|d| d.starts_with("2024"))
        );
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
