use super::keys::to_esc_str;
use alacritty_terminal::term::TermMode;
use gpui::{Keystroke, Modifiers};

fn ks(key: &str, ctrl: bool, alt: bool, shift: bool) -> Keystroke {
    Keystroke {
        key: key.into(),
        key_char: None,
        modifiers: Modifiers {
            control: ctrl,
            alt,
            shift,
            platform: false,
            function: false,
        },
    }
}

#[test]
fn basic_keys() {
    let mode = TermMode::empty();
    assert_eq!(
        to_esc_str(&ks("tab", false, false, false), &mode, false).as_deref(),
        Some("\x09")
    );
    assert_eq!(
        to_esc_str(&ks("enter", false, false, false), &mode, false).as_deref(),
        Some("\x0d")
    );
    assert_eq!(
        to_esc_str(&ks("escape", false, false, false), &mode, false).as_deref(),
        Some("\x1b")
    );
    assert_eq!(
        to_esc_str(&ks("backspace", false, false, false), &mode, false).as_deref(),
        Some("\x7f")
    );
}

#[test]
fn ctrl_keys() {
    let mode = TermMode::empty();
    assert_eq!(
        to_esc_str(&ks("c", true, false, false), &mode, false).as_deref(),
        Some("\x03")
    );
    assert_eq!(
        to_esc_str(&ks("d", true, false, false), &mode, false).as_deref(),
        Some("\x04")
    );
    assert_eq!(
        to_esc_str(&ks("a", true, false, false), &mode, false).as_deref(),
        Some("\x01")
    );
    assert_eq!(
        to_esc_str(&ks("z", true, false, false), &mode, false).as_deref(),
        Some("\x1a")
    );
    assert_eq!(
        to_esc_str(&ks("l", true, false, false), &mode, false).as_deref(),
        Some("\x0c")
    );
}

#[test]
fn option_as_meta_sends_esc_prefix() {
    let mode = TermMode::empty();
    // Alt+B → ESC b (word-back in shell)
    assert_eq!(
        to_esc_str(&ks("b", false, true, false), &mode, true).as_deref(),
        Some("\x1bb")
    );
    // Alt+F → ESC f (word-forward)
    assert_eq!(
        to_esc_str(&ks("f", false, true, false), &mode, true).as_deref(),
        Some("\x1bf")
    );
    // Alt+D → ESC d (delete-word)
    assert_eq!(
        to_esc_str(&ks("d", false, true, false), &mode, true).as_deref(),
        Some("\x1bd")
    );
}

#[test]
#[cfg(target_os = "macos")]
fn option_as_meta_disabled_returns_none() {
    let mode = TermMode::empty();
    // On macOS with option_as_meta=false, alt+letter is not handled as meta
    assert_eq!(to_esc_str(&ks("b", false, true, false), &mode, false), None);
}

#[test]
fn arrow_keys_normal_mode() {
    let mode = TermMode::empty();
    assert_eq!(
        to_esc_str(&ks("up", false, false, false), &mode, false).as_deref(),
        Some("\x1b[A")
    );
    assert_eq!(
        to_esc_str(&ks("down", false, false, false), &mode, false).as_deref(),
        Some("\x1b[B")
    );
    assert_eq!(
        to_esc_str(&ks("right", false, false, false), &mode, false).as_deref(),
        Some("\x1b[C")
    );
    assert_eq!(
        to_esc_str(&ks("left", false, false, false), &mode, false).as_deref(),
        Some("\x1b[D")
    );
}

#[test]
fn arrow_keys_app_cursor_mode() {
    let mode = TermMode::APP_CURSOR;
    assert_eq!(
        to_esc_str(&ks("up", false, false, false), &mode, false).as_deref(),
        Some("\x1bOA")
    );
    assert_eq!(
        to_esc_str(&ks("down", false, false, false), &mode, false).as_deref(),
        Some("\x1bOB")
    );
    assert_eq!(
        to_esc_str(&ks("right", false, false, false), &mode, false).as_deref(),
        Some("\x1bOC")
    );
    assert_eq!(
        to_esc_str(&ks("left", false, false, false), &mode, false).as_deref(),
        Some("\x1bOD")
    );
}

#[test]
fn function_keys() {
    let mode = TermMode::empty();
    assert_eq!(
        to_esc_str(&ks("f1", false, false, false), &mode, false).as_deref(),
        Some("\x1bOP")
    );
    assert_eq!(
        to_esc_str(&ks("f2", false, false, false), &mode, false).as_deref(),
        Some("\x1bOQ")
    );
    assert_eq!(
        to_esc_str(&ks("f5", false, false, false), &mode, false).as_deref(),
        Some("\x1b[15~")
    );
    assert_eq!(
        to_esc_str(&ks("f12", false, false, false), &mode, false).as_deref(),
        Some("\x1b[24~")
    );
}

#[test]
fn modified_arrow_keys() {
    let mode = TermMode::empty();
    // Shift+Up = modifier code 2
    assert_eq!(
        to_esc_str(&ks("up", false, false, true), &mode, false).as_deref(),
        Some("\x1b[1;2A"),
    );
    // Ctrl+Right = modifier code 5
    assert_eq!(
        to_esc_str(&ks("right", true, false, false), &mode, false).as_deref(),
        Some("\x1b[1;5C"),
    );
}

#[test]
fn special_combos() {
    let mode = TermMode::empty();
    assert_eq!(
        to_esc_str(&ks("enter", false, false, true), &mode, false).as_deref(),
        Some("\x0a")
    );
    assert_eq!(
        to_esc_str(&ks("enter", false, true, false), &mode, false).as_deref(),
        Some("\x1b\x0d")
    );
    assert_eq!(
        to_esc_str(&ks("backspace", true, false, false), &mode, false).as_deref(),
        Some("\x08")
    );
    assert_eq!(
        to_esc_str(&ks("backspace", false, true, false), &mode, false).as_deref(),
        Some("\x1b\x7f")
    );
    assert_eq!(
        to_esc_str(&ks("space", true, false, false), &mode, false).as_deref(),
        Some("\x00")
    );
}
