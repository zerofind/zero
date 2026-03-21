use std::time::{Duration, Instant};

use gpui::*;
use gpui_component::{Icon, IconName, Sizable as _, h_flex, v_flex};

use crate::theme::{self, FONT_SIZE_CAPTION, ICON_SM, RADIUS};

/// Alert severity level.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum AlertLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// A single toast notification.
#[derive(Debug, Clone)]
pub struct Alert {
    pub level: AlertLevel,
    pub message: String,
    pub created: Instant,
    pub auto_dismiss_ms: u64,
}

impl Alert {
    pub fn new(level: AlertLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            created: Instant::now(),
            auto_dismiss_ms: 4000,
        }
    }

    #[allow(dead_code)]
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(AlertLevel::Info, message)
    }

    #[allow(dead_code)]
    pub fn success(message: impl Into<String>) -> Self {
        Self::new(AlertLevel::Success, message)
    }

    #[allow(dead_code)]
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(AlertLevel::Warning, message)
    }

    #[allow(dead_code)]
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(AlertLevel::Error, message)
    }

    pub fn is_expired(&self) -> bool {
        self.created.elapsed() > Duration::from_millis(self.auto_dismiss_ms)
    }

    fn icon(&self) -> IconName {
        match self.level {
            AlertLevel::Info => IconName::Info,
            AlertLevel::Success => IconName::Check,
            AlertLevel::Warning => IconName::TriangleAlert,
            AlertLevel::Error => IconName::CircleX,
        }
    }

    fn accent_color(&self, cx: &App) -> Hsla {
        match self.level {
            AlertLevel::Info => theme::alert_info(cx),
            AlertLevel::Success => theme::alert_success(cx),
            AlertLevel::Warning => theme::alert_warning(cx),
            AlertLevel::Error => theme::alert_error(cx),
        }
    }
}

/// Renders a stack of toast alerts in the bottom-right.
pub struct AlertStack;

impl AlertStack {
    pub fn render(alerts: &[Alert], cx: &App) -> impl IntoElement {
        v_flex()
            .absolute()
            .bottom_4()
            .right_4()
            .gap_2()
            .w(theme::MODAL_SM_WIDTH)
            .children(alerts.iter().enumerate().map(|(i, alert)| {
                h_flex()
                    .id(SharedString::from(format!("alert-{i}")))
                    .w_full()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .items_center()
                    .rounded(RADIUS)
                    .border_1()
                    .border_color(alert.accent_color(cx).opacity(0.3))
                    .bg(theme::toast_bg(cx))
                    .shadow_md()
                    .child(
                        Icon::new(alert.icon())
                            .with_size(ICON_SM)
                            .text_color(alert.accent_color(cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_ellipsis()
                            .child(SharedString::from(alert.message.clone())),
                    )
            }))
    }
}
