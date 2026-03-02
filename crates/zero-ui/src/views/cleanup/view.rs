use std::collections::HashSet;
use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, IconName, Sizable as _, button::Button, checkbox::Checkbox,
    h_flex, v_flex,
};

use crate::services::SearchService;
use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS};
use crate::ui::{ConfirmDialog, EmptyState, format_size};

use super::detail::render_detail_view;
use super::{CategoryFileItem, CategoryResult};

pub struct CleanupView {
    pub(super) search: Entity<SearchService>,
    pub(super) categories: Vec<CategoryResult>,
    pub(super) selected: HashSet<usize>,
    pub(super) scanning: bool,
    pub(super) cleaning: bool,
    pub(super) scan_complete: bool,
    pub(super) scan_status: Option<String>,
    pub(super) confirm_clean: bool,
    // Drill-in state
    pub(super) detail_category: Option<usize>,
    pub(super) detail_selected: HashSet<usize>,
    pub(super) confirm_detail_clean: bool,
    pub(super) detail_focused: Option<usize>,
    pub(super) focus_handle: FocusHandle,
}

impl CleanupView {
    pub fn new(search: Entity<SearchService>, cx: &mut Context<Self>) -> Self {
        Self {
            search,
            categories: Vec::new(),
            selected: HashSet::new(),
            scanning: false,
            cleaning: false,
            scan_complete: false,
            scan_status: None,
            confirm_clean: false,
            detail_category: None,
            detail_selected: HashSet::new(),
            confirm_detail_clean: false,
            detail_focused: None,
            focus_handle: cx.focus_handle(),
        }
    }

