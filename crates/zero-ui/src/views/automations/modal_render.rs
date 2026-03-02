use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    switch::Switch,
    v_flex, ActiveTheme, Sizable as _,
};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, MODAL_LG_WIDTH, RADIUS, RADIUS_LG};

use super::modal::AutomationModal;

impl Render for AutomationModal {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let fg = cx.theme().foreground;
        let is_edit = self.editing_id.is_some();
        let title = if is_edit {
            "Edit Automation"
        } else {
            "New Automation"
        };

        let error_el: Option<AnyElement> = self.error.clone().map(|err| {
            div()
                .text_size(FONT_SIZE_CAPTION)
                .text_color(cx.theme().danger)
                .child(SharedString::from(err))
                .into_any_element()
        });

        let name_input = self.name_input.clone();
        let name_field = Self::render_field("Name", &name_input, muted).into_any_element();
        let dest_input = self.dest_input.clone();
        let dest_field = Self::render_field("Destination", &dest_input, muted).into_any_element();
        let sources = self.render_sources(muted, cx).into_any_element();
        let triggers = self.render_triggers(muted, fg, cx).into_any_element();
        let options = self.render_options(muted, fg, cx).into_any_element();
        let buttons = self.render_buttons(is_edit, cx).into_any_element();

        div()
            .id("automation-modal-overlay")
            .track_focus(&self.focus_handle)
            .absolute()
            .size_full()
            .top_0()
            .left_0()
            .bg(theme::overlay_backdrop(cx))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.dismiss(cx)),
            )
            .child(
                v_flex()
                    .w(MODAL_LG_WIDTH)
                    .max_h(px(600.0))
                    .rounded(RADIUS_LG)
                    .bg(theme::popover_bg(cx))
                    .border_1()
                    .border_color(cx.theme().border)
                    .shadow_lg()
                    .p_6()
                    .gap_5()
                    .on_mouse_down(MouseButton::Left, |_, _, _| {})
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::BOLD)
                            .child(title),
                    )
                    .child(name_field)
                    .child(sources)
                    .child(dest_field)
                    .child(triggers)
                    .child(options)
                    .children(error_el)
                    .child(buttons),
            )
    }
}

impl AutomationModal {
    fn render_field(
        label: &str,
        input: &Entity<InputState>,
        muted: Hsla,
    ) -> impl IntoElement {
        v_flex()
            .gap_1()
            .child(
                div()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(muted)
                    .font_weight(FontWeight::MEDIUM)
                    .child(label.to_string()),
            )
            .child(Input::new(input).into_any_element())
    }

    pub(super) fn render_sources(
        &self,
        muted: Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let source_rows: Vec<AnyElement> = self
            .sources
            .iter()
            .enumerate()
            .map(|(idx, path)| {
                h_flex()
                    .id(SharedString::from(format!("src-{idx}")))
                    .px_3()
                    .py_2()
                    .rounded(px(6.0))
                    .bg(cx.theme().secondary)
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_ellipsis()
                            .child(SharedString::from(path.clone())),
                    )
                    .child(
                        Button::new(SharedString::from(format!("rm-src-{idx}")))
                            .ghost()
                            .compact()
                            .small()
                            .label("Remove")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_source(idx, cx);
                            })),
                    )
                    .into_any_element()
            })
            .collect();

        let has_sources = !source_rows.is_empty();

        v_flex()
            .gap_1()
            .child(
                div()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(muted)
                    .font_weight(FontWeight::MEDIUM)
                    .child("Source Folders"),
            )
            .when(has_sources, |el| {
                el.child(v_flex().gap_1().children(source_rows))
            })
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(&self.source_input).into_any_element()),
                    )
                    .child(
                        Button::new("add-source")
                            .compact()
                            .small()
                            .rounded(RADIUS)
                            .label("Add")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_source(window, cx);
                            })),
                    ),
            )
    }

    pub(super) fn render_triggers(
        &self,
        muted: Hsla,
        fg: Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(
                div()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(muted)
                    .font_weight(FontWeight::MEDIUM)
                    .child("Triggers"),
            )
            .child(
                v_flex()
                    .gap(px(2.0))
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_BODY)
                                    .text_color(fg)
                                    .child("Run on drive mount"),
                            )
                            .child(
                                Switch::new("trigger-mount")
                                    .checked(self.on_mount)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        this.on_mount = *checked;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .child("Automatically sync when this drive is connected"),
                    ),
            )
            .child(
                v_flex()
                    .gap(px(2.0))
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_BODY)
                                    .text_color(fg)
                                    .child("Run on file change"),
                            )
                            .child(
                                Switch::new("trigger-change")
                                    .checked(self.on_change)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        this.on_change = *checked;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .child("Sync when files in the source change"),
                    ),
            )
    }

    pub(super) fn render_options(
        &self,
        muted: Hsla,
        fg: Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(
                div()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(muted)
                    .font_weight(FontWeight::MEDIUM)
                    .child("Options"),
            )
            .child(
                v_flex()
                    .gap(px(2.0))
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_BODY)
                                    .text_color(fg)
                                    .child("Verify after copy"),
                            )
                            .child(
                                Switch::new("opt-verify")
                                    .checked(self.verify)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        this.verify = *checked;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .child("Verify file integrity after each copy"),
                    ),
            )
            .child(
                v_flex()
                    .gap(px(2.0))
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_BODY)
                                    .text_color(fg)
                                    .child("Delete orphan files"),
                            )
                            .child(
                                Switch::new("opt-delete-orphans")
                                    .checked(self.delete_orphans)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        this.delete_orphans = *checked;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .when(self.delete_orphans, |el| {
                        el.child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(cx.theme().warning)
                                .child("Files deleted from source will also be deleted from the drive"),
                        )
                    }),
            )
    }

    pub(super) fn render_buttons(
        &self,
        is_edit: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let saving = self.saving;
        let save_label = if saving {
            "Saving..."
        } else if is_edit {
            "Update"
        } else {
            "Create"
        };

        h_flex()
            .justify_between()
            .child(
                Button::new("cancel")
                    .label("Cancel")
                    .compact()
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
            )
            .child(
                Button::new("save")
                    .label(save_label)
                    .compact()
                    .small()
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            )
    }
}
