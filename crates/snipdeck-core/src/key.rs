//! Fieldless enums with a stable string key per variant.
//!
//! Storage and the `.snipdeck` export write these keys, so a key must never change once
//! released. Adding a variant is fine; renaming a key needs a migration.

/// Defines a `Copy` enum whose variants each have a stable string key, with `ALL`, `key()`,
/// `Display` (the key) and `FromStr` (the key, else [`Error::Parse`](crate::Error::Parse)).
macro_rules! keyed_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident ($what:literal) {
            $( $(#[$vmeta:meta])* $variant:ident = $key:literal, )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )+
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The stable key used in storage and exports.
            pub const fn key(self) -> &'static str {
                match self {
                    $(Self::$variant => $key,)+
                }
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(self.key())
            }
        }

        impl ::core::str::FromStr for $name {
            type Err = $crate::Error;

            fn from_str(s: &str) -> ::core::result::Result<Self, Self::Err> {
                match s {
                    $($key => Ok(Self::$variant),)+
                    _ => Err($crate::Error::Parse { what: $what, value: s.to_owned() }),
                }
            }
        }
    };
}

/// Asserts that every key is unique, parses back to its variant, and that junk is rejected.
#[cfg(test)]
pub(crate) fn assert_keys_round_trip<T>(all: &[T], key: fn(T) -> &'static str)
where
    T: Copy + PartialEq + core::fmt::Debug + core::str::FromStr<Err = crate::Error>,
{
    let mut seen = std::collections::HashSet::new();
    for &variant in all {
        assert!(seen.insert(key(variant)), "duplicate key {}", key(variant));
        assert_eq!(key(variant).parse::<T>(), Ok(variant));
    }
    assert!(matches!(
        "no such key".parse::<T>(),
        Err(crate::Error::Parse { .. })
    ));
}
