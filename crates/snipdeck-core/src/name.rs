use core::fmt;

use crate::Error;

/// A user-visible name for a board, list or tag: trimmed, non-empty, single-line and at most
/// [`Name::MAX_CHARS`] characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Name(String);

impl Name {
    /// The longest name accepted, in characters (Unicode scalar values).
    pub const MAX_CHARS: usize = 100;

    /// Trims surrounding whitespace and checks the invariants.
    ///
    /// # Errors
    ///
    /// [`Error::EmptyName`], [`Error::NameTooLong`] or [`Error::NameHasControlChar`].
    pub fn new(name: &str) -> Result<Self, Error> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::EmptyName);
        }
        if name.chars().count() > Self::MAX_CHARS {
            return Err(Error::NameTooLong {
                max: Self::MAX_CHARS,
            });
        }
        if name.chars().any(char::is_control) {
            return Err(Error::NameHasControlChar);
        }
        Ok(Self(name.to_owned()))
    }

    /// The name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(
            Name::new("  Email replies \t").unwrap().as_str(),
            "Email replies"
        );
    }

    #[test]
    fn rejects_empty_and_blank() {
        assert_eq!(Name::new(""), Err(Error::EmptyName));
        assert_eq!(Name::new(" \n "), Err(Error::EmptyName));
    }

    #[test]
    fn length_is_counted_in_characters_not_bytes() {
        let max = "仕".repeat(Name::MAX_CHARS);
        assert!(Name::new(&max).is_ok());
        assert_eq!(
            Name::new(&format!("{max}仕")),
            Err(Error::NameTooLong { max: 100 })
        );
    }

    #[test]
    fn rejects_interior_control_characters() {
        assert_eq!(Name::new("Work\nPersonal"), Err(Error::NameHasControlChar));
        assert_eq!(Name::new("a\u{7}b"), Err(Error::NameHasControlChar));
    }
}
