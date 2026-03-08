use std::sync::Arc;

use alacritty_terminal::vte::ansi::CursorShape as AlacCursorShape;
use gpui::*;
use gpui_component::ActiveTheme;

use super::grid::{GridLayout, GridParams, layout_grid};
use super::{Terminal, TerminalBounds};

// Terminal font configuration
const FONT_FAMILY: &str = "Lilex";
const FONT_SIZE_PX: f32 = 13.0;
const LINE_HEIGHT_RATIO: f32 = 1.3;
const FONT_FALLBACKS: &[&str] = &["Menlo", "SF Mono", "Apple Symbols"];

// -- Layout state passed from prepaint to paint ------------------------------

pub struct TerminalLayoutState {
    hitbox: Hitbox,
    grid: GridLayout,
    background_color: Hsla,
    dimensions: TerminalBounds,
    origin: Point<Pixels>,
}

// -- TerminalElement ---------------------------------------------------------

pub struct TerminalElement {
    terminal: Entity<Terminal>,
    focus_handle: FocusHandle,
}

impl TerminalElement {
    pub fn new(terminal: Entity<Terminal>, focus_handle: FocusHandle) -> Self {
        Self {
            terminal,
            focus_handle,
        }
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = TerminalLayoutState;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::Name("terminal-element".into()))
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            size: Size {
                width: relative(1.0).into(),
                height: relative(1.0).into(),
            },
            ..Default::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);

        let font_size = px(FONT_SIZE_PX);
        let font = Font {
            family: FONT_FAMILY.into(),
            features: FontFeatures::disable_ligatures(),
            fallbacks: Some(FontFallbacks(Arc::new(
                FONT_FALLBACKS.iter().map(|s| (*s).into()).collect(),
            ))),
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        };

        // Measure cell dimensions
        let text_system = window.text_system();
        let em_run = TextRun {
            len: 1,
            font: font.clone(),
            color: Hsla::default(),
            background_color: None,
            underline: Default::default(),
            strikethrough: Default::default(),
        };
        let shaped = text_system.shape_line("m".into(), font_size, &[em_run], None);
        let cell_width = shaped.width;
        let line_height = font_size * LINE_HEIGHT_RATIO;

        let new_bounds = TerminalBounds::new(line_height, cell_width, bounds);

        let params = GridParams {
            origin: bounds.origin,
            font,
            font_size,
            fg_default: cx.theme().foreground,
            bg_default: cx.theme().background,
        };

        // Read terminal content and update size
        let content = self.terminal.update(cx, |term, cx| {
            term.set_size(new_bounds);
            term.sync(cx);
            term.last_content.clone()
        });

        let grid = layout_grid(&content, &new_bounds, &params, cx);

        TerminalLayoutState {
            hitbox,
            grid,
            background_color: params.bg_default,
            dimensions: new_bounds,
            origin: params.origin,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Background fill
        window.paint_quad(fill(bounds, layout.background_color));

        // Cell background rects
        for bg in &layout.grid.bg_rects {
            window.paint_quad(fill(Bounds::new(bg.pos, bg.size), bg.color));
        }

        // Selection highlight
        let selection_color = cx.theme().primary.opacity(0.3);
        for (pos, size) in &layout.grid.selection_rects {
            window.paint_quad(fill(Bounds::new(*pos, *size), selection_color));
        }

        // Text runs
        let shaped_runs: Vec<_> = layout
            .grid
            .runs
            .iter()
            .map(|run| {
                let shaped = window.text_system().shape_line(
                    run.text.clone(),
                    run.font_size,
                    std::slice::from_ref(&run.style),
                    Some(run.cell_width),
                );
                (shaped, run.pos)
            })
            .collect();
        for (shaped, pos) in shaped_runs {
            let _ = shaped.paint(pos, layout.dimensions.line_height, window, cx);
        }

        // Cursor
        self.paint_cursor(layout, window, cx);

        // Mouse event handlers
        self.register_mouse_events(layout, window);
    }
}

