use std::ffi::OsString;

fn run(args: Vec<OsString>) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = yt_audio::run(args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).expect("stdout is utf8"),
        String::from_utf8(err).expect("stderr is utf8"),
    )
}

fn args(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

#[test]
fn no_args_is_usage_error() {
    assert_eq!(run(vec![]).0, 2);
}

#[test]
fn help_mentions_fmt() {
    let (code, out, _) = run(args(&["--help"]));
    assert_eq!(code, 0);
    assert!(out.contains("--fmt"));
    assert!(out.contains("--dry-run"));
}

#[test]
fn cookie_flags_conflict() {
    let (code, _, _) = run(args(&[
        "--no-cookies",
        "--cookies-from-browser",
        "firefox",
        "u",
    ]));
    assert_eq!(code, 2);
}

/// Writing an executable while another thread forks makes exec fail with
/// ETXTBSY: script writers take the write lock, spawners the read lock.
static SPAWN_LOCK: std::sync::RwLock<()> = std::sync::RwLock::new(());

mod fake {
    use super::SPAWN_LOCK;
    use std::ffi::OsString;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    const FIX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

    /// A per-test work dir holding a fake yt-dlp.
    pub struct Fake {
        pub dir: PathBuf,
        script: PathBuf,
    }

    /// `listing` is a fixture name; info for URLs containing `bad_info`
    /// fails; downloads for URLs containing `bad` fail and others leave a
    /// marker file `downloaded`.
    pub fn fake(name: &str, listing: &str, bad_info: &str) -> Fake {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("yt-dlp");
        let _g = SPAWN_LOCK.write().unwrap();
        std::fs::write(
            &script,
            format!(
                r#"#!/bin/sh
for last; do :; done
case " $* " in
  *" --flat-playlist "*) cat '{FIX}/{listing}'; exit 0;;
  *" -J "*) case "$last" in *{bad_info}*) echo boom >&2; exit 1;; esac
            cat '{FIX}/artist_dash.json'; exit 0;;
esac
touch '{dir}/downloaded'
case "$last" in *bad*) exit 1;; esac
if [ -f '{dir}/input.opus' ]; then
  while [ $# -gt 0 ]; do [ "$1" = -o ] && o="$2"; shift; done
  p=$(printf %s "$o" | sed 's/%(ext)s$/opus/')
  mkdir -p "$(dirname "$p")"
  cp '{dir}/input.opus' "$p"
  echo "$p"
fi
exit 0
"#,
                dir = dir.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Fake { dir, script }
    }

    impl Fake {
        /// Make downloads produce a 1s Opus file; false if ffmpeg is missing.
        pub fn with_input(&self) -> bool {
            let _g = SPAWN_LOCK.read().unwrap();
            std::process::Command::new("ffmpeg")
                .args([
                    "-v", "error", "-y", "-f", "lavfi", "-i", "anullsrc", "-t", "1",
                ])
                .args(["-c:a", "libopus"])
                .arg(self.dir.join("input.opus"))
                .stdin(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        }

        pub fn run(&self, args: &[&str]) -> (i32, String, String) {
            let _g = SPAWN_LOCK.read().unwrap();
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let args: Vec<OsString> = ["--no-cookies"]
                .iter()
                .chain(args)
                .map(OsString::from)
                .collect();
            let code = yt_audio::run_with(args, &mut out, &mut err, self.script.as_os_str());
            (
                code,
                String::from_utf8(out).unwrap(),
                String::from_utf8(err).unwrap(),
            )
        }
    }
}

#[test]
fn dry_run_single_prints_tags() {
    let f = fake::fake("single", "artist_dash.json", "NONE");
    let (code, out, _) = f.run(&["-n", "https://y/a"]);
    assert_eq!(code, 0);
    assert!(out.contains("アリバイ"), "{out}");
    assert!(out.contains("(pattern:artist-dash)"), "{out}");
    assert!(out.contains("かめりあ; 初音ミク"), "{out}");
}

#[test]
fn dry_run_playlist_uses_playlist_folder() {
    let f = fake::fake("plist", "playlist_flat.json", "NONE");
    let (code, out, _) = f.run(&["-n", "https://y/p"]);
    assert_eq!(code, 0);
    assert!(out.contains("My Mix/"), "{out}");
}

#[test]
fn output_dir_overrides_playlist_folder() {
    let f = fake::fake("outdir", "playlist_flat.json", "NONE");
    let (code, out, _) = f.run(&["-n", "-o", "/tmp/x", "https://y/p"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("/tmp/x/"), "{out}");
    assert!(!out.contains("My Mix"), "{out}");
}

#[test]
fn run_continues_after_entry_failure() {
    let f = fake::fake("fail", "playlist_flat.json", "bad");
    let (code, out, err) = f.run(&["-n", "https://y/p"]);
    assert_eq!(code, 1);
    assert!(err.contains("1 of 2 entries failed"), "{err}");
    assert!(out.contains("アリバイ"), "{out}");
}

#[test]
fn existing_file_is_skipped() {
    let f = fake::fake("skip", "artist_dash.json", "NONE");
    let album = f.dir.join("out/アリバイ");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("アリバイ.opus"), b"x").unwrap();
    let out_dir = f.dir.join("out");
    let (code, out, err) = f.run(&["-o", out_dir.to_str().unwrap(), "https://y/a"]);
    assert_eq!(code, 0, "{err}");
    assert!(format!("{out}{err}").contains("skipped"), "{err}");
    assert!(!f.dir.join("downloaded").exists());
}

#[test]
fn partial_and_lookalike_files_are_not_skipped() {
    let f = fake::fake("partial", "artist_dash.json", "NONE");
    let album = f.dir.join("out/アリバイ");
    std::fs::create_dir_all(&album).unwrap();
    for name in ["アリバイ.opus.part", "アリバイ.live.opus", "アリバイ.webp"] {
        std::fs::write(album.join(name), b"x").unwrap();
    }
    let out_dir = f.dir.join("out");
    f.run(&["-o", out_dir.to_str().unwrap(), "https://y/a"]);
    assert!(f.dir.join("downloaded").exists(), "download not attempted");
}

#[test]
fn download_writes_resolved_tags() {
    use lofty::prelude::*;
    let f = fake::fake("tagged", "artist_dash.json", "NONE");
    if !f.with_input() {
        eprintln!("skipping: ffmpeg unavailable");
        return;
    }
    let out_dir = f.dir.join("out");
    let (code, _, err) = f.run(&["-o", out_dir.to_str().unwrap(), "https://y/a"]);
    assert_eq!(code, 0, "{err}");
    let file = out_dir.join("アリバイ/アリバイ.opus");
    let tagged = lofty::read_from_path(&file).unwrap();
    let tag = tagged.primary_tag().unwrap();
    assert_eq!(tag.title().as_deref(), Some("アリバイ"));
    assert_eq!(tag.get_strings(&ItemKey::TrackArtist).count(), 2);
}

#[test]
fn playlist_download_failure_continues() {
    let f = fake::fake("dlfail", "playlist_flat.json", "NONE");
    if !f.with_input() {
        eprintln!("skipping: ffmpeg unavailable");
        return;
    }
    let out_dir = f.dir.join("out");
    let (code, _, err) = f.run(&["-o", out_dir.to_str().unwrap(), "https://y/p"]);
    assert_eq!(code, 1);
    assert!(err.contains("1 of 2 entries failed"), "{err}");
    assert!(out_dir.join("アリバイ.opus").exists());
}

#[test]
fn verbose_echoes_commands_to_err() {
    let f = fake::fake("verbose", "artist_dash.json", "NONE");
    let (_, _, err) = f.run(&["-n", "-v", "https://y/a"]);
    assert!(err.contains("--ignore-config"), "{err}");
}

#[test]
fn duplicate_destinations_get_numbered() {
    let f = fake::fake("dupes", "playlist_flat.json", "NONE");
    let (code, out, _) = f.run(&["-n", "https://y/p"]);
    assert_eq!(code, 0);
    assert!(out.contains("/アリバイ.opus"), "{out}");
    assert!(out.contains("/アリバイ (2).opus"), "{out}");
}

#[test]
#[ignore = "needs network and yt-dlp"]
fn e2e_real_video() {
    let (code, out, _) = run(args(&[
        "-n",
        "--no-cookies",
        "https://www.youtube.com/watch?v=XWoaP3EQRoQ",
    ]));
    assert_eq!(code, 0);
    assert!(out.contains("アリバイ"), "{out}");
    assert!(out.contains("かめりあ; 初音ミク"), "{out}");
}
