//! Global hotkey chords: parsing, display, conflict detection and the numbers `RegisterHotKey` takes.
//!
//! A [`Chord`] is a set of modifiers plus one key, written `Ctrl+Alt+V`. The text form is what the
//! settings file and the hotkey editor use; [`Chord::to_win32`] gives the plain numbers for
//! `RegisterHotKey`, so everything here runs (and is tested) on any platform.
//!
//! Accepted when parsing: tokens separated by `+` (spaces around them are ignored), in any order,
//! in any case, with `Win`, `Windows`, `Super` or `Meta` for the Windows key. Printed form is
//! canonical: `Ctrl`, `Shift`, `Alt`, `Win`, then the key.
//!
//! Rules a chord must satisfy ([`Chord::validate`]):
//!
//! - at least one modifier, unless the key is `F1`..`F24`;
//! - `Shift` alone is not enough for anything but an `F` key (it would take over typing a capital
//!   letter, selecting text with the arrows, `Shift+Insert` paste and so on in every app);
//! - the key payloads are in range (a hand-built `Letter(b'1')` is rejected).
//!
//! Whether *Windows* lets Snipdeck have the chord is only known by registering it (another app, or
//! the shell, may own it); that is `crate::windows::hotkey`'s job. [`conflicts`] finds the chords
//! Snipdeck itself assigned twice.

use std::fmt;
use std::str::FromStr;

use crate::keys::Modifiers;

/// `MOD_ALT` for `RegisterHotKey`.
pub const MOD_ALT: u32 = 0x0001;
/// `MOD_CONTROL` for `RegisterHotKey`.
pub const MOD_CONTROL: u32 = 0x0002;
/// `MOD_SHIFT` for `RegisterHotKey`.
pub const MOD_SHIFT: u32 = 0x0004;
/// `MOD_WIN` for `RegisterHotKey`.
pub const MOD_WIN: u32 = 0x0008;
/// `MOD_NOREPEAT` for `RegisterHotKey`: holding the chord fires once, not repeatedly.
pub const MOD_NOREPEAT: u32 = 0x4000;

/// Why a chord is not usable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChordError {
    /// The text was empty.
    #[error("the shortcut is empty")]
    Empty,
    /// Two `+` with nothing between them, or a leading or trailing `+`.
    #[error("the shortcut has an empty part")]
    EmptyToken,
    /// A part is neither a modifier nor a key this app supports.
    #[error("unknown key or modifier \"{0}\"")]
    UnknownToken(String),
    /// A modifier appears twice (`Ctrl+Control+V`).
    #[error("{0} appears twice")]
    DuplicateModifier(&'static str),
    /// More than one non-modifier key (`Ctrl+A+B`).
    #[error("a shortcut has one key, found a second: \"{0}\"")]
    MultipleKeys(String),
    /// Only modifiers (`Ctrl+Alt`).
    #[error("the shortcut has no key")]
    MissingKey,
    /// No modifier and the key is not `F1`..`F24`.
    #[error("{0} needs at least one modifier (Ctrl, Alt, Shift or Win)")]
    NeedsModifier(ChordKey),
    /// `Shift` is the only modifier and the key is not `F1`..`F24`.
    #[error("Shift alone is not enough for {0}: add Ctrl, Alt or Win")]
    ShiftOnly(ChordKey),
    /// A key payload is out of range (built by hand, not parsed).
    #[error("not a key this app can register")]
    InvalidKey,
}

/// Punctuation keys, named by the character on a US keyboard.
///
/// Windows virtual keys for these are layout dependent (`VK_OEM_*`); on other layouts the key in
/// the same physical position is registered. Letters and digits are layout independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Punct {
    /// `;`
    Semicolon,
    /// `=`
    Equals,
    /// `,`
    Comma,
    /// `-`
    Minus,
    /// `.`
    Period,
    /// `/`
    Slash,
    /// `` ` ``
    Backtick,
    /// `[`
    LeftBracket,
    /// `\`
    Backslash,
    /// `]`
    RightBracket,
    /// `'`
    Quote,
}

impl Punct {
    const ALL: [Self; 11] = [
        Self::Semicolon,
        Self::Equals,
        Self::Comma,
        Self::Minus,
        Self::Period,
        Self::Slash,
        Self::Backtick,
        Self::LeftBracket,
        Self::Backslash,
        Self::RightBracket,
        Self::Quote,
    ];