    pub(super) fn start_scan(&mut self, cx: &mut Context<Self>) {
        self.scanning = true;
        self.scan_complete = false;
        self.scan_status = Some("Analyzing categories...".to_string());
        self.categories.clear();
        self.selected.clear();
        self.detail_category = None;
        self.detail_selected.clear();
        cx.notify();

        let search = self.search.clone();

        cx.spawn(async move |this, cx| {
            // Clone manager on main thread (fast snapshot)
            let manager_clone = this
                .update(cx, |_, cx| search.read(cx).clone_manager())
                .ok();
            let Some(manager_clone) = manager_clone else {
                return;
            };

            // Scan on background thread (heavy I/O)
            let result = cx
                .background_executor()
                .spawn(async move {
                    let Ok(summary) = zero::prelude::execute_full_cleanup_scan(&manager_clone)
                    else {
                        return Vec::new();
                    };
                    summary
                        .groups
                        .into_iter()
                        .flat_map(|g| {
                            let group_name = format!("{:?}", g.group);
                            g.categories.into_iter().map(move |cat| {
                                let files: Vec<CategoryFileItem> = cat
                                    .items
                                    .iter()
                                    .map(|item| CategoryFileItem {
                                        path: PathBuf::from(&item.node.path),
                                        name: item.node.name.clone(),
                                        size: item.node.size,
                                    })
                                    .collect();
                                CategoryResult {
                                    name: cat.category_name,
                                    group: group_name.clone(),
                                    file_count: cat.count,
                                    total_bytes: cat.total_bytes,
                                    files,
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .await;

            // Apply results on main thread (fast)
            this.update(cx, |view, cx| {
                view.categories = result;
                view.scanning = false;
                view.scan_complete = true;
                view.scan_status = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn toggle_category(&mut self, idx: usize, cx: &mut Context<Self>) {
        if self.selected.contains(&idx) {
            self.selected.remove(&idx);
        } else {
            self.selected.insert(idx);
        }
        cx.notify();
    }

    fn selected_bytes(&self) -> u64 {
        self.selected
            .iter()
            .filter_map(|&i| self.categories.get(i))
            .map(|c| c.total_bytes)
            .sum()
    }

    fn selected_file_count(&self) -> usize {
        self.selected
            .iter()
            .filter_map(|&i| self.categories.get(i))
            .map(|c| c.file_count)
            .sum()
    }

    fn clean_selected(&mut self, cx: &mut Context<Self>) {
        if self.selected.is_empty() || self.cleaning {
            return;
        }

        self.cleaning = true;
        cx.notify();

        let paths: Vec<PathBuf> = self
            .selected
            .iter()
            .filter_map(|&i| self.categories.get(i))
            .flat_map(|c| c.files.iter().map(|f| f.path.clone()))
            .collect();

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .spawn(async move {
                    for path in &paths {
                        let _ = trash::delete(path);
                    }
                })
                .await;

            this.update(cx, |view, cx| {
                view.cleaning = false;
                view.start_scan(cx);
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn drill_into(&mut self, idx: usize, cx: &mut Context<Self>) {
        self.detail_category = Some(idx);
        self.detail_selected.clear();
        self.confirm_detail_clean = false;
        self.detail_focused = None;
        cx.notify();
    }

    pub(super) fn back_from_detail(&mut self, cx: &mut Context<Self>) {
        self.detail_category = None;
        self.detail_selected.clear();
        self.confirm_detail_clean = false;
        cx.notify();
    }

    pub(super) fn toggle_detail_file(&mut self, idx: usize, cx: &mut Context<Self>) {
        if self.detail_selected.contains(&idx) {
            self.detail_selected.remove(&idx);
        } else {
            self.detail_selected.insert(idx);
        }
        cx.notify();
    }

    pub(super) fn toggle_detail_select_all(&mut self, cx: &mut Context<Self>) {
        let Some(cat_idx) = self.detail_category else {
            return;
        };
        let Some(cat) = self.categories.get(cat_idx) else {
            return;
        };
        if self.detail_selected.len() == cat.files.len() {
            self.detail_selected.clear();
        } else {
            self.detail_selected = (0..cat.files.len()).collect();
        }
        cx.notify();
    }

    pub(super) fn clean_detail_selected(&mut self, cx: &mut Context<Self>) {
        let Some(cat_idx) = self.detail_category else {
            return;
        };
        let Some(cat) = self.categories.get(cat_idx) else {
            return;
        };
        if self.detail_selected.is_empty() || self.cleaning {
            return;
        }

        self.cleaning = true;
        cx.notify();

        let paths: Vec<PathBuf> = self
            .detail_selected
            .iter()
            .filter_map(|&i| cat.files.get(i))
            .map(|f| f.path.clone())
            .collect();

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .spawn(async move {
                    for path in &paths {
                        let _ = trash::delete(path);
                    }
                })
                .await;

            this.update(cx, |view, cx| {
                view.cleaning = false;
                view.detail_category = None;
                view.detail_selected.clear();
                view.start_scan(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Move focus up in the detail view.
    pub(super) fn detail_focus_prev(&mut self, cx: &mut Context<Self>) {
        let count = self.detail_file_count();
        if count == 0 {
            return;
        }
        self.detail_focused = Some(match self.detail_focused {
            Some(0) | None => 0,
            Some(i) => i - 1,
        });
        cx.notify();
    }

    /// Move focus down in the detail view.
    pub(super) fn detail_focus_next(&mut self, cx: &mut Context<Self>) {
        let count = self.detail_file_count();
        if count == 0 {
            return;
        }
        let max = count.saturating_sub(1);
        self.detail_focused = Some(match self.detail_focused {
            None => 0,
            Some(i) if i >= max => max,
            Some(i) => i + 1,
        });
        cx.notify();
    }

    /// Toggle selection of the focused item.
    pub(super) fn detail_toggle_focused(&mut self, cx: &mut Context<Self>) {
        if let Some(idx) = self.detail_focused {
            self.toggle_detail_file(idx, cx);
        }
    }

    /// Open the focused item (context-aware).
    pub(super) fn detail_open_focused(&mut self, _cx: &mut Context<Self>) {
        let Some(focused) = self.detail_focused else {
            return;
        };
        let Some(cat_idx) = self.detail_category else {
            return;
        };
        let Some(cat) = self.categories.get(cat_idx) else {
            return;
        };
        let Some(file) = cat.files.get(focused) else {
            return;
        };
        let path = &file.path;

        if path.extension().map(|e| e == "todo").unwrap_or(false) {
            // .todo files: would emit event to open in TodoView
            // For now, OS-open them
            #[cfg(target_os = "macos")]
            crate::platform::open::open_path(path);
        } else if crate::views::editor::is_editable(path) {
            #[cfg(target_os = "macos")]
            crate::platform::open::open_path(path);
        } else if path.is_dir() {
            // Directories: OS-open
            #[cfg(target_os = "macos")]
            crate::platform::open::open_path(path);
        } else {
            #[cfg(target_os = "macos")]
            crate::platform::open::open_path(path);
        }
    }

    /// Reveal the focused item in Finder.
    #[allow(dead_code)]
    pub(super) fn detail_reveal_focused(&mut self, _cx: &mut Context<Self>) {
        let Some(focused) = self.detail_focused else {
            return;
        };
        let Some(cat_idx) = self.detail_category else {
            return;
        };
        let Some(cat) = self.categories.get(cat_idx) else {
            return;
        };
        let Some(file) = cat.files.get(focused) else {
            return;
        };
        #[cfg(target_os = "macos")]
        crate::platform::open::reveal_in_finder(&file.path);
    }

    /// Reveal a specific item by index in Finder.
    pub(super) fn detail_reveal_item(&self, idx: usize) {
        let Some(cat_idx) = self.detail_category else {
            return;
        };
        let Some(cat) = self.categories.get(cat_idx) else {
            return;
        };
        let Some(file) = cat.files.get(idx) else {
            return;
        };
        #[cfg(target_os = "macos")]
        crate::platform::open::reveal_in_finder(&file.path);
    }

    fn detail_file_count(&self) -> usize {
        self.detail_category
            .and_then(|idx| self.categories.get(idx))
            .map(|c| c.files.len())
            .unwrap_or(0)
    }

    pub(super) fn detail_selected_bytes(&self) -> u64 {
        let Some(cat_idx) = self.detail_category else {
            return 0;
        };
        let Some(cat) = self.categories.get(cat_idx) else {
            return 0;
        };
        self.detail_selected
            .iter()
            .filter_map(|&i| cat.files.get(i))
            .map(|f| f.size)
            .sum()
    }
}

impl Render for CleanupView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Drill-in detail view
        if self.detail_category.is_some() {
            return render_detail_view(self, window, cx).into_any_element();
        }

        let muted = cx.theme().muted_foreground;

        let confirm_dialog = if self.confirm_clean {
            let file_count = self.selected_file_count();
            let bytes = self.selected_bytes();
            let confirm_entity = cx.entity().clone();
            let cancel_entity = cx.entity().clone();

            Some(
                ConfirmDialog::new(
                    "Clean Selected Files",
                    format!(
                        "Move {} files ({}) to Trash? This cannot be undone.",
                        file_count,
                        format_size(bytes),
                    ),
                    move |_window, cx| {
                        confirm_entity.update(cx, |this, cx| {
                            this.confirm_clean = false;
                            this.clean_selected(cx);
                        });
                    },
                    move |_window, cx| {
                        cancel_entity.update(cx, |this, cx| {
                            this.confirm_clean = false;
                            cx.notify();
                        });
                    },
                )
                .confirm_label("Clean")
                .destructive()
                .render_element(window, cx),
            )
        } else {
            None
        };

        div()
            .relative()
            .size_full()
            .child(
                v_flex()
                    .size_full()
                    .bg(theme::content_bg(cx))
                    .p_4()
                    .gap_3()
                    // Header
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_BODY)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Storage Cleanup"),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .when(self.cleaning, |el| {
                                        el.child(
                                            Button::new("clean-btn")
                                                .label("Cleaning...")
                                                .compact()
                                                .small()
                                                .disabled(true),
                                        )
                                    })
                                    .when(!self.selected.is_empty() && !self.cleaning, |el| {
                                        el.child(
                                            Button::new("clean-btn")
                                                .label(SharedString::from(format!(
                                                    "Clean {} ({} files)",
                                                    format_size(self.selected_bytes()),
                                                    self.selected_file_count(),
                                                )))
                                                .compact()
                                                .small()
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.confirm_clean = true;
                                                    cx.notify();
                                                })),
                                        )
                                    })
                                    .child(
                                        Button::new("scan-btn")
                                            .label(if self.scanning {
                                                "Scanning..."
                                            } else {
                                                "Scan"
                                            })
                                            .compact()
                                            .small()
                                            .disabled(self.scanning || self.cleaning)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.start_scan(cx);
                                            })),
                                    ),
                            ),
                    )
                    // Content
                    .when(!self.scan_complete && !self.scanning, |el| {
                        el.child(
                            EmptyState::new(IconName::Delete, "Run a scan to find cleanable files")
                                .subtitle(
                                    "Analyzes categories like caches, logs, and temporary files",
                                ),
                        )
                    })
                    .when(self.scanning, |el| {
                        let status = self
                            .scan_status
                            .clone()
                            .unwrap_or_else(|| "Scanning...".to_string());
                        el.child(
                            EmptyState::new(IconName::Search, SharedString::from(status))
                                .subtitle("Analyzing your files by category"),
                        )
                    })
                    .when(self.scan_complete && !self.scanning, |el| {
                        let total_bytes: u64 = self.categories.iter().map(|c| c.total_bytes).sum();
                        let total_files: usize = self.categories.iter().map(|c| c.file_count).sum();

                        el.child(div().text_size(FONT_SIZE_CAPTION).text_color(muted).child(
                            SharedString::from(format!(
                                "Found {} in {} files across {} categories",
                                format_size(total_bytes),
                                total_files,
                                self.categories.len(),
                            )),
                        ))
                        .child(
                            v_flex()
                                .id("cleanup-list")
                                .flex_1()
                                .overflow_y_scroll()
                                .gap_0p5()
                                .children(self.categories.iter().enumerate().map(|(i, cat)| {
                                    let checked = self.selected.contains(&i);
                                    h_flex()
                                        .id(SharedString::from(format!("cat-{i}")))
                                        .w_full()
                                        .px_3()
                                        .py_2()
                                        .gap_3()
                                        .items_center()
                                        .rounded(RADIUS)
                                        .cursor_pointer()
                                        .hover(|s| s.bg(theme::surface_hover(cx)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.toggle_category(i, cx);
                                        }))
                                        .child(
                                            Checkbox::new(SharedString::from(format!("chk-{i}")))
                                                .checked(checked),
                                        )
                                        .child(
                                            div()
                                                .id(SharedString::from(format!("cat-name-{i}")))
                                                .flex_1()
                                                .min_w_0()
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.drill_into(i, cx);
                                                }))
                                                .child(
                                                    div().text_size(FONT_SIZE_BODY).child(
                                                        SharedString::from(cat.name.clone()),
                                                    ),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FONT_SIZE_CAPTION)
                                                        .text_color(muted)
                                                        .child(SharedString::from(
                                                            cat.group.clone(),
                                                        )),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .text_size(FONT_SIZE_CAPTION)
                                                .text_color(muted)
                                                .child(SharedString::from(format!(
                                                    "{} files",
                                                    cat.file_count
                                                ))),
                                        )
                                        .child(
                                            div()
                                                .text_size(FONT_SIZE_BODY)
                                                .min_w(px(80.0))
                                                .text_right()
                                                .child(SharedString::from(format_size(
                                                    cat.total_bytes,
                                                ))),
                                        )
                                })),
                        )
                    }),
            )
            .when_some(confirm_dialog, |el, dialog| el.child(dialog))
            .into_any_element()
    }
}
