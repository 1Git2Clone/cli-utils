# yt-audio — design

A Rust CLI in `tools/yt-audio` that downloads audio from YouTube (single videos
and playlists) and writes clean, multi-valued music tags, so the files land in a
library like Navidrome looking right without hand-editing.

> **Name is the designer's call** — rename freely before the first commit.

## Why

The author's starting point was this one-off command:

```sh
yt-dlp --cookies-from-browser "firefox:/home/hutao/.config/floorp/" \
  --force-overwrites -x --audio-format m4a --audio-quality 0 \
  --embed-metadata --embed-thumbnail \
  --parse-metadata "title:(?P<artist>.+?) - (?P<track>.+)" \
  --replace-in-metadata "track" "^\s*「\s*|\s*」.*$" "" \
  --parse-metadata "%(track)s:%(album)s" \
  --parse-metadata "artist:(?P<album_artist>.+?)(?: feat\..*)?$" \
  -o "%(album)s/%(track)s.%(ext)s" URL
```

It worked for one video and nothing else: the ordered regex chain encodes one
uploader's title format, and it is unreadable. The goal is a **reusable tool**,
not a personal script — it has to do something sensible for arbitrary videos,
and be honest when it is guessing.

## Goals

- Everything the example command does: browser cookies, audio extraction,
  embedded metadata and thumbnail, artist/title/album/album-artist tags,
  `<album>/<track>.<ext>` layout.
- **Multiple artists as real multi-valued tags** (one `ARTIST` field per
  artist), not a joined string. Navidrome shows these as `A • B`.
- Metadata that works across uploaders, via a layered strategy (below), with
  overrides for when it is wrong.
- Playlists.
- Optional LLM-assisted parsing behind a Cargo feature.
- CI for both feature sets, with dependency caching.

## Non-goals

- No agent / tool-calling loop around the LLM — a single structured request.
- No MusicBrainz lookup (revisit only if wrong guesses become a real annoyance).
- No embedded/bundled model — LLMs are reached over HTTP only.
- Not a general yt-dlp frontend; video download is out of scope.

## Anti-goals (would be failure even if it "works")

- Regex chains the user has to read or write to get correct tags.
- Silent wrong tags: a guess must be visible as a guess (`--dry-run`).
- A failed LLM call, or one bad playlist entry, aborting the whole run.
- Running as root.

## Constraints

- Rust 2024, `clap` derive, same crate shape as `tools/ls-posts` (`main.rs` /
  `lib.rs` / `cli.rs` / `error.rs`, `thiserror`, `unsafe_code = "forbid"`).
- Runtime dependencies: `yt-dlp` and `ffmpeg`. The Nix package wraps the binary
  so both are on its `PATH`.
- Must refuse to run as root (euid 0) with a clear error.
- CI never touches the network for LLM calls.

## Behaviour

### Invocation

```text
yt-audio [OPTIONS] <URL>... [-- <YT-DLP ARGS>...]
```

Each URL can be a video or a playlist. Anything after `--` is forwarded verbatim
to every yt-dlp download call (this is how "more metadata/embedding options" are
added — no per-option flags for them).

### Options

| Flag                            | Default    | Meaning                                                                                                       |
| ------------------------------- | ---------- | ------------------------------------------------------------------------------------------------------------- |
| `--cookies-from-browser <SPEC>` | auto       | Override cookie source; SPEC is passed to yt-dlp as-is (e.g. `firefox:/path/to/profile`).                     |
| `--no-cookies`                  | off        | Disable cookie lookup entirely.                                                                               |
| `--fmt <opus\|m4a\|mp3\|best>`  | `opus`     | Output audio format. `opus` and `best` keep YouTube's stream without re-encoding.                             |
| `-o, --output-dir <DIR>`        | see Layout | Override the destination folder.                                                                              |
| `--artist <A>` (repeatable)     | —          | Override artists.                                                                                             |
| `--title <T>`                   | —          | Override track title.                                                                                         |
| `--album <A>`                   | —          | Override album.                                                                                               |
| `--force`                       | off        | Overwrite existing files (default: skip them).                                                                |
| `-n, --dry-run`                 | off        | Resolve and print tags + destination path per entry, with which source produced each field; download nothing. |
| `-q / -v`                       | —          | Quiet / verbose, as in ls-posts.                                                                              |
| LLM flags                       | —          | Only compiled with the `llm` feature; see LLM.                                                                |

