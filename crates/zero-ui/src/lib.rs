mod actions;
mod app;
mod models;
mod permissions;
mod platform;
mod services;
mod session;
pub mod theme;
pub mod ui;
mod views;

use std::borrow::Cow;

use gpui::*;
use gpui_component::Root;

use actions::{
    ClearTerminal, CopyFiles, CopyPath, CutFiles, DuplicateFiles, FindInBrowser, GoBack, GoForward,
    GoUp, MoveToTrash, NewFolder, OpenCommandPalette, OpenSelected, OpenSettings, PasteFiles,
    QuickLook, Quit, Refresh, Rename, SelectAll, ToggleSidebar, ToggleSplitView, ToggleTerminal,
    ToggleToolbar, ToggleViewMode,
};
use app::ZeroApp;

/// Launch the GPUI-based file manager window.
pub fn launch() {
    // First caller wins — when launched via `zero` CLI, its init takes priority.
    zero::logging::init("zero_ui=info,alacritty_terminal=warn,vte=warn,warn");

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        platform = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        "zero-app starting"
    );

    let app = Application::new().with_assets(gpui_component_assets::Assets);

    app.run(move |cx| {
        gpui_component::init(cx);
        load_terminal_fonts(cx);

        theme::init_zero_theme(cx);
        theme::watch_user_themes(cx);

        cx.bind_keys([
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-q", Quit, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("alt-f4", Quit, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-b", ToggleSidebar, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-b", ToggleSidebar, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-,", OpenSettings, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-,", OpenSettings, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-k", OpenCommandPalette, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-k", OpenCommandPalette, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-[", GoBack, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-]", GoForward, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-left", GoBack, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-right", GoForward, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("alt-left", GoBack, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("alt-right", GoForward, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-up", GoUp, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("alt-up", GoUp, None),
            // Toolbar toggle
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-/", ToggleToolbar, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-/", ToggleToolbar, None),
            // View mode toggle
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-2", ToggleViewMode, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-2", ToggleViewMode, None),
            // File browser actions
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-down", OpenSelected, None),
            KeyBinding::new("enter", OpenSelected, None),
            KeyBinding::new("space", QuickLook, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-backspace", MoveToTrash, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("delete", MoveToTrash, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-shift-c", CopyPath, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-shift-c", CopyPath, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-r", Refresh, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("f5", Refresh, None),
            // Rename
            KeyBinding::new("f2", Rename, None),
            // New Folder
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-shift-n", NewFolder, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-shift-n", NewFolder, None),
            // File clipboard — scoped to DataTableView so text inputs keep Cmd+C/V/X
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-c", CopyFiles, Some("DataTableView")),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-c", CopyFiles, Some("DataTableView")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-x", CutFiles, Some("DataTableView")),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-x", CutFiles, Some("DataTableView")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-v", PasteFiles, Some("DataTableView")),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-v", PasteFiles, Some("DataTableView")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-d", DuplicateFiles, Some("DataTableView")),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-d", DuplicateFiles, Some("DataTableView")),
            // Split view
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-\\", ToggleSplitView, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-\\", ToggleSplitView, None),
            // Find in browser
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-f", FindInBrowser, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-f", FindInBrowser, None),
            // Selection
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-a", SelectAll, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-a", SelectAll, None),
            // Terminal toggle
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-`", ToggleTerminal, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-`", ToggleTerminal, None),
            // Clear terminal (only active when terminal is focused)
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-k", ClearTerminal, Some("Terminal")),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-k", ClearTerminal, Some("Terminal")),
        ]);

        cx.on_action(|_: &Quit, cx| cx.quit());

        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        cx.spawn(async move |cx| {
            let window_bounds = cx.update(|cx| {
                let mut s = size(px(1200.0), px(800.0));
                if let Some(display) = cx.primary_display() {
                    let ds = display.bounds().size;
                    s.width = s.width.min(ds.width * 0.85);
                    s.height = s.height.min(ds.height * 0.85);
                }
                Bounds::centered(None, s, cx)
            });

            let window = cx.open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: None,
                        appears_transparent: true,
                        // Native traffic lights moved off-screen; custom ones
                        // rendered in sidebar for proper inactive-window tinting.
                        traffic_light_position: Some(point(px(-1000.0), px(-1000.0))),
                    }),
                    window_bounds: Some(WindowBounds::Windowed(window_bounds?)),
                    ..Default::default()
                },
                |window, cx: &mut App| {
                    let view = cx.new(ZeroApp::new);
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )?;

            // Bring app to foreground — required for CLI-launched GUI on macOS
            cx.update(|cx| cx.activate(true))?;
            window.update(cx, |_, window, _| window.activate_window())?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}

/// Load bundled Lilex font (has powerline glyphs for terminal prompts).
fn load_terminal_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Lilex-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Lilex-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Lilex-Italic.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Lilex-BoldItalic.ttf")),
    ];
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        tracing::warn!("Failed to load terminal fonts: {e}");
    }
}
