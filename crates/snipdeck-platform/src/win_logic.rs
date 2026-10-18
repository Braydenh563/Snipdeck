//! The decisions behind the Windows backend that need no Windows API.
//!
//! They sit outside the `cfg(windows)` module, and use plain integers instead of `windows` crate
//! types, so they are unit-tested on every OS. The Windows code in `windows::keyboard`,
//! `windows::target` and `windows::window_style` only asks the OS for facts and passes them in.

use crate::keys::{Key, KeyEvent, Modifier, Modifiers};

// Virtual-key codes (`winuser.h`). Plain numbers: the Windows constants are checked against these
// in the `windows` module.
/// `VK_LSHIFT`.
pub const VK_LSHIFT: u16 = 0xA0;
/// `VK_RSHIFT`.
pub const VK_RSHIFT: u16 = 0xA1;
/// `VK_LCONTROL`.
pub const VK_LCONTROL: u16 = 0xA2;
/// `VK_RCONTROL`.
pub const VK_RCONTROL: u16 = 0xA3;
/// `VK_LMENU` (left `Alt`).
pub const VK_LMENU: u16 = 0xA4;
/// `VK_RMENU` (right `Alt`).
pub const VK_RMENU: u16 = 0xA5;
/// `VK_LWIN`.
pub const VK_LWIN: u16 = 0x5B;
/// `VK_RWIN`.
pub const VK_RWIN: u16 = 0x5C;
/// `VK_V`.
pub const VK_V: u16 = 0x56;
/// `VK_INSERT`.
pub const VK_INSERT: u16 = 0x2D;
/// The unassigned virtual key (0xE8) used to mask a lone `Alt` / `Win` release.
pub const VK_MASK: u16 = 0xE8;

/// A virtual key and whether `SendInput` needs `KEYEVENTF_EXTENDEDKEY` for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualKey {
    /// The virtual-key code.
    pub vk: u16,
    /// The key is on the extended part of the keyboard (`Insert`, `Win`, right `Ctrl`/`Alt`).
    pub extended: bool,
}

const fn plain(vk: u16) -> VirtualKey {
    VirtualKey {
        vk,
        extended: false,
    }
}

const fn extended(vk: u16) -> VirtualKey {
    VirtualKey { vk, extended: true }
}

/// The left and right physical keys of `modifier`.
pub const fn modifier_variants(modifier: Modifier) -> [VirtualKey; 2] {
    match modifier {
        Modifier::Ctrl => [plain(VK_LCONTROL), extended(VK_RCONTROL)],
        Modifier::Shift => [plain(VK_LSHIFT), plain(VK_RSHIFT)],
        Modifier::Alt => [plain(VK_LMENU), extended(VK_RMENU)],
        Modifier::Win => [extended(VK_LWIN), extended(VK_RWIN)],
    }
}

/// The key injected for a press of `key`. Modifiers are pressed as their left variant.
pub const fn key_to_vk(key: Key) -> VirtualKey {
    match key {
        Key::Modifier(modifier) => modifier_variants(modifier)[0],
        Key::V => plain(VK_V),
        Key::Insert => extended(VK_INSERT),
        Key::Mask => plain(VK_MASK),
    }
}

/// The modifiers held, given a way to ask whether a virtual key is down. Left and right merged.
pub fn merged_modifiers(is_down: impl Fn(u16) -> bool) -> Modifiers {
    let held = |modifier| modifier_variants(modifier).iter().any(|k| is_down(k.vk));
    Modifiers {
        ctrl: held(Modifier::Ctrl),
        shift: held(Modifier::Shift),
        alt: held(Modifier::Alt),
        win: held(Modifier::Win),
    }
}

/// Turns pipeline events into the concrete `(key, is_press)` transitions to inject.
///
/// A modifier press is injected as the left key. A modifier release is injected for whichever of
/// the left and right keys `is_down` reports as down, because Windows tracks them separately and
/// a release of the generic key may not clear a right-hand key the user is holding. When neither
/// is down (a modifier this very sequence pressed), the left key is released.
pub fn expand(events: &[KeyEvent], is_down: impl Fn(u16) -> bool) -> Vec<(VirtualKey, bool)> {
    let mut out = Vec::with_capacity(events.len());
    for event in events {
        match event.key {
            Key::Modifier(modifier) if !event.down => {
                let variants = modifier_variants(modifier);
                let down: Vec<VirtualKey> =
                    variants.into_iter().filter(|key| is_down(key.vk)).collect();
                if down.is_empty() {
                    out.push((variants[0], false));
                } else {
                    out.extend(down.into_iter().map(|key| (key, false)));
                }
            }
            key => out.push((key_to_vk(key), event.down)),
        }
    }
    out
}