    fn symbol(self) -> char {
        match self {
            Self::Semicolon => ';',
            Self::Equals => '=',
            Self::Comma => ',',
            Self::Minus => '-',
            Self::Period => '.',
            Self::Slash => '/',
            Self::Backtick => '`',
            Self::LeftBracket => '[',
            Self::Backslash => '\\',
            Self::RightBracket => ']',
            Self::Quote => '\'',
        }
    }

    /// Spelled-out names accepted when parsing (lower case).
    fn word(self) -> &'static str {
        match self {
            Self::Semicolon => "semicolon",
            Self::Equals => "equals",
            Self::Comma => "comma",
            Self::Minus => "minus",
            Self::Period => "period",
            Self::Slash => "slash",
            Self::Backtick => "backtick",
            Self::LeftBracket => "leftbracket",
            Self::Backslash => "backslash",
            Self::RightBracket => "rightbracket",
            Self::Quote => "quote",
        }
    }

    /// `VK_OEM_*` (US layout names).
    fn vk(self) -> u32 {
        match self {
            Self::Semicolon => 0xBA,
            Self::Equals => 0xBB,
            Self::Comma => 0xBC,
            Self::Minus => 0xBD,
            Self::Period => 0xBE,
            Self::Slash => 0xBF,
            Self::Backtick => 0xC0,
            Self::LeftBracket => 0xDB,
            Self::Backslash => 0xDC,
            Self::RightBracket => 0xDD,
            Self::Quote => 0xDE,
        }
    }
}

/// The non-modifier key of a [`Chord`].
///
/// The numeric payloads must be in range ([`Letter`](Self::Letter) `b'A'..=b'Z'`,
/// [`Digit`](Self::Digit) `0..=9`, [`Function`](Self::Function) `1..=24`); the constructors
/// [`letter`](Self::letter), [`digit`](Self::digit) and [`function`](Self::function) enforce it,
/// and [`Chord::validate`] rejects anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordKey {
    /// `A`..`Z`, as the upper-case ASCII byte.
    Letter(u8),
    /// The top-row digit `0`..`9`, as the number.
    Digit(u8),
    /// `F1`..`F24`, as the number.
    Function(u8),
    /// `Space`.
    Space,
    /// `Enter`.
    Enter,
    /// `Tab`.
    Tab,
    /// `Esc`.
    Esc,
    /// `Backspace`.
    Backspace,
    /// `Insert`.
    Insert,
    /// `Delete`.
    Delete,
    /// `Home`.
    Home,
    /// `End`.
    End,
    /// `PageUp`.
    PageUp,
    /// `PageDown`.
    PageDown,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// `PrintScreen`.
    PrintScreen,
    /// `Pause`.
    Pause,
    /// A punctuation key.
    Punct(Punct),
}

impl ChordKey {
    /// `A`..`Z` (either case); `None` for anything else.
    #[must_use]
    pub fn letter(c: char) -> Option<Self> {
        c.is_ascii_alphabetic()
            .then(|| Self::Letter(c.to_ascii_uppercase() as u8))
    }

    /// `0`..`9`; `None` for anything else.
    #[must_use]
    pub fn digit(c: char) -> Option<Self> {
        c.to_digit(10)
            .filter(|_| c.is_ascii_digit())
            .and_then(|d| u8::try_from(d).ok())
            .map(Self::Digit)
    }

    /// `F1`..`F24`; `None` for any other number.
    #[must_use]
    pub fn function(n: u8) -> Option<Self> {
        (1..=24).contains(&n).then_some(Self::Function(n))
    }

    /// Whether the payload is in range.
    #[must_use]
    pub fn is_valid(self) -> bool {
        match self {
            Self::Letter(b) => b.is_ascii_uppercase(),
            Self::Digit(d) => d <= 9,
            Self::Function(n) => (1..=24).contains(&n),
            _ => true,
        }
    }

    /// Whether this is `F1`..`F24`, the only keys allowed without a modifier.
    #[must_use]
    pub fn is_function(self) -> bool {
        matches!(self, Self::Function(n) if (1..=24).contains(&n))
    }

