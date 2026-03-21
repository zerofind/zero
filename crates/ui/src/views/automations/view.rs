use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

use crate::theme::{
    self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, FONT_SIZE_DISPLAY, RADIUS, brand_color,
};
use crate::ui::EmptyState;
use gpui_component::IconName;

use super::modal::{AutomationModal, EditData, ModalEvent};

// -- Cached automation display data ------------------------------------------

#[derive(Clone)]
pub(super) struct AutomationCard {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub dest_name: String,
    pub trigger_desc: String,
    pub last_run: Option<String>,
    pub sources: Vec<String>,
    pub dest_raw: String,
    pub on_mount: bool,
    pub on_change: bool,
    pub verify: bool,
    pub delete_orphans: bool,
}

// -- View --------------------------------------------------------------------

pub struct AutomationsView {
    automations: Vec<AutomationCard>,
    loading: bool,
    error: Option<String>,
    modal: Option<Entity<AutomationModal>>,
    #[allow(dead_code)]
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}

impl AutomationsView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            automations: Vec::new(),
            loading: true,
            error: None,
            modal: None,
            focus_handle: cx.focus_handle(),
            _subs: Vec::new(),
        };
        view.load_automations(cx);
        view
    }

    fn load_automations(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.error = None;

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { Self::fetch_automations().await })
                .await;

            this.update(cx, |view, cx| {
                view.loading = false;
                match result {
                    Ok(cards) => view.automations = cards,
                    Err(e) => view.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    async fn fetch_automations() -> Result<Vec<AutomationCard>, String> {
        let db = cache::CacheDb::open().map_err(|e| format!("Failed to open database: {e}"))?;

        let automations = db
            .list_automations()
            .map_err(|e| format!("Failed to list automations: {e}"))?;

        let mut cards = Vec::new();
        for auto in automations {
            let trigger_desc = {
                let mut parts = Vec::new();
                if auto.triggers.on_mount {
                    parts.push("on mount");
                }
                if auto.triggers.on_change {
                    parts.push("on change");
                }
                if parts.is_empty() {
                    "manual only".to_string()
                } else {
                    parts.join(", ")
                }
            };

            let dest_name = auto
                .dest_volume_name
                .clone()
                .or_else(|| auto.dest_path.clone())
                .unwrap_or_else(|| "Unknown".to_string());

            let dest_raw = auto.dest_path.clone().unwrap_or_default();
            let sources: Vec<String> = auto.paths.iter().map(|p| p.source.clone()).collect();

            let latest = db.get_latest_run(auto.id).ok().flatten();

            let last_run = latest.map(|r| format!("Last run: {}", r.status.as_str()));

            cards.push(AutomationCard {
                id: auto.id,
                name: auto.name,
                enabled: auto.enabled,
                dest_name,
                trigger_desc,
                last_run,
                sources,
                dest_raw,
                on_mount: auto.triggers.on_mount,
                on_change: auto.triggers.on_change,
                verify: auto.settings.verify,
                delete_orphans: auto.settings.delete_orphans,
            });
        }

        Ok(cards)
    }

    fn open_new_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!("automations: new");
        let modal = cx.new(|cx| AutomationModal::new(window, cx));
        let sub = cx.subscribe_in(&modal, window, Self::on_modal_event);
        self._subs.push(sub);
        self.modal = Some(modal);
        cx.notify();
    }

    fn open_edit_modal(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!(idx, "automations: edit");
        let Some(card) = self.automations.get(idx) else {
            return;
        };
        let data = EditData {
            id: card.id,
            name: card.name.clone(),
            sources: card.sources.clone(),
            dest: card.dest_raw.clone(),
            on_mount: card.on_mount,
            on_change: card.on_change,
            verify: card.verify,
            delete_orphans: card.delete_orphans,
        };
        let modal = cx.new(|cx| AutomationModal::edit(data, window, cx));
        let sub = cx.subscribe_in(&modal, window, Self::on_modal_event);
        self._subs.push(sub);
        self.modal = Some(modal);
        cx.notify();
    }

    fn delete_automation(&mut self, idx: usize, cx: &mut Context<Self>) {
        tracing::debug!(idx, "automations: delete");
        let Some(card) = self.automations.get(idx) else {
            return;
        };
        let id = card.id;

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let db = cache::CacheDb::open().map_err(|e| format!("{e}"))?;
                    db.delete_automation(id).map_err(|e| format!("{e}"))
                })
                .await;

            this.update(cx, |view, cx| {
                if let Err(e) = result {
                    view.error = Some(e);
                }
                view.load_automations(cx);
            })
            .ok();
        })
        .detach();
    }

    fn on_modal_event(
        &mut self,
        _: &Entity<AutomationModal>,
        event: &ModalEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ModalEvent::Saved => {
                self.modal = None;
                self.load_automations(cx);
            }
            ModalEvent::Dismissed => {
                self.modal = None;
                cx.notify();
            }
        }
    }
}

