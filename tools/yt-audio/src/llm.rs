//! Optional LLM fallback for titles the patterns cannot parse.

use llm::builder::{LLMBackend, LLMBuilder};
use llm::chat::ChatMessage;
use serde::Deserialize;

use crate::cli::{Cli, LlmBackend};
use crate::error::Error;
use crate::info::Video;
use crate::resolve::Guess;

/// How many characters of the description are sent (and trusted by the guard).
const DESC_CHARS: usize = 500;

/// Settings for the LLM client.
pub struct LlmConfig {
    pub backend: LlmBackend,
    pub url: Option<String>,
    pub model: String,
    /// From `YT_AUDIO_LLM_KEY` only, never a flag.
    pub key: Option<String>,
    pub always: bool,
}

impl LlmConfig {
    /// The config from the CLI and environment, or `None` without a model.
    pub fn from_cli(cli: &Cli) -> Option<Self> {
        Some(Self {
            backend: cli.llm_backend,
            url: cli.llm_url.clone(),
            model: cli.llm_model.clone().filter(|m| !m.is_empty())?,
            key: std::env::var("YT_AUDIO_LLM_KEY")
                .ok()
                .filter(|k| !k.is_empty()),
            always: cli.llm_always,
        })
    }
}

/// The first [`DESC_CHARS`] characters of the description.
fn excerpt(v: &Video) -> String {
    v.description
        .as_deref()
        .unwrap_or_default()
        .chars()
        .take(DESC_CHARS)
        .collect()
}

/// The channel name sent to the model: channel, else uploader.
fn channel_of(v: &Video) -> &str {
    v.channel
        .as_deref()
        .or(v.uploader.as_deref())
        .unwrap_or_default()
}

/// The request text for `v`.
pub fn prompt(v: &Video) -> String {
    format!(
        "Extract music metadata for this YouTube video. Reply with only JSON of the form \
         {{\"artists\":[\"...\"],\"title\":\"...\",\"album\":\"...\" or null}}. \
         Use only names that appear below.\n\nTitle: {}\nChannel: {}\nDescription: {}",
        v.title,
        channel_of(v),
        excerpt(v),
    )
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    artists: Vec<String>,
    title: Option<String>,
    album: Option<String>,
}

/// Parse a model reply: drop `<think>` blocks, then take the first `{` to the
/// last `}` (tolerating code fences and prose).
pub fn parse_response(text: &str) -> Option<Guess> {
    let mut rest = text;
    let mut clean = String::new();
    while let Some(start) = rest.find("<think>") {
        clean.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => rest = "",
        }
    }
    clean.push_str(rest);
    let json = clean.get(clean.find('{')?..=clean.rfind('}')?)?;
    let raw: Raw = serde_json::from_str(json).ok()?;
    Some(Guess {
        artists: raw.artists,
        title: raw.title,
        album: raw.album,
    })
}

/// Keep only values that appear (case-insensitively) in what was sent.
pub fn guard(g: Guess, v: &Video) -> Guess {
    let haystack = format!("{}\n{}\n{}", v.title, channel_of(v), excerpt(v)).to_lowercase();
    let seen = |s: &str| !s.trim().is_empty() && haystack.contains(&s.to_lowercase());
    Guess {
        artists: g.artists.into_iter().filter(|a| seen(a)).collect(),
        title: g.title.filter(|t| seen(t)),
        album: g.album.filter(|a| seen(a)),
    }
}

