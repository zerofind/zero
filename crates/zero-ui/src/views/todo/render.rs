use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, InteractiveElementExt as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    v_flex,
};

use zero::prelude::{Task, TodoManager};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS, brand_color};
use crate::ui::{ConfirmDialog, EmptyState};

pub struct TodoView {
    pub(super) manager: Option<TodoManager>,
    pub(super) tasks: Vec<Task>,
    pub(super) lists: Vec<String>,
    pub(super) current_list: String,
    pub(super) show_completed: bool,
    pub(super) input: Entity<InputState>,
    pub(super) editing_task: Option<u64>,
    pub(super) edit_input: Option<Entity<InputState>>,
    pub(super) pending_delete: Option<u64>,
    pub(super) error: Option<String>,
    pub(super) detail_task: Option<u64>,
    pub(super) detail_due_input: Option<Entity<InputState>>,
    pub(super) detail_tag_input: Option<Entity<InputState>>,
    pub(super) detail_assignee_input: Option<Entity<InputState>>,
    pub(super) focus_handle: FocusHandle,
}

impl TodoView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Add a task..."));

        let mut view = Self {
            manager: None,
            tasks: Vec::new(),
            lists: Vec::new(),
            current_list: "All".to_string(),
            show_completed: false,
            input,
            editing_task: None,
            edit_input: None,
            pending_delete: None,
            error: None,
            detail_task: None,
            detail_due_input: None,
            detail_tag_input: None,
            detail_assignee_input: None,
            focus_handle: cx.focus_handle(),
        };

        view.load_tasks();
        view
    }

    fn render_list_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        let mut tabs: Vec<String> = vec!["All".to_string()];
        tabs.extend(self.lists.clone());

        h_flex()
            .gap_1()
            .flex_wrap()
            .children(tabs.into_iter().map(|list_name| {
                let active = self.current_list == list_name;
                let name = list_name.clone();
                Button::new(SharedString::from(format!("tab-{}", list_name)))
                    .label(SharedString::from(list_name))
                    .compact()
                    .xsmall()
                    .when(active, |b| b.primary())
                    .when(!active, |b| b.ghost().text_color(muted))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_current_list(name.clone(), cx);
                    }))
            }))
    }

    fn render_task_row(&self, task: &Task, muted: Hsla, cx: &mut Context<Self>) -> AnyElement {
        let is_done = task.status.is_closed();
        let task_id = task.id;
        let is_editing = self.editing_task == Some(task_id);

        // Inline edit mode
        if is_editing && let Some(input) = &self.edit_input {
            return h_flex()
                .id(SharedString::from(format!("task-edit-{task_id}")))
                .w_full()
                .px_3()
                .py_2()
                .gap_2()
                .items_center()
                .rounded(RADIUS)
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    if ev.keystroke.key == "enter" {
                        this.confirm_edit(window, cx);
                    } else if ev.keystroke.key == "escape" {
                        this.cancel_edit(cx);
                    }
                }))
                .child(Icon::new(IconName::Minus).xsmall().text_color(muted))
                .child(div().flex_1().child(Input::new(input)))
                .into_any_element();
        }

        let detail_open = self.detail_task == Some(task_id);

        h_flex()
            .id(SharedString::from(format!("task-{task_id}")))
            .w_full()
            .px_3()
            .py_2()
            .gap_2()
            .items_center()
            .rounded(RADIUS)
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface_hover(cx)))
            .when(detail_open, |el| el.bg(theme::surface_hover(cx)))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.toggle_detail(task_id, window, cx);
            }))
            // Status icon (click to toggle completion)
            .child(
                div()
                    .id(SharedString::from(format!("status-{task_id}")))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_task(task_id, cx);
                    }))
                    .child(
                        Icon::new(if is_done {
                            IconName::CircleCheck
                        } else {
                            IconName::Minus
                        })
                        .xsmall()
                        .text_color(if is_done {
                            theme::success_color()
                        } else {
                            muted
                        }),
                    ),
            )
            // Task text (double-click to edit)
            .child(
                div()
                    .id(SharedString::from(format!("text-{task_id}")))
                    .flex_1()
                    .min_w_0()
                    .text_size(FONT_SIZE_BODY)
                    .when(is_done, |el| el.text_color(muted).line_through())
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .on_double_click(cx.listener(move |this, _, window, cx| {
                        this.start_editing(task_id, window, cx);
                    }))
                    .child(SharedString::from(task.text.clone())),
            )
            // Tags
            .children(task.tags.iter().map(|tag| {
                div()
                    .px_1()
                    .rounded(px(3.0))
                    .bg(theme::surface_hover(cx))
                    .text_size(px(10.0))
                    .text_color(muted)
                    .child(SharedString::from(tag.clone()))
            }))
            // Due date pill
            .when_some(task.due, |el, _due| {
                let pill_color = if task.is_overdue() {
                    cx.theme().danger
                } else if task.is_due_today() {
                    brand_color()
                } else {
                    muted
                };
                let label = if task.is_overdue() {
                    "Overdue"
                } else if task.is_due_today() {
                    "Today"
                } else {
                    "Upcoming"
                };
                el.child(
                    div()
                        .px_1()
                        .rounded(px(3.0))
                        .text_size(px(10.0))
                        .text_color(pill_color)
                        .child(label),
                )
            })
            // List badge
            .child(
                div()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(muted)
                    .child(SharedString::from(task.list.clone())),
            )
            // Delete button
            .child(
                div()
                    .id(SharedString::from(format!("del-{task_id}")))
                    .cursor_pointer()
                    .opacity(0.0)
                    .hover(|s| s.opacity(1.0))
                    .group_hover("", |s| s.opacity(0.5))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pending_delete = Some(task_id);
                        cx.notify();
                    }))
                    .child(Icon::new(IconName::Close).xsmall().text_color(muted)),
            )
            .into_any_element()
    }

    fn render_detail_panel(
        &self,
        task: &Task,
        muted: Hsla,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let task_id = task.id;
        let task_list = task.list.clone();

        // Due date display
        let due_str = task.due.map(format_timestamp_as_date);

        v_flex()
            .id(SharedString::from(format!("detail-{task_id}")))
            .mx_3()
            .mb_2()
            .px_3()
            .py_2()
            .rounded(RADIUS)
            .bg(theme::surface_hover(cx))
            .border_1()
            .border_color(cx.theme().border)
            .gap_2()
            // Due date row
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .min_w(px(60.0))
                            .child("Due"),
                    )
                    .when_some(self.detail_due_input.as_ref(), |el, input| {
                        el.child(div().flex_1().child(Input::new(input))).child(
                            Button::new("set-due")
                                .label("Set")
                                .compact()
                                .xsmall()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.set_due(task_id, window, cx);
                                })),
                        )
                    })
                    .when(self.detail_due_input.is_none(), |el| {
                        el.child(div().text_size(FONT_SIZE_CAPTION).text_color(muted).child(
                            SharedString::from(
                                due_str.clone().unwrap_or_else(|| "None".to_string()),
                            ),
                        ))
                    })
                    .when(task.due.is_some(), |el| {
                        el.child(
                            Button::new("clear-due")
                                .label("Clear")
                                .compact()
                                .xsmall()
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.clear_due(task_id, cx);
                                })),
                        )
                    }),
            )
            // Tags row
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .flex_wrap()
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .min_w(px(60.0))
                            .child("Tags"),
                    )
                    .children(task.tags.iter().map(|tag| {
                        let tag_clone = tag.clone();
                        h_flex()
                            .gap_0p5()
                            .items_center()
                            .px_1()
                            .rounded(px(3.0))
                            .bg(theme::surface_hover(cx))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(muted)
                                    .child(SharedString::from(tag.clone())),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("rm-tag-{tag}")))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_tag(task_id, &tag_clone, cx);
                                    }))
                                    .child(
                                        Icon::new(IconName::Close)
                                            .with_size(px(8.0))
                                            .text_color(muted),
                                    ),
                            )
                    }))
                    .when_some(self.detail_tag_input.as_ref(), |el, input| {
                        el.child(div().w(px(100.0)).child(Input::new(input))).child(
                            Button::new("add-tag")
                                .label("Add")
                                .compact()
                                .xsmall()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.add_tag_from_input(task_id, window, cx);
                                })),
                        )
                    }),
            )
            // Assignee row
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .min_w(px(60.0))
                            .child("Assignee"),
                    )
                    .when_some(self.detail_assignee_input.as_ref(), |el, input| {
                        el.child(div().flex_1().child(Input::new(input))).child(
                            Button::new("set-assignee")
                                .label("Set")
                                .compact()
                                .xsmall()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.set_assignee(task_id, window, cx);
                                })),
                        )
                    })
                    .when(self.detail_assignee_input.is_none(), |el| {
                        el.child(div().text_size(FONT_SIZE_CAPTION).text_color(muted).child(
                            SharedString::from(
                                task.assigned.as_deref().unwrap_or("Unassigned").to_string(),
                            ),
                        ))
                    })
                    .when(task.assigned.is_some(), |el| {
                        el.child(
                            Button::new("clear-assignee")
                                .label("Clear")
                                .compact()
                                .xsmall()
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.clear_assignee(task_id, cx);
                                })),
                        )
                    }),
            )
            // List + move row
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .min_w(px(60.0))
                            .child("List"),
                    )
                    .children(self.lists.iter().map(|list| {
                        let is_current = *list == task_list;
                        let list_name = list.clone();
                        Button::new(SharedString::from(format!("move-to-{list}")))
                            .label(SharedString::from(list.clone()))
                            .compact()
                            .xsmall()
                            .when(is_current, |b| b.primary())
                            .when(!is_current, |b| b.ghost())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.move_to_list(task_id, &list_name, cx);
                            }))
                    })),
            )
            // Move up/down buttons
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .min_w(px(60.0))
                            .child("Order"),
                    )
                    .child(
                        Button::new("move-up")
                            .label("Move Up")
                            .compact()
                            .xsmall()
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.move_task_up(task_id, cx);
                            })),
                    )
                    .child(
                        Button::new("move-down")
                            .label("Move Down")
                            .compact()
                            .xsmall()
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.move_task_down(task_id, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}

impl Render for TodoView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let filtered = self.filtered_tasks();
        let has_tasks = !filtered.is_empty();

        // Build task elements before the builder chain to avoid borrow issues
        let task_elements: Vec<AnyElement> = if has_tasks {
            filtered
                .iter()
                .flat_map(|task| {
                    let task_id = task.id;
                    let row = self.render_task_row(task, muted, cx);
                    let detail = if self.detail_task == Some(task_id) {
                        Some(self.render_detail_panel(task, muted, window, cx))
                    } else {
                        None
                    };
                    std::iter::once(row).chain(detail)
                })
                .collect()
        } else {
            Vec::new()
        };

        // Build confirm dialog overlay for pending delete
        let delete_dialog = self.pending_delete.map(|task_id| {
            let task_text = self
                .tasks
                .iter()
                .find(|t| t.id == task_id)
                .map(|t| t.text.clone())
                .unwrap_or_else(|| format!("Task #{task_id}"));

            let confirm_entity = cx.entity().clone();
            let cancel_entity = cx.entity().clone();

            ConfirmDialog::new(
                "Delete Task",
                format!("Delete \"{}\"? This cannot be undone.", task_text),
                move |_window, cx| {
                    confirm_entity.update(cx, |this, cx| this.delete_task(task_id, cx));
                },
                move |_window, cx| {
                    cancel_entity.update(cx, |this, cx| {
                        this.pending_delete = None;
                        cx.notify();
                    });
                },
            )
            .confirm_label("Delete")
            .destructive()
            .render_element(window, cx)
        });

        let total_count = self.tasks.len();
        let done_count = self.tasks.iter().filter(|t| t.status.is_closed()).count();

        div()
            .relative()
            .size_full()
            .child(
                v_flex()
                    .size_full()
                    .bg(theme::content_bg(cx))
                    .p_4()
                    .gap_3()
                    .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                        if ev.keystroke.key == "enter" {
                            let text = this.input.read(cx).value().to_string();
                            if !text.trim().is_empty() {
                                this.add_task(&text, window, cx);
                            }
                        }
                    }))
                    // Header
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(FONT_SIZE_BODY)
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Tasks"),
                                    )
                                    .child(
                                        div().text_size(FONT_SIZE_CAPTION).text_color(muted).child(
                                            SharedString::from(format!(
                                                "{done_count}/{total_count} done"
                                            )),
                                        ),
                                    ),
                            )
                            .child(
                                Button::new("toggle-completed")
                                    .label(if self.show_completed {
                                        "Hide completed"
                                    } else {
                                        "Show completed"
                                    })
                                    .compact()
                                    .xsmall()
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_show_completed(cx);
                                    })),
                            ),
                    )
                    // List tabs
                    .when(!self.lists.is_empty(), |el| {
                        el.child(self.render_list_tabs(cx))
                    })
                    // Content
                    .when(!has_tasks && self.error.is_none(), |el| {
                        el.child(
                            EmptyState::new(IconName::Check, "No tasks yet")
                                .subtitle("Type below to add your first task"),
                        )
                    })
                    .when_some(self.error.clone(), |el, err| {
                        el.child(EmptyState::new(IconName::Check, "Tasks").subtitle(err))
                    })
                    .when(has_tasks, |el| {
                        el.child(
                            v_flex()
                                .id("todo-list")
                                .flex_1()
                                .overflow_y_scroll()
                                .gap_0p5()
                                .children(task_elements),
                        )
                    })
                    // Add task input
                    .child(
                        div()
                            .px_1()
                            .pt_2()
                            .border_t_1()
                            .border_color(cx.theme().border)
                            .child(Input::new(&self.input)),
                    ),
            )
            // Delete confirm dialog
            .when_some(delete_dialog, |el, dialog| el.child(dialog))
    }
}

/// Format a Unix timestamp as "YYYY-MM-DD".
fn format_timestamp_as_date(ts: i64) -> String {
    const SECS_PER_DAY: i64 = 86_400;
    let days = ts / SECS_PER_DAY;

    let mut year = 1970i64;
    let mut remaining = days;

    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let month_days = [
        31,
        28 + i64::from(is_leap(year)),
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for &md in &month_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        month += 1;
    }
    let day = remaining + 1;

    format!("{year:04}-{month:02}-{day:02}")
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
