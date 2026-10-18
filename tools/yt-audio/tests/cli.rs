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