/// Build a guesser that asks the configured model, warning and returning
/// `None` on any failure.
///
/// # Errors
///
/// Fails if the client or its runtime cannot be set up.
pub fn make_guesser(cfg: &LlmConfig) -> Result<impl Fn(&Video) -> Option<Guess> + use<>, Error> {
    let mut b = LLMBuilder::new()
        .backend(match cfg.backend {
            LlmBackend::Ollama => LLMBackend::Ollama,
            LlmBackend::Openai => LLMBackend::OpenAI,
        })
        .model(&cfg.model)
        .timeout_seconds(30);
    if let Some(url) = &cfg.url {
        b = b.base_url(url);
    }
    if let Some(secret) = &cfg.key {
        b = b.api_key(secret);
    }
    let provider = b.build().map_err(|e| Error::Llm(e.to_string()))?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Llm(format!("cannot start async runtime: {e}")))?;
    Ok(move |v: &Video| {
        let msgs = [ChatMessage::user().content(prompt(v)).build()];
        let reply = rt
            .block_on(provider.chat(&msgs))
            .map_err(|e| e.to_string())
            .and_then(|r| r.text().ok_or_else(|| "empty reply".to_string()));
        match reply {
            Ok(text) => parse_response(&text).map(|g| guard(g, v)),
            Err(e) => {
                eprintln!("yt-audio: llm: {e}");
                None
            }
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::info::{Video, parse_video};

    fn video(title: &str, desc: &str) -> Video {
        let json = serde_json::json!({
            "id": "x", "title": title, "webpage_url": "https://y/x",
            "channel": "Chan", "description": desc,
        });
        parse_video(&json.to_string()).unwrap()
    }

    #[test]
    fn parses_fenced_json() {
        let g =
            parse_response("```json\n{\"artists\":[\"A\"],\"title\":\"T\",\"album\":null}\n```")
                .unwrap();
        assert_eq!(g.artists, ["A"]);
        assert_eq!(g.title.as_deref(), Some("T"));
        assert_eq!(g.album, None);
    }

    #[test]
    fn strips_think_blocks() {
        let g = parse_response(
            "<think>maybe {\"x\": 1} hmm</think>\n{\"artists\":[\"A\"],\"title\":\"T\",\"album\":null}",
        )
        .unwrap();
        assert_eq!(g.artists, ["A"]);
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_response("sorry").is_none());
    }

    #[test]
    fn guard_drops_invented_artist() {
        let g = Guess {
            artists: vec!["A".into(), "Z".into()],
            title: Some("T".into()),
            album: Some("Nope".into()),
        };
        let g = guard(g, &video("A - T", ""));
        assert_eq!(g.artists, ["A"]);
        assert_eq!(g.title.as_deref(), Some("T"));
        assert_eq!(g.album, None);
    }

    #[test]
    fn guard_accepts_uploader_sent_in_prompt() {
        let json = serde_json::json!({
            "id": "x", "title": "t", "webpage_url": "https://y/x", "uploader": "Upl",
        });
        let v = parse_video(&json.to_string()).unwrap();
        assert!(prompt(&v).contains("Channel: Upl"));
        let g = Guess {
            artists: vec!["Upl".into()],
            title: None,
            album: None,
        };
        assert_eq!(guard(g, &v).artists, ["Upl"]);
    }

    #[test]
    fn prompt_truncates_by_chars() {
        let p = prompt(&video("t", &"あ".repeat(2000)));
        assert_eq!(p.matches('あ').count(), 500);
    }

    #[test]
    fn from_cli_needs_a_model() {
        let cli = Cli::parse_from(vec!["yt-audio".into(), "u".into()]).unwrap();
        // The environment may set a model; only assert the flag-less case.
        if cli.llm_model.is_none() {
            assert!(LlmConfig::from_cli(&cli).is_none());
        }
        let cli =
            Cli::parse_from(vec!["yt-audio".into(), "u".into(), "--llm-model=m".into()]).unwrap();
        assert_eq!(LlmConfig::from_cli(&cli).unwrap().model, "m");
    }

    /// Needs `YT_AUDIO_LLM_*` in the environment and network.
    #[test]
    #[ignore]
    fn real_endpoint() {
        let cli = Cli::parse_from(vec!["yt-audio".into(), "u".into()]).unwrap();
        let cfg = LlmConfig::from_cli(&cli).expect("set YT_AUDIO_LLM_MODEL");
        let guesser = make_guesser(&cfg).unwrap();
        let v = parse_video(include_str!("../tests/fixtures/no_pattern.json")).unwrap();
        let tags = crate::resolve::resolve(&v, &Default::default(), Some(&guesser), true);
        assert!(!tags.title.value.is_empty());
        assert_eq!(tags.title.source, crate::resolve::Source::Llm);
        eprintln!(
            "{} ({}) / {:?} ({})",
            tags.title.value, tags.title.source, tags.artists.value, tags.artists.source
        );
    }
}
