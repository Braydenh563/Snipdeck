//! Paste keys and the key events that produce them.
//!
//! The user may be holding modifiers when they click an item (`Alt+click` pastes plain text,
//! `Ctrl+click` is a plain click). The target must still see exactly the paste chord, so held
//! modifiers are released first. Releasing a lone `Alt` or `Win` would open the target's menu bar
//! or the Start menu, so a [`Key::Mask`] press is injected first (the unassigned virtual key the
//! shell treats as "something else was pressed"). **Unverified on Windows** (S1 step 2c).

/// A modifier key, left and right variants merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    /// `Ctrl`.
    Ctrl,
    /// `Shift`.
    Shift,
    /// `Alt`.
    Alt,
    /// The Windows key.
    Win,
}

/// The modifiers physically held right now.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per physical modifier key; an enum set would only add indirection"
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// `Ctrl` is down.
    pub ctrl: bool,
    /// `Shift` is down.
    pub shift: bool,
    /// `Alt` is down.
    pub alt: bool,
    /// The Windows key is down.
    pub win: bool,
}

impl Modifiers {
    /// Whether `modifier` is held.
    pub fn has(self, modifier: Modifier) -> bool {
        match modifier {
            Modifier::Ctrl => self.ctrl,
            Modifier::Shift => self.shift,
            Modifier::Alt => self.alt,
            Modifier::Win => self.win,
        }
    }
}

/// A key the platform layer can inject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A modifier.
    Modifier(Modifier),
    /// The `V` key.
    V,
    /// The `Insert` key (an extended key on Windows).
    Insert,
    /// The `Left` arrow (an extended key on Windows), pressed to move the caret back after a
    /// paste (`{cursor}`).
    Left,
    /// An unassigned virtual key, pressed and released while `Alt` or `Win` is held so that
    /// releasing it does not open a menu bar or the Start menu.
    Mask,
}

/// One injected key transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    /// The key.
    pub key: Key,
    /// `true` for a press, `false` for a release.
    pub down: bool,
}

/// The key chord a Target uses to paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteKey {
    /// `Ctrl+V`: nearly every app.
    CtrlV,
    /// `Ctrl+Shift+V`: terminals that reserve `Ctrl+V`.
    CtrlShiftV,
    /// `Shift+Insert`: terminals that do not accept `Ctrl+V` at all.
    ShiftInsert,
}

impl PasteKey {
    /// The modifiers of the chord.
    pub fn modifiers(self) -> &'static [Modifier] {
        match self {
            Self::CtrlV => &[Modifier::Ctrl],
            Self::CtrlShiftV => &[Modifier::Ctrl, Modifier::Shift],
            Self::ShiftInsert => &[Modifier::Shift],
        }
    }

    /// The non-modifier key of the chord.
    pub fn main_key(self) -> Key {
        match self {
            Self::CtrlV | Self::CtrlShiftV => Key::V,
            Self::ShiftInsert => Key::Insert,
        }
    }
}

/// Programs whose paste key is not `Ctrl+V`, by lower-case executable name: the rules a new
/// settings file starts with (Settings > Programs). Everything else uses `Ctrl+V` (Windows
/// Terminal, conhost, PowerShell and cmd accept it). **Unverified**: the terminal rows come
/// from each app's documented defaults, not from a Windows run (S1 step 9).
pub const BUILTIN_PASTE_KEYS: &[(&str, PasteKey)] = &[
    ("mintty.exe", PasteKey::ShiftInsert),
    ("putty.exe", PasteKey::ShiftInsert),
    ("kitty.exe", PasteKey::ShiftInsert),
    ("alacritty.exe", PasteKey::CtrlShiftV),
    ("wezterm-gui.exe", PasteKey::CtrlShiftV),
];

/// The paste key the user chose for one program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteRule {
    /// The executable file name (case-insensitive).
    pub exe: String,
    /// What to press there.
    pub key: PasteKey,
}

