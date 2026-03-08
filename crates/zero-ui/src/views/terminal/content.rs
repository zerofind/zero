use std::collections::VecDeque;

use alacritty_terminal::event::{Event as AlacTermEvent, EventListener};
use alacritty_terminal::index::{Column, Line, Point as AlacPoint};
use alacritty_terminal::selection::{Selection, SelectionRange};
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::{RenderableCursor, TermMode};
use alacritty_terminal::vte::ansi::CursorShape as AlacCursorShape;
use futures::channel::mpsc::UnboundedSender;

use super::TerminalBounds;

// -- Events ------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum TerminalEvent {
    Wakeup,
    Bell,
    Close,
    TitleChanged(String),
}

// -- IndexedCell -------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct IndexedCell {
    pub point: AlacPoint,
    pub cell: Cell,
}

impl std::ops::Deref for IndexedCell {
    type Target = Cell;
    fn deref(&self) -> &Cell {
        &self.cell
    }
}

// -- TerminalContent ---------------------------------------------------------

#[derive(Clone)]
pub struct TerminalContent {
    pub cells: Vec<IndexedCell>,
    pub mode: TermMode,
    pub display_offset: usize,
    pub selection_text: Option<String>,
    pub selection: Option<SelectionRange>,
    pub cursor: RenderableCursor,
    pub cursor_char: char,
    pub terminal_bounds: TerminalBounds,
}

impl Default for TerminalContent {
    fn default() -> Self {
        Self {
            cells: Vec::new(),
            mode: TermMode::empty(),
            display_offset: 0,
            selection_text: None,
            selection: None,
            cursor: RenderableCursor {
                shape: AlacCursorShape::Block,
                point: AlacPoint::new(Line(0), Column(0)),
            },
            cursor_char: ' ',
            terminal_bounds: TerminalBounds::default(),
        }
    }
}

// -- Internal events ---------------------------------------------------------

#[derive(Clone)]
pub enum InternalEvent {
    Resize(TerminalBounds),
    Scroll(alacritty_terminal::grid::Scroll),
    SetSelection(Option<(Selection, AlacPoint)>),
    Clear,
}

// -- ZeroListener (bridges alacritty -> gpui) --------------------------------

#[derive(Clone)]
pub struct ZeroListener(pub UnboundedSender<AlacTermEvent>);

impl EventListener for ZeroListener {
    fn send_event(&self, event: AlacTermEvent) {
        self.0.unbounded_send(event).ok();
    }
}

// -- Events queue helper -----------------------------------------------------

pub type EventQueue = VecDeque<InternalEvent>;
