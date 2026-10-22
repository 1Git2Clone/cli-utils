# yt-audio

Download audio from any site yt-dlp supports, with tidy tags and filenames.
yt-dlp fetches the audio and cover art; yt-audio resolves the metadata and
writes the text tags itself, so multiple artists are stored as separate values.

## Usage

```console
yt-audio https://www.youtube.com/watch?v=XWoaP3EQRoQ
yt-audio -n <URL>                    # dry run: show tags and paths only
yt-audio -o ~/Music --fmt m4a <URL>
yt-audio --artist A --artist B <URL> # override artists
yt-audio -j 8 <PLAYLIST URL>         # fetch and download 8 entries at once
yt-audio <URL> -- --limit-rate 1M    # extra yt-dlp download arguments
```

Each URL can be a video or a playlist. A video lands in
`<output-dir or root>/<album>/<title>.<ext>`; a playlist in
`<output-dir or root/<playlist title>>/<title>.<ext>`. The root is `--root`,
else `$YT_AUDIO_ROOT`, else the current directory; point it at a music library
to file albums and playlists there from anywhere. Existing files are skipped
unless `--force`. Entries are fetched and downloaded concurrently (`--jobs`,
default 4) but reported in playlist order. A failing entry is reported and the
run continues; the exit code is 1 if any entry failed.

## Formats

`--fmt opus` (default) and `best` keep the site's stream without re-encoding
when it already is that format; `m4a` and `mp3` re-encode.

## Cover art

The site's thumbnail is embedded as a PNG, cropped to its centre square:
YouTube serves album art as a 16:9 frame with bars either side. A thumbnail
that really is 16:9 loses its sides.

## Cookies

Unless `--no-cookies` or `--cookies-from-browser <SPEC>` is given, each URL
gets the first browser, in the order Firefox, Floorp, Zen, Chrome, holding
cookies for that URL's site; for YouTube that means a logged-in session.
Floorp and Zen are passed to yt-dlp as `firefox:<profile dir>`. No browser
with cookies for a site means no cookies for it. Sites are matched by the last
two host labels, so `youtu.be` links get none.

## Metadata sources

First hit wins per field, and `--dry-run` shows which one won:

1. CLI overrides (`--artist`, `--title`, `--album`)
2. The site's music fields (e.g. YouTube Music's artist, track, album)
3. Title patterns (`Artist - Title`, `Title / Artist`, `Artist「Title」`)
4. An LLM guess, if the `llm` feature is built and configured
5. Fallback: the cleaned title, and the channel name as artist

## LLM

Only compiled with the `llm` feature (`cargo build --features llm`; the Nix
`yt-audio` package has it, `yt-audio-no-llm` does not). It is used only when a
model is configured, and only for entries the site fields and title patterns
left unresolved, unless `--llm-always` is given.

| Flag            | Env var                | Meaning                          |
| --------------- | ---------------------- | -------------------------------- |
| `--llm-backend` | `YT_AUDIO_LLM_BACKEND` | `ollama` (default) or `openai`   |
| `--llm-url`     | `YT_AUDIO_LLM_URL`     | Endpoint base URL                |
| `--llm-model`   | `YT_AUDIO_LLM_MODEL`   | Model name; enables the LLM      |
| (none)          | `YT_AUDIO_LLM_KEY`     | API key; env only, never a flag  |

`openai` means an endpoint serving OpenAI's Responses API (`/v1/responses`),
such as OpenAI or MiniMax (`https://api.minimax.io/v1`); servers offering only
`/v1/chat/completions` are not supported. The model must answer by calling a
tool, so its reply is typed JSON even when it reasons aloud. It sees the
title, channel and the first 500 characters of the description, and any
artist, title or album it returns that does not appear in that text is
discarded. Errors and timeouts (30 s) warn and fall back; they never abort a
download.

## Releasing

Bump `version` in `Cargo.toml` (and `flake.nix`'s `ytAudio`), commit, then
push an annotated tag; its message becomes the release notes:

```console
git tag -a yt-audio-v0.1.2 -m "yt-audio 0.1.2" -m "What changed."
git push origin yt-audio-v0.1.2
```

`.forgejo/workflows/yt-audio-release.yml` then runs clippy and the Nix builds
(which run the tests) on the tagged commit. Only if they pass does it publish
to crates.io and create the Forgejo release. It needs two repository secrets:
`CARGO_REGISTRY_TOKEN` (crates.io, publish-update) and `RELEASE_TOKEN`
(a Forgejo token with repository write on cli-utils).

## Development

```console
cargo test       # inside `nix develop`; no network needed
cargo test -- --ignored e2e_real_video   # needs network and yt-dlp
```
