use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn case_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create case dir");
    dir
}

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    fs::write(path, b"").expect("create file");
}

fn run(args: Vec<OsString>) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = ls_posts::run(args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).expect("stdout is utf8"),
        String::from_utf8(err).expect("stderr is utf8"),
    )
}

fn sample_tree(name: &str) -> PathBuf {
    let root = case_dir(name);
    touch(&root.join("twitter/wlpyx/2093996823895753175.jpg"));
    touch(&root.join("twitter/wlpyx/2093996823895753175_2.jpg"));
    touch(&root.join("facebook/347478011374936.mp4"));
    touch(&root.join("pixiv/149425341_p3.jpg"));
    touch(&root.join("danbooru/12245510.jpg"));
    touch(&root.join("weirdsite/123.jpg"));
    root
}

#[test]
fn json_mode_reports_every_known_site() {
    let root = sample_tree("json");
    let (code, out, err) = run(vec!["--json".into(), root.into()]);
    assert_eq!(code, 0);
    assert!(err.contains("skipped 1"), "stderr was: {err}");

    let records: Vec<Value> = serde_json::from_str(&out).expect("valid json array");
    assert_eq!(records.len(), 5, "one record per known file");

    let any_url_starts_with = |prefix: &str| {
        records.iter().any(|record| {
            record["url"]
                .as_str()
                .is_some_and(|url| url.starts_with(prefix))
        })
    };
    assert!(any_url_starts_with(
        "https://x.com/wlpyx/status/2093996823895753175"
    ));
    assert!(any_url_starts_with(
        "https://www.facebook.com/watch/?v=347478011374936"
    ));
    assert!(any_url_starts_with(
        "https://www.pixiv.net/en/artworks/149425341"
    ));
    assert!(any_url_starts_with(
        "https://danbooru.donmai.us/posts/12245510"
    ));

    let second_image = records
        .iter()
        .find(|record| {
            record["media_url"]
                .as_str()
                .is_some_and(|url| url.ends_with("/photo/2"))
        })
        .expect("the _2 file is present");
    assert_eq!(second_image["image"], 2);
    assert_eq!(second_image["page"], 1);
}

#[test]
fn default_mode_prints_one_url_per_line() {
    let root = sample_tree("lines");
    let (code, out, _err) = run(vec![root.into()]);
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 5);
    assert!(lines.iter().all(|line| line.starts_with("https://")));
}

#[test]
fn long_mode_prints_path_then_url() {
    let root = sample_tree("long");
    let (code, out, _err) = run(vec!["-l".into(), root.into()]);
    assert_eq!(code, 0);
    assert!(
        out.lines()
            .all(|line| line.contains('\t') && line.contains("https://")),
        "stdout was: {out}"
    );
}

#[test]
fn jsonl_mode_is_one_object_per_line() {
    let root = sample_tree("jsonl");
    let (code, out, _err) = run(vec!["--jsonl".into(), root.into()]);
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 5);
    for line in lines {
        let _: Value = serde_json::from_str(line).expect("each line is a json object");
    }
}

#[test]
fn unknown_placeholder_is_a_usage_error() {
    let root = case_dir("badformat");
    let (code, _out, err) = run(vec!["--format".into(), "{bogus}".into(), root.into()]);
    assert_eq!(code, 2);
    assert!(err.contains("unknown placeholder"), "stderr was: {err}");
}

#[test]
fn missing_path_warns_without_failing() {
    let (code, _out, err) = run(vec!["definitely/not/here/xyz".into()]);
    assert_eq!(code, 0);
    assert!(
        err.contains("no such file or directory"),
        "stderr was: {err}"
    );
}