/// The keys left pressed after the first `accepted` transitions of `sequence` were injected.
/// Used to release them again when `SendInput` took only part of a sequence.
pub fn left_down(sequence: &[(VirtualKey, bool)], accepted: usize) -> Vec<VirtualKey> {
    let mut down: Vec<VirtualKey> = Vec::new();
    for &(key, press) in sequence.iter().take(accepted) {
        down.retain(|k| k.vk != key.vk);
        if press {
            down.push(key);
        }
    }
    down
}

/// UI Automation `UIA_EditControlTypeId`.
pub const UIA_EDIT: i32 = 50004;
/// UI Automation `UIA_DocumentControlTypeId`.
pub const UIA_DOCUMENT: i32 = 50030;

/// What UI Automation said about the focused element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementFacts {
    /// `UIA_ControlTypePropertyId`.
    pub control_type: i32,
    /// `IsKeyboardFocusable`.
    pub keyboard_focusable: bool,
    /// `IsEnabled`.
    pub enabled: bool,
    /// `ValuePattern.IsReadOnly`, or `None` when the element has no `ValuePattern`.
    pub value_read_only: Option<bool>,
    /// The element supports `TextPattern`.
    pub has_text_pattern: bool,
}

/// Whether the focused element can take pasted text.
///
/// It must be enabled and keyboard-focusable. A `ValuePattern` is decisive: read-only rejects,
/// writable accepts. Without one, an Edit or Document control, or anything with `TextPattern`
/// (terminals such as Windows Terminal expose only that), is taken to be a text surface.
pub fn accepts_text(facts: ElementFacts) -> bool {
    if !facts.keyboard_focusable || !facts.enabled {
        return false;
    }
    match facts.value_read_only {
        Some(read_only) => !read_only,
        None => matches!(facts.control_type, UIA_EDIT | UIA_DOCUMENT) || facts.has_text_pattern,
    }
}

/// Whether input can be injected into a process: UIPI drops it when the target's integrity level
/// is above ours. An unreadable token (ours or theirs) counts as not injectable, because that is
/// what an elevated process looks like from a non-elevated one.
pub fn can_inject(own_integrity: Option<u32>, target_integrity: Option<u32>) -> bool {
    matches!((own_integrity, target_integrity), (Some(own), Some(target)) if target <= own)
}

/// The file name of an executable path, either separator.
pub fn exe_file_name(path: &str) -> Option<&str> {
    path.rsplit(['\\', '/'])
        .next()
        .filter(|name| !name.is_empty())
}

/// Window classes of the desktop and taskbar shell. A click there makes such a window the
/// foreground window and `GetGUIThreadInfo` reports a focus window in it, but it takes no text:
/// a paste would drop files onto the desktop or stall.
const SHELL_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Progman",
    "WorkerW",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
];

/// Whether a window of this class (case-insensitive) is part of the desktop or taskbar shell and
/// so never a paste Target.
pub fn is_shell_surface_class(class: &str) -> bool {
    SHELL_CLASSES.iter().any(|c| c.eq_ignore_ascii_case(class))
}

/// `WS_EX_NOACTIVATE`.
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
/// `WS_EX_TOOLWINDOW`.
pub const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
/// `WS_EX_APPWINDOW`.
pub const WS_EX_APPWINDOW: u32 = 0x0004_0000;

/// The extended style the panel must have: never activated by a click, no taskbar button, no
/// Alt+Tab entry. winit adds `WS_EX_APPWINDOW` to top-level windows, which would override
/// `WS_EX_TOOLWINDOW`, so it is stripped.
pub const fn panel_ex_style(style: u32) -> u32 {
    (style | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW
}

/// One `SetWindowPos` z-order step of [`raise_steps`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZStep {
    /// `HWND_TOPMOST`: above every non-topmost window.
    Topmost,
    /// `HWND_NOTOPMOST`: leave the topmost band, staying at the top of the normal windows.
    NotTopmost,
}

/// The z-order steps that bring a window to the front without activating it.
///
/// `ShowWindow` on a hidden non-activating window (the Panel) keeps the place the window had in
/// the stack when it was hidden, which is behind whatever the user used since, and a process that
/// is not in the foreground cannot just `SetForegroundWindow` its way to the top. Moving the
/// window to the topmost band and back needs no activation. A window the user pinned on top
/// (`was_topmost`) stays there: only the first step runs.
#[must_use]
pub const fn raise_steps(was_topmost: bool) -> &'static [ZStep] {
    if was_topmost {
        &[ZStep::Topmost]
    } else {
        &[ZStep::Topmost, ZStep::NotTopmost]
    }
}