    /// The Windows virtual-key code, or `0` for an out-of-range payload.
    #[must_use]
    pub fn vk(self) -> u32 {
        match self {
            Self::Letter(b) if b.is_ascii_uppercase() => u32::from(b),
            Self::Digit(d) if d <= 9 => 0x30 + u32::from(d),
            Self::Function(n) if (1..=24).contains(&n) => 0x70 + u32::from(n) - 1,
            Self::Space => 0x20,
            Self::Enter => 0x0D,
            Self::Tab => 0x09,
            Self::Esc => 0x1B,
            Self::Backspace => 0x08,
            Self::Insert => 0x2D,
            Self::Delete => 0x2E,
            Self::Home => 0x24,
            Self::End => 0x23,
            Self::PageUp => 0x21,
            Self::PageDown => 0x22,
            Self::Up => 0x26,
            Self::Down => 0x28,
            Self::Left => 0x25,
            Self::Right => 0x27,
            Self::PrintScreen => 0x2C,
            Self::Pause => 0x13,
            Self::Punct(p) => p.vk(),
            Self::Letter(_) | Self::Digit(_) | Self::Function(_) => 0,
        }
    }

    /// Parses a key name (case-insensitive); `None` if it is not a key.
    fn parse(token: &str) -> Option<Self> {
        let lower = token.to_ascii_lowercase();
        let mut chars = token.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            if let Some(key) = Self::letter(c).or_else(|| Self::digit(c)) {
                return Some(key);
            }
            if let Some(p) = Punct::ALL.into_iter().find(|p| p.symbol() == c) {
                return Some(Self::Punct(p));
            }
        }
        if let Some(n) = lower.strip_prefix('f') {
            // Only plain decimal digits: "F+1", "F٣" and "f01" are not function keys.
            if !n.is_empty()
                && n.len() <= 2
                && !n.starts_with('0')
                && n.bytes().all(|b| b.is_ascii_digit())
            {
                return n.parse().ok().and_then(Self::function);
            }
        }
        if let Some(p) = Punct::ALL.into_iter().find(|p| p.word() == lower) {
            return Some(Self::Punct(p));
        }
        Some(match lower.as_str() {
            "space" | "spacebar" => Self::Space,
            "enter" | "return" => Self::Enter,
            "tab" => Self::Tab,
            "esc" | "escape" => Self::Esc,
            "backspace" | "back" => Self::Backspace,
            "insert" | "ins" => Self::Insert,
            "delete" | "del" => Self::Delete,
            "home" => Self::Home,
            "end" => Self::End,
            "pageup" | "pgup" => Self::PageUp,
            "pagedown" | "pgdn" | "pgdown" => Self::PageDown,
            "up" | "arrowup" => Self::Up,
            "down" | "arrowdown" => Self::Down,
            "left" | "arrowleft" => Self::Left,
            "right" | "arrowright" => Self::Right,
            "printscreen" | "prtsc" | "prtscn" => Self::PrintScreen,
            "pause" | "break" => Self::Pause,
            _ => return None,
        })
    }
}

impl fmt::Display for ChordKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Letter(b) => write!(f, "{}", char::from(*b)),
            Self::Digit(d) => write!(f, "{d}"),
            Self::Function(n) => write!(f, "F{n}"),
            Self::Space => f.write_str("Space"),
            Self::Enter => f.write_str("Enter"),
            Self::Tab => f.write_str("Tab"),
            Self::Esc => f.write_str("Esc"),
            Self::Backspace => f.write_str("Backspace"),
            Self::Insert => f.write_str("Insert"),
            Self::Delete => f.write_str("Delete"),
            Self::Home => f.write_str("Home"),
            Self::End => f.write_str("End"),
            Self::PageUp => f.write_str("PageUp"),
            Self::PageDown => f.write_str("PageDown"),
            Self::Up => f.write_str("Up"),
            Self::Down => f.write_str("Down"),
            Self::Left => f.write_str("Left"),
            Self::Right => f.write_str("Right"),
            Self::PrintScreen => f.write_str("PrintScreen"),
            Self::Pause => f.write_str("Pause"),
            Self::Punct(p) => write!(f, "{}", p.symbol()),
        }
    }
}

