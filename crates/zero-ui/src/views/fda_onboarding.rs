use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

use crate::permissions;
use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, FONT_SIZE_DISPLAY, RADIUS_LG};

pub enum FdaOnboardingEvent {
    Granted,
}

impl EventEmitter<FdaOnboardingEvent> for FdaOnboardingView {}

pub struct FdaOnboardingView {
    focus_handle: FocusHandle,
}

impl FdaOnboardingView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let this = Self {
            focus_handle: cx.focus_handle(),
        };

        // Poll every 2 seconds to detect when user grants FDA
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(2))
                    .await;

                let should_break = this
                    .update(cx, |_, cx| {
                        if permissions::has_full_disk_access() {
                            cx.emit(FdaOnboardingEvent::Granted);
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(true);

                if should_break {
                    break;
                }
            }
        })
        .detach();

        this
    }
}

impl Render for FdaOnboardingView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let brand = theme::brand_color(cx);

        div()
            .id("fda-onboarding")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(theme::content_bg(cx))
            .flex()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .w(px(520.0))
                    .rounded(RADIUS_LG)
                    .bg(theme::popover_bg(cx))
                    .border_1()
                    .border_color(cx.theme().border)
                    .shadow_lg()
                    .p_8()
                    .gap_6()
                    // Title
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_DISPLAY)
                                    .font_weight(FontWeight::BOLD)
                                    .child("Full Disk Access Required"),
                            )
                            .child(div().text_size(FONT_SIZE_BODY).text_color(muted).child(
                                "Zero needs Full Disk Access to index and search your files. \
                                         This is a one-time setup.",
                            )),
                    )
                    // Steps
                    .child(
                        v_flex().gap_3().children(
                            [
                                (
                                    "1",
                                    "Open System Settings",
                                    "Click the button below to open Privacy & Security settings.",
                                ),
                                (
                                    "2",
                                    "Find Zero in the list",
                                    "Scroll to Zero in the Full Disk Access section.",
                                ),
                                (
                                    "3",
                                    "Toggle it on",
                                    "Zero will detect the change automatically.",
                                ),
                            ]
                            .into_iter()
                            .map(move |(num, title, desc)| {
                                h_flex()
                                    .gap_3()
                                    .items_start()
                                    .child(
                                        div()
                                            .w(px(24.0))
                                            .h(px(24.0))
                                            .rounded(px(12.0))
                                            .bg(brand.opacity(0.15))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(FONT_SIZE_CAPTION)
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(brand)
                                            .child(num),
                                    )
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
                        ),
                    )
                    // Action button
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                Button::new("open-settings")
                                    .label("Open System Settings")
                                    .small()
                                    .primary()
                                    .on_click(cx.listener(|_, _, _, _| {
                                        #[cfg(target_os = "macos")]
                                        permissions::open_full_disk_access_settings();
                                    })),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(muted)
                                    .text_align(gpui::TextAlign::Center)
                                    .child("Waiting for permission to be granted..."),
                            ),
                    ),
            )
    }
}
