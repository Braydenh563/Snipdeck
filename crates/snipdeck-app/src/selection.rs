//! `{selection}`: the text selected in the Target, read before the fill form takes the focus.
//!
//! The only way into another app's selection is to ask it to copy: this writes a probe text to the
//! clipboard, sends `Ctrl+C`, waits for the clipboard to stop being the probe, reads the text,
//! and puts the user's own clipboard back. Apps that answer `Ctrl+C` with something else are
//! never asked ([`copy_is_safe`]): a console would take it as an interrupt.
//!
//! **Unverified on Windows:** the sequence runs against fakes only. The owner must try it in
//! Notepad, a browser, Word and Windows Terminal (`tools/windows-test`).

use std::time::Duration;

use snipdeck_platform::{
    keys::copy_sequence,
    payload::{Data, Payload},
    pipeline::{Clipboard, Env, Keyboard, Timing},
    route::Route,
};

use crate::host::ClipboardText;

/// The probe the clipboard holds until the Target answers. It starts with a zero-width space
/// so no selection a person makes is equal to it.
const PROBE: &str = "\u{200b}snipdeck-selection-probe";
/// How long between two looks at the clipboard.
const POLL: Duration = Duration::from_millis(25);
/// How many looks (so the Target has 400 ms to copy).
const POLLS: u32 = 16;
/// Consoles, where `Ctrl+C` interrupts the running program when nothing is selected.
const CONSOLES: &[&str] = &[
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "conhost.exe",
    "openconsole.exe",
    "windowsterminal.exe",
    "wt.exe",
];

/// Whether `Ctrl+C` in the Target means "copy": the paste route is the plain `Ctrl+V` one and
/// the app is no console.
#[must_use]
pub fn copy_is_safe(route: &Route, app: Option<&str>) -> bool {
    matches!(
        route,
        Route::Paste {
            key: snipdeck_platform::keys::PasteKey::CtrlV
        }
    ) && !app.is_some_and(|a| CONSOLES.iter().any(|c| a.eq_ignore_ascii_case(c)))
}

/// The text selected in the Target, or `None` when nothing is selected, the Target does not
/// answer, or the clipboard could not be kept whole (then nothing is touched).
pub fn capture<C, K, T>(env: &mut Env<'_, C, K, T>) -> Option<String>
where
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    let snapshot = env.clipboard.snapshot().ok()?;
    if !env.clipboard.is_complete(&snapshot) {
        return None;
    }
    let probe = Payload::new(vec![Data::Text(PROBE.to_owned())])?;
    let written = env.clipboard.write_transient(&probe).ok()?;
    let mut ready = false;
    for _ in 0..POLLS {
        if env.clipboard.write_confirmed(&written) {
            ready = true;
            break;
        }
        env.timing.sleep(POLL);
    }
    let mut answer = None;
    let held = env.keyboard.held_modifiers();
    if ready && env.keyboard.send(&copy_sequence(held)).is_ok() {
        for _ in 0..POLLS {
            match env.clipboard.read_text() {
                Some(text) if text != PROBE => {
                    answer = Some(text);
                    break;
                }
                _ => env.timing.sleep(POLL),
            }
        }
    }
    // The user's clipboard comes back whatever happened. A failure to put it back is the
    // clipboard's own error: there is nothing better to do than not use the text.
    if env.clipboard.restore_forced(snapshot).is_err() {
        return None;
    }
    answer.filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use snipdeck_platform::{
        keys::{Key, PasteKey},
        pipeline::Error,
        testing::{FakeClipboard, FakeKeyboard, FakeTiming},
    };

    use super::*;

    struct World {
        clipboard: FakeClipboard,
        keyboard: FakeKeyboard,
        timing: FakeTiming,
    }

    impl World {
        fn new() -> Self {
            Self {
                clipboard: FakeClipboard::default(),
                keyboard: FakeKeyboard::default(),
                timing: FakeTiming::default(),
            }
        }
        fn capture(&mut self) -> Option<String> {
            capture(&mut Env {
                clipboard: &mut self.clipboard,
                keyboard: &mut self.keyboard,
                timing: &mut self.timing,
            })
        }
    }

    #[test]
    fn library_23_the_selection_is_copied_read_and_the_clipboard_put_back() {
        let mut w = World::new();
        w.clipboard.copy_arrives = Some((2, "chosen words".into()));
        assert_eq!(w.capture().as_deref(), Some("chosen words"));
        assert_eq!(
            w.clipboard.log.first().map(String::as_str),
            Some("snapshot")
        );
        assert_eq!(
            w.clipboard.log.last().map(String::as_str),
            Some("restore_forced")
        );
        let sent = &w.keyboard.sent;
        assert_eq!(sent.len(), 1);
        assert!(sent[0].iter().any(|e| e.key == Key::C && e.down));
        assert!(!sent[0].iter().any(|e| e.key == Key::V));
        // The wait was made with the fake clock only.
        assert!(w.timing.now < Duration::from_millis(400));
    }

    #[test]
    fn library_23_nothing_selected_gives_nothing_and_still_restores() {
        let mut w = World::new();
        assert_eq!(w.capture(), None);
        assert_eq!(
            w.clipboard.log.last().map(String::as_str),
            Some("restore_forced")
        );
        assert!(w.timing.now >= Duration::from_millis(400));
    }

    #[test]
    fn library_23_a_clipboard_that_cannot_be_kept_is_not_touched() {
        let mut w = World::new();
        w.clipboard.fail_snapshot = true;
        assert_eq!(w.capture(), None);
        assert_eq!(w.keyboard.sent.len(), 0)
        assert_eq!(w.clipboard.written.len(), 0)

        let mut partial = World::new();
        partial.clipboard.incomplete_snapshot = true;
        assert_eq!(partial.capture(), None);
        assert_eq!(partial.keyboard.sent.len(), 0)
        assert_eq!(partial.clipboard.written.len(), 0)

        let mut lost = World::new();
        lost.clipboard.copy_arrives = Some((0, "x".into()));
        lost.clipboard.restore_forced_result = Some(Err(Error::Clipboard("busy".into())));
        assert_eq!(lost.capture(), None);

        let mut blocked = World::new();
        blocked.keyboard.fail = true;
        blocked.clipboard.copy_arrives = Some((0, "x".into()));
        assert_eq!(blocked.capture(), None);
    }

    #[test]
    fn library_23_consoles_and_other_paste_keys_are_never_asked_to_copy() {
        let ctrl_v = Route::Paste {
            key: PasteKey::CtrlV,
        };
        assert!(copy_is_safe(&ctrl_v, Some("notepad.exe")));
        assert!(copy_is_safe(&ctrl_v, None));
        assert!(!copy_is_safe(&ctrl_v, Some("WindowsTerminal.exe")));
        assert!(!copy_is_safe(&ctrl_v, Some("pwsh.exe")));
        assert!(!copy_is_safe(
            &Route::Paste {
                key: PasteKey::CtrlShiftV
            },
            Some("alacritty.exe")
        ));
    }
}
