use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputEvent, InputState},
    tab::TabBar,
};

use zero_llm::{LlmConfig, StreamEvent};

use crate::services::{LlmEvent, LlmService};
use crate::theme::{
    APP_ICON_MD, FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XL,
    TITLEBAR_HEIGHT,
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

    /// Text queued by Enter key (subscribe has no window access to clear input).
    pending_send: Option<String>,

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
            pending_send: None,
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
            tools: Vec::new(),
        });
        self.messages.push(Message {
            role: Role::Assistant,
            text: String::new(),
            tools: Vec::new(),
        });
        self.loading = true;

        let rx = self.llm.update(cx, |llm, _| llm.ask(&text));

        let Some(mut rx) = rx else {
            if let Some(last) = self.messages.last_mut() {
                last.text = "LLM not configured — open settings.".to_string();
            }
            self.loading = false;
            cx.notify();
            return;
        };

        let llm = self.llm.clone();
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.recv().await {
                let ok = this.update(cx, |this, cx| {
                    let Some(last) = this.messages.last_mut() else {
                        return;
                    };
                    match event {
                        StreamEvent::TextDelta(t) => last.text.push_str(&t),
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

        div()
            .id("ask-view")
            .size_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(self.render_header(has_messages, ready, cx))
            .when(show_picker, |el| el.child(self.render_model_picker(cx)))
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
                        self.render_messages(cx).into_any_element()
                    }),
            )
            .when(ready && !show_setup, |el| {
                el.child(ChatPrompt::render(
                    &input,
                    loading,
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
        let model_name = if ready {
            let llm = self.llm.read(cx);
            LlmConfig::model_display_name(llm.model()).to_string()
        } else {
            String::new()
        };

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
            .when(ready, |el| {
                let picker_active = self.show_model_picker;
                el.child(
                    div()
                        .id("model-selector")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(3.0))
                        .cursor_pointer()
                        .rounded(RADIUS)
                        .px(px(6.0))
                        .py(px(2.0))
                        .hover(|s| s.bg(cx.theme().secondary))
                        .when(picker_active, |el| el.bg(cx.theme().secondary))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_model_picker = !this.show_model_picker;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child(SharedString::from(model_name)),
                        )
                        .child(
                            Icon::new(IconName::ChevronDown)
                                .with_size(px(12.0))
                                .text_color(muted),
                        ),
                )
            })
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
        let provider = llm.provider();
        let current_model = llm.model().to_string();
        let models = LlmConfig::available_models(provider);

        div()
            .w_full()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .py(px(4.0))
            .px(SPACE_MD)
            .child(div().flex().flex_col().children(models.iter().map(|m| {
                let is_active = m.id == current_model;
                let model_id = m.id.to_string();

                div()
                    .id(SharedString::from(format!("model-{}", m.id)))
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
                            llm.update(cx, |llm, cx| llm.set_model(&model_id, cx));
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
            })))
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

    fn render_messages(&self, cx: &App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let loading = self.loading;

        div()
            .id("ask-messages")
            .size_full()
            .overflow_y_scroll()
            .px(SPACE_LG)
            .py(SPACE_MD)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(SPACE_MD)
                    .children(self.messages.iter().map(|msg| {
                        match msg.role {
                            Role::User => UserBubble::new(msg.text.clone()).into_any_element(),
                            Role::Assistant => {
                                AssistantMessage::new(msg.text.clone(), msg.tools.clone())
                                    .into_any_element()
                            }
                        }
                    }))
                    .when(loading, |el| {
                        el.child(
                            div()
                                .py(SPACE_SM)
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child("Thinking..."),
                        )
                    }),
            )
    }
}
