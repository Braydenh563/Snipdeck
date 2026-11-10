//! Typed-trigger expansion, the portable half: what a key press means to the matcher, and the
//! rolling buffer of what was typed (v1.1 slice 5b).
//!
//! The Windows keyboard hook (`windows::typing_hook`) translates each key press with the
//! foreground layout and hands a [`Stroke`] to [`classify`]; the result feeds a [`TypeBuffer`],
//! whose suffix is compared with the item triggers.
//!
//! **Privacy.** Typed characters live only in a [`TypeBuffer`] of [`BUFFER_CHARS`] characters,
//! which is wiped on reset and on drop. Nothing here implements `Debug` with the characters, so
//! they cannot reach a log or a trace by accident; keep it that way.

use zeroize::Zeroize;

/// How many of the latest typed characters are kept; the longest trigger must fit.
pub const BUFFER_CHARS: usize = 32;

/// `VK_BACK`.
pub const VK_BACK: u16 = 0x08;
/// `VK_TAB`.
pub const VK_TAB: u16 = 0x09;
/// `VK_RETURN`.
pub const VK_RETURN: u16 = 0x0D;
/// `VK_SHIFT`, `VK_CONTROL`, `VK_MENU` (the generic modifier keys).
pub const VK_SHIFT: u16 = 0x10;
/// `VK_CONTROL`.
pub const VK_CONTROL: u16 = 0x11;
/// `VK_MENU` (`Alt`).
pub const VK_MENU: u16 = 0x12;
/// `VK_CAPITAL` (`Caps Lock`).
pub const VK_CAPITAL: u16 = 0x14;
/// `VK_ESCAPE`.
pub const VK_ESCAPE: u16 = 0x1B;
/// `VK_PRIOR` (`Page Up`) .. `VK_DOWN`: `Page Up`, `Page Down`, `End`, `Home`, arrows.
pub const VK_PRIOR: u16 = 0x21;
/// `VK_DOWN`, the last of the navigation block.
pub const VK_DOWN: u16 = 0x28;
/// `VK_DELETE`.
pub const VK_DELETE: u16 = 0x2E;
/// `VK_LWIN`.
pub const VK_LWIN: u16 = 0x5B;
/// `VK_RWIN`.
pub const VK_RWIN: u16 = 0x5C;
/// `VK_NUMLOCK`.
pub const VK_NUMLOCK: u16 = 0x90;
/// `VK_SCROLL`.
pub const VK_SCROLL: u16 = 0x91;
/// `VK_LSHIFT` .. `VK_RMENU`: the left and right modifier keys.
pub const VK_LSHIFT: u16 = 0xA0;
/// `VK_RMENU`, the last of the left/right modifier block.
pub const VK_RMENU: u16 = 0xA5;
/// `VK_PROCESSKEY`: an IME is composing; the key does not map to a character.
pub const VK_PROCESSKEY: u16 = 0xE5;

/// What `ToUnicodeEx` (flag `0x4`, foreground layout) made of a key press.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Translation {
    /// Exactly one character.
    Text(char),
    /// A dead key (accent waiting for a letter).
    Dead,
    /// No character, or more than one.
    Nothing,
}

// A character is a typed secret candidate: never print it.
impl std::fmt::Debug for Translation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(_) => f.write_str("Text(..)"),
            Self::Dead => f.write_str("Dead"),
            Self::Nothing => f.write_str("Nothing"),
        }
    }
}

/// One key press as the hook sees it.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per physical modifier key"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stroke {
    /// The virtual-key code.
    pub vk: u16,
    /// `Ctrl` is down.
    pub ctrl: bool,
    /// `Alt` is down. `Ctrl` and `Alt` together are `AltGr` on layouts that have it.
    pub alt: bool,
    /// A Windows key is down.
    pub win: bool,
    /// The translation of this press.
    pub text: Translation,
}

/// What the matcher does with a key press.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TypedEvent {
    /// A character was typed.
    Char(char),
    /// The last character was erased.
    Backspace,
    /// The typing context is gone (focus moved, caret jumped, an IME composes): forget everything.
    Reset,
}

// A character is a typed secret candidate: never print it.
impl std::fmt::Debug for TypedEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Char(_) => f.write_str("Char(..)"),
            Self::Backspace => f.write_str("Backspace"),
            Self::Reset => f.write_str("Reset"),
        }
    }
}

