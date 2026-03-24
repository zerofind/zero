use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::CursorShape as AlacCursorShape;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};
use gpui::*;
use gpui_component::ActiveTheme;

use super::colors::to_gpui_color;
use super::{TerminalBounds, TerminalContent};

// -- Cell styling ------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
struct CellStyle {
    fg: Hsla,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    dim: bool,
}

impl CellStyle {
    fn from_cell(fg: Hsla, flags: Flags) -> Self {
        Self {
            fg,
            bold: flags.contains(Flags::BOLD),
            italic: flags.contains(Flags::ITALIC),
            underline: flags.contains(Flags::UNDERLINE),
            strikethrough: flags.contains(Flags::STRIKEOUT),
            dim: flags.contains(Flags::DIM),
        }
    }
}

// -- Layout data stored between prepaint and paint ---------------------------

pub struct GridLayout {
    pub runs: Vec<BatchedRun>,
    pub bg_rects: Vec<BgRect>,
    pub cursor_rect: Option<(Point<Pixels>, Size<Pixels>, Hsla)>,
    pub cursor_shape: AlacCursorShape,
    pub cursor_char: Option<(Point<Pixels>, SharedString, TextRun, Pixels)>,
    pub selection_rects: Vec<(Point<Pixels>, Size<Pixels>)>,
}

pub struct BatchedRun {
    pub pos: Point<Pixels>,
    pub text: SharedString,
    pub style: TextRun,
    pub font_size: Pixels,
    pub cell_width: Pixels,
}

pub struct BgRect {
    pub pos: Point<Pixels>,
    pub size: Size<Pixels>,
    pub color: Hsla,
}

pub struct GridParams {
    pub origin: Point<Pixels>,
    pub font: Font,
    pub font_size: Pixels,
    pub fg_default: Hsla,
    pub bg_default: Hsla,
}

// -- Helpers -----------------------------------------------------------------

fn is_blank(c: char, bg: AnsiColor, flags: Flags) -> bool {
    c == ' '
        && matches!(bg, AnsiColor::Named(NamedColor::Background))
        && !flags.intersects(Flags::INVERSE | Flags::UNDERLINE | Flags::STRIKEOUT)
}

// -- Grid layout -------------------------------------------------------------

