use std::collections::HashSet;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputEvent, InputState},
    tab::TabBar,
    text::TextView,
};

use llm::{LlmConfig, StreamEvent};

use crate::services::{LlmEvent, LlmService};
use crate::theme::{
    APP_ICON_MD, FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XL,
    SPACE_XS, TITLEBAR_HEIGHT,
};

use super::message::{AssistantMessage, ToolCall, UserBubble};
use super::prompt::ChatPrompt;

// -- Events ------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum AskViewEvent {
    Close,
}

impl EventEmitter<AskViewEvent> for AskView {}

// -- Message model -----------------------------------------------------------

#[derive(Clone, Debug)]
enum Role {
    User,
    Assistant,
}

#[derive(Clone)]
struct Message {
    role: Role,
    text: String,
    thinking: String,
    tools: Vec<ToolCall>,
}

// -- Provider options --------------------------------------------------------

const PROVIDERS: &[(&str, &str)] = &[("anthropic", "Anthropic"), ("openai", "OpenAI")];

// -- View --------------------------------------------------------------------

pub struct AskView {
    messages: Vec<Message>,
    input: Entity<InputState>,
    llm: Entity<LlmService>,
    loading: bool,
    scroll_handle: ScrollHandle,

    /// Text queued by Enter key (subscribe has no window access to clear input).
    pending_send: Option<String>,

    /// Indices of messages with expanded thinking blocks.
    expanded_thinking: HashSet<usize>,

    selected_provider: usize,
    key_input: Entity<InputState>,
    setup_error: Option<String>,
    show_setup: bool,
    show_model_picker: bool,

    _subs: Vec<Subscription>,
}

