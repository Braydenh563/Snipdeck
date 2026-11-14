//! `snipdeck-licence-issuer`: the seller's licence tool. See `docs/licensing.md`, "Keys for friends".

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use snipdeck_licence_issuer::{USAGE, cli, run};

fn main() -> ExitCode {
    let mut stderr = std::io::stderr();
    let command = match cli::parse(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            let _ = writeln!(stderr, "snipdeck-licence-issuer: {error}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        });
    match run(&command, now, &mut std::io::stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(stderr, "snipdeck-licence-issuer: {error}");
            ExitCode::FAILURE
        }
    }
}
