use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

use crate::theme::{
    self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, FONT_SIZE_DISPLAY, RADIUS_LG, RADIUS_SM, brand_color,
};

// -- Events ------------------------------------------------------------------

pub enum OnboardingEvent {
    Complete(Vec<PathBuf>),
    Skip,
}

impl EventEmitter<OnboardingEvent> for OnboardingView {}

// -- Steps -------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    Welcome,
    GrantAccess,
    Complete,
}

// -- View --------------------------------------------------------------------

pub struct OnboardingView {
    step: Step,
    selected_roots: Vec<PathBuf>,
    focus_handle: FocusHandle,
}

impl OnboardingView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            step: Step::Welcome,
            selected_roots: Vec::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    fn advance(&mut self, cx: &mut Context<Self>) {
        tracing::debug!(step = ?self.step, "onboarding: advance");
        self.step = match self.step {
            Step::Welcome => Step::GrantAccess,
            Step::GrantAccess => {
                cx.emit(OnboardingEvent::Complete(self.selected_roots.clone()));
                Step::Complete
            }
            Step::Complete => Step::Complete,
        };
        cx.notify();
    }

    fn skip(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("onboarding: skip");
        cx.emit(OnboardingEvent::Skip);
    }

    fn add_folder(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("onboarding: add folder");
        if let Some(home) = dirs::home_dir()
            && !self.selected_roots.contains(&home)
        {
            self.selected_roots.push(home);
            cx.notify();
        }
    }
}

impl Render for OnboardingView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        let step_content: AnyElement = match self.step {
            Step::Welcome => self.render_welcome(muted, cx).into_any_element(),
            Step::GrantAccess => self.render_grant_access(muted, cx).into_any_element(),
            Step::Complete => self.render_complete(muted, cx).into_any_element(),
        };

        div()
            .id("onboarding-overlay")
            .track_focus(&self.focus_handle)
            .absolute()
            .size_full()
            .top_0()
            .left_0()
            .bg(theme::overlay_backdrop(cx))
            .flex()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .w(px(480.0))
                    .rounded(RADIUS_LG)
                    .bg(theme::popover_bg(cx))
                    .border_1()
                    .border_color(cx.theme().border)
                    .shadow_lg()
                    .p_6()
                    .gap_5()
                    .child(step_content),
            )
    }
}

impl OnboardingView {
    fn render_welcome(&mut self, muted: Hsla, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_size(FONT_SIZE_DISPLAY)
                            .font_weight(FontWeight::BOLD)
                            .child("Welcome to Zero"),
                    )
                    .child(div().text_size(FONT_SIZE_BODY).text_color(muted).child(
                        "A privacy-first Finder replacement. \
                                 All search is 100% local \u{2014} nothing leaves your Mac.",
                    )),
            )
            .child({
                let brand = brand_color(cx);
                v_flex().gap_1().children(
                    [
                        ("Instant local search", "Find any file in milliseconds"),
                        ("Resumable sync", "Copy files with verification and resume"),
                        ("Duplicate detection", "Reclaim wasted storage space"),
                    ]
                    .into_iter()
                    .map(move |(title, desc)| {
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().text_color(brand).child("\u{2713}"))
                            .child(
                                v_flex()
                                    .child(
                                        div()
                                            .text_size(FONT_SIZE_BODY)
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .text_size(FONT_SIZE_CAPTION)
                                            .text_color(muted)
                                            .child(desc),
                                    ),
                            )
                    }),
                )
            })
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        Button::new("skip")
                            .label("Skip")
                            .compact()
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.skip(cx);
                            })),
                    )
                    .child(
                        Button::new("get-started")
                            .label("Get Started")
                            .compact()
                            .small()
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.advance(cx);
                            })),
                    ),
            )
    }

    fn render_grant_access(&mut self, muted: Hsla, cx: &mut Context<Self>) -> impl IntoElement {
        let folder_rows: Vec<_> = self
            .selected_roots
            .iter()
            .map(|p| {
                h_flex()
                    .px_3()
                    .py_2()
                    .rounded(RADIUS_SM)
                    .bg(cx.theme().secondary)
                    .gap_2()
                    .items_center()
                    .child(div().text_color(brand_color(cx)).child("\u{1F4C1}"))
                    .child(
                        div()
                            .flex_1()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_ellipsis()
                            .child(SharedString::from(p.to_string_lossy().to_string())),
                    )
            })
            .collect();

        let has_folders = !folder_rows.is_empty();

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_size(FONT_SIZE_DISPLAY)
                            .font_weight(FontWeight::BOLD)
                            .child("Choose Folders to Index"),
                    )
                    .child(div().text_size(FONT_SIZE_BODY).text_color(muted).child(
                        "Select which folders Zero should index for instant search. \
                                 Your home directory is recommended.",
                    )),
            )
            .when(has_folders, |el| {
                el.child(v_flex().gap_1().children(folder_rows))
            })
            .child(
                Button::new("add-folder")
                    .label("Add Home Directory")
                    .compact()
                    .small()
                    .ghost()
                    .disabled(has_folders)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.add_folder(cx);
                    })),
            )
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        Button::new("skip-access")
                            .label("Skip")
                            .compact()
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.skip(cx);
                            })),
                    )
                    .child(
                        Button::new("continue-access")
                            .label("Continue")
                            .compact()
                            .small()
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.advance(cx);
                            })),
                    ),
            )
    }

    fn render_complete(&mut self, muted: Hsla, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_size(FONT_SIZE_DISPLAY)
                            .font_weight(FontWeight::BOLD)
                            .child("You're All Set!"),
                    )
                    .child(div().text_size(FONT_SIZE_BODY).text_color(muted).child(
                        "Zero is indexing your files in the background. \
                                 You can start using the app right away.",
                    )),
            )
            .child(
                h_flex().justify_end().child(
                    Button::new("finish")
                        .label("Start Using Zero")
                        .compact()
                        .small()
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.skip(cx);
                        })),
                ),
            )
    }
}
