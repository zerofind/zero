use std::borrow::Cow;

use alacritty_terminal::term::TermMode;
use gpui::Keystroke;

#[derive(Debug, PartialEq, Eq)]
enum AlacModifiers {
    None,
    Alt,
    Ctrl,
    Shift,
    CtrlShift,
    Other,
}

impl AlacModifiers {
    fn new(ks: &Keystroke) -> Self {
        match (
            ks.modifiers.alt,
            ks.modifiers.control,
            ks.modifiers.shift,
            ks.modifiers.platform,
        ) {
            (false, false, false, false) => AlacModifiers::None,
            (true, false, false, false) => AlacModifiers::Alt,
            (false, true, false, false) => AlacModifiers::Ctrl,
            (false, false, true, false) => AlacModifiers::Shift,
            (false, true, true, false) => AlacModifiers::CtrlShift,
            _ => AlacModifiers::Other,
        }
    }

    fn any(&self) -> bool {
        !matches!(self, AlacModifiers::None)
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)] // API consistency with many callers
pub fn to_esc_str(
    keystroke: &Keystroke,
    mode: &TermMode,
    option_as_meta: bool,
) -> Option<Cow<'static, str>> {
    let modifiers = AlacModifiers::new(keystroke);

    let manual_esc_str: Option<&'static str> = match (keystroke.key.as_ref(), &modifiers) {
        ("tab", AlacModifiers::None) => Some("\x09"),
        ("escape", AlacModifiers::None) => Some("\x1b"),
        ("enter", AlacModifiers::None) => Some("\x0d"),
        ("enter", AlacModifiers::Shift) => Some("\x0a"),
        ("enter", AlacModifiers::Alt) => Some("\x1b\x0d"),
        ("backspace", AlacModifiers::None) => Some("\x7f"),
        ("tab", AlacModifiers::Shift) => Some("\x1b[Z"),
        ("backspace", AlacModifiers::Ctrl) => Some("\x08"),
        ("backspace", AlacModifiers::Alt) => Some("\x1b\x7f"),
        ("backspace", AlacModifiers::Shift) => Some("\x7f"),
        ("space", AlacModifiers::Ctrl) => Some("\x00"),
        ("home", AlacModifiers::Shift) if mode.contains(TermMode::ALT_SCREEN) => Some("\x1b[1;2H"),
        ("end", AlacModifiers::Shift) if mode.contains(TermMode::ALT_SCREEN) => Some("\x1b[1;2F"),
        ("pageup", AlacModifiers::Shift) if mode.contains(TermMode::ALT_SCREEN) => {
            Some("\x1b[5;2~")
        }
        ("pagedown", AlacModifiers::Shift) if mode.contains(TermMode::ALT_SCREEN) => {
            Some("\x1b[6;2~")
        }
        ("home", AlacModifiers::None) if mode.contains(TermMode::APP_CURSOR) => Some("\x1bOH"),
        ("home", AlacModifiers::None) if !mode.contains(TermMode::APP_CURSOR) => Some("\x1b[H"),
        ("end", AlacModifiers::None) if mode.contains(TermMode::APP_CURSOR) => Some("\x1bOF"),
        ("end", AlacModifiers::None) if !mode.contains(TermMode::APP_CURSOR) => Some("\x1b[F"),
        ("up", AlacModifiers::None) if mode.contains(TermMode::APP_CURSOR) => Some("\x1bOA"),
        ("up", AlacModifiers::None) if !mode.contains(TermMode::APP_CURSOR) => Some("\x1b[A"),
        ("down", AlacModifiers::None) if mode.contains(TermMode::APP_CURSOR) => Some("\x1bOB"),
        ("down", AlacModifiers::None) if !mode.contains(TermMode::APP_CURSOR) => Some("\x1b[B"),
        ("right", AlacModifiers::None) if mode.contains(TermMode::APP_CURSOR) => Some("\x1bOC"),
        ("right", AlacModifiers::None) if !mode.contains(TermMode::APP_CURSOR) => Some("\x1b[C"),
        ("left", AlacModifiers::None) if mode.contains(TermMode::APP_CURSOR) => Some("\x1bOD"),
        ("left", AlacModifiers::None) if !mode.contains(TermMode::APP_CURSOR) => Some("\x1b[D"),
        ("back", AlacModifiers::None) => Some("\x7f"),
        ("insert", AlacModifiers::None) => Some("\x1b[2~"),
        ("delete", AlacModifiers::None) => Some("\x1b[3~"),
        ("pageup", AlacModifiers::None) => Some("\x1b[5~"),
        ("pagedown", AlacModifiers::None) => Some("\x1b[6~"),
        ("f1", AlacModifiers::None) => Some("\x1bOP"),
        ("f2", AlacModifiers::None) => Some("\x1bOQ"),
        ("f3", AlacModifiers::None) => Some("\x1bOR"),
        ("f4", AlacModifiers::None) => Some("\x1bOS"),
        ("f5", AlacModifiers::None) => Some("\x1b[15~"),
        ("f6", AlacModifiers::None) => Some("\x1b[17~"),
        ("f7", AlacModifiers::None) => Some("\x1b[18~"),
        ("f8", AlacModifiers::None) => Some("\x1b[19~"),
        ("f9", AlacModifiers::None) => Some("\x1b[20~"),
        ("f10", AlacModifiers::None) => Some("\x1b[21~"),
        ("f11", AlacModifiers::None) => Some("\x1b[23~"),
        ("f12", AlacModifiers::None) => Some("\x1b[24~"),
        // Ctrl + letter caret notation
        ("a", AlacModifiers::Ctrl) => Some("\x01"),
        ("A", AlacModifiers::CtrlShift) => Some("\x01"),
        ("b", AlacModifiers::Ctrl) => Some("\x02"),
        ("B", AlacModifiers::CtrlShift) => Some("\x02"),
        ("c", AlacModifiers::Ctrl) => Some("\x03"),
        ("C", AlacModifiers::CtrlShift) => Some("\x03"),
        ("d", AlacModifiers::Ctrl) => Some("\x04"),
        ("D", AlacModifiers::CtrlShift) => Some("\x04"),
        ("e", AlacModifiers::Ctrl) => Some("\x05"),
        ("E", AlacModifiers::CtrlShift) => Some("\x05"),
        ("f", AlacModifiers::Ctrl) => Some("\x06"),
        ("F", AlacModifiers::CtrlShift) => Some("\x06"),
        ("g", AlacModifiers::Ctrl) => Some("\x07"),
        ("G", AlacModifiers::CtrlShift) => Some("\x07"),
        ("h", AlacModifiers::Ctrl) => Some("\x08"),
        ("H", AlacModifiers::CtrlShift) => Some("\x08"),
        ("i", AlacModifiers::Ctrl) => Some("\x09"),
        ("I", AlacModifiers::CtrlShift) => Some("\x09"),
        ("j", AlacModifiers::Ctrl) => Some("\x0a"),
        ("J", AlacModifiers::CtrlShift) => Some("\x0a"),
        ("k", AlacModifiers::Ctrl) => Some("\x0b"),
        ("K", AlacModifiers::CtrlShift) => Some("\x0b"),
        ("l", AlacModifiers::Ctrl) => Some("\x0c"),
        ("L", AlacModifiers::CtrlShift) => Some("\x0c"),
        ("m", AlacModifiers::Ctrl) => Some("\x0d"),
        ("M", AlacModifiers::CtrlShift) => Some("\x0d"),
        ("n", AlacModifiers::Ctrl) => Some("\x0e"),
        ("N", AlacModifiers::CtrlShift) => Some("\x0e"),
        ("o", AlacModifiers::Ctrl) => Some("\x0f"),
        ("O", AlacModifiers::CtrlShift) => Some("\x0f"),
        ("p", AlacModifiers::Ctrl) => Some("\x10"),
        ("P", AlacModifiers::CtrlShift) => Some("\x10"),
        ("q", AlacModifiers::Ctrl) => Some("\x11"),
        ("Q", AlacModifiers::CtrlShift) => Some("\x11"),
        ("r", AlacModifiers::Ctrl) => Some("\x12"),
        ("R", AlacModifiers::CtrlShift) => Some("\x12"),
        ("s", AlacModifiers::Ctrl) => Some("\x13"),
        ("S", AlacModifiers::CtrlShift) => Some("\x13"),
        ("t", AlacModifiers::Ctrl) => Some("\x14"),
        ("T", AlacModifiers::CtrlShift) => Some("\x14"),
        ("u", AlacModifiers::Ctrl) => Some("\x15"),
        ("U", AlacModifiers::CtrlShift) => Some("\x15"),
        ("v", AlacModifiers::Ctrl) => Some("\x16"),
        ("V", AlacModifiers::CtrlShift) => Some("\x16"),
        ("w", AlacModifiers::Ctrl) => Some("\x17"),
        ("W", AlacModifiers::CtrlShift) => Some("\x17"),
        ("x", AlacModifiers::Ctrl) => Some("\x18"),
        ("X", AlacModifiers::CtrlShift) => Some("\x18"),
        ("y", AlacModifiers::Ctrl) => Some("\x19"),
        ("Y", AlacModifiers::CtrlShift) => Some("\x19"),
        ("z", AlacModifiers::Ctrl) => Some("\x1a"),
        ("Z", AlacModifiers::CtrlShift) => Some("\x1a"),
        ("@", AlacModifiers::Ctrl) => Some("\x00"),
        ("[", AlacModifiers::Ctrl) => Some("\x1b"),
        ("\\", AlacModifiers::Ctrl) => Some("\x1c"),
        ("]", AlacModifiers::Ctrl) => Some("\x1d"),
        ("^", AlacModifiers::Ctrl) => Some("\x1e"),
        ("_", AlacModifiers::Ctrl) => Some("\x1f"),
        ("?", AlacModifiers::Ctrl) => Some("\x7f"),
        _ => None,
    };
    if let Some(esc_str) = manual_esc_str {
        return Some(Cow::Borrowed(esc_str));
    }

    // Modified special keys
    if modifiers.any() {
        let modifier_code = modifier_code(keystroke);
        let modified_esc_str = match keystroke.key.as_ref() {
            "up" => Some(format!("\x1b[1;{modifier_code}A")),
            "down" => Some(format!("\x1b[1;{modifier_code}B")),
            "right" => Some(format!("\x1b[1;{modifier_code}C")),
            "left" => Some(format!("\x1b[1;{modifier_code}D")),
            "f1" => Some(format!("\x1b[1;{modifier_code}P")),
            "f2" => Some(format!("\x1b[1;{modifier_code}Q")),
            "f3" => Some(format!("\x1b[1;{modifier_code}R")),
            "f4" => Some(format!("\x1b[1;{modifier_code}S")),
            "f5" => Some(format!("\x1b[15;{modifier_code}~")),
            "f6" => Some(format!("\x1b[17;{modifier_code}~")),
            "f7" => Some(format!("\x1b[18;{modifier_code}~")),
            "f8" => Some(format!("\x1b[19;{modifier_code}~")),
            "f9" => Some(format!("\x1b[20;{modifier_code}~")),
            "f10" => Some(format!("\x1b[21;{modifier_code}~")),
            "f11" => Some(format!("\x1b[23;{modifier_code}~")),
            "f12" => Some(format!("\x1b[24;{modifier_code}~")),
            _ if modifier_code == 2 => None,
            "insert" => Some(format!("\x1b[2;{modifier_code}~")),
            "pageup" => Some(format!("\x1b[5;{modifier_code}~")),
            "pagedown" => Some(format!("\x1b[6;{modifier_code}~")),
            "end" => Some(format!("\x1b[1;{modifier_code}F")),
            "home" => Some(format!("\x1b[1;{modifier_code}H")),
            _ => None,
        };
        if let Some(esc_str) = modified_esc_str {
            return Some(Cow::Owned(esc_str));
        }
    }

    // Alt-as-meta: ESC prefix for ascii characters
    if !cfg!(target_os = "macos") || option_as_meta {
        let is_alt_lowercase_ascii = modifiers == AlacModifiers::Alt && keystroke.key.is_ascii();
        let is_alt_uppercase_ascii =
            keystroke.modifiers.alt && keystroke.modifiers.shift && keystroke.key.is_ascii();
        if is_alt_lowercase_ascii || is_alt_uppercase_ascii {
            let key = if is_alt_uppercase_ascii {
                &keystroke.key.to_ascii_uppercase()
            } else {
                &keystroke.key
            };
            return Some(Cow::Owned(format!("\x1b{key}")));
        }
    }

    None
}

fn modifier_code(keystroke: &Keystroke) -> u32 {
    let mut code = 0;
    if keystroke.modifiers.shift {
        code |= 1;
    }
    if keystroke.modifiers.alt {
        code |= 1 << 1;
    }
    if keystroke.modifiers.control {
        code |= 1 << 2;
    }
    code + 1
}
