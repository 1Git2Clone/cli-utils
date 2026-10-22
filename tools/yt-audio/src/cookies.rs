//! Choosing which browser's cookies yt-dlp uses for each site.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::future::join_all;
use url::Url;

use crate::error::Error;
use crate::ytdlp;

/// Cookie source for one yt-dlp call.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Cookies {
    /// No cookies.
    #[default]
    None,
    /// A `--cookies-from-browser` spec such as `firefox:/path/to/profile`.
    Browser(String),
}

impl Cookies {
    /// The yt-dlp arguments selecting this source.
    pub fn args(&self) -> Vec<String> {
        match self {
            Self::None => vec![],
            Self::Browser(spec) => vec!["--cookies-from-browser".into(), spec.clone()],
        }
    }
}

/// A browser yt-dlp can read cookies from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Browser {
    Firefox,
    /// Firefox fork; read as `firefox:<profile dir>`.
    Floorp,
    /// Firefox fork; read as `firefox:<profile dir>`.
    Zen,
    Chrome,
}

impl Browser {
    /// The `--cookies-from-browser` spec, or `None` for a fork with no
    /// profile directory under `home`.
    pub fn spec(self, home: &Path) -> Option<String> {
        let fork = |dirs: [&str; 2]| {
            dirs.iter()
                .map(|d| home.join(d))
                .find(|d| d.exists())
                .map(|d| format!("firefox:{}", d.display()))
        };
        match self {
            Self::Firefox => Some("firefox".into()),
            Self::Floorp => fork([".floorp", ".config/floorp"]),
            Self::Zen => fork([".zen", ".config/zen"]),
            Self::Chrome => Some("chrome".into()),
        }
    }
}

/// Browsers to probe, most preferred first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserOrder(pub Vec<Browser>);

impl Default for BrowserOrder {
    fn default() -> Self {
        Self(vec![
            Browser::Firefox,
            Browser::Floorp,
            Browser::Zen,
            Browser::Chrome,
        ])
    }
}

/// Cookie names that mark a logged-in session, for sites where they are known.
const LOGIN_COOKIES: &[(&str, &[&str])] = &[(
    "youtube.com",
    &["SAPISID", "__Secure-3PAPISID", "LOGIN_INFO"],
)];

/// The site a host belongs to: its last two labels.
// ponytail: no public-suffix list, so `example.co.uk` hosts all collapse to
// `co.uk`; switch to the `psl` crate if a multi-label suffix site matters.
fn site(host: &str) -> String {
    let host = host.trim_start_matches('.').to_ascii_lowercase();
    let mut labels = host.rsplitn(3, '.');
    match (labels.next(), labels.next()) {
        (Some(tld), Some(name)) => format!("{name}.{tld}"),
        _ => host,
    }
}

/// Which sites one browser holds cookies, and a known login session, for.
/// Domains only; cookie values never leave the probe.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Jar {
    sites: HashSet<String>,
    logged_in: HashSet<String>,
}

impl Jar {
    /// Summarise a Netscape cookie jar.
    fn summarize(netscape: &str) -> Self {
        let mut jar = Self::default();
        for f in netscape.lines().map(|l| l.split('\t').collect::<Vec<_>>()) {
            let [domain, _, _, _, _, name, _] = f[..] else {
                continue;
            };
            let domain = domain.strip_prefix("#HttpOnly_").unwrap_or(domain);
            let site = site(domain);
            let login = LOGIN_COOKIES
                .iter()
                .any(|(s, names)| *s == site && names.contains(&name));
            if login {
                jar.logged_in.insert(site.clone());
            }
            jar.sites.insert(site);
        }
        jar
    }

    /// Whether this jar is good enough for `site`: logged in where login
    /// cookies are known, otherwise any cookie for it.
    fn serves(&self, site: &str) -> bool {
        if LOGIN_COOKIES.iter().any(|(s, _)| *s == site) {
            self.logged_in.contains(site)
        } else {
            self.sites.contains(site)
        }
    }
}

/// How cookies are picked for a URL.
#[derive(Debug)]
pub enum CookieChoice {
    /// The same source for every URL (`--cookies-from-browser`, `--no-cookies`).
    Fixed(Cookies),
    /// The first probed browser, in preference order, serving the URL's site.
    PerSite(Vec<(String, Jar)>),
}

impl CookieChoice {
    /// The cookie source for `url`.
    pub fn for_url(&self, url: &Url) -> Cookies {
        match self {
            Self::Fixed(c) => c.clone(),
            Self::PerSite(jars) => {
                let Some(site) = url.host_str().map(site) else {
                    return Cookies::None;
                };
                jars.iter()
                    .find(|(_, jar)| jar.serves(&site))
                    .map_or(Cookies::None, |(spec, _)| Cookies::Browser(spec.clone()))
            }
        }
    }

    /// Whether no browser serves any site at all.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::PerSite(jars) if jars.iter().all(|(_, j)| j.sites.is_empty()))
    }
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

/// Export one browser's cookie jar via `program` (yt-dlp) and summarise it.
///
/// The jar (every cookie in the browser) lands in a fresh private directory
/// removed when the probe ends, so it never outlives or is reused across
/// probes. `Err` when the probe cannot run at all (no private directory, or
/// `program` cannot be started).
async fn probe(program: &OsStr, spec: &str, url: &Url) -> Result<Jar, Error> {
    let dir = tempfile::Builder::new()
        .prefix("yt-audio-")
        .tempdir_in(jar_base())?;
    let jar = dir.path().join("cookies.txt");
    let mut args = vec!["--ignore-config".to_string()];
    args.extend(Cookies::Browser(spec.to_owned()).args());
    args.extend(["--cookies".into(), jar.display().to_string()]);
    args.extend(
        [
            "--skip-download",
            "--simulate",
            "--no-warnings",
            // Feeds and channels can be endless; one flat entry is enough.
            "--flat-playlist",
            "--playlist-end",
            "1",
            "--",
        ]
        .map(String::from),
    );
    args.push(url.to_string());
    if let Err(e @ Error::Spawn(_)) = ytdlp::run(program, &args, Some(PROBE_TIMEOUT)).await {
        return Err(e);
    }
    Ok(tokio::fs::read_to_string(&jar)
        .await
        .map(|j| Jar::summarize(&j))
        .unwrap_or_default())
}

