//! Picks the licence public key at build time and refuses to build a release without a
//! production one. The policy lives in `src/licence/key_selection.rs`, where it is unit
//! tested; this script only feeds it the environment and writes the result.
//!
//! The key is a compile-time constant on purpose (see `docs/licensing.md`): nothing at run
//! time can redirect it.

use std::path::PathBuf;
use std::process::ExitCode;
use std::{env, fs};

#[path = "src/licence/key_selection.rs"]
mod key_selection;

fn main() -> ExitCode {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/licence/key_selection.rs");
    println!("cargo::rerun-if-changed=src/licence/development_key.in");
    println!("cargo::rerun-if-env-changed={}", key_selection::ENV_VAR);

    let configured = env::var(key_selection::ENV_VAR).ok();
    // `PROFILE` is `release` for the release profile and every profile inheriting from it.
    let release = env::var("PROFILE").is_ok_and(|profile| profile == "release");
    let selected = match key_selection::select(configured.as_deref(), release) {
        Ok(selected) => selected,
        Err(message) => {
            println!("cargo::error={message}");
            return ExitCode::FAILURE;
        }
    };

    let bytes = selected
        .bytes
        .iter()
        .map(|byte| format!("{byte:#04x}"))
        .collect::<Vec<_>>()
        .join(", ");
    let generated = format!(
        "/// The Ed25519 public key compiled into this build, as 32 raw bytes.\n\
         ///\n\
         /// Chosen by `build.rs`: the key in `{var}` if set, otherwise (debug builds only) the\n\
         /// public development key. A release build without a production key does not compile.\n\
         pub const LICENCE_PUBLIC_KEY: [u8; 32] = [{bytes}];\n\
         \n\
         /// Whether [`LICENCE_PUBLIC_KEY`] is the development key, whose private seed is public.\n\
         /// The app should show a \"development licence key\" warning while this is `true`.\n\
         /// Always `false` in a release build.\n\
         pub const IS_DEVELOPMENT_KEY: bool = {development};\n",
        var = key_selection::ENV_VAR,
        development = selected.development,
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap_or_default()).join("licence_key.rs");
    if let Err(error) = fs::write(&out, generated) {
        println!("cargo::error=cannot write {}: {error}", out.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
