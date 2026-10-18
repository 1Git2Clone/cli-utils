//! Optional LLM fallback for titles the patterns cannot parse.
//!
//! The model is forced to call one tool whose arguments are the metadata, so
//! the reply is typed JSON regardless of any reasoning text around it.

use llm::LLMProvider;
use llm::builder::{FunctionBuilder, LLMBackend, LLMBuilder};
use llm::chat::{ChatMessage, FunctionTool, Tool, ToolChoice};
use serde::Deserialize;
use url::Url;

use crate::cli::{Cli, LlmBackend};
use crate::error::Error;
use crate::info::Video;
use crate::resolve::Guess;

/// How many characters of the description are sent (and trusted by the guard).
const DESC_CHARS: usize = 500;

/// The tool the model must call.
const TOOL: &str = "set_metadata";

/// Settings for the LLM client.
pub struct LlmConfig {
    pub backend: LlmBackend,
    pub url: Option<Url>,
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
        "Record the music metadata of this video with {TOOL}. Use only names that \
         appear below; leave album empty if unknown.\n\nTitle: {}\nChannel: {}\nDescription: {}",
        v.title,
        channel_of(v),
        excerpt(v),
    )
}

/// What the tool does, for the model.
const TOOL_DESCRIPTION: &str = "Record the track's music metadata.";

/// The tool's parameters; plain strings rather than nullable unions, which
/// some OpenAI-compatible servers (`MiniMax`) reject.
fn tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["artists", "title", "album"],
        "properties": {
            "artists": {"type": "array", "items": {"type": "string"}},
            "title": {"type": "string"},
            "album": {"type": "string", "description": "empty if unknown"}
        }
    })
}

/// The tool's arguments as the model sends them.
#[derive(Deserialize)]
struct Args {
    #[serde(default)]
    artists: Vec<String>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    album: String,
}

impl From<Args> for Guess {
    fn from(a: Args) -> Self {
        let some = |s: String| Some(s).filter(|s| !s.trim().is_empty());
        Self {
            artists: a.artists,
            title: some(a.title),
            album: some(a.album),
        }
    }
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

/// A configured model that guesses tags.
pub struct Guesser {
    provider: Box<dyn LLMProvider>,
    /// Passed with every request too: the Ollama backend only sends tools
    /// given per call, not ones registered on the builder.
    tool: Tool,
}

impl Guesser {
    /// Build the client.
    ///
    /// # Errors
    ///
    /// Fails if the client cannot be set up.
    pub fn new(cfg: &LlmConfig) -> Result<Self, Error> {
        let mut b = LLMBuilder::new()
            .backend(match cfg.backend {
                LlmBackend::Ollama => LLMBackend::Ollama,
                LlmBackend::Openai => LLMBackend::OpenAI,
            })
            .model(&cfg.model)
            .function(
                FunctionBuilder::new(TOOL)
                    .description(TOOL_DESCRIPTION)
                    .json_schema(tool_schema()),
            )
            .tool_choice(ToolChoice::Tool(TOOL.into()))
            .timeout_seconds(30);
        if let Some(url) = &cfg.url {
            b = b.base_url(url.as_str());
        }
        if let Some(secret) = &cfg.key {
            b = b.api_key(secret);
        }
        Ok(Self {
            provider: b.build().map_err(|e| Error::Llm(e.to_string()))?,
            tool: Tool {
                tool_type: "function".into(),
                function: FunctionTool {
                    name: TOOL.into(),
                    description: TOOL_DESCRIPTION.into(),
                    parameters: tool_schema(),
                },
                cache_control: None,
            },
        })
    }