impl Render for AutomationsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let modal_view = self.modal.clone();

        let card_elements: Vec<AnyElement> = (0..self.automations.len())
            .map(|i| {
                let card = &self.automations[i];
                self.render_card(i, card, muted, cx)
            })
            .collect();

        v_flex()
            .id("automations-view")
            .size_full()
            .bg(theme::content_bg(cx))
            .overflow_y_scroll()
            .p_6()
            .child(
                v_flex()
                    .max_w(px(640.0))
                    .mx_auto()
                    .gap_4()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_DISPLAY)
                                    .font_weight(FontWeight::BOLD)
                                    .child("Automations"),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("refresh")
                                            .label("Refresh")
                                            .compact()
                                            .small()
                                            .ghost()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.load_automations(cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("new-automation")
                                            .label("New")
                                            .compact()
                                            .small()
                                            .primary()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.open_new_modal(window, cx);
                                            })),
                                    ),
                            ),
                    )
                    .child(div().text_size(FONT_SIZE_BODY).text_color(muted).child(
                        "Automated sync tasks that run when drives connect or files change.",
                    ))
                    .when(self.loading, |el| {
                        el.child(
                            div()
                                .text_size(FONT_SIZE_BODY)
                                .text_color(muted)
                                .child("Loading..."),
                        )
                    })
                    .when_some(self.error.clone(), |el, err| {
                        el.child(
                            div()
                                .text_size(FONT_SIZE_BODY)
                                .text_color(cx.theme().danger)
                                .child(SharedString::from(err)),
                        )
                    })
                    .when(
                        !self.loading && self.automations.is_empty() && self.error.is_none(),
                        |el| {
                            el.child(
                                EmptyState::new(IconName::Settings, "No automations configured")
                                    .subtitle("Click \"New\" to create your first automation."),
                            )
                        },
                    )
                    .children(card_elements),
            )
            .when_some(modal_view, |el, modal| el.child(modal))
    }
}

impl AutomationsView {
    fn render_card(
        &self,
        idx: usize,
        card: &AutomationCard,
        muted: Hsla,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = card.enabled;

        div()
            .id(SharedString::from(format!("auto-{idx}")))
            .px_4()
            .py_3()
            .rounded(RADIUS)
            .border_1()
            .border_color(cx.theme().border)
            .bg(theme::surface_hover(cx))
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .w(px(8.0))
                                            .h(px(8.0))
                                            .rounded(px(4.0))
                                            .bg(if enabled { brand_color(cx) } else { muted }),
                                    )
                                    .child(
                                        div()
                                            .text_size(FONT_SIZE_BODY)
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(SharedString::from(card.name.clone())),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(
                                        Button::new(SharedString::from(format!("edit-{idx}")))
                                            .ghost()
                                            .compact()
                                            .small()
                                            .label("Edit")
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.open_edit_modal(idx, window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!("del-{idx}")))
                                            .ghost()
                                            .compact()
                                            .small()
                                            .danger()
                                            .label("Delete")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.delete_automation(idx, cx);
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_3()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(muted)
                                    .child(SharedString::from(format!("To: {}", card.dest_name))),
                            )
                            .child(div().text_size(FONT_SIZE_CAPTION).text_color(muted).child(
                                SharedString::from(format!("Trigger: {}", card.trigger_desc)),
                            )),
                    )
                    .when_some(card.last_run.clone(), |el, last| {
                        el.child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child(SharedString::from(last)),
                        )
                    }),
            )
            .into_any_element()
    }
}
