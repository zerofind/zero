use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use alacritty_terminal::Term;
use alacritty_terminal::event::Event as AlacTermEvent;
use alacritty_terminal::event_loop::{EventLoop, Notifier};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Config;
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{
    CursorShape as AlacCursorShape, CursorStyle as AlacCursorStyle, Handler, NamedPrivateMode,
    PrivateMode, Rgb as AlacRgb,
};

use futures::channel::mpsc::unbounded;
use futures::{FutureExt, StreamExt};
use gpui::*;

use super::bounds::TerminalBounds;
use super::content::{EventQueue, TerminalContent, TerminalEvent, ZeroListener};
use super::{SCROLL_HISTORY_LINES, Terminal};

impl Terminal {
    pub fn spawn(cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

        let mut env: HashMap<String, String> = HashMap::new();
        env.insert("TERM".into(), "xterm-256color".into());
        env.insert("COLORTERM".into(), "truecolor".into());
        env.insert("TERM_PROGRAM".into(), "zero".into());
        env.insert(
            "LANG".into(),
            std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into()),
        );

        for var in &["PATH", "HOME", "USER"] {
            if let Ok(val) = std::env::var(var) {
                env.insert((*var).into(), val);
            }
        }

        let pty_options = tty::Options {
            shell: Some(alacritty_terminal::tty::Shell::new(
                shell,
                vec!["-l".into()],
            )),
            working_directory: Some(cwd),
            drain_on_exit: true,
            env,
            #[cfg(windows)]
            escape_args: false,
        };

        let config = Config {
            scrolling_history: SCROLL_HISTORY_LINES,
            default_cursor_style: AlacCursorStyle {
                shape: AlacCursorShape::Block,
                blinking: false,
            },
            ..Config::default()
        };

        let (events_tx, events_rx) = unbounded();

        let mut term = Term::new(
            config,
            &TerminalBounds::default(),
            ZeroListener(events_tx.clone()),
        );
        term.unset_private_mode(PrivateMode::Named(NamedPrivateMode::AlternateScroll));
        let term = Arc::new(FairMutex::new(term));

        let window_id = 0u64;
        let pty = tty::new(&pty_options, TerminalBounds::default().into(), window_id)
            .expect("terminal: failed to create PTY — is your shell valid?");

        let event_loop = EventLoop::new(
            term.clone(),
            ZeroListener(events_tx),
            pty,
            pty_options.drain_on_exit,
            false,
        )
        .expect("terminal: failed to start event loop");

        let pty_tx = Notifier(event_loop.channel());
        let _io_thread = event_loop.spawn();

        let event_loop_task =
            cx.spawn(async move |terminal, cx| Self::event_loop(terminal, events_rx, cx).await);

        Self {
            term,
            pty_tx,
            events: EventQueue::with_capacity(10),
            last_content: TerminalContent::default(),
            content_dirty: true,
            event_loop_task,
            scroll_px: px(0.0),
        }
    }

    async fn event_loop(
        terminal: WeakEntity<Self>,
        mut events_rx: futures::channel::mpsc::UnboundedReceiver<AlacTermEvent>,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<()> {
        while let Some(event) = events_rx.next().await {
            terminal.update(cx, |this, cx| {
                this.process_event(event, cx);
            })?;

            'outer: loop {
                let mut events = Vec::new();
                let mut timer = cx
                    .background_executor()
                    .timer(std::time::Duration::from_millis(4))
                    .fuse();
                let mut wakeup = false;

                loop {
                    futures::select_biased! {
                        () = timer => break,
                        event = events_rx.next() => {
                            if let Some(event) = event {
                                if matches!(event, AlacTermEvent::Wakeup) {
                                    wakeup = true;
                                } else {
                                    events.push(event);
                                }
                                if events.len() > 100 {
                                    break;
                                }
                            } else {
                                break;
                            }
                        },
                    }
                }

                if events.is_empty() && !wakeup {
                    smol::future::yield_now().await;
                    break 'outer;
                }

                terminal.update(cx, |this, cx| {
                    if wakeup {
                        this.process_event(AlacTermEvent::Wakeup, cx);
                    }
                    for event in events {
                        this.process_event(event, cx);
                    }
                })?;
                smol::future::yield_now().await;
            }
        }
        Ok(())
    }

    fn process_event(&mut self, event: AlacTermEvent, cx: &mut Context<Self>) {
        match event {
            AlacTermEvent::Wakeup => {
                self.content_dirty = true;
                cx.emit(TerminalEvent::Wakeup);
            }
            AlacTermEvent::Bell => {
                cx.emit(TerminalEvent::Bell);
            }
            AlacTermEvent::Exit | AlacTermEvent::ChildExit(_) => {
                cx.emit(TerminalEvent::Close);
            }
            AlacTermEvent::Title(title) => {
                cx.emit(TerminalEvent::TitleChanged(title));
            }
            AlacTermEvent::ClipboardStore(_, data) => {
                cx.write_to_clipboard(ClipboardItem::new_string(data));
            }
            AlacTermEvent::ClipboardLoad(_, format) => {
                let text = cx.read_from_clipboard().and_then(|item| item.text());
                let response = match &text {
                    Some(t) => format(t),
                    None => format(""),
                };
                self.write_to_pty(response.into_bytes());
            }
            AlacTermEvent::PtyWrite(out) => {
                self.write_to_pty(out.into_bytes());
            }
            AlacTermEvent::TextAreaSizeRequest(format) => {
                let ws: alacritty_terminal::event::WindowSize =
                    self.last_content.terminal_bounds.into();
                self.write_to_pty(format(ws).into_bytes());
            }
            AlacTermEvent::ColorRequest(index, format) => {
                let color =
                    self.term.lock_unfair().colors()[index].unwrap_or(AlacRgb { r: 0, g: 0, b: 0 });
                self.write_to_pty(format(color).into_bytes());
            }
            _ => {}
        }
    }
}
