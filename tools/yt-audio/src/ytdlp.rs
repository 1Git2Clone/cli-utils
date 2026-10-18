//! yt-dlp command construction and execution.

use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::cli::AudioFormat;
use crate::cookies::Cookies;
use crate::error::Error;

/// Number of trailing stderr lines kept in [`Error::YtDlp`].
const STDERR_TAIL_LINES: usize = 20;

/// Start of every invocation, plus the cookie flag when one is chosen.
fn base(c: &Cookies) -> Vec<String> {
    let mut args = vec!["--ignore-config".to_string()];
    args.extend(c.args());
    args
}

/// Arguments to dump a (possibly playlist) listing as flat JSON.
pub fn listing_args(url: &str, c: &Cookies) -> Vec<String> {
    let mut args = base(c);
    args.extend(["-J", "--flat-playlist"].map(String::from));
    args.extend(["--".to_string(), url.to_string()]);
    args
}

/// Arguments to dump one video's full JSON.
pub fn info_args(url: &str, c: &Cookies) -> Vec<String> {
    let mut args = base(c);
    args.extend(["-J", "--no-playlist"].map(String::from));
    args.extend(["--".to_string(), url.to_string()]);
    args
}

/// Arguments to download and extract audio to `<dest_no_ext>.<ext>`.
///
/// yt-dlp prints the final path on stdout; `extra` goes just before the `--` that ends options.
pub fn download_args(
    url: &str,
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

/// Turn a finished process into its stdout, or [`Error::YtDlp`].
fn finish(status: std::process::ExitStatus, stdout: &[u8], stderr: &[u8]) -> Result<String, Error> {
    if !status.success() {
        let stderr = String::from_utf8_lossy(stderr);
        let lines: Vec<&str> = stderr.lines().collect();
        let tail = lines[lines.len().saturating_sub(STDERR_TAIL_LINES)..].join("\n");
        return Err(Error::YtDlp {
            status,
            stderr_tail: tail,
        });
    }
    Ok(String::from_utf8_lossy(stdout).into_owned())
}

/// Run `program` with `args`, returning its stdout.
///
/// A non-zero exit becomes [`Error::YtDlp`] carrying the last lines of stderr.
pub fn run(program: &OsStr, args: &[String]) -> Result<String, Error> {
    let output = Command::new(program).args(args).output()?;
    finish(output.status, &output.stdout, &output.stderr)
}

/// Like [`run`], but kills the process and returns [`Error::Timeout`] if it
/// has not exited after `timeout`.
pub fn run_with_timeout(
    program: &OsStr,
    args: &[String],
    timeout: Duration,
) -> Result<String, Error> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Drain both pipes on threads so a chatty child cannot block on a full pipe.
    fn drain(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    }
    let out = child.stdout.take().map(drain);
    let err = child.stderr.take().map(drain);
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let join = |h: Option<std::thread::JoinHandle<Vec<u8>>>| {
        h.and_then(|h| h.join().ok()).unwrap_or_default()
    };
    // ponytail: a grandchild holding the pipes open could delay this join past
    // the deadline; yt-dlp's helpers exit with it, so no process-group kill yet.
    let (stdout, stderr) = (join(out), join(err));
    match status {
        Some(status) => finish(status, &stdout, &stderr),
        None => Err(Error::Timeout(timeout)),
    }
}

/// Test-only: writing an executable while another thread forks leaves the
/// child holding the write fd, so exec fails with ETXTBSY. Script writers take
/// the write lock; anything that spawns takes the read lock.
#[cfg(test)]
pub(crate) static SPAWN_LOCK: std::sync::RwLock<()> = std::sync::RwLock::new(());

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn download_opus() {
        let extra = s(&["--foo"]);
        let a = download_args(
            "U",
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
        assert_eq!(a[a.len() - 3..], s(&["--foo", "--", "U"]));
        let a = download_args(
            "U",
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
        let a = download_args("U", Path::new("x"), AudioFormat::Best, true, &c, &[]);
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
        let l = listing_args("U", &c);
        assert_eq!(l[0], "--ignore-config");
        assert!(l.contains(&"--flat-playlist".to_string()));
        assert_eq!(l[l.len() - 2..], s(&["--", "U"]));
        let i = info_args("U", &Cookies::None);
        assert_eq!(i, s(&["--ignore-config", "-J", "--no-playlist", "--", "U"]));
    }

    #[test]
    fn run_reports_failure_tail() {
        let _g = SPAWN_LOCK.read().unwrap();
        let args = s(&[
            "-c",
            "echo out; for i in $(seq 30); do echo e$i >&2; done; exit 3",
        ]);
        match run(OsStr::new("sh"), &args) {
            Err(Error::YtDlp { stderr_tail, .. }) => {
                assert_eq!(stderr_tail.lines().count(), 20);
                assert!(stderr_tail.ends_with("e30"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            run(OsStr::new("sh"), &s(&["-c", "echo hi"])).unwrap(),
            "hi\n"
        );
    }

    #[test]
    fn timeout_kills_slow_process() {
        let _g = SPAWN_LOCK.read().unwrap();
        let t = Instant::now();
        let r = run_with_timeout(
            OsStr::new("sh"),
            &s(&["-c", "sleep 30"]),
            Duration::from_secs(1),
        );
        assert!(matches!(r, Err(Error::Timeout(_))), "{r:?}");
        assert!(t.elapsed() < Duration::from_secs(10));
        let ok = run_with_timeout(
            OsStr::new("sh"),
            &s(&["-c", "echo hi"]),
            Duration::from_secs(5),
        );
        assert_eq!(ok.unwrap(), "hi\n");
    }
}
