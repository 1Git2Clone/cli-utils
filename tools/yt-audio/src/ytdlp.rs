//! yt-dlp command construction and execution.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

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
    args.push(url.to_string());
    args
}

/// Arguments to dump one video's full JSON.
pub fn info_args(url: &str, c: &Cookies) -> Vec<String> {
    let mut args = base(c);
    args.extend(["-J", "--no-playlist"].map(String::from));
    args.push(url.to_string());
    args
}

/// Arguments to download and extract audio to `<dest_no_ext>.<ext>`.
///
/// yt-dlp prints the final path on stdout; `extra` goes just before the URL.
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
    args.push(format!("{}.%(ext)s", dest_no_ext.display()));
    args.extend(["--print", "after_move:filepath"].map(String::from));
    args.extend(extra.iter().cloned());
    args.push(url.to_string());
    args
}

/// Run `program` with `args`, returning its stdout.
///
/// A non-zero exit becomes [`Error::YtDlp`] carrying the last lines of stderr.
pub fn run(program: &OsStr, args: &[String]) -> Result<String, Error> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let lines: Vec<&str> = stderr.lines().collect();
        let tail = lines[lines.len().saturating_sub(STDERR_TAIL_LINES)..].join("\n");
        return Err(Error::YtDlp {
            status: output.status,
            stderr_tail: tail,
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

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
        assert_eq!(a[a.len() - 2..], s(&["--foo", "U"]));
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
        assert_eq!(l.last().unwrap(), "U");
        let i = info_args("U", &Cookies::None);
        assert_eq!(i, s(&["--ignore-config", "-J", "--no-playlist", "U"]));
    }

    #[test]
    fn run_reports_failure_tail() {
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
}