/// Maps a key press to what the matcher should do, or `None` when the press changes nothing
/// (a bare modifier, Caps Lock).
///
/// Resets: the Windows key, `Ctrl` or `Alt` alone (a shortcut), `Ctrl+Backspace` (a word),
/// arrows, `Home`, `End`, `Page Up/Down`, `Enter`, `Esc`, `Tab`, `Delete`, `VK_PROCESSKEY`
/// (IME), a dead key (the composed letter cannot be known, so the buffer is dropped), and any
/// press that yields no single printable character. `Ctrl` with `Alt` is `AltGr`: its
/// translation is typed text.
pub fn classify(stroke: Stroke) -> Option<TypedEvent> {
    let vk = stroke.vk;
    if matches!(vk, VK_LWIN | VK_RWIN) {
        return Some(TypedEvent::Reset);
    }
    if matches!(
        vk,
        VK_SHIFT | VK_CONTROL | VK_MENU | VK_CAPITAL | VK_NUMLOCK | VK_SCROLL
    ) || (VK_LSHIFT..=VK_RMENU).contains(&vk)
    {
        return None;
    }
    let shortcut = stroke.win || (stroke.ctrl != stroke.alt);
    if vk == VK_PROCESSKEY
        || matches!(vk, VK_TAB | VK_RETURN | VK_ESCAPE | VK_DELETE)
        || (VK_PRIOR..=VK_DOWN).contains(&vk)
        || shortcut
    {
        return Some(TypedEvent::Reset);
    }
    if vk == VK_BACK {
        return Some(TypedEvent::Backspace);
    }
    match stroke.text {
        Translation::Text(c) if !c.is_control() => Some(TypedEvent::Char(c)),
        _ => Some(TypedEvent::Reset),
    }
}

/// The latest [`BUFFER_CHARS`] typed characters. `Debug` shows no contents.
pub struct TypeBuffer {
    chars: [char; BUFFER_CHARS],
    len: usize,
}

impl std::fmt::Debug for TypeBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeBuffer").finish_non_exhaustive()
    }
}

impl Default for TypeBuffer {
    fn default() -> Self {
        Self {
            chars: ['\0'; BUFFER_CHARS],
            len: 0,
        }
    }
}

impl Drop for TypeBuffer {
    fn drop(&mut self) {
        self.reset();
    }
}

impl TypeBuffer {
    /// An empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies one event.
    pub fn apply(&mut self, event: TypedEvent) {
        match event {
            TypedEvent::Char(c) => self.push(c),
            TypedEvent::Backspace => self.len = self.len.saturating_sub(1),
            TypedEvent::Reset => self.reset(),
        }
        if self.len < BUFFER_CHARS {
            self.chars[self.len..].zeroize();
        }
    }

    fn push(&mut self, c: char) {
        if self.len == BUFFER_CHARS {
            self.chars.copy_within(1.., 0);
            self.len -= 1;
        }
        self.chars[self.len] = c;
        self.len += 1;
    }

    /// Forgets everything typed, wiping the storage.
    pub fn reset(&mut self) {
        self.chars.zeroize();
        self.len = 0;
    }