/// A global hotkey: modifiers plus a key.
///
/// Build one with [`Chord::new`] or [`str::parse`]; a hand-built value can be checked with
/// [`Chord::validate`]. The default key set is the product's decision, not this module's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    /// The modifiers that must be held.
    pub modifiers: Modifiers,
    /// The key.
    pub key: ChordKey,
}

impl Chord {
    /// A validated chord.
    ///
    /// # Errors
    ///
    /// See [`Chord::validate`].
    pub fn new(modifiers: Modifiers, key: ChordKey) -> Result<Self, ChordError> {
        let chord = Self { modifiers, key };
        chord.validate()?;
        Ok(chord)
    }

    /// Checks the rules in the module docs.
    ///
    /// # Errors
    ///
    /// [`ChordError::InvalidKey`], [`ChordError::NeedsModifier`] or [`ChordError::ShiftOnly`].
    pub fn validate(&self) -> Result<(), ChordError> {
        if !self.key.is_valid() {
            return Err(ChordError::InvalidKey);
        }
        // RegisterHotKey docs: F12 is reserved for the debugger at all times.
        if self.key.vk() == 0x7B {
            return Err(ChordError::InvalidKey);
        }
        let m = self.modifiers;
        if self.key.is_function() {
            return Ok(());
        }
        if !(m.ctrl || m.shift || m.alt || m.win) {
            return Err(ChordError::NeedsModifier(self.key));
        }
        if m.shift && !(m.ctrl || m.alt || m.win) {
            return Err(ChordError::ShiftOnly(self.key));
        }
        Ok(())
    }

    /// The numbers for `RegisterHotKey(hwnd, id, fsModifiers, vk)`: the `MOD_*` flags **including
    /// [`MOD_NOREPEAT`]** and the virtual-key code. An unvalidated chord with an out-of-range key
    /// yields `vk == 0`, which `RegisterHotKey` refuses.
    #[must_use]
    pub fn to_win32(&self) -> (u32, u32) {
        let m = self.modifiers;
        let mut flags = MOD_NOREPEAT;
        if m.ctrl {
            flags |= MOD_CONTROL;
        }
        if m.shift {
            flags |= MOD_SHIFT;
        }
        if m.alt {
            flags |= MOD_ALT;
        }
        if m.win {
            flags |= MOD_WIN;
        }
        (flags, self.key.vk())
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = self.modifiers;
        for (on, name) in [
            (m.ctrl, "Ctrl"),
            (m.shift, "Shift"),
            (m.alt, "Alt"),
            (m.win, "Win"),
        ] {
            if on {
                f.write_str(name)?;
                f.write_str("+")?;
            }
        }
        write!(f, "{}", self.key)
    }
}

impl FromStr for Chord {
    type Err = ChordError;

    fn from_str(s: &str) -> Result<Self, ChordError> {
        if s.trim().is_empty() {
            return Err(ChordError::Empty);
        }
        let mut modifiers = Modifiers::default();
        let mut key: Option<ChordKey> = None;
        for token in s.split('+').map(str::trim) {
            if token.is_empty() {
                return Err(ChordError::EmptyToken);
            }
            let (flag, name) = match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "ctl" => (Some(&mut modifiers.ctrl), "Ctrl"),
                "shift" => (Some(&mut modifiers.shift), "Shift"),
                "alt" | "option" => (Some(&mut modifiers.alt), "Alt"),
                "win" | "windows" | "super" | "meta" => (Some(&mut modifiers.win), "Win"),
                _ => (None, ""),
            };
            if let Some(flag) = flag {
                if *flag {
                    return Err(ChordError::DuplicateModifier(name));
                }
                *flag = true;
                continue;
            }
            let Some(parsed) = ChordKey::parse(token) else {
                return Err(ChordError::UnknownToken(token.to_owned()));
            };
            if key.replace(parsed).is_some() {
                return Err(ChordError::MultipleKeys(token.to_owned()));
            }
        }
        let key = key.ok_or(ChordError::MissingKey)?;
        Self::new(modifiers, key)
    }
}

/// One chord assigned to more than one id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict<Id> {
    /// The shared chord.
    pub chord: Chord,
    /// The ids that use it, in input order (at least two).
    pub ids: Vec<Id>,
}

