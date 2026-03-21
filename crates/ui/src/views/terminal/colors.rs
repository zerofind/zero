use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor, Rgb as AlacRgb};
use gpui::{Hsla, Rgba};
use gpui_component::ActiveTheme;

// Catppuccin Mocha palette for ANSI colors without theme mappings.
const ANSI_BLACK: [u8; 3] = [0x1e, 0x1e, 0x2e];
const ANSI_MAGENTA: [u8; 3] = [0xf5, 0xc2, 0xe7];
const ANSI_CYAN: [u8; 3] = [0x94, 0xe2, 0xd5];
const DIM_BLACK: [u8; 3] = [0x45, 0x47, 0x5a];
const DIM_RED: [u8; 3] = [0xa0, 0x40, 0x40];
const DIM_GREEN: [u8; 3] = [0x60, 0x90, 0x60];
const DIM_YELLOW: [u8; 3] = [0xb0, 0x90, 0x40];
const DIM_BLUE: [u8; 3] = [0x50, 0x60, 0xa0];
const DIM_MAGENTA: [u8; 3] = [0x90, 0x60, 0x90];
const DIM_CYAN: [u8; 3] = [0x50, 0x90, 0x90];
const DIM_WHITE: [u8; 3] = [0xa0, 0xa0, 0xa0];

pub fn to_gpui_color(color: AnsiColor, cx: &gpui::App) -> Hsla {
    match color {
        AnsiColor::Named(named) => named_to_theme(named, cx),
        AnsiColor::Spec(rgb) => Rgba {
            r: rgb.r as f32 / 255.0,
            g: rgb.g as f32 / 255.0,
            b: rgb.b as f32 / 255.0,
            a: 1.0,
        }
        .into(),
        AnsiColor::Indexed(idx) => indexed_color(idx, cx),
    }
}

#[allow(dead_code)]
pub fn to_alac_rgb(color: impl Into<Rgba>) -> AlacRgb {
    let color = color.into();
    let r = ((color.r * color.a) * 255.) as u8;
    let g = ((color.g * color.a) * 255.) as u8;
    let b = ((color.b * color.a) * 255.) as u8;
    AlacRgb { r, g, b }
}

fn named_to_theme(named: NamedColor, cx: &gpui::App) -> Hsla {
    let theme = cx.theme();
    match named {
        NamedColor::Black => ansi(ANSI_BLACK),
        NamedColor::Red => theme.danger,
        NamedColor::Green => theme.success,
        NamedColor::Yellow => theme.warning,
        NamedColor::Blue => theme.link,
        NamedColor::Magenta => ansi(ANSI_MAGENTA),
        NamedColor::Cyan => ansi(ANSI_CYAN),
        NamedColor::White => theme.foreground,
        NamedColor::BrightBlack => theme.muted_foreground,
        NamedColor::BrightRed => theme.danger,
        NamedColor::BrightGreen => theme.success,
        NamedColor::BrightYellow => theme.warning,
        NamedColor::BrightBlue => theme.link,
        NamedColor::BrightMagenta => ansi(ANSI_MAGENTA),
        NamedColor::BrightCyan => ansi(ANSI_CYAN),
        NamedColor::BrightWhite => theme.foreground,
        NamedColor::Foreground => theme.foreground,
        NamedColor::Background => theme.background,
        NamedColor::Cursor => theme.foreground,
        NamedColor::DimForeground => theme.muted_foreground,
        NamedColor::BrightForeground => theme.foreground,
        NamedColor::DimBlack => ansi(DIM_BLACK),
        NamedColor::DimRed => ansi(DIM_RED),
        NamedColor::DimGreen => ansi(DIM_GREEN),
        NamedColor::DimYellow => ansi(DIM_YELLOW),
        NamedColor::DimBlue => ansi(DIM_BLUE),
        NamedColor::DimMagenta => ansi(DIM_MAGENTA),
        NamedColor::DimCyan => ansi(DIM_CYAN),
        NamedColor::DimWhite => ansi(DIM_WHITE),
    }
}

fn indexed_color(idx: u8, cx: &gpui::App) -> Hsla {
    // 0-15: standard named colors
    if idx < 16 {
        let named = match idx {
            0 => NamedColor::Black,
            1 => NamedColor::Red,
            2 => NamedColor::Green,
            3 => NamedColor::Yellow,
            4 => NamedColor::Blue,
            5 => NamedColor::Magenta,
            6 => NamedColor::Cyan,
            7 => NamedColor::White,
            8 => NamedColor::BrightBlack,
            9 => NamedColor::BrightRed,
            10 => NamedColor::BrightGreen,
            11 => NamedColor::BrightYellow,
            12 => NamedColor::BrightBlue,
            13 => NamedColor::BrightMagenta,
            14 => NamedColor::BrightCyan,
            15 => NamedColor::BrightWhite,
            _ => unreachable!(),
        };
        return named_to_theme(named, cx);
    }

    // 16-231: 6x6x6 color cube
    if idx < 232 {
        let idx = idx - 16;
        let r = (idx / 36) % 6;
        let g = (idx / 6) % 6;
        let b = idx % 6;
        let component = |c: u8| if c == 0 { 0 } else { 55 + c * 40 };
        return rgb(component(r), component(g), component(b));
    }

    // 232-255: grayscale ramp
    let level = 8 + (idx - 232) * 10;
    rgb(level, level, level)
}

fn ansi(c: [u8; 3]) -> Hsla {
    rgb(c[0], c[1], c[2])
}

fn rgb(r: u8, g: u8, b: u8) -> Hsla {
    Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
    .into()
}