impl AskView {
    pub fn new(llm: Entity<LlmService>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Ask about your files..."));
        let key_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("sk-...")
                .masked(true)
        });

        let input_sub = cx.subscribe(&input, |this: &mut Self, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                let text = this.input.read(cx).value().to_string();
                let text = text.trim().to_string();
                if !text.is_empty() && !this.loading {
                    this.pending_send = Some(text);
                    cx.notify();
                }
            }
        });

        let key_sub = cx.subscribe(&key_input, |this: &mut Self, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.connect_provider(cx);
            }
        });

        let llm_sub = cx.subscribe(&llm, |this: &mut Self, _, ev: &LlmEvent, cx| match ev {
            LlmEvent::Configured => {
                this.show_setup = false;
                this.show_model_picker = false;
                this.setup_error = None;
                cx.notify();
            }
            LlmEvent::Error(msg) => {
                this.setup_error = Some(msg.clone());
                cx.notify();
            }
        });

        Self {
            messages: Vec::new(),
            input,
            llm,
            loading: false,
            scroll_handle: ScrollHandle::new(),
            pending_send: None,
            expanded_thinking: HashSet::new(),
            selected_provider: 0,
            key_input,
            setup_error: None,
            show_setup: false,
            show_model_picker: false,
            _subs: vec![input_sub, key_sub, llm_sub],
        }
    }

    fn connect_provider(&mut self, cx: &mut Context<Self>) {
        let key = self.key_input.read(cx).value().to_string();
        let key = key.trim().to_string();
        if key.is_empty() {
            self.setup_error = Some("API key is required.".to_string());
            cx.notify();
            return;
        }

        let (provider_id, _) = PROVIDERS[self.selected_provider];
        self.setup_error = None;

        self.llm.update(cx, |llm, cx| {
            llm.set_api_key(provider_id, &key, cx);
        });
    }

    fn send_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let text = self.input.read(cx).value().to_string();
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }

        self.input
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.submit_text(text, cx);
    }

    fn submit_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.messages.push(Message {
            role: Role::User,
            text: text.clone(),
            thinking: String::new(),
            tools: Vec::new(),
        });
        self.messages.push(Message {
            role: Role::Assistant,
            text: String::new(),
            thinking: String::new(),
            tools: Vec::new(),
        });
        self.loading = true;

        let rx = self.llm.update(cx, |llm, _| llm.ask(&text));

        let Some(rx) = rx else {
            if let Some(last) = self.messages.last_mut() {
                last.text = "LLM not configured — open settings.".to_string();
            }
            self.loading = false;
            cx.notify();
            return;
        };

        let llm = self.llm.clone();
        cx.spawn(async move |this, cx| {
            while let Ok(event) = rx.recv().await {
                let ok = this.update(cx, |this, cx| {
                    let Some(last) = this.messages.last_mut() else {
                        return;
                    };
                    match event {
                        StreamEvent::TextDelta(t) => last.text.push_str(&t),
                        StreamEvent::ThinkingDelta(t) => last.thinking.push_str(&t),
                        StreamEvent::ToolCallStart(name) => {
                            last.tools.push(ToolCall { name, done: false });
                        }
                        StreamEvent::ToolCallDone(name, _) => {
                            for tool in &mut last.tools {
                                if tool.name == name {
                                    tool.done = true;
                                }
                            }
                        }
                        StreamEvent::Done(full) => {
                            this.loading = false;
                            llm.update(cx, |llm, _| llm.record_response(&full));
                        }
                        StreamEvent::Error(e) => {
                            this.loading = false;
                            last.text = format!("Error: {e}");
                        }
                    }
                    cx.notify();
                });
                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();

        cx.notify();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.messages.clear();
        self.loading = false;
        self.llm.update(cx, |llm, _| llm.clear_history());
        cx.notify();
    }

    fn is_ready(&self, cx: &App) -> bool {
        self.llm.read(cx).is_ready()
    }
}

// -- Render ------------------------------------------------------------------

impl Render for AskView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Process Enter-key send (subscribe callback has no window access).
        if let Some(text) = self.pending_send.take() {
            self.input
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.submit_text(text, cx);
        }

        let ready = self.is_ready(cx);
        let has_messages = !self.messages.is_empty();
        let show_setup = self.show_setup || !ready;
        let show_picker = self.show_model_picker && ready && !show_setup;
        let input = self.input.clone();
        let loading = self.loading;

        let model_name = if ready {
            let llm = self.llm.read(cx);
            LlmConfig::model_display_name(llm.model(), llm.thinking()).to_string()
        } else {
            String::new()
        };

        div()
            .id("ask-view")
            .size_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(self.render_header(has_messages, ready, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(if show_setup {
                        self.render_setup(cx).into_any_element()
                    } else if !has_messages {
                        self.render_empty_state(cx).into_any_element()
                    } else {
                        self.render_messages(window, cx).into_any_element()
                    }),
            )
            // Model picker dropdown (above prompt area)
            .when(show_picker, |el| el.child(self.render_model_picker(cx)))
            // Prompt area
            .when(ready && !show_setup, |el| {
                el.child(ChatPrompt::render(
                    &input,
                    loading,
                    &model_name,
                    cx.listener(|this, _, _, cx| {
                        this.show_model_picker = !this.show_model_picker;
                        cx.notify();
                    }),
                    cx.listener(|this, _, window, cx| this.send_message(window, cx)),
                    cx,
                ))
            })
    }
}

// -- Render helpers ----------------------------------------------------------

