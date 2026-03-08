mod bounds;
mod colors;
mod content;
mod element;
mod grid;
mod keys;
mod mouse;
mod spawn;
mod view;

#[cfg(test)]
mod keys_test;

pub use bounds::TerminalBounds;
pub use content::{IndexedCell, TerminalContent, TerminalEvent};
pub use view::{TerminalView, TerminalViewEvent};

use content::{EventQueue, InternalEvent, ZeroListener};

use std::borrow::Cow;
use std::cmp;
use std::sync::Arc;

use alacritty_terminal::Term;
use alacritty_terminal::event_loop::{Msg, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point as AlacPoint};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::vte::ansi::Handler;

use gpui::*;

use keys::to_esc_str;
use mouse::{alt_scroll, grid_point, grid_point_and_side, mouse_button_report, mouse_moved_report};

const SCROLL_HISTORY_LINES: usize = 10_000;

// -- Terminal ----------------------------------------------------------------

#[allow(dead_code)]
pub struct Terminal {
    term: Arc<FairMutex<Term<ZeroListener>>>,
    pty_tx: Notifier,
    events: EventQueue,
    pub last_content: TerminalContent,
    content_dirty: bool,
    event_loop_task: Task<anyhow::Result<()>>,
    scroll_px: Pixels,
}

impl Terminal {
    fn write_to_pty(&self, input: impl Into<Cow<'static, [u8]>>) {
        self.pty_tx.0.send(Msg::Input(input.into())).ok();
    }

    pub fn set_size(&mut self, new_bounds: TerminalBounds) {
        let mut nb = new_bounds;
        nb.bounds.size.height = cmp::max(nb.line_height, nb.bounds.size.height);
        nb.bounds.size.width = cmp::max(nb.cell_width, nb.bounds.size.width);

        if self.last_content.terminal_bounds != nb {
            self.events.push_back(InternalEvent::Resize(nb));
            self.content_dirty = true;
        }
    }

    pub fn sync(&mut self, _cx: &mut Context<Self>) {
        if !self.content_dirty && self.events.is_empty() {
            return;
        }

        let term = self.term.clone();
        let mut terminal = term.lock_unfair();

        while let Some(e) = self.events.pop_front() {
            match e {
                InternalEvent::Resize(nb) => {
                    self.last_content.terminal_bounds = nb;
                    self.pty_tx.0.send(Msg::Resize(nb.into())).ok();
                    terminal.resize(nb);
                }
                InternalEvent::Scroll(scroll) => {
                    terminal.scroll_display(scroll);
                }
                InternalEvent::SetSelection(sel) => {
                    if let Some((selection, _)) = sel {
                        terminal.selection = Some(selection);
                    } else {
                        terminal.selection = None;
                    }
                }
                InternalEvent::Clear => {
                    terminal.clear_screen(alacritty_terminal::vte::ansi::ClearMode::Saved);
                    let cursor = terminal.grid().cursor.point;
                    terminal.grid_mut().reset_region(..cursor.line);
                    let line = terminal.grid()[cursor.line][..Column(terminal.grid().columns())]
                        .iter()
                        .cloned()
                        .enumerate()
                        .collect::<Vec<(usize, Cell)>>();
                    for (i, cell) in line {
                        terminal.grid_mut()[Line(0)][Column(i)] = cell;
                    }
                    terminal.grid_mut().cursor.point =
                        AlacPoint::new(Line(0), terminal.grid_mut().cursor.point.column);
                    let new_cursor = terminal.grid().cursor.point;
                    if (new_cursor.line.0 as usize) < terminal.screen_lines() - 1 {
                        terminal.grid_mut().reset_region((new_cursor.line + 1)..);
                    }
                }
            }
        }

        // Update cached content
        let content = terminal.renderable_content();
        let estimated_size = content.display_iter.size_hint().0;
        let mut cells = Vec::with_capacity(estimated_size);
        cells.extend(content.display_iter.map(|ic| IndexedCell {
            point: ic.point,
            cell: ic.cell.clone(),
        }));

        let selection_text = if content.selection.is_some() {
            terminal.selection_to_string()
        } else {
            None
        };

        let cursor_char = terminal.grid()[content.cursor.point].c;

        self.last_content = TerminalContent {
            cells,
            mode: content.mode,
            display_offset: content.display_offset,
            selection_text,
            selection: content.selection,
            cursor: content.cursor,
            cursor_char,
            terminal_bounds: self.last_content.terminal_bounds,
        };
        self.content_dirty = false;
    }

