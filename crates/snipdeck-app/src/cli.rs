//! The command line: a handful of flags, parsed by hand (no dependency for five flags).

use std::path::PathBuf;

/// What the command line asked for.
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
}

/// The text of `--help`.
pub const USAGE: &str = "\
Usage: snipdeck-app [options]

  --demo                 run on a seeded in-memory library (nothing is saved)
  --screenshot <file>    render the demo Panel to a PNG and exit (implies --demo)
  --dark                 use the dark theme
  --hidden               start with the Panel hidden
  --data-dir <folder>    keep the library in <folder>
  --help                 show this text
";

/// Parses the arguments after the program name.
///
/// # Errors
///
/// See [`ArgError`].
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Args, ArgError> {
    let mut parsed = Args::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => parsed.demo = true,
            "--hidden" => parsed.hidden = true,
            "--dark" => parsed.dark = true,
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
            _ => return Err(ArgError::Unknown(arg)),
        }
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(args: &[&str]) -> Result<Args, ArgError> {
        parse(args.iter().map(|a| (*a).to_owned()))
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