impl AskView {
    fn render_header(
        &mut self,
        has_messages: bool,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        div()
            .flex()
            .flex_row()
            .items_center()
            .h(TITLEBAR_HEIGHT)
            .w_full()
            .px(SPACE_MD)
            .gap(SPACE_SM)
            .child(
                div()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(muted)
                    .child("Ask"),
            )
            .child(div().flex_1())
            .when(has_messages, |el| {
                el.child(
                    Button::new("ask-clear")
                        .ghost()
                        .small()
                        .icon(IconName::Redo)
                        .text_color(muted)
                        .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
                )
            })
            .child(
                Button::new("ask-settings")
                    .ghost()
                    .small()
                    .icon(IconName::Settings)
                    .text_color(if self.show_setup || !ready {
                        cx.theme().foreground
                    } else {
                        muted
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_setup = !this.show_setup;
                        this.show_model_picker = false;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("ask-close")
                    .ghost()
                    .small()
                    .icon(IconName::Close)
                    .text_color(muted)
                    .on_click(cx.listener(|_this, _, _, cx| cx.emit(AskViewEvent::Close))),
            )
    }

    fn render_model_picker(&self, cx: &App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let llm = self.llm.read(cx);
        let current_model = llm.model().to_string();
        let current_thinking = llm.thinking();

        let active_key = if current_thinking {
            format!("{}:thinking", current_model)
        } else {
            current_model
        };

        // Show models from all providers that have keys
        let providers = llm.config().providers_with_keys();

        div()
            .w_full()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .py(px(4.0))
            .px(SPACE_MD)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .children(providers.into_iter().map(|provider| {
                        let models = LlmConfig::available_models(provider);
                        let label = match provider {
                            "anthropic" => "Anthropic",
                            "openai" => "OpenAI",
                            _ => provider,
                        };

                        div()
                            .flex()
                            .flex_col()
                            // Provider header
                            .child(
                                div()
                                    .px(px(6.0))
                                    .py(px(4.0))
                                    .text_size(px(10.0))
                                    .text_color(muted.opacity(0.6))
                                    .child(label),
                            )
                            // Model rows
                            .children(models.iter().map(|m| {
                                let picker_key = m.picker_key();
                                let is_active = picker_key == active_key;
                                let model_id = m.id.to_string();
                                let thinking = m.thinking;

                                div()
                                    .id(SharedString::from(format!("model-{}", picker_key)))
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(SPACE_SM)
                                    .px(px(6.0))
                                    .py(px(4.0))
                                    .rounded(RADIUS)
                                    .cursor_pointer()
                                    .hover(|s| s.bg(cx.theme().secondary))
                                    .when(is_active, |el| el.bg(cx.theme().secondary))
                                    .on_click({
                                        let llm = self.llm.clone();
                                        move |_, _, cx| {
                                            let model_id = model_id.clone();
                                            llm.update(cx, |llm, cx| {
                                                llm.set_model(&model_id, thinking, cx);
                                            });
                                        }
                                    })
                                    .child(
                                        div()
                                            .w(px(14.0))
                                            .text_size(FONT_SIZE_CAPTION)
                                            .text_color(if is_active {
                                                cx.theme().foreground
                                            } else {
                                                gpui::transparent_black()
                                            })
                                            .child(if is_active { "●" } else { "" }),
                                    )
                                    .child(
                                        div()
                                            .text_size(FONT_SIZE_CAPTION)
                                            .text_color(if is_active {
                                                cx.theme().foreground
                                            } else {
                                                muted
                                            })
                                            .child(m.name),
                                    )
                            }))
                    })),
            )
    }

    fn render_setup(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let ready = self.is_ready(cx);
        let error = self.setup_error.clone();

        // Outer: sized container, no scroll/flex conflict
        div().id("ask-setup").size_full().overflow_y_scroll().child(
            // Inner: flex layout for centering content
            div()
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .pt(px(48.0))
                .px(SPACE_XL)
                .pb(SPACE_XL)
                .gap(SPACE_XL)
                // Provider tabs (segmented, full width)
                .child(
                    div().w_full().max_w(px(300.0)).child(
                        TabBar::new("ask-provider-tabs")
                            .segmented()
                            .small()
                            .selected_index(self.selected_provider)
                            .child("Anthropic")
                            .child("OpenAI")
                            .on_click(cx.listener(|this, idx: &usize, _, cx| {
                                this.selected_provider = *idx;
                                this.setup_error = None;
                                cx.notify();
                            })),
                    ),
                )
                // API key input
                .child(
                    div()
                        .w_full()
                        .max_w(px(300.0))
                        .flex()
                        .flex_col()
                        .gap(SPACE_SM)
                        .child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child("API Key"),
                        )
                        .child(
                            div()
                                .w_full()
                                .child(Input::new(&self.key_input).cleanable(false)),
                        )
                        .when_some(error, |el, msg| {
                            el.child(
                                div()
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(cx.theme().danger)
                                    .child(SharedString::from(msg)),
                            )
                        }),
                )
                // Save button
                .child(
                    div().w_full().max_w(px(300.0)).child(
                        Button::new("ask-connect")
                            .primary()
                            .rounded(RADIUS)
                            .label("Save")
                            .w_full()
                            .on_click(cx.listener(|this, _, _, cx| this.connect_provider(cx))),
                    ),
                )
                // Back to chat (when already configured)
                .when(ready, |el| {
                    el.child(
                        Button::new("ask-back-to-chat")
                            .ghost()
                            .small()
                            .label("Back to chat")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_setup = false;
                                cx.notify();
                            })),
                    )
                }),
        )
    }

    fn render_empty_state(&self, cx: &App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(SPACE_MD)
                    .child(
                        Icon::new(IconName::Bot)
                            .with_size(APP_ICON_MD)
                            .text_color(muted.opacity(0.4)),
                    )
                    .child(
                        div()
                            .text_size(FONT_SIZE_BODY)
                            .text_color(muted)
                            .child("Ask about your files..."),
                    ),
            )
    }

    fn render_messages(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let loading = self.loading;
        let msg_count = self.messages.len();

        // Auto-scroll to bottom when streaming
        if loading && msg_count > 0 {
            self.scroll_handle.scroll_to_item(msg_count - 1);
        }

        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().primary.opacity(0.3);

        // Pre-collect message data to avoid borrow conflicts
        let message_data: Vec<_> = self
            .messages
            .iter()
            .enumerate()
            .map(|(idx, msg)| {
                (
                    idx,
                    msg.role.clone(),
                    msg.text.clone(),
                    msg.thinking.clone(),
                    msg.tools.clone(),
                )
            })
            .collect();

        let expanded_thinking = self.expanded_thinking.clone();

        div()
            .id("ask-messages")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .px(SPACE_LG)
            .py(SPACE_MD)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(SPACE_MD)
                    .children(message_data.into_iter().map(
                        |(idx, role, text, thinking, tools)| match role {
                            Role::User => UserBubble::new(text).into_any_element(),
                            Role::Assistant => {
                                let is_last = idx == msg_count - 1;
                                let has_thinking = !thinking.is_empty();
                                let thinking_expanded = expanded_thinking.contains(&idx);

                                let am = AssistantMessage::new(text, tools)
                                    .index(idx)
                                    .streaming(loading && is_last);

                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(SPACE_SM)
                                    // Thinking toggle + content
                                    .when(has_thinking, |el| {
                                        el.child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(SPACE_XS)
                                                // Toggle header
                                                .child(
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "thinking-toggle-{idx}"
                                                        )))
                                                        .flex()
                                                        .flex_row()
                                                        .items_center()
                                                        .gap(SPACE_XS)
                                                        .cursor_pointer()
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                if this
                                                                    .expanded_thinking
                                                                    .contains(&idx)
                                                                {
                                                                    this.expanded_thinking
                                                                        .remove(&idx);
                                                                } else {
                                                                    this.expanded_thinking
                                                                        .insert(idx);
                                                                }
                                                                cx.notify();
                                                            },
                                                        ))
                                                        .child(
                                                            Icon::new(if thinking_expanded {
                                                                IconName::ChevronDown
                                                            } else {
                                                                IconName::ChevronRight
                                                            })
                                                            .with_size(px(10.0))
                                                            .text_color(muted),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_size(FONT_SIZE_CAPTION)
                                                                .text_color(muted)
                                                                .child("Thinking"),
                                                        ),
                                                )
                                                // Expanded thinking content
                                                .when(thinking_expanded, |el| {
                                                    el.child(
                                                        div()
                                                            .ml(px(6.0))
                                                            .pl(SPACE_MD)
                                                            .border_l_1()
                                                            .border_color(accent)
                                                            .child(
                                                                TextView::markdown(
                                                                    ElementId::Name(
                                                                        format!(
                                                                            "thinking-md-{idx}"
                                                                        )
                                                                        .into(),
                                                                    ),
                                                                    thinking,
                                                                    window,
                                                                    cx,
                                                                )
                                                                .text_size(FONT_SIZE_CAPTION)
                                                                .text_color(muted),
                                                            ),
                                                    )
                                                }),
                                        )
                                    })
                                    .child(am)
                                    .into_any_element()
                            }
                        },
                    )),
            )
    }
}
