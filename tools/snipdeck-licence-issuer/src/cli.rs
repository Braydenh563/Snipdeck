//! The command line, parsed by hand (three subcommands, a handful of flags).

use std::ffi::OsString;
use std::path::PathBuf;

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Print the usage.
    Help,
    /// Create a key pair in `out`.
    GenKey {
        /// The directory that receives `private.key` and `public.key`.
        out: PathBuf,
    },
    /// Sign a licence key.
    Sign {
        /// The private key file.
        key: PathBuf,
        /// The order id (`GIFT-<name>` for friends).
        order: String,
        /// The optional hash of the buyer's email, lower-case hex.
        email_hash: Option<String>,
        /// Unix seconds to put in `issued` instead of the current time (for reproducible keys).
        issued: Option<i64>,
    },
    /// Check a licence key and print what it says.
    Verify {
        /// The public key file.
        public: PathBuf,
        /// The licence key text.
        token: String,
    },
}

/// A command line that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CliError {
    /// No subcommand.
    #[error("no command given")]
    NoCommand,
    /// A subcommand this tool does not have.
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    /// A flag this subcommand does not have.
    #[error("unknown option `{0}`")]
    UnknownOption(String),
    /// A flag without its value.
    #[error("option `{0}` needs a value")]
    MissingValue(&'static str),
    /// A required flag that was not given.
    #[error("missing required option `{0}`")]
    MissingOption(&'static str),
    /// A value that is not usable.
    #[error("bad value for `{0}`")]
    BadValue(&'static str),
    /// More arguments than the subcommand takes.
    #[error("unexpected argument `{0}`")]
    Unexpected(String),
}

fn text(value: &OsString) -> String {
    value.to_string_lossy().into_owned()
}

/// Parses the arguments after the program name.
///
/// # Errors
///
/// [`CliError`] when the arguments are not a valid command.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, CliError> {
    let mut args = args.into_iter();
    let name = text(&args.next().ok_or(CliError::NoCommand)?);
    match name.as_str() {
        "help" | "--help" | "-h" => Ok(Command::Help),
        "genkey" => {
            let mut out = None;
            while let Some(flag) = args.next() {
                match text(&flag).as_str() {
                    "--out" => out = Some(PathBuf::from(value(&mut args, "--out")?)),
                    other => return Err(CliError::UnknownOption(other.to_owned())),
                }
            }
            Ok(Command::GenKey {
                out: out.ok_or(CliError::MissingOption("--out"))?,
            })
        }
        "sign" => {
            let (mut key, mut order, mut email_hash, mut issued) = (None, None, None, None);
            while let Some(flag) = args.next() {
                match text(&flag).as_str() {
                    "--key" => key = Some(PathBuf::from(value(&mut args, "--key")?)),
                    "--order" => order = Some(text(&value(&mut args, "--order")?)),
                    "--email-hash" => {
                        email_hash = Some(text(&value(&mut args, "--email-hash")?));
                    }
                    "--issued" => {
                        issued = Some(
                            text(&value(&mut args, "--issued")?)
                                .parse()
                                .map_err(|_| CliError::BadValue("--issued"))?,
                        );
                    }
                    other => return Err(CliError::UnknownOption(other.to_owned())),
                }
            }
            Ok(Command::Sign {
                key: key.ok_or(CliError::MissingOption("--key"))?,
                order: order.ok_or(CliError::MissingOption("--order"))?,
                email_hash,
                issued,
            })
        }
        "verify" => {
            let (mut public, mut token) = (None, None);
            while let Some(arg) = args.next() {
                match text(&arg).as_str() {
                    "--public" => public = Some(PathBuf::from(value(&mut args, "--public")?)),
                    flag if flag.starts_with("--") => {
                        return Err(CliError::UnknownOption(flag.to_owned()));
                    }
                    word if token.is_none() => token = Some(word.to_owned()),
                    extra => return Err(CliError::Unexpected(extra.to_owned())),
                }
            }
            Ok(Command::Verify {
                public: public.ok_or(CliError::MissingOption("--public"))?,
                token: token.ok_or(CliError::MissingOption("KEY"))?,
            })
        }
        _ => Err(CliError::UnknownCommand(name)),
    }
}

fn value(
    args: &mut impl Iterator<Item = OsString>,
    flag: &'static str,
) -> Result<OsString, CliError> {
    args.next().ok_or(CliError::MissingValue(flag))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(line: &str) -> Result<Command, CliError> {
        parse(line.split_whitespace().map(OsString::from))
    }

    #[test]
    fn genkey_takes_out() {
        assert_eq!(
            parse_str("genkey --out keys"),
            Ok(Command::GenKey { out: "keys".into() })
        );
        assert_eq!(parse_str("genkey"), Err(CliError::MissingOption("--out")));
        assert_eq!(
            parse_str("genkey --out"),
            Err(CliError::MissingValue("--out"))
        );
    }

    #[test]
    fn sign_takes_key_order_and_optional_fields() {
        assert_eq!(
            parse_str("sign --key private.key --order GIFT-alex --email-hash ab01 --issued 5"),
            Ok(Command::Sign {
                key: "private.key".into(),
                order: "GIFT-alex".into(),
                email_hash: Some("ab01".into()),
                issued: Some(5),
            })
        );
        assert_eq!(
            parse_str("sign --order GIFT-a"),
            Err(CliError::MissingOption("--key"))
        );
        assert_eq!(
            parse_str("sign --key k --order o --issued soon"),
            Err(CliError::BadValue("--issued"))
        );
        assert_eq!(
            parse_str("sign --key k --order o --expires 2030-01-01"),
            Err(CliError::UnknownOption("--expires".into()))
        );
    }

    #[test]
    fn verify_takes_public_and_the_key_text() {
        assert_eq!(
            parse_str("verify --public public.key SNIP1.a.b"),
            Ok(Command::Verify {
                public: "public.key".into(),
                token: "SNIP1.a.b".into(),
            })
        );
        assert_eq!(
            parse_str("verify SNIP1.a.b"),
            Err(CliError::MissingOption("--public"))
        );
        assert_eq!(
            parse_str("verify --public p a b"),
            Err(CliError::Unexpected("b".into()))
        );
    }

    #[test]
    fn unknown_and_missing_commands() {
        assert_eq!(parse_str(""), Err(CliError::NoCommand));
        assert_eq!(parse_str("--help"), Ok(Command::Help));
        assert_eq!(
            parse_str("mint"),
            Err(CliError::UnknownCommand("mint".into()))
        );
    }
}
