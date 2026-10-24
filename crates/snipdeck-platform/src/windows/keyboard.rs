//! `WinKeyboard`: keyboard state and key injection through `GetAsyncKeyState` and `SendInput`.
//!
//! Slint's own modifier state is stale in a window that never had keyboard focus (the panel is
//! non-activating), so the modifiers the user holds when they click are read from the keyboard
//! hardware state instead. Injection is one `SendInput` call per paste, so no other input can be
//! interleaved between the events of a chord.
//!
//! `SendInput` does **not** report input that UIPI blocked (the Target runs at a higher integrity
//! level): it still returns the full count. That case is prevented up front, by the elevation
//! check in [`super::target`] and [`crate::route::decide`], never detected here.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_INSERT, VK_LCONTROL,
    VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_V,
};

use crate::keys::{KeyEvent, Modifiers};
use crate::pipeline::{Error, Keyboard};
use crate::win_logic::{self, VirtualKey};

// The portable table in `win_logic` uses plain numbers; make sure they are Windows' numbers.
const _: () = {
    assert!(win_logic::VK_LSHIFT == VK_LSHIFT.0);
    assert!(win_logic::VK_RSHIFT == VK_RSHIFT.0);
    assert!(win_logic::VK_LCONTROL == VK_LCONTROL.0);
    assert!(win_logic::VK_RCONTROL == VK_RCONTROL.0);
    assert!(win_logic::VK_LMENU == VK_LMENU.0);
    assert!(win_logic::VK_RMENU == VK_RMENU.0);
    assert!(win_logic::VK_LWIN == VK_LWIN.0);
    assert!(win_logic::VK_RWIN == VK_RWIN.0);
    assert!(win_logic::VK_V == VK_V.0);
    assert!(win_logic::VK_INSERT == VK_INSERT.0);
    // Unverified on Windows (Fable v1 sweep A5).
    assert!(win_logic::VK_LEFT == VK_LEFT.0);
};

/// [`Keyboard`] on the real keyboard. Stateless; cheap to create.
#[derive(Debug, Default, Clone, Copy)]
pub struct WinKeyboard;

impl WinKeyboard {
    /// A keyboard handle.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Keyboard for WinKeyboard {
    /// Reads the physical state with `GetAsyncKeyState`, merging the left and right keys.
    fn held_modifiers(&mut self) -> Modifiers {
        win_logic::merged_modifiers(key_is_down)
    }

    /// Injects `events` with one `SendInput` call.
    ///
    /// A modifier release is sent for whichever of its left and right keys is physically down
    /// (see [`win_logic::expand`]). If Windows takes only part of the sequence, the keys that
    /// were left pressed are released again, so a failed paste cannot leave `Ctrl` stuck down.
    fn send(&mut self, events: &[KeyEvent]) -> Result<(), Error> {
        if events.is_empty() {
            return Ok(());
        }
        let sequence = win_logic::expand(events, key_is_down);
        let accepted = inject(&sequence)?;
        if accepted == sequence.len() {
            return Ok(());
        }
        let stuck: Vec<(VirtualKey, bool)> = win_logic::left_down(&sequence, accepted)
            .into_iter()
            .map(|key| (key, false))
            .collect();
        // Best effort: the sequence already failed, so a failure here changes nothing.
        let _ = inject(&stuck);
        Err(Error::Input(format!(
            "Windows accepted {accepted} of {} key events",
            sequence.len()
        )))
    }
}

/// Whether the physical key `vk` is down now (the high bit of `GetAsyncKeyState`).
fn key_is_down(vk: u16) -> bool {
    // SAFETY: `GetAsyncKeyState` takes a plain integer and has no preconditions.
    (unsafe { GetAsyncKeyState(i32::from(vk)) }) < 0
}

/// Sends `sequence` in one call; returns how many events Windows accepted.
fn inject(sequence: &[(VirtualKey, bool)]) -> Result<usize, Error> {
    if sequence.is_empty() {
        return Ok(0);
    }
    let inputs: Vec<INPUT> = sequence
        .iter()
        .map(|&(key, press)| key_input(key, press))
        .collect();
    let size = i32::try_from(size_of::<INPUT>())
        .map_err(|_| Error::Input("INPUT size does not fit in an i32".into()))?;
    // SAFETY: `inputs` is a live slice of fully initialised `INPUT` structs and `size` is the
    // size of one of them, as `SendInput` requires.
    let sent = unsafe { SendInput(&inputs, size) };
    Ok(usize::try_from(sent).unwrap_or(0))
}

fn key_input(key: VirtualKey, press: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if !press {
        flags |= KEYEVENTF_KEYUP;
    }
    if key.extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(key.vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_input_sets_keyup_and_extended_flags() {
        let insert = VirtualKey {
            vk: win_logic::VK_INSERT,
            extended: true,
        };
        let down = key_input(insert, true);
        let up = key_input(insert, false);
        // SAFETY: both were built by `key_input`, which initialises the `ki` variant.
        let (down, up) = unsafe { (down.Anonymous.ki, up.Anonymous.ki) };
        assert_eq!(down.dwFlags, KEYEVENTF_EXTENDEDKEY);
        assert_eq!(up.dwFlags, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP);
        assert_eq!(down.wVk, VK_INSERT);
    }
}
