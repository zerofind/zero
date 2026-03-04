use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    select::Select,
    switch::Switch,
    v_flex,
};

use crate::theme::{FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS, RADIUS_LG};
use crate::ui::{ConfirmDialog, format_number};

use super::SettingsView;
use super::helpers::{card_border, group_label, setting_row};

impl SettingsView {
    pub(super) fn render_general(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let border = card_border(cx);
        let muted = cx.theme().muted_foreground;
        let fg = cx.theme().foreground;
        let rebuilding = self.rebuilding;
        let rebuild_files = self.rebuild_files;

        let content = v_flex()
            .gap_6()
            // Appearance — mode + light/dark theme selectors
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Appearance", fg))
                    .child(
                        v_flex()
                            .rounded(RADIUS_LG)
                            .border_1()
                            .border_color(border)
                            .overflow_hidden()
                            .child(setting_row(
                                "Mode",
                                "Use light, dark, or follow the OS appearance.",
                                Select::new(&self.mode_select)
                                    .small()
                                    .menu_width(px(160.0))
                                    .into_any_element(),
                                muted,
                                fg,
                                border,
                                true,
                            ))
                            .child(setting_row(
                                "Light theme",
                                "Theme used in light mode.",
                                Select::new(&self.light_theme_select)
                                    .small()
                                    .menu_width(px(220.0))
                                    .into_any_element(),
                                muted,
                                fg,
                                border,
                                true,
                            ))
                            .child(setting_row(
                                "Dark theme",
                                "Theme used in dark mode.",
                                Select::new(&self.dark_theme_select)
                                    .small()
                                    .menu_width(px(220.0))
                                    .into_any_element(),
                                muted,
                                fg,
                                border,
                                false,
                            )),
                    ),
            )
            // Duplicate Finder
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Duplicate Finder", fg))
                    .child(
                        v_flex()
                            .rounded(RADIUS_LG)
                            .border_1()
                            .border_color(border)
                            .overflow_hidden()
                            .child(setting_row(
                                "Verify with checksums",
                                "Use cryptographic hashes to confirm duplicates. Slower but more accurate.",
                                Switch::new("dedup-checksums")
                                    .checked(self.settings.dedup_use_checksums)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        this.settings.dedup_use_checksums = *checked;
                                        this.settings.save();
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                                muted,
                                fg,
                                border,
                                false,
                            )),
                    ),
            )
            // Index
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Search Index", fg))
                    .child(
                        v_flex()
                            .rounded(RADIUS_LG)
                            .border_1()
                            .border_color(border)
                            .overflow_hidden()
                            .child(setting_row(
                                "Rebuild index",
                                "Re-scan all search roots and rebuild the index.",
                                Button::new("rebuild-index")
                                    .small()
                                    .rounded(RADIUS)
                                    .label(SharedString::from(if rebuilding {
                                        format!(
                                            "Rebuilding... {}",
                                            format_number(rebuild_files)
                                        )
                                    } else {
                                        "Rebuild".to_string()
                                    }))
                                    .disabled(rebuilding)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.rebuild_index(cx);
                                    }))
                                    .into_any_element(),
                                muted,
                                fg,
                                border,
                                true,
                            ))
                            .child(setting_row(
                                "Data directory",
                                "Settings, indexes, databases, and logs.",
                                div()
                                    .text_size(FONT_SIZE_BODY)
                                    .text_color(muted)
                                    .child(SharedString::from(
                                        zero::dirs::data_dir()
                                            .map(|d| d.to_string_lossy().to_string())
                                            .unwrap_or_else(|| "~/.zero".to_string()),
                                    ))
                                    .into_any_element(),
                                muted,
                                fg,
                                border,
                                false,
                            )),
                    ),
            )
            // Updates
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Updates", fg))
                    .child(
                        v_flex()
                            .rounded(RADIUS_LG)
                            .border_1()
                            .border_color(border)
                            .overflow_hidden()
                            .child(setting_row(
                                "Auto update",
                                "Check for new versions in the background and notify you.",
                                Switch::new("auto-update")
                                    .checked(self.settings.auto_update)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        this.settings.auto_update = *checked;
                                        this.settings.save();
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                                muted,
                                fg,
                                border,
                                false,
                            )),
                    ),
            )
            // Danger zone
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Danger Zone", fg))
                    .child(
                        v_flex()
                            .rounded(RADIUS_LG)
                            .border_1()
                            .border_color(border)
                            .overflow_hidden()
                            .child(setting_row(
                                "Clear index",
                                "Remove all indexed data. You'll need to re-index after.",
                                Button::new("clear-index")
                                    .danger()
                                    .small()
                                    .rounded(RADIUS)
                                    .label("Clear")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.request_clear_index(cx);
                                    }))
                                    .into_any_element(),
                                muted,
                                fg,
                                border,
                                true,
                            ))
                            .child(setting_row(
                                "Reset settings",
                                "Restore all settings to defaults.",
                                Button::new("reset-settings")
                                    .danger()
                                    .small()
                                    .rounded(RADIUS)
                                    .label("Reset")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.request_reset_settings(cx);
                                    }))
                                    .into_any_element(),
                                muted,
                                fg,
                                border,
                                false,
                            )),
                    ),
            )
            // Version info
            .child(
                div()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(muted)
                    .child(SharedString::from(format!(
                        "zero v{}",
                        zero::VERSION,
                    ))),
            );

        // Wrap in a relative container for confirmation dialogs
        let clear_dialog = if self.confirm_clear_index {
            let confirm_entity = cx.entity().clone();
            let cancel_entity = cx.entity().clone();
            Some(
                ConfirmDialog::new(
                    "Clear Search Index",
                    "Remove all indexed data? You will need to rebuild the index afterwards.",
                    move |_window, cx| {
                        confirm_entity.update(cx, |this, cx| this.confirm_clear_index(cx));
                    },
                    move |_window, cx| {
                        cancel_entity.update(cx, |this, cx| {
                            this.confirm_clear_index = false;
                            cx.notify();
                        });
                    },
                )
                .confirm_label("Clear Index")
                .destructive()
                .render_element(window, cx),
            )
        } else {
            None
        };

        let reset_dialog = if self.confirm_reset {
            let confirm_entity = cx.entity().clone();
            let cancel_entity = cx.entity().clone();
            Some(
                ConfirmDialog::new(
                    "Reset All Settings",
                    "Restore all settings to their defaults? This cannot be undone.",
                    move |_window, cx| {
                        confirm_entity.update(cx, |this, cx| this.confirm_reset_settings(cx));
                    },
                    move |_window, cx| {
                        cancel_entity.update(cx, |this, cx| {
                            this.confirm_reset = false;
                            cx.notify();
                        });
                    },
                )
                .confirm_label("Reset")
                .destructive()
                .render_element(window, cx),
            )
        } else {
            None
        };

        div()
            .relative()
            .w_full()
            .child(content)
            .when_some(clear_dialog, |el, d| el.child(d))
            .when_some(reset_dialog, |el, d| el.child(d))
    }
}