// -- Paint helpers -----------------------------------------------------------

impl TerminalElement {
    fn paint_cursor(&self, layout: &TerminalLayoutState, window: &mut Window, cx: &mut App) {
        let focused = self.focus_handle.is_focused(window);
        let Some((pos, size, color)) = &layout.grid.cursor_rect else {
            return;
        };

        if !focused {
            window.paint_quad(PaintQuad {
                bounds: Bounds::new(*pos, *size),
                corner_radii: Corners::default(),
                background: transparent_black().into(),
                border_widths: Edges::all(px(1.0)),
                border_color: *color,
                border_style: BorderStyle::Solid,
            });
            return;
        }

        match layout.grid.cursor_shape {
            AlacCursorShape::Block => {
                window.paint_quad(fill(Bounds::new(*pos, *size), *color));
                if let Some((char_pos, text, run, font_size)) = &layout.grid.cursor_char {
                    let shaped = window.text_system().shape_line(
                        text.clone(),
                        *font_size,
                        std::slice::from_ref(run),
                        Some(layout.dimensions.cell_width),
                    );
                    let _ = shaped.paint(*char_pos, layout.dimensions.line_height, window, cx);
                }
            }
            AlacCursorShape::Beam => {
                let beam_size = Size {
                    width: px(2.0),
                    height: size.height,
                };
                window.paint_quad(fill(Bounds::new(*pos, beam_size), *color));
            }
            AlacCursorShape::Underline => {
                let underline_pos = Point::new(pos.x, pos.y + size.height - px(2.0));
                let underline_size = Size {
                    width: size.width,
                    height: px(2.0),
                };
                window.paint_quad(fill(Bounds::new(underline_pos, underline_size), *color));
            }
            _ => {
                window.paint_quad(PaintQuad {
                    bounds: Bounds::new(*pos, *size),
                    corner_radii: Corners::default(),
                    background: transparent_black().into(),
                    border_widths: Edges::all(px(1.0)),
                    border_color: *color,
                    border_style: BorderStyle::Solid,
                });
            }
        }
    }

    fn register_mouse_events(&self, layout: &TerminalLayoutState, window: &mut Window) {
        let terminal = self.terminal.clone();

        // Scroll
        window.on_mouse_event({
            let terminal = terminal.clone();
            let hitbox = layout.hitbox.clone();
            move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                    return;
                }
                let delta = match event.delta {
                    ScrollDelta::Pixels(p) => p.y,
                    ScrollDelta::Lines(l) => l.y * px(20.0),
                };
                terminal.update(cx, |term, cx| {
                    term.scroll_wheel(delta);
                    cx.notify();
                });
            }
        });

        // Mouse down
        window.on_mouse_event({
            let terminal = terminal.clone();
            let hitbox = layout.hitbox.clone();
            let origin = layout.origin;
            move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                    return;
                }
                let pos = event.position - origin;
                terminal.update(cx, |term, cx| {
                    term.mouse_down(event.button, pos, event.modifiers, event.click_count);
                    cx.notify();
                });
            }
        });

        // Mouse up
        window.on_mouse_event({
            let terminal = terminal.clone();
            let hitbox = layout.hitbox.clone();
            let origin = layout.origin;
            move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                    return;
                }
                let pos = event.position - origin;
                terminal.update(cx, |term, cx| {
                    term.mouse_up(event.button, pos, event.modifiers);
                    cx.notify();
                });
            }
        });

        // Mouse move (drag selection)
        window.on_mouse_event({
            let terminal = terminal.clone();
            let hitbox = layout.hitbox.clone();
            let origin = layout.origin;
            move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                    return;
                }
                let pos = event.position - origin;
                terminal.update(cx, |term, cx| {
                    term.mouse_move(pos, event.pressed_button, event.modifiers);
                    cx.notify();
                });
            }
        });
    }
}