/// Which Rust type owns each window class name in this process.
///
/// A window class is registered once per name and keeps the window procedure of its first
/// registration. The hidden windows store a pointer to their handler type in the window and the
/// procedure casts it back, so a second *type* under an already-registered name would be cast
/// wrongly. [`ClassNames::claim`] refuses that.
#[derive(Debug, Default)]
pub struct ClassNames {
    claimed: Vec<(&'static str, std::any::TypeId)>,
}

impl ClassNames {
    /// Records that `owner` uses the class `name`. `true` if the name is new or already owned by
    /// the same type; `false` if another type owns it.
    pub fn claim(&mut self, name: &'static str, owner: std::any::TypeId) -> bool {
        if let Some((_, existing)) = self.claimed.iter().find(|(claimed, _)| *claimed == name) {
            return *existing == owner;
        }
        self.claimed.push((name, owner));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_class_name_belongs_to_one_handler_type() {
        use std::any::TypeId;
        struct A;
        struct B;
        let mut names = ClassNames::default();
        assert!(names.claim("One", TypeId::of::<A>()));
        assert!(names.claim("One", TypeId::of::<A>()), "same type again");
        assert!(!names.claim("One", TypeId::of::<B>()), "another type");
        assert!(names.claim("Two", TypeId::of::<B>()), "another name");
    }

    fn ev(key: Key, down: bool) -> KeyEvent {
        KeyEvent { key, down }
    }

    fn m(modifier: Modifier, down: bool) -> KeyEvent {
        ev(Key::Modifier(modifier), down)
    }

    fn nothing_down(_: u16) -> bool {
        false
    }

    #[test]
    fn keys_map_to_the_documented_virtual_keys() {
        assert_eq!(key_to_vk(Key::V), plain(0x56));
        assert_eq!(key_to_vk(Key::Insert), extended(0x2D));
        assert_eq!(key_to_vk(Key::Mask), plain(0xE8));
        assert_eq!(key_to_vk(Key::Modifier(Modifier::Ctrl)), plain(0xA2));
        assert_eq!(key_to_vk(Key::Modifier(Modifier::Shift)), plain(0xA0));
        assert_eq!(key_to_vk(Key::Modifier(Modifier::Alt)), plain(0xA4));
        assert_eq!(key_to_vk(Key::Modifier(Modifier::Win)), extended(0x5B));
    }

    #[test]
    fn extended_flag_is_set_only_for_extended_keys() {
        for modifier in [
            Modifier::Ctrl,
            Modifier::Shift,
            Modifier::Alt,
            Modifier::Win,
        ] {
            let [left, right] = modifier_variants(modifier);
            assert_ne!(left.vk, right.vk);
            // Win keys are extended; right Ctrl/Alt are; Shift and left Ctrl/Alt are not.
            assert_eq!(left.extended, modifier == Modifier::Win);
            assert_eq!(right.extended, modifier != Modifier::Shift);
        }
    }

    #[test]
    fn held_modifiers_merge_left_and_right() {
        let held = merged_modifiers(|vk| matches!(vk, VK_RCONTROL | VK_LSHIFT | VK_RWIN));
        assert_eq!(
            held,
            Modifiers {
                ctrl: true,
                shift: true,
                alt: false,
                win: true
            }
        );
        assert_eq!(merged_modifiers(nothing_down), Modifiers::default());
        assert!(merged_modifiers(|vk| vk == VK_RMENU).alt);
    }

    #[test]
    fn expand_presses_the_left_key_and_releases_the_left_key_when_none_is_down() {
        let events = [
            m(Modifier::Ctrl, true),
            ev(Key::V, true),
            ev(Key::V, false),
            m(Modifier::Ctrl, false),
        ];
        assert_eq!(
            expand(&events, nothing_down),
            [
                (plain(VK_LCONTROL), true),
                (plain(VK_V), true),
                (plain(VK_V), false),
                (plain(VK_LCONTROL), false),
            ]
        );
    }

    #[test]
    fn expand_releases_the_right_hand_key_the_user_holds() {
        let events = [m(Modifier::Alt, false)];
        assert_eq!(
            expand(&events, |vk| vk == VK_RMENU),
            [(extended(VK_RMENU), false)]
        );
    }

    #[test]
    fn expand_releases_both_variants_when_both_are_down() {
        let events = [m(Modifier::Shift, false)];
        assert_eq!(
            expand(&events, |vk| matches!(vk, VK_LSHIFT | VK_RSHIFT)),
            [(plain(VK_LSHIFT), false), (plain(VK_RSHIFT), false)]
        );
    }

    #[test]
    fn expand_keeps_the_mask_press_before_the_alt_release() {
        let events = [
            ev(Key::Mask, true),
            ev(Key::Mask, false),
            m(Modifier::Alt, false),
        ];
        let out = expand(&events, |vk| vk == VK_LMENU);
        assert_eq!(
            out,
            [
                (plain(VK_MASK), true),
                (plain(VK_MASK), false),
                (plain(VK_LMENU), false)
            ]
        );
    }

    #[test]
    fn left_down_reports_keys_still_pressed_after_a_partial_send() {
        let seq = [
            (plain(VK_LCONTROL), true),
            (plain(VK_V), true),
            (plain(VK_V), false),
            (plain(VK_LCONTROL), false),
        ];
        assert_eq!(left_down(&seq, 0), []);
        assert_eq!(left_down(&seq, 1), [plain(VK_LCONTROL)]);
        assert_eq!(left_down(&seq, 2), [plain(VK_LCONTROL), plain(VK_V)]);
        assert_eq!(left_down(&seq, 3), [plain(VK_LCONTROL)]);
        assert_eq!(left_down(&seq, 4), []);
        assert_eq!(left_down(&seq, 99), []);
    }

    fn facts() -> ElementFacts {
        ElementFacts {
            control_type: 50020, // Text
            keyboard_focusable: true,
            enabled: true,
            value_read_only: None,
            has_text_pattern: false,
        }
    }

    #[test]
    fn edit_and_document_controls_accept_text() {
        for control_type in [UIA_EDIT, UIA_DOCUMENT] {
            assert!(accepts_text(ElementFacts {
                control_type,
                ..facts()
            }));
        }
    }

    #[test]
    fn a_terminal_with_only_text_pattern_accepts_text() {
        assert!(accepts_text(ElementFacts {
            has_text_pattern: true,
            ..facts()
        }));
    }

    #[test]
    fn a_plain_focusable_control_does_not() {
        // A button: focusable, no value or text pattern.
        assert!(!accepts_text(ElementFacts {
            control_type: 50000,
            ..facts()
        }));
    }

    #[test]
    fn value_pattern_decides_by_read_only() {
        assert!(accepts_text(ElementFacts {
            value_read_only: Some(false),
            ..facts()
        }));
        // Read-only wins even for an Edit control with text pattern.
        assert!(!accepts_text(ElementFacts {
            control_type: UIA_EDIT,
            has_text_pattern: true,
            value_read_only: Some(true),
            ..facts()
        }));
    }

    #[test]
    fn unfocusable_or_disabled_elements_never_accept() {
        let editable = ElementFacts {
            control_type: UIA_EDIT,
            ..facts()
        };
        assert!(accepts_text(editable));
        assert!(!accepts_text(ElementFacts {
            keyboard_focusable: false,
            ..editable
        }));
        assert!(!accepts_text(ElementFacts {
            enabled: false,
            ..editable
        }));
    }

    #[test]
    fn integrity_gate_blocks_higher_and_unreadable() {
        assert!(can_inject(Some(0x2000), Some(0x2000)));
        assert!(can_inject(Some(0x3000), Some(0x2000)));
        assert!(!can_inject(Some(0x2000), Some(0x3000)));
        assert!(!can_inject(Some(0x2000), None));
        assert!(!can_inject(None, Some(0x1000)));
    }

    #[test]
    fn exe_file_name_takes_the_last_component() {
        assert_eq!(
            exe_file_name(r"C:\Windows\System32\notepad.exe"),
            Some("notepad.exe")
        );
        assert_eq!(exe_file_name("/usr/bin/x.exe"), Some("x.exe"));
        assert_eq!(exe_file_name("plain.exe"), Some("plain.exe"));
        assert_eq!(exe_file_name(r"C:\dir\"), None);
        assert_eq!(exe_file_name(""), None);
    }

    #[test]
    fn shell_classes_are_not_targets() {
        for class in [
            "Shell_TrayWnd",
            "shell_secondarytraywnd",
            "Progman",
            "WorkerW",
            "NotifyIconOverflowWindow",
            "TopLevelWindowForOverflowXamlIsland",
        ] {
            assert!(is_shell_surface_class(class), "{class}");
        }
        for class in [
            "Notepad",
            "CASCADIA_HOSTING_WINDOW_CLASS",
            "Chrome_WidgetWin_1",
            "",
        ] {
            assert!(!is_shell_surface_class(class), "{class}");
        }
    }

    #[test]
    fn raising_the_panel_goes_through_the_topmost_band_unless_it_is_pinned_there() {
        assert_eq!(raise_steps(false), [ZStep::Topmost, ZStep::NotTopmost]);
        assert_eq!(
            raise_steps(true),
            [ZStep::Topmost],
            "a pinned Panel stays on top"
        );
    }

    #[test]
    fn panel_style_is_noactivate_toolwindow_and_not_appwindow() {
        let style = panel_ex_style(WS_EX_APPWINDOW | 0x1);
        assert_ne!(style & WS_EX_NOACTIVATE, 0);
        assert_ne!(style & WS_EX_TOOLWINDOW, 0);
        assert_eq!(style & WS_EX_APPWINDOW, 0);
        assert_eq!(style & 0x1, 0x1, "other bits are kept");
        assert_eq!(panel_ex_style(style), style, "idempotent");
    }
}
