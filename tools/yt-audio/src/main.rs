use std::io::Write;
use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    let code = yt_audio::run(std::env::args_os().skip(1), &mut stdout, &mut stderr).await;
    let _ = stdout.flush();
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
