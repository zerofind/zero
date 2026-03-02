use std::rc::Rc;

use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme, Sizable as _,
};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS_LG, MODAL_MD_WIDTH};

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

#[allow(dead_code)]
/// A reusable confirmation dialog rendered as an overlay.
///
/// Usage: store `Option<ConfirmDialog>` in your view state, render it
/// conditionally via `.when_some()`, and handle `on_confirm`/`on_cancel`
/// callbacks to clear the dialog state.
pub struct ConfirmDialog {
    title: SharedString,
    message: SharedString,
    confirm_label: SharedString,
    cancel_label: SharedString,
    destructive: bool,
    on_confirm: Handler,
    on_cancel: Handler,
}

#[allow(dead_code)]
impl ConfirmDialog {
    pub fn new(
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
        on_confirm: impl Fn(&mut Window, &mut App) + 'static,
        on_cancel: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            confirm_label: "Confirm".into(),
            cancel_label: "Cancel".into(),
            destructive: false,
            on_confirm: Rc::new(on_confirm),
            on_cancel: Rc::new(on_cancel),
        }
    }

    #[allow(dead_code)]
    pub fn confirm_label(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm_label = label.into();
        self
    }

    #[allow(dead_code)]
    pub fn cancel_label(mut self, label: impl Into<SharedString>) -> Self {
        self.cancel_label = label.into();
        self
    }

    #[allow(dead_code)]
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub fn render_element(self, _window: &mut Window, cx: &mut App) -> Stateful<Div> {
        let muted = cx.theme().muted_foreground;

        let backdrop_cancel = self.on_cancel.clone();
        let confirm_handler = self.on_confirm.clone();
        let cancel_handler = self.on_cancel;

        div()
            .id("confirm-dialog-overlay")
            .absolute()
            .size_full()
            .top_0()
            .left_0()
            .bg(theme::overlay_backdrop(cx))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, move |_ev, window, cx| {
                backdrop_cancel(window, cx);
            })
            .child(
                v_flex()
                    .on_mouse_down(MouseButton::Left, |_, _, _| {
                        // Stop propagation
                    })
                    .w(MODAL_MD_WIDTH)
                    .rounded(RADIUS_LG)
                    .bg(theme::popover_bg(cx))
                    .border_1()
                    .border_color(cx.theme().border)
                    .shadow_lg()
                    .p_5()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_BODY)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.title),
                            )
                            .child(
                                div()
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(muted)
                                    .child(self.message),
                            ),
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("dialog-cancel")
                                    .label(self.cancel_label)
                                    .compact()
                                    .small()
                                    .ghost()
                                    .on_click(move |_ev, window, cx| {
                                        cancel_handler(window, cx);
                                    }),
                            )
                            .child({
                                let btn = Button::new("dialog-confirm")
                                    .label(self.confirm_label)
                                    .compact()
                                    .small();
                                if self.destructive {
                                    btn.danger().on_click(move |_ev, window, cx| {
                                        confirm_handler(window, cx);
                                    })
                                } else {
                                    btn.primary().on_click(move |_ev, window, cx| {
                                        confirm_handler(window, cx);
                                    })
                                }
                            }),
                    ),
            )
    }
}