/// Finds chords assigned to more than one id, in order of first appearance.
///
/// Only Snipdeck's own assignments are compared; whether another app owns a chord is found out by
/// registering it.
#[must_use]
pub fn conflicts<Id: Clone>(set: &[(Id, Chord)]) -> Vec<Conflict<Id>> {
    let mut groups: Vec<Conflict<Id>> = Vec::new();
    for (id, chord) in set {
        match groups.iter_mut().find(|g| g.chord == *chord) {
            Some(group) => group.ids.push(id.clone()),
            None => groups.push(Conflict {
                chord: *chord,
                ids: vec![id.clone()],
            }),
        }
    }
    groups.retain(|g| g.ids.len() > 1);
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(s: &str) -> Chord {
        s.parse().unwrap_or_else(|e| panic!("{s:?}: {e}"))
    }

    /// Modifiers from a bit set: 1 Ctrl, 2 Shift, 4 Alt, 8 Win.
    fn mods(bits: u8) -> Modifiers {
        Modifiers {
            ctrl: bits & 1 != 0,
            shift: bits & 2 != 0,
            alt: bits & 4 != 0,
            win: bits & 8 != 0,
        }
    }

    #[test]
    fn parses_any_case_and_order_and_prints_canonically() {
        for s in [
            "Ctrl+Alt+V",
            "ctrl+alt+v",
            "ALT+CTRL+V",
            "v+alt+ctrl",
            " Ctrl + Alt + v ",
        ] {
            assert_eq!(chord(s).to_string(), "Ctrl+Alt+V", "{s}");
        }
        assert_eq!(
            chord("win+alt+shift+ctrl+k").to_string(),
            "Ctrl+Shift+Alt+Win+K"
        );
    }

    #[test]
    fn windows_key_aliases() {
        for s in ["Win", "Windows", "Super", "meta", "WIN"] {
            assert_eq!(chord(&format!("{s}+V")), chord("Win+V"), "{s}");
        }
        assert_eq!(chord("Control+Option+V"), chord("Ctrl+Alt+V"));
    }

    #[test]
    fn every_key_round_trips() {
        let mut keys = vec![
            ChordKey::Space,
            ChordKey::Enter,
            ChordKey::Tab,
            ChordKey::Esc,
            ChordKey::Backspace,
            ChordKey::Insert,
            ChordKey::Delete,
            ChordKey::Home,
            ChordKey::End,
            ChordKey::PageUp,
            ChordKey::PageDown,
            ChordKey::Up,
            ChordKey::Down,
            ChordKey::Left,
            ChordKey::Right,
            ChordKey::PrintScreen,
            ChordKey::Pause,
        ];
        keys.extend(('A'..='Z').filter_map(ChordKey::letter));
        keys.extend(('0'..='9').filter_map(ChordKey::digit));
        // F12 is reserved for the debugger and rejected by `Chord::new`.
        keys.extend((1..=24).filter(|&n| n != 12).filter_map(ChordKey::function));
        keys.extend(Punct::ALL.map(ChordKey::Punct));
        assert_eq!(keys.len(), 17 + 26 + 10 + 23 + 11);
        for key in keys {
            let c = Chord::new(mods(1 | 4), key).unwrap();
            let text = c.to_string();
            assert_eq!(chord(&text), c, "{text}");
            // Printing the parsed value again is stable.
            assert_eq!(chord(&text).to_string(), text);
        }
    }

    #[test]
    fn spelled_out_key_names() {
        assert_eq!(chord("Ctrl+Return").key, ChordKey::Enter);
        assert_eq!(chord("Ctrl+escape").key, ChordKey::Esc);
        assert_eq!(chord("Ctrl+Del").key, ChordKey::Delete);
        assert_eq!(chord("Ctrl+PgUp").key, ChordKey::PageUp);
        assert_eq!(chord("Ctrl+ArrowLeft").key, ChordKey::Left);
        assert_eq!(
            chord("Ctrl+Semicolon").key,
            ChordKey::Punct(Punct::Semicolon)
        );
        assert_eq!(chord("Ctrl+;").to_string(), "Ctrl+;");
        assert_eq!(chord("Ctrl+Alt+,").key, ChordKey::Punct(Punct::Comma));
    }

    #[test]
    fn function_keys_need_no_modifier() {
        assert_eq!(chord("F1").to_string(), "F1");
        assert_eq!(chord("f24").to_string(), "F24");
        assert_eq!(chord("Shift+F5").to_string(), "Shift+F5");
    }

    #[test]
    fn function_key_numbers_are_strict() {
        for bad in ["F0", "F25", "F01", "F100", "F-1", "F٣"] {
            assert!(
                matches!(bad.parse::<Chord>(), Err(ChordError::UnknownToken(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn other_keys_need_a_modifier() {
        for s in ["V", "Space", "5", "Esc", "Insert", ";"] {
            assert!(
                matches!(s.parse::<Chord>(), Err(ChordError::NeedsModifier(_))),
                "{s}"
            );
        }
        assert!(matches!(
            "Ctrl+Alt+Win".parse::<Chord>(),
            Err(ChordError::MissingKey)
        ));
    }

    #[test]
    fn shift_alone_is_not_enough_except_for_function_keys() {
        for s in [
            "Shift+V",
            "Shift+Space",
            "Shift+Insert",
            "Shift+Left",
            "Shift+1",
        ] {
            assert!(
                matches!(s.parse::<Chord>(), Err(ChordError::ShiftOnly(_))),
                "{s}"
            );
        }
        assert!(
            "Shift+Ctrl+V".parse::<Chord>().is_ok()
                && "Shift+Alt+V".parse::<Chord>().is_ok()
                && "Shift+Win+V".parse::<Chord>().is_ok()
        );
        assert!("Alt+V".parse::<Chord>().is_ok());
        assert!("Win+V".parse::<Chord>().is_ok());
    }

    #[test]
    fn rejects_malformed_text() {
        assert_eq!("".parse::<Chord>(), Err(ChordError::Empty));
        assert_eq!("   ".parse::<Chord>(), Err(ChordError::Empty));
        for s in ["Ctrl+", "+V", "Ctrl++V", "Ctrl+ +V"] {
            assert_eq!(s.parse::<Chord>(), Err(ChordError::EmptyToken), "{s}");
        }
        assert_eq!(
            "Ctrl+Hyper+V".parse::<Chord>(),
            Err(ChordError::UnknownToken("Hyper".into()))
        );
        assert_eq!(
            "Ctrl+VV".parse::<Chord>(),
            Err(ChordError::UnknownToken("VV".into()))
        );
        assert_eq!(
            "Ctrl+é".parse::<Chord>(),
            Err(ChordError::UnknownToken("é".into()))
        );
    }

    #[test]
    fn rejects_duplicate_modifiers_and_keys() {
        assert_eq!(
            "Ctrl+Ctrl+V".parse::<Chord>(),
            Err(ChordError::DuplicateModifier("Ctrl"))
        );
        assert_eq!(
            "Ctrl+Control+V".parse::<Chord>(),
            Err(ChordError::DuplicateModifier("Ctrl"))
        );
        assert_eq!(
            "Win+Super+V".parse::<Chord>(),
            Err(ChordError::DuplicateModifier("Win"))
        );
        assert_eq!(
            "Ctrl+A+B".parse::<Chord>(),
            Err(ChordError::MultipleKeys("B".into()))
        );
        assert_eq!(
            "Ctrl+V+V".parse::<Chord>(),
            Err(ChordError::MultipleKeys("V".into()))
        );
    }

    #[test]
    fn hand_built_out_of_range_keys_are_invalid() {
        let m = mods(1);
        for key in [
            ChordKey::Letter(b'1'),
            ChordKey::Letter(b'a'),
            ChordKey::Digit(10),
            ChordKey::Function(0),
            ChordKey::Function(25),
        ] {
            assert_eq!(Chord::new(m, key), Err(ChordError::InvalidKey), "{key:?}");
            assert_eq!(key.vk(), 0, "{key:?}");
            assert_eq!(Chord { modifiers: m, key }.to_win32().1, 0);
        }
        assert_eq!(ChordKey::letter('1'), None);
        assert_eq!(ChordKey::letter('é'), None);
        assert_eq!(ChordKey::digit('a'), None);
        assert_eq!(ChordKey::digit('٣'), None);
        assert_eq!(ChordKey::function(0), None);
        assert_eq!(ChordKey::function(25), None);
    }

    #[test]
    fn win32_numbers() {
        let (m, vk) = chord("Ctrl+Alt+V").to_win32();
        assert_eq!(m, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT);
        assert_eq!(m, 0x4003);
        assert_eq!(vk, 0x56);

        assert_eq!(chord("Ctrl+Shift+Alt+Win+Space").to_win32(), (0x400F, 0x20));
        assert_eq!(chord("Win+Shift+1").to_win32(), (0x400C, 0x31));
        // No modifier: only MOD_NOREPEAT.
        assert_eq!(chord("F1").to_win32(), (0x4000, 0x70));
        assert_eq!(chord("F24").to_win32(), (0x4000, 0x87));
        assert_eq!(chord("Ctrl+Z").to_win32().1, 0x5A);
        assert_eq!(chord("Ctrl+0").to_win32().1, 0x30);
        assert_eq!(chord("Ctrl+9").to_win32().1, 0x39);
    }

    #[test]
    fn win32_virtual_keys_for_named_keys() {
        let cases = [
            ("Enter", 0x0D),
            ("Tab", 0x09),
            ("Esc", 0x1B),
            ("Backspace", 0x08),
            ("Insert", 0x2D),
            ("Delete", 0x2E),
            ("Home", 0x24),
            ("End", 0x23),
            ("PageUp", 0x21),
            ("PageDown", 0x22),
            ("Left", 0x25),
            ("Up", 0x26),
            ("Right", 0x27),
            ("Down", 0x28),
            ("PrintScreen", 0x2C),
            ("Pause", 0x13),
            (";", 0xBA),
            ("=", 0xBB),
            (",", 0xBC),
            ("-", 0xBD),
            (".", 0xBE),
            ("/", 0xBF),
            ("`", 0xC0),
            ("[", 0xDB),
            ("\\", 0xDC),
            ("]", 0xDD),
            ("'", 0xDE),
        ];
        for (name, vk) in cases {
            assert_eq!(chord(&format!("Ctrl+{name}")).to_win32().1, vk, "{name}");
        }
    }

    #[test]
    fn to_win32_gives_each_chord_a_distinct_pair() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        let mut all = 0;
        for key in ('A'..='Z').filter_map(ChordKey::letter) {
            for bits in 1..16_u8 {
                let m = mods(bits);
                if let Ok(c) = Chord::new(m, key) {
                    all += 1;
                    assert!(seen.insert(c.to_win32()));
                }
            }
        }
        assert_eq!(all, seen.len());
    }

    #[test]
    fn conflicts_groups_duplicates_in_order() {
        let set = [
            ("panel", chord("Ctrl+Alt+V")),
            ("picker", chord("Ctrl+Alt+P")),
            ("capture", chord("ctrl+alt+v")),
            ("main", chord("Win+Shift+M")),
            ("extra", chord("Alt+Ctrl+V")),
            ("other", chord("Win+Shift+M")),
        ];
        assert_eq!(
            conflicts(&set),
            vec![
                Conflict {
                    chord: chord("Ctrl+Alt+V"),
                    ids: vec!["panel", "capture", "extra"],
                },
                Conflict {
                    chord: chord("Win+Shift+M"),
                    ids: vec!["main", "other"],
                },
            ]
        );
    }

    #[test]
    fn conflicts_is_empty_for_distinct_or_empty_sets() {
        assert_eq!(conflicts::<u32>(&[]), Vec::new());
        assert_eq!(
            conflicts(&[(1, chord("Ctrl+Alt+V")), (2, chord("Ctrl+Alt+B"))]),
            Vec::new()
        );
        assert_eq!(conflicts(&[(1, chord("Ctrl+Alt+V"))]), Vec::new());
    }

    #[test]
    fn error_messages_are_readable() {
        assert_eq!(
            "Shift+V".parse::<Chord>().unwrap_err().to_string(),
            "Shift alone is not enough for V: add Ctrl, Alt or Win"
        );
        assert_eq!(
            "V".parse::<Chord>().unwrap_err().to_string(),
            "V needs at least one modifier (Ctrl, Alt, Shift or Win)"
        );
    }

    #[test]
    fn f12_is_reserved_for_the_debugger() {
        assert!(matches!(
            "F12".parse::<Chord>(),
            Err(ChordError::InvalidKey)
        ));
        assert!(matches!(
            "Ctrl+F12".parse::<Chord>(),
            Err(ChordError::InvalidKey)
        ));
        assert!("F11".parse::<Chord>().is_ok());
    }
}
