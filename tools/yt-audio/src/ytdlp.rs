//! yt-dlp command construction and execution.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use url::Url;

use crate::cli::AudioFormat;
use crate::cookies::Cookies;
use crate::error::Error;

/// Start of every invocation, plus the cookie flag when one is chosen.
fn base(c: &Cookies) -> Vec<String> {
    let mut args = vec!["--ignore-config".to_string()];
    args.extend(c.args());
    args
}

/// Arguments to dump a (possibly playlist) listing as flat JSON.
pub fn listing_args(url: &Url, c: &Cookies) -> Vec<String> {
    let mut args = base(c);
    args.extend(["-J", "--flat-playlist"].map(String::from));
    args.extend(["--".to_string(), url.to_string()]);
    args
}

/// Arguments to dump one video's full JSON.
pub fn info_args(url: &Url, c: &Cookies) -> Vec<String> {
    let mut args = base(c);
    args.extend(["-J", "--no-playlist"].map(String::from));
    args.extend(["--".to_string(), url.to_string()]);
    args
}

/// Arguments to download and extract audio to `<dest_no_ext>.<ext>`.
///
/// yt-dlp prints the final path on stdout; `extra` goes just before the `--` that ends options.
pub fn download_args(
    url: &Url,
    dest_no_ext: &Path,
    fmt: AudioFormat,
    force: bool,
    c: &Cookies,
    extra: &[String],
) -> Vec<String> {
    let mut args = base(c);
    args.extend(["--no-playlist", "-f", "bestaudio", "-x"].map(String::from));
    let audio = match fmt {
        AudioFormat::Opus => Some("opus"),
        AudioFormat::M4a => Some("m4a"),
        AudioFormat::Mp3 => Some("mp3"),
        AudioFormat::Best => None,
    };
    if let Some(audio) = audio {
        args.extend(["--audio-format", audio, "--audio-quality", "0"].map(String::from));
    }
    args.push("--embed-thumbnail".to_string());
    args.push(
        if force {
            "--force-overwrites"
        } else {
            "--no-overwrites"
        }
        .to_string(),
    );
    args.push("-o".to_string());
    args.push(format!(
        "{}.%(ext)s",
        dest_no_ext.display().to_string().replace('%', "%%")
    ));
    args.extend(["--print", "after_move:filepath"].map(String::from));
    args.extend(extra.iter().cloned());
    args.extend(["--".to_string(), url.to_string()]);
    args
}

/// Run `program` with `args`, returning its stdout.
///
/// A non-zero exit becomes [`Error::YtDlp`] carrying the full stderr. With a
/// `timeout`, the process is killed and [`Error::Timeout`] returned if it has
/// not exited in time.
///
/// # Errors
///
/// [`Error::Spawn`], [`Error::Timeout`] or [`Error::YtDlp`] as above.
pub async fn run(
    program: &OsStr,
    args: &[String],
    timeout: Option<Duration>,
) -> Result<String, Error> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = match timeout {
        Some(t) => tokio::time::timeout(t, output)
            .await
            .map_err(|_| Error::Timeout(t))?,
        None => output.await,
    }
    .map_err(Error::Spawn)?;
    if !output.status.success() {
        return Err(Error::YtDlp {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Test-only: writing an executable while another thread forks leaves the
/// child holding the write fd, so exec fails with ETXTBSY. Script writers take
/// the write lock; anything that spawns takes the read lock.
#[cfg(test)]
pub(crate) static SPAWN_LOCK: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(ToString::to_string).collect()
    }

    fn u() -> Url {
        Url::parse("https://example.com/v/1").unwrap()
    }

    #[test]
    fn download_opus() {
        let extra = s(&["--foo"]);
        let a = download_args(
            &u(),
            Path::new("/d/x"),
            AudioFormat::Opus,
            false,
            &Cookies::None,
            &extra,
        );
        assert_eq!(a[0], "--ignore-config");
        assert!(a.windows(2).any(|w| w == ["--audio-format", "opus"]));
        assert!(a.contains(&"--no-overwrites".to_string()));
        assert!(a.windows(2).any(|w| w == ["-o", "/d/x.%(ext)s"]));
        assert_eq!(
            a[a.len() - 3..],
            s(&["--foo", "--", "https://example.com/v/1"])
        );
        let a = download_args(
            &u(),
            Path::new("/d/100%(x)s"),
            AudioFormat::Opus,
            false,
            &Cookies::None,
            &[],
        );
        assert!(a.windows(2).any(|w| w == ["-o", "/d/100%%(x)s.%(ext)s"]));
    }

    #[test]
    fn download_best_force_cookies() {
        let c = Cookies::Browser("chrome".into());
        let a = download_args(&u(), Path::new("x"), AudioFormat::Best, true, &c, &[]);
        assert!(!a.contains(&"--audio-format".to_string()));
        assert!(a.contains(&"--force-overwrites".to_string()));
        assert!(
            a.windows(2)
                .any(|w| w == ["--cookies-from-browser", "chrome"])
        );
    }

    #[test]
    fn listing_and_info() {
        let c = Cookies::Browser("chrome".into());
        let l = listing_args(&u(), &c);
        assert_eq!(l[0], "--ignore-config");
        assert!(l.contains(&"--flat-playlist".to_string()));
        assert_eq!(l[l.len() - 2..], s(&["--", "https://example.com/v/1"]));
        let i = info_args(&u(), &Cookies::None);
        assert_eq!(
            i,
            s(&[
                "--ignore-config",
                "-J",
                "--no-playlist",
                "--",
                "https://example.com/v/1"
            ])
        );
    }

    #[tokio::test]
    async fn run_keeps_full_stderr() {
        let _g = SPAWN_LOCK.read().await;
        let script = "echo out; for i in $(seq 30); do echo e$i >&2; done; exit 3";
        match run(OsStr::new("sh"), &s(&["-c", script]), None).await {
            Err(e @ Error::YtDlp { .. }) => {
                let Error::YtDlp { stderr, .. } = &e else {
                    unreachable!()
                };
                assert_eq!(stderr.lines().count(), 30);
                let shown = e.to_string();
                assert!(shown.ends_with("e30") && !shown.contains("e10\n"));
            }
            other => panic!("{other:?}"),
        }
        let ok = run(OsStr::new("sh"), &s(&["-c", "echo hi"]), None).await;
        assert_eq!(ok.unwrap(), "hi\n");
    }

    #[tokio::test]
    async fn timeout_kills_slow_process() {
        let _g = SPAWN_LOCK.read().await;
        let t = std::time::Instant::now();
        let r = run(
            OsStr::new("sh"),
            &s(&["-c", "sleep 30"]),
            Some(Duration::from_secs(1)),
        )
        .await;
        assert!(matches!(r, Err(Error::Timeout(_))), "{r:?}");
        assert!(t.elapsed() < Duration::from_secs(10));
    }

    #[tokio::test]
    async fn missing_program_is_a_spawn_error() {
        let r = run(OsStr::new("/nonexistent/yt-dlp"), &[], None).await;
        assert!(matches!(r, Err(Error::Spawn(_))), "{r:?}");
    }
}
