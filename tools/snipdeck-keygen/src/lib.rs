//! The seller's licence tool. `docs/licensing.md`, "Keys for friends", is the user guide.
//!
//! [`run`] does the work with the clock and the output passed in, so tests are deterministic
//! and `main` stays a few lines. The private key is only ever read from the file the caller
//! names, and is never printed.

#![forbid(unsafe_code)]

pub mod cli;

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use snipdeck_core::licence::signing::{SignError, SigningKey, pro_payload, sign_token};
use snipdeck_core::licence::{LicenceError, VerifyingKey, verify_licence};

pub use cli::Command;

/// The command-line help.
pub const USAGE: &str = "\
usage:
  snipdeck-keygen genkey --out DIR
      write DIR/private.key and DIR/public.key; print the build setting for the public key
  snipdeck-keygen sign --key private.key --order GIFT-name [--email-hash HEX] [--issued UNIX]
      print a SNIP1. licence key (the format has no expiry field)
  snipdeck-keygen verify --public public.key KEY
      check a licence key and print its fields";

const PRIVATE_FILE: &str = "private.key";
const PUBLIC_FILE: &str = "public.key";

/// Why a command failed. Messages never contain key material.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// A file could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: String,
        /// The cause.
        source: io::Error,
    },
    /// A key file is not base64 of 32 bytes.
    #[error("{0} is not a key file (expected base64 of 32 bytes)")]
    BadKeyFile(String),
    /// The system random generator failed.
    #[error("the system random generator failed")]
    Random,
    /// The licence key to sign was refused.
    #[error(transparent)]
    Sign(#[from] SignError),
    /// The licence key did not verify.
    #[error("licence key rejected: {0}")]
    Rejected(#[from] LicenceError),
    /// Output could not be written.
    #[error("cannot write output: {0}")]
    Output(#[from] io::Error),
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> ToolError {
    let path = path.display().to_string();
    move |source| ToolError::Io { path, source }
}

/// Runs `command`, printing results to `out`. `now` is the current Unix time in seconds.
///
/// # Errors
///
/// [`ToolError`] when a file cannot be used or the key is refused.
pub fn run(command: &Command, now: i64, out: &mut dyn Write) -> Result<(), ToolError> {
    match command {
        Command::Help => Ok(writeln!(out, "{USAGE}")?),
        Command::GenKey { out: dir } => gen_key(dir, out),
        Command::Sign {
            key,
            order,
            email_hash,
            issued,
        } => {
            let signing = SigningKey::from_bytes(&read_key_file(key)?);
            let payload = pro_payload(order, issued.unwrap_or(now), email_hash.as_deref())?;
            Ok(writeln!(out, "{}", sign_token(&signing, &payload))?)
        }
        Command::Verify { public, token } => {
            let key = VerifyingKey::from_bytes(&read_key_file(public)?)
                .map_err(|_| ToolError::BadKeyFile(public.display().to_string()))?;
            describe(token, &key, out)
        }
    }
}

fn gen_key(dir: &Path, out: &mut dyn Write) -> Result<(), ToolError> {
    fs::create_dir_all(dir).map_err(io_error(dir))?;
    let mut seed = [0_u8; 32];
    getrandom::fill(&mut seed).map_err(|_| ToolError::Random)?;
    let signing = SigningKey::from_bytes(&seed);
    let public = STANDARD.encode(signing.verifying_key().to_bytes());

    // `create_new`: never overwrite an existing key, which would orphan every key it signed.
    let private_path = dir.join(PRIVATE_FILE);
    write_new(&private_path, &STANDARD.encode(seed), true)?;
    write_new(&dir.join(PUBLIC_FILE), &public, false)?;
    writeln!(
        out,
        "wrote {} (secret: never commit or share it) and {}",
        private_path.display(),
        dir.join(PUBLIC_FILE).display()
    )?;
    writeln!(out, "SNIPDECK_LICENCE_PUBLIC_KEY={public}")?;
    Ok(())
}

/// Creates `path` (failing if it exists) holding `text` and a newline; owner-only when `secret`.
fn write_new(path: &Path, text: &str, secret: bool) -> Result<(), ToolError> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if secret {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    // On Windows a new file inherits the folder's ACL; keep the folder private (see the guide).
    #[cfg(not(unix))]
    let _ = secret;
    let mut file = options.open(path).map_err(io_error(path))?;
    writeln!(file, "{text}").map_err(io_error(path))
}

fn read_key_file(path: &Path) -> Result<[u8; 32], ToolError> {
    let text = fs::read_to_string(path).map_err(io_error(path))?;
    STANDARD
        .decode(text.trim())
        .ok()
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .ok_or_else(|| ToolError::BadKeyFile(path.display().to_string()))
}

fn describe(token: &str, key: &VerifyingKey, out: &mut dyn Write) -> Result<(), ToolError> {
    let licence = verify_licence(token, key)?;
    let mut text = String::from("valid\ntier: pro\n");
    // Writing to a `String` cannot fail.
    let _ = writeln!(text, "order: {}", licence.order());
    let _ = writeln!(
        text,
        "issued: {} (Unix seconds)",
        licence.issued().as_second()
    );
    if let Some(hash) = licence.email_hash() {
        let _ = writeln!(text, "email_hash: {hash}");
    }
    let features: Vec<&str> = licence.features().iter().map(|f| f.key()).collect();
    if !features.is_empty() {
        let _ = writeln!(text, "features: {}", features.join(", "));
    }
    Ok(write!(out, "{text}")?)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    const NOW: i64 = 1_760_000_000;

    fn exec(line: &[&str]) -> Result<String, ToolError> {
        let command = cli::parse(line.iter().map(OsString::from)).unwrap();
        let mut out = Vec::new();
        run(&command, NOW, &mut out)?;
        Ok(String::from_utf8(out).unwrap())
    }

    /// A temp dir holding a fresh key pair, as `genkey` writes it.
    fn keys() -> (tempfile::TempDir, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap().to_owned();
        exec(&["genkey", "--out", &path]).unwrap();
        let private = format!("{path}/private.key");
        let public = format!("{path}/public.key");
        (dir, private, public)
    }

    fn sign(private: &str, order: &str) -> Result<String, ToolError> {
        exec(&["sign", "--key", private, "--order", order]).map(|text| text.trim().to_owned())
    }

    #[test]
    fn genkey_sign_verify_round_trip_with_the_core_verifier() {
        let (_dir, private, public) = keys();
        let token = sign(&private, "GIFT-alex").unwrap();
        assert!(token.starts_with("SNIP1."));

        let key = VerifyingKey::from_bytes(&read_key_file(Path::new(&public)).unwrap()).unwrap();
        let licence = verify_licence(&token, &key).unwrap();
        assert_eq!(licence.order(), "GIFT-alex");
        assert_eq!(licence.issued().as_second(), NOW);

        let shown = exec(&["verify", "--public", &public, &token]).unwrap();
        assert!(
            shown.starts_with("valid\ntier: pro\norder: GIFT-alex\n"),
            "{shown}"
        );
    }

    #[test]
    fn genkey_prints_the_build_setting_and_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();
        let printed = exec(&["genkey", "--out", path]).unwrap();
        let public = fs::read_to_string(dir.path().join("public.key")).unwrap();
        assert!(
            printed.contains(&format!("SNIPDECK_LICENCE_PUBLIC_KEY={}\n", public.trim())),
            "{printed}"
        );
        assert!(matches!(
            exec(&["genkey", "--out", path]),
            Err(ToolError::Io { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn the_private_key_is_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;
        let (dir, _, _) = keys();
        let mode = fs::metadata(dir.path().join("private.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_tampered_key_is_rejected() {
        let (_dir, private, public) = keys();
        let token = sign(&private, "GIFT-alex").unwrap();
        // Change the first character of the signed payload (the second segment).
        let mut parts: Vec<String> = token.split('.').map(str::to_owned).collect();
        let flipped = if parts[1].starts_with('e') { "f" } else { "e" };
        parts[1].replace_range(0..1, flipped);
        let tampered = parts.join(".");
        assert!(matches!(
            exec(&["verify", "--public", &public, &tampered]),
            Err(ToolError::Rejected(_))
        ));
        // A key from another pair is not accepted either.
        let (_other_dir, _, other_public) = keys();
        assert!(matches!(
            exec(&["verify", "--public", &other_public, &token]),
            Err(ToolError::Rejected(LicenceError::BadSignature))
        ));
    }

    #[test]
    fn bad_order_ids_are_refused() {
        let (_dir, private, _) = keys();
        for bad in ["", "GIFT alex", "GIFT/alex", "gift-\"x\""] {
            assert!(
                matches!(
                    sign(&private, bad),
                    Err(ToolError::Sign(SignError::BadOrderId))
                ),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn email_hash_and_issued_are_carried() {
        let (_dir, private, public) = keys();
        let token = exec(&[
            "sign",
            "--key",
            &private,
            "--order",
            "GIFT-b",
            "--email-hash",
            "ab01",
            "--issued",
            "42",
        ])
        .unwrap();
        let shown = exec(&["verify", "--public", &public, token.trim()]).unwrap();
        assert!(shown.contains("issued: 42 "), "{shown}");
        assert!(shown.contains("email_hash: ab01\n"), "{shown}");
        assert!(matches!(
            exec(&[
                "sign",
                "--key",
                &private,
                "--order",
                "GIFT-b",
                "--email-hash",
                "NOT HEX"
            ]),
            Err(ToolError::Sign(SignError::BadEmailHash))
        ));
    }

    #[test]
    fn a_missing_or_garbled_key_file_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let garbled = dir.path().join("private.key");
        fs::write(&garbled, "not base64 !!").unwrap();
        let garbled = garbled.to_str().unwrap();
        assert!(matches!(
            sign(garbled, "GIFT-a"),
            Err(ToolError::BadKeyFile(_))
        ));
        assert!(matches!(
            sign("/nonexistent/private.key", "GIFT-a"),
            Err(ToolError::Io { .. })
        ));
    }
}
