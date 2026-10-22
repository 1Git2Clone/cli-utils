use std::io::Write;
use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    // Here, not in `run`, so the tests can drive `run` inside a build
    // sandbox that runs as root (the CI runner's).
    if yt_audio::is_root() {
        let _ = writeln!(stderr, "yt-audio: {}", yt_audio::error::Error::Root);
        return ExitCode::from(1);
    }
    let code = yt_audio::run(std::env::args_os().skip(1), &mut stdout, &mut stderr).await;
    let _ = stdout.flush();
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
