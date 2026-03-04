use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme,
    input::{Input, InputEvent, InputState},
    v_flex,
};

use crate::theme::{self, FONT_SIZE_BODY};
use crate::ui::ConfirmDialog;

// -- Events ------------------------------------------------------------------

pub enum EditorEvent {
    Close,
}

impl EventEmitter<EditorEvent> for EditorView {}

// -- View --------------------------------------------------------------------

pub struct EditorView {
    path: PathBuf,
    input: Entity<InputState>,
    original_content: String,
    modified: bool,
    saving: bool,
    error: Option<String>,
    confirm_close: bool,
    pub focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}

impl EditorView {
    /// Maximum file size we'll open in the editor (2 MB).
    const MAX_EDITOR_FILE_SIZE: u64 = 2 * 1024 * 1024;

    pub fn new(path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (content, error) = match std::fs::metadata(&path) {
            Ok(meta) if meta.len() > Self::MAX_EDITOR_FILE_SIZE => (
                String::new(),
                Some(format!(
                    "File too large to edit ({:.1} MB). Maximum is 2 MB.",
                    meta.len() as f64 / (1024.0 * 1024.0),
                )),
            ),
            _ => match std::fs::read_to_string(&path) {
                Ok(c) => (c, None),
                Err(e) => (String::new(), Some(e.to_string())),
            },
        };

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();

        let original_content = content.clone();

        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor(SharedString::from(ext))
                .default_value(&content)
        });

        let sub = cx.subscribe(&input, Self::on_input_event);

        Self {
            path,
            input,
            original_content,
            modified: false,
            saving: false,
            error,
            confirm_close: false,
            focus_handle: cx.focus_handle(),
            _subs: vec![sub],
        }
    }

    fn on_input_event(
        &mut self,
        _: Entity<InputState>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            let current = self.input.read(cx).value().to_string();
            self.modified = current != self.original_content;
            cx.notify();
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if self.modified {
            self.confirm_close = true;
            cx.notify();
        } else {
            cx.emit(EditorEvent::Close);
        }
    }

    fn confirm_close_save(&mut self, cx: &mut Context<Self>) {
        self.confirm_close = false;
        // Save, then close on completion
        if self.saving {
            return;
        }
        let content = self.input.read(cx).value().to_string();
        let path = self.path.clone();

        self.saving = true;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { std::fs::write(&path, &content) })
                .await;

            this.update(cx, |view, cx| {
                view.saving = false;
                match result {
                    Ok(()) => {
                        let current = view.input.read(cx).value().to_string();
                        view.original_content = current;
                        view.modified = false;
                        cx.emit(EditorEvent::Close);
                    }
                    Err(e) => {
                        view.error = Some(format!("Save failed: {e}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn confirm_close_discard(&mut self, cx: &mut Context<Self>) {
        self.confirm_close = false;
        cx.emit(EditorEvent::Close);
    }

    pub fn save(&mut self, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let content = self.input.read(cx).value().to_string();
        let path = self.path.clone();

        self.saving = true;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { std::fs::write(&path, &content) })
                .await;

            this.update(cx, |view, cx| {
                view.saving = false;
                match result {
                    Ok(()) => {
                        let current = view.input.read(cx).value().to_string();
                        view.original_content = current;
                        view.modified = false;
                    }
                    Err(e) => {
                        view.error = Some(format!("Save failed: {e}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.to_string_lossy().to_string())
    }

    pub fn is_modified(&self) -> bool {
        self.modified
    }

    pub fn is_saving(&self) -> bool {
        self.saving
    }

    pub fn path_str(&self) -> String {
        self.path.to_string_lossy().to_string()
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        let content: AnyElement = if let Some(err) = &self.error {
            div()
                .p_4()
                .text_size(FONT_SIZE_BODY)
                .text_color(muted)
                .child(SharedString::from(format!("Error: {err}")))
                .into_any_element()
        } else {
            div()
                .id("editor-input-wrapper")
                .flex_1()
                .size_full()
                .overflow_hidden()
                .font_family("Berkeley Mono, Menlo, Monaco, monospace")
                .child(
                    Input::new(&self.input)
                        .size_full()
                        .cleanable(false)
                        .into_any_element(),
                )
                .into_any_element()
        };

        // Unsaved changes confirmation dialog
        let close_dialog = if self.confirm_close {
            let file_name = self.file_name();
            let save_entity = cx.entity().clone();
            let discard_entity = cx.entity().clone();

            Some(
                ConfirmDialog::new(
                    "Unsaved Changes",
                    format!("Do you want to save changes to \"{}\"?", file_name),
                    move |_window, cx| {
                        save_entity.update(cx, |this, cx| {
                            this.confirm_close_save(cx);
                        });
                    },
                    move |_window, cx| {
                        discard_entity.update(cx, |this, cx| {
                            this.confirm_close_discard(cx);
                        });
                    },
                )
                .confirm_label("Save")
                .cancel_label("Don't Save")
                .render_element(window, cx),
            )
        } else {
            None
        };

        div()
            .relative()
            .size_full()
            .child(
                v_flex()
                    .track_focus(&self.focus_handle)
                    .key_context("EditorView")
                    .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                        if ev.keystroke.key == "escape" {
                            this.close(cx);
                        }
                        // Cmd+S / Ctrl+S to save
                        if ev.keystroke.key == "s"
                            && (ev.keystroke.modifiers.platform || ev.keystroke.modifiers.control)
                        {
                            this.save(cx);
                        }
                        // Cmd+W / Ctrl+W to close
                        if ev.keystroke.key == "w"
                            && (ev.keystroke.modifiers.platform || ev.keystroke.modifiers.control)
                        {
                            this.close(cx);
                        }
                    }))
                    .size_full()
                    .bg(theme::content_bg(cx))
                    .child(content),
            )
            .when_some(close_dialog, |el, dialog| el.child(dialog))
    }
}