    /// How many characters are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The matcher hook point: the index (into `triggers`) of the longest non-empty trigger that
    /// the typed text ends with. Empty and over-long triggers never match. The caller decides
    /// whether to expand; on an expansion it erases `trigger.chars().count()` characters with
    /// [`crate::keys::backspaces`] and calls [`reset`](Self::reset).
    pub fn suffix_match<S: AsRef<str>>(&self, triggers: &[S]) -> Option<usize> {
        let typed = &self.chars[..self.len];
        let mut best: Option<(usize, usize)> = None;
        for (index, trigger) in triggers.iter().enumerate() {
            let mut count = 0;
            let mut matches = true;
            // Compare from the end of both.
            for (a, b) in trigger.as_ref().chars().rev().zip(typed.iter().rev()) {
                if a != *b {
                    matches = false;
                    break;
                }
            }
            if matches {
                count = trigger.as_ref().chars().count();
            }
            if matches && count > 0 && count <= self.len && best.is_none_or(|(_, n)| count > n) {
                best = Some((index, count));
            }
        }
        best.map(|(index, _)| index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(vk: u16, text: Translation) -> Stroke {
        Stroke {
            vk,
            ctrl: false,
            alt: false,
            win: false,
            text,
        }
    }

    fn ch(vk: u16, c: char) -> Stroke {
        stroke(vk, Translation::Text(c))
    }

    #[test]
    fn the_key_event_table() {
        // Plain letters, Shift (the translation already carries the case), Caps Lock.
        assert_eq!(classify(ch(0x41, 'a')), Some(TypedEvent::Char('a')));
        assert_eq!(classify(ch(0x41, 'A')), Some(TypedEvent::Char('A')));
        assert_eq!(classify(ch(0xBA, ';')), Some(TypedEvent::Char(';')));
        assert_eq!(classify(ch(0x20, ' ')), Some(TypedEvent::Char(' ')));
        // The modifier keys themselves change nothing.
        for vk in [
            VK_SHIFT, VK_CONTROL, VK_MENU, VK_CAPITAL, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5,
        ] {
            assert_eq!(classify(stroke(vk, Translation::Nothing)), None, "{vk:#x}");
        }
        // AltGr is Ctrl+Alt: typed text.
        let altgr = Stroke {
            ctrl: true,
            alt: true,
            ..ch(0x51, '@')
        };
        assert_eq!(classify(altgr), Some(TypedEvent::Char('@')));
        // Ctrl alone, Alt alone, Win: shortcuts.
        for (ctrl, alt, win) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let shortcut = Stroke {
                ctrl,
                alt,
                win,
                ..ch(0x56, 'v')
            };
            assert_eq!(classify(shortcut), Some(TypedEvent::Reset));
        }
        assert_eq!(
            classify(stroke(VK_LWIN, Translation::Nothing)),
            Some(TypedEvent::Reset)
        );
        // Backspace erases; Ctrl+Backspace erases a word we cannot follow.
        assert_eq!(
            classify(stroke(VK_BACK, Translation::Nothing)),
            Some(TypedEvent::Backspace)
        );
        let word = Stroke {
            ctrl: true,
            ..stroke(VK_BACK, Translation::Nothing)
        };
        assert_eq!(classify(word), Some(TypedEvent::Reset));
        // Dead key, IME, navigation, keys with no character.
        assert_eq!(
            classify(stroke(0xDC, Translation::Dead)),
            Some(TypedEvent::Reset)
        );
        assert_eq!(
            classify(stroke(VK_PROCESSKEY, Translation::Text('x'))),
            Some(TypedEvent::Reset)
        );
        for vk in [
            0x25, 0x26, 0x27, 0x28, 0x24, 0x23, 0x21, 0x22, VK_RETURN, VK_ESCAPE, VK_TAB, VK_DELETE,
        ] {
            assert_eq!(
                classify(stroke(vk, Translation::Nothing)),
                Some(TypedEvent::Reset),
                "{vk:#x}"
            );
        }
        assert_eq!(
            classify(stroke(0x74, Translation::Nothing)),
            Some(TypedEvent::Reset)
        );
        assert_eq!(classify(ch(0x41, '\u{1}')), Some(TypedEvent::Reset));
    }

    fn typed(buffer: &mut TypeBuffer, text: &str) {
        for c in text.chars() {
            buffer.apply(TypedEvent::Char(c));
        }
    }

    #[test]
    fn the_buffer_keeps_the_last_32_characters() {
        let mut buffer = TypeBuffer::new();
        typed(&mut buffer, &"x".repeat(40));
        typed(&mut buffer, ";sig");
        assert_eq!(buffer.len(), BUFFER_CHARS);
        assert_eq!(buffer.suffix_match(&[";sig"]), Some(0));
    }

    #[test]
    fn backspace_and_reset_edit_the_buffer() {
        let mut buffer = TypeBuffer::new();
        typed(&mut buffer, ";sigx");
        assert_eq!(buffer.suffix_match(&[";sig"]), None);
        buffer.apply(TypedEvent::Backspace);
        assert_eq!(buffer.suffix_match(&[";sig"]), Some(0));
        buffer.apply(TypedEvent::Reset);
        assert!(buffer.is_empty());
        buffer.apply(TypedEvent::Backspace);
        assert!(buffer.is_empty());
        assert_eq!(buffer.suffix_match(&[";sig"]), None);
    }

    #[test]
    fn the_longest_suffix_wins_and_empty_triggers_never_match() {
        let mut buffer = TypeBuffer::new();
        typed(&mut buffer, "a;sig");
        let triggers = ["", "sig", ";sig", ";signature"];
        assert_eq!(buffer.suffix_match(&triggers), Some(2));
        assert_eq!(buffer.suffix_match::<&str>(&[]), None);
        // A trigger longer than what was typed cannot match.
        buffer.apply(TypedEvent::Reset);
        typed(&mut buffer, "ig");
        assert_eq!(buffer.suffix_match(&[";sig"]), None);
    }

    #[test]
    fn debug_shows_no_typed_text() {
        let mut buffer = TypeBuffer::new();
        typed(&mut buffer, "hunter2");
        assert!(!format!("{buffer:?}").contains("hunter"));
        assert!(!format!("{:?}", TypedEvent::Char('Z')).contains('Z'));
        assert!(!format!("{:?}", ch(0x48, 'Z')).contains('Z'));
    }
}
