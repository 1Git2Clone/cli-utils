use std::cell::Cell;

use yt_audio::info::{Video, parse_video};
use yt_audio::resolve::{Guess, Overrides, Source, resolve};

fn video(json: &str) -> Video {
    parse_video(json).unwrap()
}

fn topic() -> Video {
    video(include_str!("fixtures/topic.json"))
}

fn dash() -> Video {
    video(include_str!("fixtures/artist_dash.json"))
}

fn no_pattern() -> Video {
    video(include_str!("fixtures/no_pattern.json"))
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn topic_uses_youtube_fields() {
    let t = resolve(&topic(), &Overrides::default(), None, false);
    assert_eq!(t.artists.value, strs(&["Halozy", "Nanahira"]));
    assert_eq!(t.artists.source, Source::YouTube);
    assert_eq!(t.album.value, "Lucky 7");
    assert_eq!(t.track_number, Some(3));
    assert_eq!(t.date.as_deref(), Some("2016-08-12"));
    assert_eq!(t.comment, "https://www.youtube.com/watch?v=topic1");
}

#[test]
fn artist_dash_splits_feat() {
    let t = resolve(&dash(), &Overrides::default(), None, false);
    assert_eq!(t.artists.value, strs(&["かめりあ", "初音ミク"]));
    assert_eq!(t.artists.source, Source::Pattern("artist-dash"));
    assert_eq!(t.title.value, "アリバイ");
    assert_eq!(t.album.value, "アリバイ");
    assert_eq!(t.album_artist, "かめりあ");
}

#[test]
fn title_slash() {
    let v = video(include_str!("fixtures/title_slash.json"));
    let t = resolve(&v, &Overrides::default(), None, false);
    assert_eq!(t.artists.value, strs(&["みきとP"]));
    assert_eq!(t.title.value, "ロキ");
}

#[test]
fn fallback_uses_channel_minus_topic() {
    let t = resolve(&no_pattern(), &Overrides::default(), None, false);
    assert_eq!(t.title.value, "ただの曲名");
    assert_eq!(t.title.source, Source::Fallback);
    assert_eq!(t.artists.value, strs(&["Some Channel"]));
    assert_eq!(t.date.as_deref(), Some("2026-01-01"));
}

#[test]
fn partial_youtube_fields_merge_per_field() {
    let v = video(include_str!("fixtures/partial.json"));
    let t = resolve(&v, &Overrides::default(), None, false);
    assert_eq!(t.artists.source, Source::YouTube);
    assert_eq!(t.title.value, "Lucky 7");
    assert_eq!(t.title.source, Source::Pattern("artist-dash"));
}

#[test]
fn override_wins() {
    let o = Overrides {
        title: Some("X".into()),
        ..Default::default()
    };
    let t = resolve(&topic(), &o, None, false);
    assert_eq!(t.title.value, "X");
    assert_eq!(t.title.source, Source::Override);
    assert_eq!(t.artists.source, Source::YouTube);
}

fn guess(_: &Video) -> Option<Guess> {
    Some(Guess {
        artists: strs(&["Some Channel"]),
        title: Some("ただの曲名".into()),
        album: None,
    })
}

#[test]
fn llm_only_when_unresolved() {
    let calls = Cell::new(0);
    let llm = |v: &Video| {
        calls.set(calls.get() + 1);
        guess(v)
    };
    resolve(&topic(), &Overrides::default(), Some(&llm), false);
    assert_eq!(calls.get(), 0);

    let t = resolve(&no_pattern(), &Overrides::default(), Some(&llm), false);
    assert_eq!(calls.get(), 1);
    assert_eq!(t.title.source, Source::Llm);
    assert_eq!(t.artists.source, Source::Llm);
    assert_eq!(t.artists.value, strs(&["Some Channel"]));
}

#[test]
fn llm_always_outranks_pattern_not_youtube() {
    let llm = |_: &Video| {
        Some(Guess {
            artists: vec![],
            title: Some("LLM".into()),
            album: None,
        })
    };
    let t = resolve(&dash(), &Overrides::default(), Some(&llm), true);
    assert_eq!(t.title.value, "LLM");
    assert_eq!(t.title.source, Source::Llm);
    let t = resolve(&topic(), &Overrides::default(), Some(&llm), true);
    assert_eq!(t.title.source, Source::YouTube);
}

#[test]
fn date_format() {
    let t = resolve(&topic(), &Overrides::default(), None, false);
    assert_eq!(t.date.as_deref(), Some("2016-08-12"));
    assert_eq!(Source::Pattern("p").to_string(), "pattern:p");
}
