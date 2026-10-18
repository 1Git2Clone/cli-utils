//! Finding a browser with a logged-in YouTube session.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::Error;
use crate::ytdlp;

/// Cookie source for yt-dlp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cookies {
    /// No cookies.
    None,
    /// A `--cookies-from-browser` spec such as `firefox:/path/to/profile`.
    Browser(String),
}

impl Cookies {
    /// The yt-dlp arguments selecting this source.
    pub fn args(&self) -> Vec<String> {
        match self {
            Cookies::None => vec![],
            Cookies::Browser(spec) => vec!["--cookies-from-browser".into(), spec.clone()],
        }
    }
}

/// Cookie names that only exist for a logged-in YouTube user.
const LOGIN_COOKIES: [&str; 3] = ["SAPISID", "__Secure-3PAPISID", "LOGIN_INFO"];

/// Whether a Netscape cookie jar holds a YouTube login cookie.
pub fn has_youtube_login(netscape_jar: &str) -> bool {
    netscape_jar.lines().any(|line| {
        let f: Vec<&str> = line.split('\t').collect();
        f.len() >= 7 && f[0].ends_with("youtube.com") && LOGIN_COOKIES.contains(&f[5])
    })
}

/// Browser specs to try, in order of preference.
///
/// Floorp and Zen are Firefox forks, so they are passed as `firefox:<dir>`.
pub fn candidates(home: &Path) -> Vec<String> {
    let mut out = vec!["firefox".to_string()];
    for dirs in [[".floorp", ".config/floorp"], [".zen", ".config/zen"]] {
        if let Some(dir) = dirs.iter().map(|d| home.join(d)).find(|d| d.exists()) {
            out.push(format!("firefox:{}", dir.display()));
        }
    }
    out.push("chrome".to_string());
    out
}

/// Time allowed for one browser probe.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// Directory for a private jar: `$XDG_RUNTIME_DIR` when usable, else the
/// system temp dir.
fn jar_base() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

/// Pick the first candidate browser whose cookies hold a YouTube login.
///
/// Each candidate is probed by having `program` (yt-dlp) export its cookie jar
/// (every cookie in the browser) into a fresh private directory that is
/// removed when the probe ends, so no jar outlives or is reused across probes.
/// Returns [`Cookies::None`] if nothing is logged in or `program` cannot be
/// started.
pub fn detect(home: &Path, program: &OsStr) -> Cookies {
    for spec in candidates(home) {
        let Ok(dir) = tempfile::Builder::new()
            .prefix("yt-audio-")
            .tempdir_in(jar_base())
        else {
            return Cookies::None;
        };
        let jar = dir.path().join("cookies.txt");
        let mut args = vec!["--ignore-config".to_string()];
        args.extend(Cookies::Browser(spec.clone()).args());
        args.extend(["--cookies".into(), jar.display().to_string()]);
        args.extend(
            [
                "--skip-download",
                "--simulate",
                "--no-warnings",
                // Logged in, the home page is an endless feed; fetch one flat entry.
                "--flat-playlist",
                "--playlist-end",
                "1",
                "https://www.youtube.com/",
            ]
            .map(String::from),
        );
        if let Err(Error::Io(_)) = ytdlp::run_with_timeout(program, &args, PROBE_TIMEOUT) {
            return Cookies::None;
        }
        if std::fs::read_to_string(&jar).is_ok_and(|j| has_youtube_login(&j)) {
            return Cookies::Browser(spec);
        }
    }
    Cookies::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_detection() {
        assert!(has_youtube_login(
            ".youtube.com\tTRUE\t/\tTRUE\t0\tSAPISID\tx"
        ));
        assert!(!has_youtube_login(
            ".google.com\tTRUE\t/\tTRUE\t0\tSAPISID\tx"
        ));
        assert!(!has_youtube_login(
            ".youtube.com\tTRUE\t/\tTRUE\t0\tPREF\tx"
        ));
    }

    #[test]
    fn candidates_floorp_config() {
        let home = std::env::temp_dir().join(format!("yta-home-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".config/floorp")).unwrap();
        let got = candidates(&home);
        std::fs::remove_dir_all(&home).unwrap();
        assert_eq!(
            got,
            [
                "firefox".to_string(),
                format!("firefox:{}/.config/floorp", home.display()),
                "chrome".to_string()
            ]
        );
    }

    #[test]
    fn detect_with_fake_program() {
        use std::os::unix::fs::PermissionsExt;
        let work = tempfile::tempdir().unwrap();
        let marker = work.path().join("marker");
        let script = work.path().join("fake-yt-dlp");
        std::fs::write(
            &script,
            format!(
                r#"#!/bin/sh
while [ $# -gt 0 ]; do
  case "$1" in
    --cookies) jar="$2";; --cookies-from-browser) b="$2";;
  esac
  shift
done
echo "$jar" >> '{}'
[ "$b" = chrome ] && printf '.youtube.com\tTRUE\t/\tTRUE\t0\tSAPISID\tx\n' > "$jar"
exit 0
"#,
                marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let home = work.path().join("home");
        std::fs::create_dir(&home).unwrap();
        assert_eq!(
            detect(&home, script.as_os_str()),
            Cookies::Browser("chrome".into())
        );
        let seen = std::fs::read_to_string(&marker).unwrap();
        let jars: Vec<&str> = seen.lines().collect();
        assert_eq!(jars.len(), 2, "firefox then chrome");
        assert_ne!(jars[0], jars[1], "fresh dir per probe");
        for j in jars {
            assert!(!Path::new(j).exists());
            assert!(!Path::new(j).parent().unwrap().exists());
        }
    }

    #[test]
    fn detect_stops_when_program_missing() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            detect(home.path(), OsStr::new("/nonexistent/yt-dlp")),
            Cookies::None
        );
    }

    #[test]
    #[ignore = "needs yt-dlp on PATH and a logged-in browser"]
    fn detect_real_browsers() {
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
        let got = detect(&home, OsStr::new("yt-dlp"));
        // Report only the spec, never cookie values.
        eprintln!("detected: {got:?}");
        assert!(matches!(got, Cookies::Browser(_)));
    }
}
