//! Custom macOS-style traffic light buttons (close / minimize / zoom).
//!
//! Rendered entirely in GPUI — no native Obj-C calls. The native traffic
//! lights are moved off-screen and these take their place, giving us full
//! control over inactive-window tinting.

use gpui::*;
use gpui_component::h_flex;

/// Diameter of each traffic-light dot.
const DOT_SIZE: Pixels = px(12.0);
/// Gap between dot edges.
const DOT_GAP: Pixels = px(8.0);

// Active colors (standard macOS)
const fn color_close() -> Rgba {
    Rgba {
        r: 1.0,
        g: 0.373,
        b: 0.341,
        a: 1.0,
    } // #FF5F57
}
const fn color_minimize() -> Rgba {
    Rgba {
        r: 1.0,
        g: 0.741,
        b: 0.180,
        a: 1.0,
    } // #FFBD2E
}
const fn color_zoom() -> Rgba {
    Rgba {
        r: 0.157,
        g: 0.784,
        b: 0.251,
        a: 1.0,
    } // #28C840
}

// Inactive color from design system (editor.line_number / scrollbar.thumb)
const fn color_inactive() -> Rgba {
    Rgba {
        r: 0.322,
        g: 0.322,
        b: 0.322,
        a: 1.0,
    } // #525252
}

/// Custom traffic light buttons rendered in the sidebar header area.
#[derive(IntoElement)]
pub struct TrafficLights;

impl TrafficLights {
    pub fn new() -> Self {
        Self
    }
}

impl RenderOnce for TrafficLights {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let active = window.is_window_active();

        h_flex()
            .items_center()
            .gap(DOT_GAP)
            .child(traffic_dot(
                "tl-close",
                active,
                color_close(),
                |window, _cx| {
                    window.remove_window();
                },
            ))
            .child(traffic_dot(
                "tl-minimize",
                active,
                color_minimize(),
                |window, _cx| {
                    window.minimize_window();
                },
            ))
            .child(traffic_dot(
                "tl-zoom",
                active,
                color_zoom(),
                |window, _cx| {
                    window.zoom_window();
                },
            ))
    }
}

fn traffic_dot(
    id: &'static str,
    active: bool,
    active_color: Rgba,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let color = if active {
        Hsla::from(active_color)
    } else {
        Hsla::from(color_inactive())
    };

    let hover_color = if active {
        Hsla::from(active_color).opacity(0.8)
    } else {
        Hsla::from(color_inactive()).opacity(0.8)
    };

    div()
        .id(id)
        .w(DOT_SIZE)
        .h(DOT_SIZE)
        .rounded(DOT_SIZE)
        .bg(color)
        .hover(move |s| s.bg(hover_color))
        .cursor_pointer()
        .on_click(move |_, window, cx| on_click(window, cx))
}
