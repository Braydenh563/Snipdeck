//! The rules for a master password (SPEC 5.7): how short is too short, and a hint, in words, of
//! how strong a typed one is. Slint-free, and nothing here stores or logs a password: every
//! function reads the text it is given and returns a verdict.
//!
//! The rules are deliberately light. The master password protects an offline key record against
//! someone who already has the user's files, so the Argon2id cost does most of the work; a long
//! passphrase is the best advice, and the length floor stops the worst choices.

/// Fewest characters a new master password may have.
pub const MIN_LEN: usize = 8;

/// After this long without using a sensitive item, the library locks and asks again
/// (SPEC 5.7 "idle timeout").
pub const IDLE_LOCK: std::time::Duration = std::time::Duration::from_mins(15);

/// What is wrong with a new password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewProblem {
    /// Shorter than [`MIN_LEN`] characters (an empty one included).
    TooShort,
    /// The two typed copies differ.
    Mismatch,
}

/// How strong a password looks, in the four words the page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strength {
    /// Too short or made of very few different characters.
    Weak,
    /// Passes the minimum and little more.
    Fair,
    /// Long, or mixed.
    Good,
    /// Long and mixed.
    Strong,
}

impl Strength {
    /// The level the Slint page takes: 0 weak .. 3 strong.
    #[must_use]
    pub const fn level(self) -> i32 {
        match self {
            Self::Weak => 0,
            Self::Fair => 1,
            Self::Good => 2,
            Self::Strong => 3,
        }
    }
}

/// Checks a new password and its repeat.
///
/// # Errors
///
/// [`NewProblem::TooShort`] first (length is counted in characters, not bytes), then
/// [`NewProblem::Mismatch`].
pub fn check_new(password: &str, again: &str) -> Result<(), NewProblem> {
    if password.chars().count() < MIN_LEN {
        return Err(NewProblem::TooShort);
    }
    if password != again {
        return Err(NewProblem::Mismatch);
    }
    Ok(())
}

/// How strong `password` looks. A hint only: [`check_new`] is what is enforced.
#[must_use]
pub fn strength(password: &str) -> Strength {
    let len = password.chars().count();
    let distinct = password
        .chars()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    if len < MIN_LEN || distinct < 4 {
        return Strength::Weak;
    }
    let classes = [
        password.chars().any(char::is_lowercase),
        password.chars().any(char::is_uppercase),
        password.chars().any(|c| c.is_ascii_digit()),
        password
            .chars()
            .any(|c| !c.is_alphanumeric() && !c.is_whitespace()),
        // A space between words is what a passphrase is made of.
        password.chars().any(char::is_whitespace),
    ]
    .into_iter()
    .filter(|&c| c)
    .count();
    let points = usize::from(len >= 12) + usize::from(len >= 16) + usize::from(classes >= 3);
    match points {
        0 => Strength::Fair,
        1 => Strength::Good,
        _ => Strength::Strong,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_and_empty_passwords_are_too_short() {
        assert_eq!(check_new("", ""), Err(NewProblem::TooShort));
        assert_eq!(check_new("1234567", "1234567"), Err(NewProblem::TooShort));
        assert_eq!(check_new("12345678", "12345678"), Ok(()));
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        // Seven characters, more than eight bytes.
        assert_eq!(check_new("ééééééé", "ééééééé"), Err(NewProblem::TooShort));
        assert_eq!(check_new("éééééééé", "éééééééé"), Ok(()));
    }

    #[test]
    fn a_different_repeat_is_a_mismatch_after_the_length_check() {
        assert_eq!(
            check_new("correct horse", "correct hors"),
            Err(NewProblem::Mismatch)
        );
        assert_eq!(check_new("short", "other"), Err(NewProblem::TooShort));
    }

    #[test]
    fn strength_rises_with_length_and_variety() {
        assert_eq!(strength(""), Strength::Weak);
        assert_eq!(strength("abc"), Strength::Weak);
        assert_eq!(
            strength("aaaaaaaaaaaa"),
            Strength::Weak,
            "one repeated character"
        );
        assert_eq!(strength("abcdefgh"), Strength::Fair);
        assert_eq!(strength("abcdefghijkl"), Strength::Good);
        assert_eq!(strength("Abcdef1!"), Strength::Good, "mixed classes");
        assert_eq!(strength("correct horse battery staple"), Strength::Strong);
        assert!(strength("Tr0ub4dor&3xyz!!") >= Strength::Strong);
    }

    #[test]
    fn levels_are_zero_to_three_in_order() {
        let all = [
            Strength::Weak,
            Strength::Fair,
            Strength::Good,
            Strength::Strong,
        ];
        assert_eq!(all.map(Strength::level), [0, 1, 2, 3]);
    }
}