pub fn layout_grid(
    content: &TerminalContent,
    dimensions: &TerminalBounds,
    params: &GridParams,
    cx: &App,
) -> GridLayout {
    let cell_width = dimensions.cell_width;
    let line_height = dimensions.line_height;
    let display_offset = content.display_offset;
    let origin = params.origin;
    let font = &params.font;
    let font_size = params.font_size;
    let fg_default = params.fg_default;
    let bg_default = params.bg_default;

    let mut runs: Vec<BatchedRun> = Vec::new();
    let mut bg_rects: Vec<BgRect> = Vec::new();
    let mut selection_rects = Vec::new();

    let mut current_line: i32 = i32::MIN;
    let mut line_text = String::new();
    let mut line_start_col: usize = 0;
    let mut line_style = CellStyle {
        fg: fg_default,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        dim: false,
    };
    let mut line_bg_start: Option<(usize, Hsla)> = None;

    let flush_run =
        |runs: &mut Vec<BatchedRun>, text: &str, start_col: usize, line: i32, style: CellStyle| {
            if text.is_empty() {
                return;
            }
            let pos = Point::new(
                origin.x + start_col as f32 * cell_width,
                origin.y + line as f32 * line_height,
            );
            let mut run_font = font.clone();
            if style.bold {
                run_font.weight = FontWeight::BOLD;
            }
            if style.italic {
                run_font.style = FontStyle::Italic;
            }
            let color = if style.dim {
                Hsla {
                    a: style.fg.a * 0.7,
                    ..style.fg
                }
            } else {
                style.fg
            };
            runs.push(BatchedRun {
                pos,
                text: SharedString::from(text.to_string()),
                style: TextRun {
                    len: text.len(),
                    font: run_font,
                    color,
                    background_color: None,
                    underline: if style.underline {
                        Some(UnderlineStyle {
                            thickness: px(1.0),
                            color: Some(color),
                            wavy: false,
                        })
                    } else {
                        None
                    },
                    strikethrough: if style.strikethrough {
                        Some(StrikethroughStyle {
                            thickness: px(1.0),
                            color: Some(color),
                        })
                    } else {
                        None
                    },
                },
                font_size,
                cell_width,
            });
        };

    let flush_bg =
        |bg_rects: &mut Vec<BgRect>, bg_start: Option<(usize, Hsla)>, end_col: usize, line: i32| {
            if let Some((start, color)) = bg_start.filter(|(_, c)| *c != bg_default)
                && end_col > start
            {
                bg_rects.push(BgRect {
                    pos: Point::new(
                        origin.x + start as f32 * cell_width,
                        origin.y + line as f32 * line_height,
                    ),
                    size: Size {
                        width: (end_col - start) as f32 * cell_width,
                        height: line_height,
                    },
                    color,
                });
            }
        };

    for indexed in &content.cells {
        let display_line = indexed.point.line.0 + display_offset as i32;
        let col = indexed.point.column.0;

        if indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }

        let c = indexed.cell.c;

        // Skip blank cells (space + default bg + no styling flags)
        if is_blank(c, indexed.cell.bg, indexed.cell.flags) {
            continue;
        }

        let is_inverse = indexed.cell.flags.contains(Flags::INVERSE);

        let raw_fg = to_gpui_color(indexed.cell.fg, cx);
        let raw_bg = to_gpui_color(indexed.cell.bg, cx);

        let (fg, bg) = if is_inverse {
            (raw_bg, raw_fg)
        } else {
            (raw_fg, raw_bg)
        };

        let cell_style = CellStyle::from_cell(fg, indexed.cell.flags);

        let in_selection = content
            .selection
            .is_some_and(|sel| sel.contains(indexed.point));

        if display_line != current_line {
            if current_line != i32::MIN {
                flush_run(
                    &mut runs,
                    &line_text,
                    line_start_col,
                    current_line,
                    line_style,
                );
                let end_col = line_start_col + line_text.chars().count();
                flush_bg(&mut bg_rects, line_bg_start, end_col, current_line);
            }
            current_line = display_line;
            line_text.clear();
            line_start_col = col;
            line_style = cell_style;
            line_bg_start = Some((col, bg));
        }

        while line_start_col + line_text.chars().count() < col {
            line_text.push(' ');
        }

        if cell_style != line_style {
            flush_run(
                &mut runs,
                &line_text,
                line_start_col,
                current_line,
                line_style,
            );
            line_start_col = col;
            line_text.clear();
            line_style = cell_style;
        }

        if let Some((_, prev_bg)) = line_bg_start {
            if bg != prev_bg {
                flush_bg(&mut bg_rects, line_bg_start, col, current_line);
                line_bg_start = Some((col, bg));
            }
        } else {
            line_bg_start = Some((col, bg));
        }

        if in_selection {
            let w = if indexed.cell.flags.contains(Flags::WIDE_CHAR) {
                2.0
            } else {
                1.0
            };
            selection_rects.push((
                Point::new(
                    origin.x + col as f32 * cell_width,
                    origin.y + display_line as f32 * line_height,
                ),
                Size {
                    width: cell_width * w,
                    height: line_height,
                },
            ));
        }

        line_text.push(c);

        // Append zero-width characters (combining marks, variation selectors)
        if let Some(zerowidth) = indexed.cell.zerowidth() {
            for &zw in zerowidth {
                line_text.push(zw);
            }
        }
    }

    // Flush last line
    if current_line != i32::MIN {
        flush_run(
            &mut runs,
            &line_text,
            line_start_col,
            current_line,
            line_style,
        );
        let end_col = line_start_col + line_text.chars().count();
        flush_bg(&mut bg_rects, line_bg_start, end_col, current_line);
    }

    // Cursor
    let cursor_display_line = content.cursor.point.line.0 + display_offset as i32;
    let cursor_col = content.cursor.point.column.0;
    let cursor_pos = Point::new(
        origin.x + cursor_col as f32 * cell_width,
        origin.y + cursor_display_line as f32 * line_height,
    );
    let cursor_size = Size {
        width: cell_width,
        height: line_height,
    };
    let cursor_color = cx.theme().foreground;
    let cursor_rect = Some((cursor_pos, cursor_size, cursor_color));
    let cursor_shape = content.cursor.shape;

    // Cursor character with inverted colors for block cursor
    let cursor_char_info = if content.cursor_char == ' ' {
        None
    } else {
        let inverted_fg = bg_default;
        let cursor_font = font.clone();
        let ch = content.cursor_char.to_string();
        let run = TextRun {
            len: ch.len(),
            font: cursor_font,
            color: inverted_fg,
            background_color: None,
            underline: Option::default(),
            strikethrough: Option::default(),
        };
        Some((cursor_pos, SharedString::from(ch), run, font_size))
    };

    GridLayout {
        runs,
        bg_rects,
        cursor_rect,
        cursor_shape,
        cursor_char: cursor_char_info,
        selection_rects,
    }
}