Overrides apply to every entry; with a playlist that mainly makes sense for
`--album` / `--artist`. (Designer's call.)

### Cookies

Default (no flag): probe browsers in this order and use the first that has a
logged-in YouTube session — **Firefox, Floorp, Zen, Chrome**. ("Zed" in the
conversation is read as the Zen browser — designer's call, confirm.) Floorp and
Zen are Firefox forks: pass them to yt-dlp as `firefox:<profile dir>`.

"Has valid YouTube cookies" = the browser's cookie store contains a
`youtube.com` auth cookie (`SAPISID`, `__Secure-3PAPISID` or `LOGIN_INFO`). How
to check is the builder's call (e.g. have yt-dlp export cookies to a temp file
with `--cookies-from-browser B --cookies <tmp>` and inspect it).

None found → warn once and continue without cookies.

### Format

Default `opus`: YouTube serves Opus at ~160 kbps VBR, so the varying bitrates
seen in Navidrome (150/153/174…) are normal averages. Extract without
re-encoding. The example's `m4a` re-encoded Opus→AAC (lossy-to-lossy, 417 kbps
files); it stays available but is no longer the default. Thumbnail embedding
must work for Opus.

### Metadata resolution

Per entry, fill `artists[]`, `title`, `album`, `album_artist`. Sources, first
hit wins **per field**, and the winning source is reported in `--dry-run`:

1. **CLI overrides.**
2. **YouTube's music fields** from `yt-dlp -J`: `artists`/`artist`, `track`,
   `album`, `release_year`/`release_date`. Present for Topic channels, YouTube
   Music and "Provided to YouTube by…" uploads.
3. **Title patterns**, each a small pure function with its own tests, tried in
   order after stripping noise tags (`【MV】`, `[MV]`, `(Official Video)`,
   `(Official Audio)`, `[Lyrics]`, `(Lyric Video)` and similar):
   - `Artist - Title`
   - `Title / Artist` (common for Japanese/Vocaloid uploads)
   - `Artist「Title」` and `Artist『Title』` Exact pattern list and order beyond
     these is the builder's call; adding a pattern later should mean adding one
     function plus tests.
4. **LLM** (feature `llm`, only if configured) — see below.
5. **Fallback:** title = whole cleaned title, artist = channel name minus a
   trailing `" - Topic"`.

Post-processing, regardless of source:

- **Artist splitting:** an artist string is split into multiple artists on
  `feat.`, `ft.`, `featuring` (any case, with or without the dot) and `/`. `&`,
  `x`, `,` are **not** split on (band names). Builder may refine with tests.
- **album_artist** = first artist.
- **album** = YouTube's album field if present, otherwise the title (a single).
- **date** = release date if present, otherwise upload date.
- **comment** = the video URL (matches what the example produced).
- **track number** = only when YouTube's music fields give one.

### Writing tags

