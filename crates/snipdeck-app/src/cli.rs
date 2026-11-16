//! The command line: a handful of flags, parsed by hand (no dependency for five flags).

use std::{ffi::OsString, path::PathBuf};

use snipdeck_platform::handover::{self, Request};

/// What the command line asked for.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent command-line flags, not a state machine"
)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Args {
    /// Run on a seeded in-memory library instead of the user's data.
    pub demo: bool,
    /// Start with the Panel hidden (Start with Windows).
    pub hidden: bool,
    /// Use the dark theme whatever the settings say (dev).
    pub dark: bool,
    /// Where the data folder is, instead of the default.
    pub data_dir: Option<PathBuf>,
    /// Render the demo Panel to this PNG and exit.
    pub screenshot: Option<PathBuf>,
    /// Run the library's integrity check, print the result and exit.
    pub check_db: bool,
    /// Print the Troubleshoot report and exit.
    pub troubleshoot: bool,
    /// Ask the Snipdeck running in this Windows session to quit, then exit.
    pub quit: bool,
    /// What the shell asked for: a `.snipdeck` file to open or a `snipdeck:` address. Acted on by
    /// the running instance (handed over), or by this one when it is the first.
    pub request: Option<Request>,
    /// Print the usage and exit.
    pub help: bool,
}

/// A command line that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgError {
    /// A flag Snipdeck does not know.
    #[error("unknown option `{0}`")]
    Unknown(String),
    /// A flag that needs a value came last.
    #[error("`{0}` needs a value")]
    MissingValue(&'static str),
    /// A file or address that is not something Snipdeck opens (or a second one).
    #[error("`{0}` is not a .snipdeck file or a snipdeck:// address Snipdeck can open")]
    NotOpenable(String),
}

/// The text of `--help`.
pub const USAGE: &str = "\
Usage: snipdeck-app [options]

  --demo                 run on a seeded in-memory library (nothing is saved)
  --screenshot <file>    render the demo Panel to a PNG and exit (implies --demo)
  --dark                 use the dark theme
  --hidden               start with the Panel hidden
  --data-dir <folder>    keep the library in <folder>
  --check-db             check the library for damage and exit (0 healthy, 1 problems, 2 not run)
  --troubleshoot         print the Troubleshoot report and exit (close Snipdeck first)
  --quit                 ask the running Snipdeck to quit, as the tray's Quit does, and exit (0
                         whether or not one was running; Windows only)
  --help                 show this text
";

/// Parses the arguments after the program name.
///
/// # Errors
///
/// See [`ArgError`].
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Args, ArgError> {
    let mut parsed = Args::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str().unwrap_or_default() {
            "--demo" => parsed.demo = true,
            "--hidden" => parsed.hidden = true,
            "--dark" => parsed.dark = true,
            "--check-db" => parsed.check_db = true,
            "--troubleshoot" => parsed.troubleshoot = true,
            "--quit" => parsed.quit = true,
            "--help" | "-h" => parsed.help = true,
            "--data-dir" => {
                let value = args.next().ok_or(ArgError::MissingValue("--data-dir"))?;
                parsed.data_dir = Some(value.into());
            }
            "--screenshot" => {
                let value = args.next().ok_or(ArgError::MissingValue("--screenshot"))?;
                parsed.screenshot = Some(value.into());
                parsed.demo = true;
            }
            other if !other.starts_with('-') && !other.is_empty() => {
                parsed.request = Some(open_request(&arg, parsed.request.is_some())?);
            }
            _ => return Err(ArgError::Unknown(arg.to_string_lossy().into_owned())),
        }
    }
    Ok(parsed)
}

