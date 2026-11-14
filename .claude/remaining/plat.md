# plat (platform crate, v1.1 slices 5b + 7)
Done: typing.rs (classify, TypeBuffer, suffix_match), keys::backspaces + Key::Backspace, windows/typing_hook.rs (TypingHook::start/stop), TargetTracker::focus_is_password, WinClipboard::read_rich + clipboard_watch::RichClip/rich_flavours, FakeClipboard held_html/held_rtf.
Next (app agent): wire hook, Target.password field, ClipboardText::read_rich, trigger_ctl.
Blocked: nothing. Windows behaviour unverified until the owner runs it.
