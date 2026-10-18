//! Finding a browser with a logged-in YouTube session.

use std::ffi::OsStr;
use std::path::Path;

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

/// Pick the first candidate browser whose cookies hold a YouTube login.
///
/// Each candidate is probed by having `program` (yt-dlp) export its cookie jar.
pub fn detect(home: &Path, program: &OsStr) -> Cookies {
    let jar = std::env::temp_dir().join(format!("yt-audio-cookies-{}.txt", std::process::id()));
    let mut found = Cookies::None;
    for spec in candidates(home) {
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
        let _ = ytdlp::run(program, &args);
        let logged_in = std::fs::read_to_string(&jar).is_ok_and(|j| has_youtube_login(&j));
        let _ = std::fs::remove_file(&jar);
        if logged_in {
            found = Cookies::Browser(spec);
            break;
        }
    }
    found
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
    #[ignore = "needs yt-dlp on PATH and a logged-in browser"]
    fn detect_real_browsers() {
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
        let got = detect(&home, OsStr::new("yt-dlp"));
        // Report only the spec, never cookie values.
        eprintln!("detected: {got:?}");
        assert!(matches!(got, Cookies::Browser(_)));
    }
}