/// Probe every browser in `order` (concurrently, with `url` as the request
/// that makes yt-dlp export the jar) and index which sites each serves.
pub async fn detect(home: &Path, program: &OsStr, url: &Url, order: &BrowserOrder) -> CookieChoice {
    let specs: Vec<String> = order.0.iter().filter_map(|b| b.spec(home)).collect();
    let jars = join_all(specs.iter().map(|spec| probe(program, spec, url))).await;
    CookieChoice::PerSite(
        specs
            .into_iter()
            .zip(jars)
            .filter_map(|(spec, jar)| Some((spec, jar.ok()?)))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const YT_LOGIN: &str = ".youtube.com\tTRUE\t/\tTRUE\t0\tSAPISID\tx";
    const YT_ANON: &str = ".youtube.com\tTRUE\t/\tTRUE\t0\tPREF\tx";
    const BC: &str = "#HttpOnly_.bandcamp.com\tTRUE\t/\tTRUE\t0\tsession\tx";

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn site_is_last_two_labels() {
        assert_eq!(site(".music.youtube.com"), "youtube.com");
        assert_eq!(site("WWW.YouTube.com"), "youtube.com");
        assert_eq!(site("localhost"), "localhost");
    }

    #[test]
    fn jar_summary_needs_login_where_known() {
        let anon = Jar::summarize(YT_ANON);
        assert!(anon.sites.contains("youtube.com") && !anon.serves("youtube.com"));
        assert!(Jar::summarize(YT_LOGIN).serves("youtube.com"));
        assert!(Jar::summarize(BC).serves("bandcamp.com"));
        assert!(!Jar::summarize("garbage\nline").serves("bandcamp.com"));
    }

    #[test]
    fn per_site_choice_follows_preference_order() {
        let choice = CookieChoice::PerSite(vec![
            (
                "firefox".into(),
                Jar::summarize(&format!("{YT_ANON}\n{BC}")),
            ),
            ("chrome".into(), Jar::summarize(YT_LOGIN)),
        ]);
        let yt = url("https://music.youtube.com/watch?v=x");
        assert_eq!(choice.for_url(&yt), Cookies::Browser("chrome".into()));
        let bc = url("https://artist.bandcamp.com/track/t");
        assert_eq!(choice.for_url(&bc), Cookies::Browser("firefox".into()));
        let other = url("https://example.org/x");
        assert_eq!(choice.for_url(&other), Cookies::None);
        assert!(!choice.is_empty());
    }

    #[test]
    fn default_order_and_fork_specs() {
        assert_eq!(
            BrowserOrder::default().0,
            [
                Browser::Firefox,
                Browser::Floorp,
                Browser::Zen,
                Browser::Chrome
            ]
        );
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".config/floorp")).unwrap();
        assert_eq!(
            Browser::Floorp.spec(home.path()),
            Some(format!("firefox:{}/.config/floorp", home.path().display()))
        );
        assert_eq!(Browser::Zen.spec(home.path()), None);
    }

    #[tokio::test]
    async fn detect_with_fake_program() {
        use std::os::unix::fs::PermissionsExt;
        let work = tempfile::tempdir().unwrap();
        let marker = work.path().join("marker");
        let script = work.path().join("fake-yt-dlp");
        {
            let _w = ytdlp::SPAWN_LOCK.write().await;
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
        }
        let _g = ytdlp::SPAWN_LOCK.read().await;
        let home = work.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let yt = url("https://www.youtube.com/watch?v=x");
        let choice = detect(&home, script.as_os_str(), &yt, &BrowserOrder::default()).await;
        assert_eq!(choice.for_url(&yt), Cookies::Browser("chrome".into()));
        let seen = std::fs::read_to_string(&marker).unwrap();
        let jars: Vec<&str> = seen.lines().collect();
        assert_eq!(jars.len(), 2, "firefox and chrome; no fork profiles");
        assert_ne!(jars[0], jars[1], "fresh dir per probe");
        for j in jars {
            assert!(!Path::new(j).parent().unwrap().exists());
        }
    }

    #[tokio::test]
    async fn detect_without_program_serves_nothing() {
        let home = tempfile::tempdir().unwrap();
        let yt = url("https://www.youtube.com/watch?v=x");
        let choice = detect(
            home.path(),
            OsStr::new("/nonexistent/yt-dlp"),
            &yt,
            &BrowserOrder::default(),
        )
        .await;
        assert!(choice.is_empty());
        assert_eq!(choice.for_url(&yt), Cookies::None);
    }

    #[tokio::test]
    #[ignore = "needs yt-dlp on PATH and a logged-in browser"]
    async fn detect_real_browsers() {
        let home = PathBuf::from(std::env::var("HOME").unwrap());
        let yt = url("https://www.youtube.com/watch?v=XWoaP3EQRoQ");
        let choice = detect(&home, OsStr::new("yt-dlp"), &yt, &BrowserOrder::default()).await;
        // Report only the spec, never cookie values.
        let got = choice.for_url(&yt);
        eprintln!("detected: {got:?}");
        assert!(matches!(got, Cookies::Browser(_)));
    }
}