    /// Ask the model about `v`, keeping only names the guard accepts.
    ///
    /// # Errors
    ///
    /// Fails on a transport error or a reply without a well-formed tool call.
    pub async fn guess(&self, v: &Video) -> Result<Guess, Error> {
        let msgs = [ChatMessage::user().content(prompt(v)).build()];
        let reply = self
            .provider
            .chat_with_tools(&msgs, Some(std::slice::from_ref(&self.tool)))
            .await
            .map_err(|e| Error::Llm(e.to_string()))?;
        let call = reply
            .tool_calls()
            .and_then(|calls| calls.into_iter().find(|c| c.function.name == TOOL))
            .ok_or_else(|| Error::Llm(format!("reply did not call {TOOL}")))?;
        let args: Args = serde_json::from_str(&call.function.arguments)?;
        Ok(guard(args.into(), v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::info::parse_video;

    fn video(title: &str, desc: &str) -> Video {
        let json = serde_json::json!({
            "id": "x", "title": title, "webpage_url": "https://y.example/x",
            "channel": "Chan", "description": desc,
        });
        parse_video(&json.to_string()).unwrap()
    }

    #[test]
    fn tool_arguments_become_a_guess() {
        let args: Args =
            serde_json::from_str(r#"{"artists":["A"],"title":"T","album":""}"#).unwrap();
        assert_eq!(
            Guess::from(args),
            Guess {
                artists: vec!["A".into()],
                title: Some("T".into()),
                album: None,
            }
        );
        assert!(serde_json::from_str::<Args>("sorry").is_err());
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
            "id": "x", "title": "t", "webpage_url": "https://y.example/x", "uploader": "Upl",
        });
        let v = parse_video(&json.to_string()).unwrap();
        assert!(prompt(&v).contains("Channel: Upl"));
        let g = Guess {
            artists: vec!["Upl".into()],
            ..Guess::default()
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
        let cli = Cli::parse_from(vec!["yt-audio".into(), "https://e.example/u".into()]).unwrap();
        // The environment may set a model; only assert the flag-less case.
        if cli.llm_model.is_none() {
            assert!(LlmConfig::from_cli(&cli).is_none());
        }
        let cli = Cli::parse_from(vec![
            "yt-audio".into(),
            "https://e.example/u".into(),
            "--llm-model=m".into(),
            "--llm-url=https://api.example/v1".into(),
        ])
        .unwrap();
        let cfg = LlmConfig::from_cli(&cli).unwrap();
        assert_eq!(cfg.model, "m");
        assert_eq!(cfg.url.unwrap().as_str(), "https://api.example/v1");
    }

    /// Answer one HTTP request with `body`, returning the base URL and the
    /// request as received.
    async fn serve_once(body: String) -> (Url, tokio::task::JoinHandle<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!("http://{}/v1", listener.local_addr().unwrap())).unwrap();
        let served = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req = Vec::new();
            let mut buf = [0; 4096];
            // Read the headers, then exactly content-length body bytes.
            let complete = |req: &[u8]| {
                let text = String::from_utf8_lossy(req);
                let (head, rest) = text.split_once("\r\n\r\n")?;
                let len: usize = head.lines().find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")?
                        .trim()
                        .parse()
                        .ok()
                })?;
                (rest.len() >= len).then_some(())
            };
            while complete(&req).is_none() {
                let n = sock.read(&mut buf).await.unwrap();
                assert!(n > 0, "client closed early");
                req.extend_from_slice(&buf[..n]);
            }
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&req).into_owned()
        });
        (url, served)
    }

    fn config(url: Url) -> LlmConfig {
        LlmConfig {
            backend: LlmBackend::Openai,
            url: Some(url),
            model: "m".into(),
            key: Some("test-key".into()),
            always: false,
        }
    }

    /// A Responses API reply (the `openai` backend's protocol).
    fn response(output: serde_json::Value) -> String {
        serde_json::json!({"id": "r", "object": "response", "output": output}).to_string()
    }

    fn text_item(text: &str) -> serde_json::Value {
        serde_json::json!({"type": "message",
                           "content": [{"type": "output_text", "text": text}]})
    }

    #[tokio::test]
    async fn guess_reads_the_forced_tool_call() {
        let args = r#"{"artists":["Chan","Invented"],"title":"Song","album":""}"#;
        let (url, served) = serve_once(response(serde_json::json!([
            text_item("<think>reasoning the server mixes in</think>"),
            {"type": "function_call", "call_id": "t", "name": TOOL, "arguments": args}
        ])))
        .await;
        let g = Guesser::new(&config(url)).unwrap();
        let guess = g.guess(&video("Song", "")).await.unwrap();
        assert_eq!(guess.artists, ["Chan"], "guard drops names not sent");
        assert_eq!(guess.title.as_deref(), Some("Song"));
        assert_eq!(guess.album, None);
        let req = served.await.unwrap();
        assert!(req.starts_with("POST /v1/responses "), "{req}");
        assert!(
            req.contains("\"tool_choice\"") && req.contains(TOOL),
            "{req}"
        );
    }

    #[tokio::test]
    async fn guess_without_the_tool_call_is_an_error() {
        let (url, _served) = serve_once(response(serde_json::json!([text_item("a table")]))).await;
        let g = Guesser::new(&config(url)).unwrap();
        let err = g.guess(&video("Song", "")).await.unwrap_err();
        assert!(err.to_string().contains("did not call"), "{err}");
    }

    /// Needs `YT_AUDIO_LLM_*` in the environment and network.
    #[tokio::test]
    #[ignore = "needs YT_AUDIO_LLM_* and network"]
    async fn real_endpoint() {
        let cli = Cli::parse_from(vec!["yt-audio".into(), "https://e.example/u".into()]).unwrap();
        let cfg = LlmConfig::from_cli(&cli).expect("set YT_AUDIO_LLM_MODEL");
        let v = parse_video(include_str!("../tests/fixtures/no_pattern.json")).unwrap();
        let guess = Guesser::new(&cfg).unwrap().guess(&v).await.unwrap();
        let tags = crate::resolve::resolve(
            &v,
            &crate::resolve::Overrides::default(),
            Some(&guess),
            true,
        );
        assert_eq!(tags.title.source, crate::resolve::Source::Llm);
        eprintln!(
            "{} ({}) / {:?} ({})",
            tags.title.value, tags.title.source, tags.artists.value, tags.artists.source
        );
    }
}
