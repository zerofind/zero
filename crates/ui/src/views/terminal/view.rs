use std::path::PathBuf;

use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
};

use crate::actions::ClearTerminal;
use crate::theme::{FONT_SIZE_CAPTION, SPACE_XS};

use super::element::TerminalElement;
use super::{Terminal, TerminalEvent};

#[derive(Clone, Debug)]
pub enum TerminalViewEvent {
    Close,
}

pub struct TerminalView {
    pub terminal: Entity<Terminal>,
    pub focus_handle: FocusHandle,
    title: String,
    _subs: Vec<Subscription>,
}

impl TerminalView {
    pub fn new(cwd: PathBuf, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let terminal = cx.new(|cx| Terminal::spawn(cwd, cx));

        let focus_handle = cx.focus_handle();

        let wakeup_sub = cx.subscribe(&terminal, |this, _terminal, event, cx| {
            match event {
                TerminalEvent::Wakeup => cx.notify(),
                TerminalEvent::Close => cx.emit(TerminalViewEvent::Close),
                TerminalEvent::Bell => {
                    // Could play a sound or flash here
                }
                TerminalEvent::TitleChanged(title) => {
                    this.title.clone_from(title);
                    cx.notify();
                }
            }
        });

        Self {
            terminal,
            focus_handle,
            title: "Terminal".to_string(),
            _subs: vec![wakeup_sub],
        }
    }
}

impl EventEmitter<TerminalViewEvent> for TerminalView {}

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let terminal = self.terminal.clone();
        let focus = self.focus_handle.clone();
        let muted = cx.theme().muted_foreground;
        let title = self.title.clone();

        div()
            .id("terminal-view")
            .track_focus(&self.focus_handle)
            .key_context("Terminal")
            .size_full()
            .bg(cx.theme().background)
            .on_action(cx.listener(|this, _: &ClearTerminal, _, cx| {
                this.terminal.update(cx, |term, _cx| {
                    term.clear();
                });
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                let keystroke = &event.keystroke;

                // Let Cmd+C copy when there's a selection
                if keystroke.modifiers.platform && keystroke.key == "c" {
                    let selection = this.terminal.read(cx).last_content.selection_text.clone();
                    if let Some(text) = selection {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        return;
                    }
                }

                // Let Cmd+V paste
                if keystroke.modifiers.platform && keystroke.key == "v" {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        this.terminal.update(cx, |term, _cx| {
                            term.paste(&text);
                        });
                    }
                    return;
                }

                // Send key_char directly for printable characters without modifiers
                if !keystroke.modifiers.platform {
                    let handled = this
                        .terminal
                        .update(cx, |term, _cx| term.try_keystroke(keystroke, true));
                    if handled {
                        cx.stop_propagation();
                        return;
                    }

                    // For plain text input (no modifiers besides shift), send key_char
                    if let Some(key_char) = keystroke
                        .key_char
                        .as_ref()
                        .filter(|_| !keystroke.modifiers.control)
                    {
                        this.terminal.update(cx, |term, _cx| {
                            term.input(key_char.as_bytes().to_vec());
                        });
                        cx.stop_propagation();
                    }
                }
            }))
            // Panel header
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h(px(28.0))
                    .w_full()
                    .px(px(8.0))
                    .gap(SPACE_XS)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Icon::new(IconName::SquareTerminal)
                            .with_size(px(12.0))
                            .text_color(muted),
                    )
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .min_w_0()
                            .child(SharedString::from(title)),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("terminal-close")
                            .ghost()
                            .small()
                            .icon(IconName::Close)
                            .text_color(muted)
                            .on_click(cx.listener(|_this, _, _, cx| {
                                cx.emit(TerminalViewEvent::Close);
                            })),
                    ),
            )
            .child(TerminalElement::new(terminal, focus))
    }
}
