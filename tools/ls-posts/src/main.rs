use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    let code = ls_posts::run(std::env::args_os().skip(1), &mut stdout, &mut stderr);
    let _ = stdout.flush();
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
