//! The public key Snipdeck builds trust.
//!
//! Swapping the key is a source change to this file, made when cutting a release (see
//! `docs/licensing.md`); it is deliberately never read from the environment or from disk at
//! run time, because a key an attacker can point at their own file is no check at all.

/// The Ed25519 public key compiled into this build, as 32 raw bytes.
///
/// **DEVELOPMENT KEY.** It belongs to a private key whose seed is published in this
/// repository's tests (the ASCII text `snipdeck development licence key`), so anybody can mint a licence this build
/// accepts. Release builds must replace it with the production public key.
pub const LICENCE_PUBLIC_KEY: [u8; 32] = DEVELOPMENT_PUBLIC_KEY;

/// Whether [`LICENCE_PUBLIC_KEY`] is the development key. Flip to `false` together with the
/// key; the app can show a "development licence key" warning while it is `true`.
pub const IS_DEVELOPMENT_KEY: bool = true;

/// The development public key: the verifying key of the published development seed.
pub const DEVELOPMENT_PUBLIC_KEY: [u8; 32] = [
    0xa7, 0x92, 0xa0, 0x5f, 0x26, 0x31, 0x7c, 0xa9, 0x21, 0xe8, 0xa0, 0xc2, 0x02, 0x6f, 0x53, 0x6e,
    0xe3, 0xb2, 0xb9, 0xd9, 0x7d, 0x44, 0x8b, 0x24, 0xda, 0x44, 0x74, 0x15, 0x7d, 0x4f, 0xe9, 0x2b,
];