yt-dlp downloads, extracts and embeds the thumbnail; **yt-audio then writes the
text tags itself** (e.g. with the `lofty` crate), so multiple artists are stored
as separate values. No `--parse-metadata` / `--replace-in-metadata` anywhere.
(Designer's call — it is what makes multi-artist correct for every player, not
only Navidrome's string splitting.)

### Layout

- Single video: `<output-dir or .>/<album>/<title>.<ext>` (as the example).
- Playlist: `<output-dir or ./<playlist title>>/<title>.<ext>`, flat.
- File and folder names are sanitised (`/`, NUL, and anything else illegal on
  common filesystems).

### Flow and failure handling

1. `yt-dlp -J` per URL to get info (playlists expand to entries).
2. Resolve metadata for every entry (this is all `--dry-run` does, then prints).
3. Per entry: download with an explicit output path → write tags.
4. A failing entry is reported and skipped; the run continues. Exit non-zero if
   any entry failed, with a summary at the end.

## LLM (Cargo feature `llm`)

- Off by default in Cargo. Built **without** it, the binary has no LLM code,
  flags or async runtime.
- Client: the [`llm`](https://crates.io/crates/llm) crate with
  `default-features = false` and only `openai`, `ollama`, `rustls-tls` (its
  defaults pull a whole TUI app). It is async, so `tokio` is an optional
  dependency enabled by this feature.
- Config (flag or env var, flag wins):
  - `--llm-backend <openai|ollama>` — `openai` means any OpenAI-compatible
    endpoint (MiniMax, OpenRouter, LM Studio, llama.cpp server…).
  - `--llm-url`, `--llm-model`.
  - API key from an **env var only** (e.g. `YT_AUDIO_LLM_KEY`), never a flag —
    keys on argv leak into shell history and `ps`.
  - LLM is active only when a model is configured.
- When: only for entries where sources 1–3 left title or artist unresolved,
  unless `--llm-always` is passed (then it runs for every entry and outranks
  title patterns, but not YouTube's music fields or overrides).
- Request: title, channel name, first ~500 chars of description. Response must
  be JSON `{ "artists": [..], "title": "..", "album": ".." | null }`.
- **Hallucination guard:** discard any artist or title that does not appear
  (case-insensitive) in the title, channel or description sent.
- Timeout (builder's call, ~30 s), and on any error/timeout/invalid JSON: warn
  and fall through to the fallback. Never abort the download.

## Testing

- Unit tests for every title pattern, noise stripping, artist splitting,
  filename sanitising, and the source-priority logic.
- Integration tests run the resolver against **canned `yt-dlp -J` JSON
  fixtures** checked into `tests/fixtures` (a Topic-channel video, a "Artist -
  Title" upload, a 「」 upload, a `Title / Artist` upload, a playlist). No
  network.
- LLM: the guard and the response parsing are unit-tested with canned responses.
  A real-endpoint test exists but is `#[ignore]` and reads its key from the
  environment; it is run locally only.

## Nix

- New `buildRustPackage` in `toolsFor`, following the ls-posts entry (incl. the
  `CARGO_HOME` preConfigure), wrapped so `yt-dlp` and `ffmpeg` are on `PATH`.
- Two attributes (designer's call): `yt-audio` built **with** `llm` (the
  author's daily driver; this is what `default` bundles) and `yt-audio-no-llm`
  without.
- `rustManifest` currently names only ls-posts; the pre-commit rust hooks
  (rustfmt, clippy, cargo-check) must cover the new crate too. How (second hook
  instance, a Cargo workspace, …) is the builder's call — a workspace is likely
  simplest.

## CI

Both `.forgejo/workflows/ci.yml` and its `.github` twin get a cargo job,
modelled on `~/Projects/serenity-discord-bot/.forgejo/workflows/ci.yml`:

- Matrix over features: `""` and `"--features llm"`; each leg runs
  `cargo clippy --all-targets <features> -- -D warnings` and
  `cargo test <features>` for `tools/yt-audio`.
- `Swatinem/rust-cache@v2` with a **per-leg key**: the feature string sanitised
  to alphanumerics and truncated (`tr -cd '[:alnum:]' | cut -c1-32`), so legs
  don't overwrite each other's target dir. `workspaces:` points at the crate (or
  workspace root). rust-cache's own key already derives from `Cargo.lock` and
  toolchain, which is the "stable keys" requirement.
- `CARGO_PROFILE_DEV_DEBUG: 0` to shrink what the cache uploads.
- On Forgejo, third-party actions need full `https://github.com/...` URLs (as in
  serenity). Runner label and whether the toolchain comes from
  `dtolnay/rust-toolchain` or `nix develop .#ci` is the builder's call —
  cli-utils currently uses `nix-node`.
- The existing `nix build .#default` step keeps building the packaged tool.

## Left to the builder

Crate layout inside `src/`, exact pattern list beyond the listed ones, how
cookies are probed, the tag-writing crate, workspace vs. two manifests, CI
toolchain source, timeouts, and output formatting of `--dry-run`.