/// [`BUILTIN_PASTE_KEYS`] as rules.
pub fn builtin_paste_rules() -> Vec<PasteRule> {
    BUILTIN_PASTE_KEYS
        .iter()
        .map(|&(exe, key)| PasteRule {
            exe: exe.to_owned(),
            key,
        })
        .collect()
}

/// The paste key for the app with this executable file name (case-insensitive): its rule, or
/// `Ctrl+V` when it has none. The built-in table is not consulted here: a settings file starts
/// with it as rules, so that removing a row means `Ctrl+V` again.
pub fn paste_key_for(exe: &str, rules: &[PasteRule]) -> PasteKey {
    rules
        .iter()
        .find(|r| r.exe.eq_ignore_ascii_case(exe))
        .map_or(PasteKey::CtrlV, |r| r.key)
}

fn event(key: Key, down: bool) -> KeyEvent {
    KeyEvent { key, down }
}

/// The key events that make the Target see exactly `paste`, given the modifiers `held`.
///
/// Held modifiers that are not part of the chord are released, after a [`Key::Mask`] press when
/// `Alt` or `Win` is among them. Modifiers the user already holds and the chord needs are left
/// down. Modifiers pressed here are released again at the end.
pub fn paste_sequence(held: Modifiers, paste: PasteKey) -> Vec<KeyEvent> {
    let chord = paste.modifiers();
    let mut events = Vec::new();

    let pressed: Vec<Modifier> = chord.iter().copied().filter(|&m| !held.has(m)).collect();
    for &modifier in &pressed {
        events.push(event(Key::Modifier(modifier), true));
    }

    let stray: Vec<Modifier> = [
        Modifier::Alt,
        Modifier::Win,
        Modifier::Shift,
        Modifier::Ctrl,
    ]
    .into_iter()
    .filter(|&m| held.has(m) && !chord.contains(&m))
    .collect();
    if stray
        .iter()
        .any(|m| matches!(m, Modifier::Alt | Modifier::Win))
    {
        events.push(event(Key::Mask, true));
        events.push(event(Key::Mask, false));
    }
    for &modifier in &stray {
        events.push(event(Key::Modifier(modifier), false));
    }

    events.push(event(paste.main_key(), true));
    events.push(event(paste.main_key(), false));

    for &modifier in pressed.iter().rev() {
        events.push(event(Key::Modifier(modifier), false));
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOWN: bool = true;
    const UP: bool = false;

    fn m(modifier: Modifier, down: bool) -> KeyEvent {
        event(Key::Modifier(modifier), down)
    }

    #[test]
    fn plain_ctrl_v_with_nothing_held() {
        assert_eq!(
            paste_sequence(Modifiers::default(), PasteKey::CtrlV),
            [
                m(Modifier::Ctrl, DOWN),
                event(Key::V, DOWN),
                event(Key::V, UP),
                m(Modifier::Ctrl, UP),
            ]
        );
    }

    #[test]
    fn held_ctrl_is_reused_and_not_released() {
        let held = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        assert_eq!(
            paste_sequence(held, PasteKey::CtrlV),
            [event(Key::V, DOWN), event(Key::V, UP)]
        );
    }

    #[test]
    fn held_alt_is_masked_before_release() {
        let held = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        assert_eq!(
            paste_sequence(held, PasteKey::CtrlV),
            [
                m(Modifier::Ctrl, DOWN),
                event(Key::Mask, DOWN),
                event(Key::Mask, UP),
                m(Modifier::Alt, UP),
                event(Key::V, DOWN),
                event(Key::V, UP),
                m(Modifier::Ctrl, UP),
            ]
        );
    }

    #[test]
    fn held_win_is_masked_before_release() {
        let held = Modifiers {
            win: true,
            ..Modifiers::default()
        };
        let events = paste_sequence(held, PasteKey::CtrlV);
        let mask = events
            .iter()
            .position(|e| e.key == Key::Mask && e.down)
            .unwrap();
        let win_up = events
            .iter()
            .position(|e| *e == m(Modifier::Win, UP))
            .unwrap();
        assert!(mask < win_up, "mask must precede the Win release");
    }

    #[test]
    fn held_shift_alone_needs_no_mask() {
        let held = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        let events = paste_sequence(held, PasteKey::CtrlV);
        assert!(events.iter().all(|e| e.key != Key::Mask));
        assert!(events.contains(&m(Modifier::Shift, UP)));
    }

    #[test]
    fn ctrl_shift_v_keeps_held_shift() {
        let held = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert_eq!(
            paste_sequence(held, PasteKey::CtrlShiftV),
            [
                m(Modifier::Ctrl, DOWN),
                event(Key::V, DOWN),
                event(Key::V, UP),
                m(Modifier::Ctrl, UP),
            ]
        );
    }

    #[test]
    fn shift_insert_releases_a_held_ctrl() {
        let held = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        assert_eq!(
            paste_sequence(held, PasteKey::ShiftInsert),
            [
                m(Modifier::Shift, DOWN),
                m(Modifier::Ctrl, UP),
                event(Key::Insert, DOWN),
                event(Key::Insert, UP),
                m(Modifier::Shift, UP),
            ]
        );
    }

    #[test]
    fn every_modifier_pressed_is_released_and_nothing_extra_is_left_down() {
        for bits in 0..16u8 {
            let held = Modifiers {
                ctrl: bits & 1 != 0,
                shift: bits & 2 != 0,
                alt: bits & 4 != 0,
                win: bits & 8 != 0,
            };
            for paste in [PasteKey::CtrlV, PasteKey::CtrlShiftV, PasteKey::ShiftInsert] {
                let mut down = held;
                for e in paste_sequence(held, paste) {
                    if let Key::Modifier(modifier) = e.key {
                        let slot = match modifier {
                            Modifier::Ctrl => &mut down.ctrl,
                            Modifier::Shift => &mut down.shift,
                            Modifier::Alt => &mut down.alt,
                            Modifier::Win => &mut down.win,
                        };
                        *slot = e.down;
                    }
                }
                // At the end only modifiers the user kept holding that the chord uses may be down.
                for modifier in [
                    Modifier::Ctrl,
                    Modifier::Shift,
                    Modifier::Alt,
                    Modifier::Win,
                ] {
                    let expected = held.has(modifier) && paste.modifiers().contains(&modifier);
                    assert_eq!(
                        down.has(modifier),
                        expected,
                        "{held:?} {paste:?} {modifier:?}"
                    );
                }
            }
        }
    }

    fn rule(exe: &str, key: PasteKey) -> PasteRule {
        PasteRule {
            exe: exe.into(),
            key,
        }
    }

    #[test]
    fn a_rule_picks_the_paste_key_case_insensitively_and_the_rest_get_ctrl_v() {
        let rules = [
            rule("wt.exe", PasteKey::CtrlShiftV),
            rule("mintty.exe", PasteKey::ShiftInsert),
        ];
        assert_eq!(paste_key_for("WT.exe", &rules), PasteKey::CtrlShiftV);
        assert_eq!(paste_key_for("MinTTY.exe", &rules), PasteKey::ShiftInsert);
        assert_eq!(paste_key_for("notepad.exe", &rules), PasteKey::CtrlV);
        assert_eq!(paste_key_for("", &rules), PasteKey::CtrlV);
        // No built-in fallback: with the row gone the program pastes with Ctrl+V.
        assert_eq!(paste_key_for("mintty.exe", &[]), PasteKey::CtrlV);
    }

    #[test]
    fn the_built_in_rules_are_what_a_new_settings_file_starts_with() {
        let rules = builtin_paste_rules();
        assert_eq!(rules.len(), BUILTIN_PASTE_KEYS.len());
        assert_eq!(paste_key_for("MinTTY.exe", &rules), PasteKey::ShiftInsert);
        assert_eq!(paste_key_for("alacritty.exe", &rules), PasteKey::CtrlShiftV);
        assert_eq!(paste_key_for("notepad.exe", &rules), PasteKey::CtrlV);
    }
}