/// The one file or address the shell passed. A relative path is made absolute here, against this
/// process's folder: the running instance has its own.
fn open_request(arg: &OsString, already_has_one: bool) -> Result<Request, ArgError> {
    let not_openable = || ArgError::NotOpenable(arg.to_string_lossy().into_owned());
    let text = arg.to_str().ok_or_else(not_openable)?;
    if already_has_one {
        return Err(not_openable());
    }
    let text = if handover::is_url_argument(text) {
        text.to_owned()
    } else {
        std::path::absolute(text)
            .ok()
            .and_then(|p| p.to_str().map(str::to_owned))
            .ok_or_else(not_openable)?
    };
    handover::classify_argument(&text).map_err(|_| not_openable())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(args: &[&str]) -> Result<Args, ArgError> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_is_the_normal_start() {
        assert_eq!(parse_all(&[]), Ok(Args::default()));
    }

    #[test]
    fn flags_and_values_are_read() {
        let args =
            parse_all(&["--demo", "--dark", "--hidden", "--data-dir", "C:\\Snip"]).expect("args");
        assert!(args.demo && args.dark && args.hidden);
        assert_eq!(args.data_dir, Some(PathBuf::from("C:\\Snip")));
    }

    #[cfg(unix)]
    #[test]
    fn a_data_dir_that_is_not_utf8_is_kept_not_a_panic() {
        use std::os::unix::ffi::OsStringExt;
        let folder = OsString::from_vec(vec![b'/', 0xff, b'x']);
        let args = parse(["--data-dir".into(), folder.clone()]).expect("args");
        assert_eq!(args.data_dir, Some(PathBuf::from(folder)));
    }

    #[test]
    fn the_check_flags_are_read() {
        let args = parse_all(&["--check-db", "--troubleshoot"]).expect("args");
        assert!(args.check_db && args.troubleshoot);
        assert!(!parse_all(&[]).expect("args").check_db);
    }

    #[test]
    fn a_snipdeck_file_or_address_becomes_a_request() {
        use snipdeck_platform::handover::UrlAction;
        let file = std::env::temp_dir().join("backup.snipdeck");
        let file = file.to_str().expect("utf-8 temp dir");
        let args = parse_all(&[file]).expect("args");
        assert_eq!(args.request, Some(Request::OpenFile(PathBuf::from(file))));
        let args = parse_all(&["--hidden", "snipdeck://settings"]).expect("args");
        assert_eq!(args.request, Some(Request::Url(UrlAction::Settings)));
        assert!(args.hidden);
        assert_eq!(parse_all(&[]).expect("args").request, None);
    }

    #[test]
    fn a_relative_file_is_made_absolute_here() {
        let args = parse_all(&["backup.snipdeck"]).expect("args");
        let Some(Request::OpenFile(path)) = args.request else {
            panic!("a file request");
        };
        assert!(path.is_absolute(), "{path:?}");
        assert!(path.ends_with("backup.snipdeck"));
    }

    #[test]
    fn anything_else_is_refused_by_name() {
        for bad in [
            "notes.txt",
            "snipdeck://evil",
            "snipdeck://open?x=1",
            "x.exe",
        ] {
            assert_eq!(
                parse_all(&[bad]),
                Err(ArgError::NotOpenable(bad.to_owned())),
                "{bad}"
            );
        }
        // Only one thing to open per launch.
        let file = std::env::temp_dir().join("a.snipdeck");
        let file = file.to_str().expect("utf-8 temp dir");
        assert!(parse_all(&[file, file]).is_err());
        assert!(parse_all(&["snipdeck://open", file]).is_err());
    }

    #[test]
    fn quit_is_a_flag_on_its_own() {
        assert!(parse_all(&["--quit"]).expect("args").quit);
        assert!(!parse_all(&[]).expect("args").quit);
        assert!(USAGE.contains("--quit"));
    }

    #[test]
    fn a_screenshot_implies_the_demo() {
        let args = parse_all(&["--screenshot", "out.png"]).expect("args");
        assert!(args.demo);
        assert_eq!(args.screenshot, Some(PathBuf::from("out.png")));
    }

    #[test]
    fn mistakes_are_named() {
        assert_eq!(
            parse_all(&["--wat"]),
            Err(ArgError::Unknown("--wat".into()))
        );
        assert_eq!(
            parse_all(&["--data-dir"]),
            Err(ArgError::MissingValue("--data-dir"))
        );
        assert_eq!(
            parse_all(&["--screenshot"]),
            Err(ArgError::MissingValue("--screenshot"))
        );
        assert!(parse_all(&["-h"]).expect("args").help);
    }
}
