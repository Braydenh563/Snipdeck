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
//! - the key payloads are in range (a hand-built `Letter(b'1')` is rejected);
//! - the chord is not on the reserved list ([`Chord::reserved`]): clipboard chords, bare `F1`..`F12`,
//!   `Alt` with a letter, and what Windows keeps for itself.
//!
//! Whether *Windows* lets Snipdeck have the chord is only known by registering it (another app, or
//! the shell, may own it); that is `crate::windows::hotkey`'s job. [`conflicts`] finds the chords
//! Snipdeck itself assigned twice.

use std::fmt;
use std::str::FromStr;
use std::sync::RwLock;

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
    /// A chord other programs or Windows itself own; see [`Chord::reserved`].
    #[error("{0}")]
    Reserved(Reserved),
}

/// Why a chord is on the reserved list ([`Chord::reserved`]): taking it as a global hotkey would
/// break something every program (or Windows) relies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Reserved {
    /// Copy, cut, paste, undo and select-all, and the paste keys Snipdeck itself sends: as a
    /// global hotkey the chord would stop working in every program, and a paste Snipdeck injects
    /// would trigger the hotkey again.
    #[error("it is a copy, paste or edit shortcut that every program uses")]
    Clipboard,
    /// `F1`..`F12` with no modifier: each program uses them for its own commands.
    #[error("a function key on its own is used by every program (add Ctrl, Alt or Win)")]
    BareFunctionKey,
    /// `Alt` with a letter, a digit or a window key: menus and window commands.
    #[error("Alt with a letter, a digit or a window key opens menus and moves windows")]
    Menu,
    /// Chords Windows keeps for itself (`Win+L`, `Alt+Tab`, `Ctrl+Shift+Esc`).
    #[error("Windows keeps it for itself")]
    System,
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
    /// [`ChordError::InvalidKey`], [`ChordError::NeedsModifier`], [`ChordError::ShiftOnly`] or
    /// [`ChordError::Reserved`].
    pub fn validate(&self) -> Result<(), ChordError> {
        if !self.key.is_valid() {
            return Err(ChordError::InvalidKey);
        }
        // RegisterHotKey docs: F12 is reserved for the debugger at all times.
        if self.key.vk() == 0x7B {
            return Err(ChordError::InvalidKey);
        }
        if let Some(reason) = self.reserved() {
            return Err(ChordError::Reserved(reason));
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

    /// Why this chord must not be a global hotkey, if it is on the reserved list: one table for
    /// every refusal. Checked by [`validate`](Self::validate), after the shape rules, so it also
    /// stops a bare `F5` and a hand-edited settings file.
    ///
    /// The list: the clipboard and edit chords (`Ctrl+C/V/X/Z/Y/A`, `Ctrl+Insert`, and
    /// `Ctrl+Shift+V`, which Snipdeck sends itself to terminals); `F1`..`F12` with no modifier
    /// (`F13`..`F24` have no key on most keyboards, so no program binds them); `Alt` alone with a
    /// letter, a digit, `F4`, `Tab`, `Esc`, `Space`, `Enter` or an arrow; and what Windows owns:
    /// `Win` (with or without `Shift`) and a letter, digit or navigation key, `Alt+Shift+Tab`,
    /// `Ctrl+Esc`, `Ctrl+Shift+Esc`, `Ctrl+Alt+Delete`, `Ctrl+Alt+Tab`.
    #[must_use]
    pub fn reserved(&self) -> Option<Reserved> {
        let m = self.modifiers;
        let key = self.key;
        let navigation = matches!(
            key,
            ChordKey::Space
                | ChordKey::Tab
                | ChordKey::Left
                | ChordKey::Right
                | ChordKey::Up
                | ChordKey::Down
                | ChordKey::Home
                | ChordKey::End
                | ChordKey::PrintScreen
        );
        match (m.ctrl, m.shift, m.alt, m.win, key) {
            (false, false, false, false, ChordKey::Function(n)) if n <= 12 => {
                Some(Reserved::BareFunctionKey)
            }
            (
                true,
                false,
                false,
                false,
                ChordKey::Letter(b'C' | b'V' | b'X' | b'Z' | b'Y' | b'A') | ChordKey::Insert,
            )
            | (true, true, false, false, ChordKey::Letter(b'V')) => Some(Reserved::Clipboard),
            (false, false, true, false, ChordKey::Letter(_) | ChordKey::Digit(_)) => {
                Some(Reserved::Menu)
            }
            (
                false,
                false,
                true,
                false,
                ChordKey::Function(4)
                | ChordKey::Tab
                | ChordKey::Esc
                | ChordKey::Space
                | ChordKey::Enter
                | ChordKey::Left
                | ChordKey::Right
                | ChordKey::Up,
            ) => Some(Reserved::Menu),
            (false, _, false, true, _)
                if matches!(key, ChordKey::Letter(_) | ChordKey::Digit(_)) || navigation =>
            {
                Some(Reserved::System)
            }
            (false, true, true, false, ChordKey::Tab)
            | (true, _, false, false, ChordKey::Esc)
            | (true, false, true, false, ChordKey::Delete | ChordKey::Tab) => {
                Some(Reserved::System)
            }
            _ => None,
        }
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

/// What the keyboard layout prints on the key behind a virtual-key code, or `None` when it is
/// not known or prints nothing. On Windows this is `MapVirtualKeyExW(vk, MAPVK_VK_TO_CHAR)` for
/// the current layout; tests bring a fake.
pub type LayoutLookup = fn(u32) -> Option<char>;

static LAYOUT: RwLock<Option<LayoutLookup>> = RwLock::new(None);

/// Sets the lookup [`Chord::label`] uses (the Windows host does this once at start-up; without
/// it, labels name keys as a US keyboard does).
pub fn set_layout_lookup(lookup: Option<LayoutLookup>) {
    if let Ok(mut slot) = LAYOUT.write() {
        *slot = lookup;
    }
}

impl Punct {
    /// The character on this key for `layout`: the layout's own character (upper-cased, as a key
    /// cap shows it) if it is a visible one, else the US symbol the key is named by.
    #[must_use]
    pub fn label_with(self, layout: &dyn Fn(u32) -> Option<char>) -> String {
        match layout(self.vk()) {
            Some(c) if !c.is_control() && !c.is_whitespace() => {
                let upper: String = c.to_uppercase().collect();
                // "ß" upper-cases to "SS", which is not what the key cap shows.
                if upper.chars().count() == 1 {
                    upper
                } else {
                    c.to_string()
                }
            }
            _ => self.symbol().to_string(),
        }
    }
}

impl Chord {
    /// The chord as the user's keyboard prints it: the text of [`Display`](fmt::Display), except
    /// that a punctuation key is shown with the character `layout` puts on it (`Ctrl+Alt+Ü` where
    /// the US name is `Ctrl+Alt+;`). For display only: the stored text and the registered virtual
    /// key stay the US ones.
    #[must_use]
    pub fn label_with(&self, layout: &dyn Fn(u32) -> Option<char>) -> String {
        let text = self.to_string();
        let ChordKey::Punct(p) = self.key else {
            return text;
        };
        let keep = text.len().saturating_sub(p.symbol().len_utf8());
        format!("{}{}", &text[..keep], p.label_with(layout))
    }

    /// [`label_with`](Self::label_with) for the layout set by [`set_layout_lookup`].
    #[must_use]
    pub fn label(&self) -> String {
        let lookup = LAYOUT.read().ok().and_then(|slot| *slot);
        match lookup {
            Some(lookup) => self.label_with(&lookup),
            None => self.to_string(),
        }
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
            assert_eq!(chord(&format!("{s}+Ctrl+V")), chord("Win+Ctrl+V"), "{s}");
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
            let c = Chord::new(mods(1 | 2 | 4), key).unwrap();
            let text = c.to_string();
            assert_eq!(chord(&text), c, "{text}");
            // Printing the parsed value again is stable.
            assert_eq!(chord(&text).to_string(), text);
        }
    }

    #[test]
    fn spelled_out_key_names() {
        assert_eq!(chord("Ctrl+Return").key, ChordKey::Enter);
        assert_eq!(chord("Ctrl+Alt+escape").key, ChordKey::Esc);
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
        assert_eq!(chord("F13").to_string(), "F13");
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
            "Shift+Ctrl+K".parse::<Chord>().is_ok()
                && "Shift+Alt+V".parse::<Chord>().is_ok()
                && "Shift+Win+F9".parse::<Chord>().is_ok()
        );
        assert!("Alt+F9".parse::<Chord>().is_ok());
        assert!("Win+F9".parse::<Chord>().is_ok());
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
        assert_eq!(chord("Win+Ctrl+1").to_win32(), (0x400A, 0x31));
        assert_eq!(chord("Ctrl+F1").to_win32(), (0x4002, 0x70));
        // No modifier: only MOD_NOREPEAT.
        assert_eq!(chord("F24").to_win32(), (0x4000, 0x87));
        assert_eq!(chord("Ctrl+Alt+Z").to_win32().1, 0x5A);
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
            assert_eq!(
                chord(&format!("Ctrl+Alt+Shift+{name}")).to_win32().1,
                vk,
                "{name}"
            );
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
            ("main", chord("Win+Ctrl+M")),
            ("extra", chord("Alt+Ctrl+V")),
            ("other", chord("Win+Ctrl+M")),
        ];
        assert_eq!(
            conflicts(&set),
            vec![
                Conflict {
                    chord: chord("Ctrl+Alt+V"),
                    ids: vec!["panel", "capture", "extra"],
                },
                Conflict {
                    chord: chord("Win+Ctrl+M"),
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
        assert!("Ctrl+F11".parse::<Chord>().is_ok());
    }

    /// The characters a German keyboard prints on the `VK_OEM_*` keys (unshifted).
    fn german(vk: u32) -> Option<char> {
        match vk {
            0xBA => Some('ü'),
            0xBB => Some('+'),
            0xBC => Some(','),
            0xBD => Some('-'),
            0xBE => Some('.'),
            0xBF => Some('#'),
            0xC0 => Some('ö'),
            0xDB => Some('ß'),
            0xDC => Some('^'),
            0xDD => Some('´'),
            0xDE => Some('ä'),
            _ => None,
        }
    }

    #[test]
    fn a_punctuation_key_is_labelled_with_what_the_layout_prints_on_it() {
        let us = |_: u32| None;
        assert_eq!(chord("Ctrl+Alt+;").label_with(&us), "Ctrl+Alt+;");
        assert_eq!(chord("Ctrl+Alt+;").label_with(&german), "Ctrl+Alt+Ü");
        assert_eq!(chord("Ctrl+Alt+`").label_with(&german), "Ctrl+Alt+Ö");
        assert_eq!(chord("Ctrl+Alt+'").label_with(&german), "Ctrl+Alt+Ä");
        assert_eq!(chord("Ctrl+Alt+/").label_with(&german), "Ctrl+Alt+#");
        assert_eq!(chord("Ctrl+Alt+[").label_with(&german), "Ctrl+Alt+ß");
        // Keys that are the same on every layout are untouched, and so is the stored text.
        assert_eq!(chord("Ctrl+Alt+K").label_with(&german), "Ctrl+Alt+K");
        assert_eq!(chord("Ctrl+Alt+F13").label_with(&german), "Ctrl+Alt+F13");
        assert_eq!(chord("Ctrl+Alt+;").to_string(), "Ctrl+Alt+;");
    }

    #[test]
    fn a_layout_that_prints_nothing_visible_falls_back_to_the_us_name() {
        let blank = |_: u32| Some(' ');
        let control = |_: u32| Some('\u{1}');
        assert_eq!(chord("Ctrl+Alt+;").label_with(&blank), "Ctrl+Alt+;");
        assert_eq!(chord("Ctrl+Alt+,").label_with(&control), "Ctrl+Alt+,");
    }

    #[test]
    fn the_label_uses_the_installed_lookup_and_the_us_names_without_one() {
        set_layout_lookup(None);
        assert_eq!(chord("Ctrl+Alt+;").label(), "Ctrl+Alt+;");
        set_layout_lookup(Some(german));
        assert_eq!(chord("Ctrl+Alt+;").label(), "Ctrl+Alt+Ü");
        set_layout_lookup(None);
        assert_eq!(chord("Ctrl+Alt+;").label(), "Ctrl+Alt+;");
    }

    #[test]
    fn clipboard_and_edit_chords_are_reserved_and_say_why() {
        for text in [
            "Ctrl+V",
            "Ctrl+C",
            "Ctrl+X",
            "Ctrl+Z",
            "Ctrl+Y",
            "Ctrl+A",
            "Ctrl+Insert",
            // Snipdeck sends this one itself to terminals: a global hotkey would loop.
            "Ctrl+Shift+V",
        ] {
            assert_eq!(
                text.parse::<Chord>(),
                Err(ChordError::Reserved(Reserved::Clipboard)),
                "{text}"
            );
        }
        // The same keys with a second modifier are free.
        for text in ["Ctrl+Alt+V", "Ctrl+Shift+C", "Ctrl+Alt+X"] {
            assert!(text.parse::<Chord>().is_ok(), "{text}");
        }
    }

    #[test]
    fn a_bare_function_key_is_reserved_except_the_ones_no_keyboard_has() {
        for n in 1..=11 {
            assert_eq!(
                format!("F{n}").parse::<Chord>(),
                Err(ChordError::Reserved(Reserved::BareFunctionKey)),
                "F{n}"
            );
        }
        for n in 13..=24 {
            assert!(format!("F{n}").parse::<Chord>().is_ok(), "F{n}");
        }
        assert!("Ctrl+F5".parse::<Chord>().is_ok());
        assert!("Shift+F5".parse::<Chord>().is_ok());
    }

    #[test]
    fn alt_with_a_letter_or_a_window_key_is_reserved() {
        for text in [
            "Alt+V",
            "Alt+1",
            "Alt+F4",
            "Alt+Tab",
            "Alt+Esc",
            "Alt+Space",
            "Alt+Left",
        ] {
            assert_eq!(
                text.parse::<Chord>(),
                Err(ChordError::Reserved(Reserved::Menu)),
                "{text}"
            );
        }
        assert!("Alt+F9".parse::<Chord>().is_ok());
        assert!("Ctrl+Alt+V".parse::<Chord>().is_ok());
    }

    #[test]
    fn what_windows_keeps_for_itself_is_reserved() {
        for text in [
            "Win+L",
            "Win+D",
            "Win+Space",
            "Win+Tab",
            "Win+Left",
            "Win+1",
            "Win+Shift+S",
            "Win+Shift+Left",
            "Alt+Shift+Tab",
            "Ctrl+Esc",
            "Ctrl+Shift+Esc",
            "Ctrl+Alt+Delete",
            "Ctrl+Alt+Tab",
        ] {
            assert_eq!(
                text.parse::<Chord>(),
                Err(ChordError::Reserved(Reserved::System)),
                "{text}"
            );
        }
        for text in ["Win+Ctrl+K", "Win+Alt+K", "Ctrl+Alt+Space", "Win+F9"] {
            assert!(text.parse::<Chord>().is_ok(), "{text}");
        }
    }

    #[test]
    fn a_hand_built_reserved_chord_fails_validate() {
        let paste = Chord {
            modifiers: mods(1),
            key: ChordKey::Letter(b'V'),
        };
        assert_eq!(paste.reserved(), Some(Reserved::Clipboard));
        assert_eq!(
            paste.validate(),
            Err(ChordError::Reserved(Reserved::Clipboard))
        );
        assert_eq!(chord("Ctrl+Alt+V").reserved(), None);
        assert!(
            ChordError::Reserved(Reserved::System)
                .to_string()
                .contains("Windows")
        );
    }
}
