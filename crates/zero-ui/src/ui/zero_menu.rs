use gpui::*;
use gpui_component::{ActiveTheme, h_flex, kbd::Kbd, menu::PopupMenu};

/// Extension trait for PopupMenu that renders menu items with correct column
/// alignment (label | shortcut). Uses the `ElementItem` rendering path
/// which has proper `flex_1()` layout instead of the broken `w_full()` in the
/// standard `Item` path.
pub trait ZeroMenuExt {
    /// Add a menu item with label, action, and auto-resolved shortcut.
    fn action_row(self, label: impl Into<SharedString>, action: Box<dyn Action>) -> Self;
}

impl ZeroMenuExt for PopupMenu {
    fn action_row(self, label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        let label: SharedString = label.into();
        let action_for_kbd = action.boxed_clone();
        self.menu_element(action, move |window, cx| {
            let kbd = Kbd::binding_for_action(action_for_kbd.as_ref(), None, window).map(|k| {
                k.p_0()
                    .flex_nowrap()
                    .border_0()
                    .bg(gpui::transparent_black())
                    .text_color(cx.theme().muted_foreground)
            });

            h_flex()
                .flex_1()
                .gap_3()
                .items_center()
                .justify_between()
                .child(label.clone())
                .children(kbd)
        })
    }
}