    pub fn input(&mut self, input: impl Into<Cow<'static, [u8]>>) {
        self.events.push_back(InternalEvent::Scroll(
            alacritty_terminal::grid::Scroll::Bottom,
        ));
        self.events.push_back(InternalEvent::SetSelection(None));
        self.content_dirty = true;
        self.write_to_pty(input);
    }

    pub fn paste(&mut self, text: &str) {
        let paste_text = if self.last_content.mode.contains(TermMode::BRACKETED_PASTE) {
            format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
        } else {
            text.replace("\r\n", "\r").replace('\n', "\r")
        };
        self.input(paste_text.into_bytes());
    }

    #[allow(dead_code)]
    pub fn copy(&self) -> Option<String> {
        self.last_content.selection_text.clone()
    }

    pub fn try_keystroke(&mut self, keystroke: &Keystroke, option_as_meta: bool) -> bool {
        let esc = to_esc_str(keystroke, &self.last_content.mode, option_as_meta);
        if let Some(esc) = esc {
            match esc {
                Cow::Borrowed(s) => self.input(s.as_bytes()),
                Cow::Owned(s) => self.input(s.into_bytes()),
            }
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.events.push_back(InternalEvent::Clear);
    }

    #[allow(dead_code)]
    pub fn scroll(&mut self, scroll: alacritty_terminal::grid::Scroll) {
        self.events.push_back(InternalEvent::Scroll(scroll));
    }

    pub fn scroll_wheel(&mut self, delta: Pixels) {
        let line_height = self.last_content.terminal_bounds.line_height;
        if line_height <= px(0.0) {
            return;
        }

        self.scroll_px += delta;
        let lines = (f32::from(self.scroll_px) / f32::from(line_height)) as i32;
        if lines != 0 {
            self.scroll_px -= px(lines as f32 * f32::from(line_height));

            self.content_dirty = true;
            if self.last_content.mode.contains(TermMode::ALT_SCREEN) {
                self.write_to_pty(alt_scroll(lines));
            } else {
                self.events.push_back(InternalEvent::Scroll(
                    alacritty_terminal::grid::Scroll::Delta(lines),
                ));
            }
        }
    }

    pub fn mouse_down(
        &mut self,
        button: MouseButton,
        position: Point<Pixels>,
        modifiers: Modifiers,
        click_count: usize,
    ) {
        let bounds = self.last_content.terminal_bounds;
        let display_offset = self.last_content.display_offset;
        let mode = self.last_content.mode;
        let point = grid_point(position, bounds, display_offset);

        if mode.intersects(TermMode::MOUSE_MODE) && !modifiers.shift {
            if let Some(report) = mouse_button_report(point, button, modifiers, true, mode) {
                self.write_to_pty(report);
            }
            return;
        }

        let (alac_point, side) = grid_point_and_side(position, bounds, display_offset);
        let selection_type = match click_count {
            2 => SelectionType::Semantic,
            3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        let selection = Selection::new(selection_type, alac_point, side);
        self.events
            .push_back(InternalEvent::SetSelection(Some((selection, alac_point))));
        self.content_dirty = true;
    }

    pub fn mouse_up(&mut self, button: MouseButton, position: Point<Pixels>, modifiers: Modifiers) {
        let bounds = self.last_content.terminal_bounds;
        let display_offset = self.last_content.display_offset;
        let mode = self.last_content.mode;
        let point = grid_point(position, bounds, display_offset);

        if mode.intersects(TermMode::MOUSE_MODE)
            && let Some(report) = mouse_button_report(point, button, modifiers, false, mode)
        {
            self.write_to_pty(report);
        }
    }

    pub fn mouse_move(
        &mut self,
        position: Point<Pixels>,
        pressed_button: Option<MouseButton>,
        modifiers: Modifiers,
    ) {
        let bounds = self.last_content.terminal_bounds;
        let display_offset = self.last_content.display_offset;
        let mode = self.last_content.mode;
        let point = grid_point(position, bounds, display_offset);

        if mode.intersects(TermMode::MOUSE_MODE) {
            if let Some(report) = mouse_moved_report(point, pressed_button, modifiers, mode) {
                self.write_to_pty(report);
            }
            return;
        }

        if pressed_button.is_some() {
            let (alac_point, side) = grid_point_and_side(position, bounds, display_offset);
            let term = self.term.lock_unfair();
            if let Some(selection) = term.selection.as_ref() {
                let mut new_selection = selection.clone();
                new_selection.update(alac_point, side);
                drop(term);
                self.events.push_back(InternalEvent::SetSelection(Some((
                    new_selection,
                    alac_point,
                ))));
                self.content_dirty = true;
            }
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.pty_tx.0.send(Msg::Shutdown).ok();
    }
}

impl EventEmitter<TerminalEvent> for Terminal {}
