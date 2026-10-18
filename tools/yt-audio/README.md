# yt-audio

Download YouTube audio with tidy tags and filenames. yt-dlp fetches the audio
and cover art; yt-audio resolves the metadata and writes the text tags itself,
so multiple artists are stored as separate values.

## Usage

```console
yt-audio https://www.youtube.com/watch?v=XWoaP3EQRoQ
yt-audio -n <URL>                    # dry run: show tags and paths only
yt-audio -o ~/Music --fmt m4a <URL>
yt-audio --artist A --artist B <URL> # override artists
yt-audio <URL> -- --limit-rate 1M    # extra yt-dlp download arguments
```

Each URL can be a video or a playlist. A video lands in
`<output-dir or .>/<album>/<title>.<ext>`; a playlist in
`<output-dir or ./<playlist title>>/<title>.<ext>`. Existing files are skipped
unless `--force`. A failing entry is reported and the run continues; the exit
code is 1 if any entry failed.

## Formats

`--fmt opus` (default) and `best` keep YouTube's stream without re-encoding;
`m4a` and `mp3` re-encode.

## Cookies

Unless `--no-cookies` or `--cookies-from-browser <SPEC>` is given, the first
browser with a YouTube login is used, probed in the order Firefox, Floorp,
Zen, Chrome. Floorp and Zen are passed to yt-dlp as `firefox:<profile dir>`.
None found: a warning, then no cookies.

## Metadata sources

First hit wins per field, and `--dry-run` shows which one won:

1. CLI overrides (`--artist`, `--title`, `--album`)
2. YouTube's music fields
3. Title patterns (`Artist - Title`, `Title / Artist`, `Artist「Title」`)
4. An LLM guess, if the `llm` feature is built and configured
5. Fallback: the cleaned title, and the channel name as artist

## LLM

Only compiled with the `llm` feature (`cargo build --features llm`; the Nix
`yt-audio` package has it, `yt-audio-no-llm` does not). It is used only when a
model is configured, and only for entries the YouTube fields and title patterns
left unresolved, unless `--llm-always` is given.

| Flag            | Env var                | Meaning                          |
| --------------- | ---------------------- | -------------------------------- |
| `--llm-backend` | `YT_AUDIO_LLM_BACKEND` | `ollama` (default) or `openai`   |
| `--llm-url`     | `YT_AUDIO_LLM_URL`     | Endpoint base URL                |
| `--llm-model`   | `YT_AUDIO_LLM_MODEL`   | Model name; enables the LLM      |
| (none)          | `YT_AUDIO_LLM_KEY`     | API key; env only, never a flag  |

`openai` means any OpenAI-compatible endpoint. The model sees the title,
channel and the first 500 characters of the description, and any artist, title
or album it returns that does not appear in that text is discarded. Errors and
timeouts (30 s) warn and fall back; they never abort a download.

## Development

```console
cargo test       # inside `nix develop`; no network needed
cargo test -- --ignored e2e_real_video   # needs network and yt-dlp
```
