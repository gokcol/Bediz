//! Ribbon visibility control, floating inspector, and resizable pinned panel.
use super::{
    EditorState,
    inspector_widgets::{self, Glyph},
};
use crate::document::{Document, EditHistory};
use eframe::egui::{self, CursorIcon, Id, Key, Pos2, Rect, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};

pub(super) const TOGGLE_WIDTH: f32 = 34.;
pub(super) const OVERLAY_ID: &str = "inspector-overlay";
const MIN_WIDTH: f32 = 260.;
const MAX_WIDTH: f32 = 560.;
const PANEL_MARGIN: i8 = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    #[default]
    Styles,
    Layers,
    Presets,
    Effects,
}

impl Panel {
    const ALL: [Self; 4] = [Self::Styles, Self::Layers, Self::Presets, Self::Effects];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Styles => "Styles",
            Self::Layers => "Layers",
            Self::Presets => "Presets",
            Self::Effects => "Quick Effects",
        }
    }

    fn glyph(self) -> Glyph {
        use crate::appearance::Icon;
        match self {
            Self::Styles => Glyph::App(Icon::Settings),
            Self::Layers => Glyph::Stack,
            Self::Presets => Glyph::App(Icon::Favorite),
            Self::Effects => Glyph::App(Icon::Effects),
        }
    }
}

/// Workspace preferences are separate from document edits and tool appearances.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub panel: Panel,
    pub pinned: bool,
    pub expanded: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct State {
    pub preferences: Preferences,
    changed: bool,
}

impl State {
    pub fn is(&self, panel: Panel) -> bool {
        self.preferences.panel == panel
    }

    pub fn select(&mut self, panel: Panel) {
        self.changed |= self.preferences.panel != panel;
        self.preferences.panel = panel;
    }

    pub fn show(&mut self, panel: Panel) {
        self.select(panel);
        self.changed |= !self.preferences.expanded;
        self.preferences.expanded = true;
    }

    pub fn close(&mut self) {
        self.changed |= self.preferences.expanded;
        self.preferences.expanded = false;
    }

    pub fn toggle_pin(&mut self) {
        self.preferences.pinned = !self.preferences.pinned;
        self.changed = true;
    }

    #[cfg(test)]
    pub(super) fn pinned(panel: Panel) -> Self {
        Self {
            preferences: Preferences {
                panel,
                expanded: true,
                pinned: true,
            },
            changed: false,
        }
    }
}

impl EditorState {
    pub(super) fn show_inspector_panel(
        &mut self,
        panel: Panel,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        match panel {
            Panel::Effects => self.open_image_effects(document, history),
            Panel::Presets => {
                self.cancel_image_tools();
                self.inspector.show(panel);
            }
            _ => self.inspector.show(panel),
        }
    }

    pub fn set_inspector_preferences(&mut self, preferences: Preferences) {
        self.inspector = State {
            preferences,
            changed: false,
        };
        self.inspector.select(Panel::Styles);
    }

    pub fn take_inspector_preferences_change(&mut self) -> Option<Preferences> {
        std::mem::take(&mut self.inspector.changed).then_some(self.inspector.preferences)
    }
}

pub(super) fn toggle_button(ui: &mut egui::Ui, expanded: bool) -> egui::Response {
    use crate::appearance::{self, TileTone};

    let (rect, response) = ui.allocate_exact_size(Vec2::new(TOGGLE_WIDTH, 38.), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            ui.is_enabled(),
            expanded,
            "Properties",
        )
    });
    let tone = if expanded {
        TileTone::Blue
    } else {
        TileTone::Slate
    };
    let ink = appearance::satin_button(
        ui,
        rect,
        &response,
        expanded,
        appearance::satin_ink(ui.ctx(), tone),
    );
    let window = Rect::from_center_size(rect.center(), Vec2::new(21., 15.));
    let divider = window.right() - 7.;
    let painter = ui.painter();
    if expanded {
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(divider, window.top() + 1.),
                window.right_bottom() - Vec2::splat(1.),
            ),
            1.,
            ink.gamma_multiply(0.28),
        );
    }
    let stroke = Stroke::new(1.5, ink);
    painter.rect_stroke(window, 3., stroke, egui::StrokeKind::Inside);
    painter.line_segment(
        [
            egui::pos2(divider, window.top() + 1.),
            egui::pos2(divider, window.bottom() - 1.),
        ],
        stroke,
    );
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(if expanded {
            "Hide properties panel"
        } else {
            "Show properties panel"
        })
}

pub(super) enum HeaderAction {
    Close,
    TogglePin,
    Show(Panel),
}

pub(super) fn header(ui: &mut egui::Ui, preferences: Preferences) -> Option<HeaderAction> {
    use crate::appearance::{self, TileTone};

    let mut action = None;
    ui.horizontal(|ui| {
        let gap = 4.;
        let height = 24.;
        ui.spacing_mut().item_spacing.x = gap;
        for panel in Panel::ALL {
            let selected = preferences.panel == panel;
            let tab = ui.add_sized([28., height], egui::Button::new("").frame(false));
            tab.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    ui.is_enabled(),
                    selected,
                    format!("Show {} panel", panel.label()),
                )
            });
            let tone = match panel {
                Panel::Styles => TileTone::Slate,
                Panel::Layers => TileTone::Blue,
                Panel::Presets => TileTone::Clay,
                Panel::Effects => TileTone::Violet,
            };
            let ink = appearance::satin_button(
                ui,
                tab.rect,
                &tab,
                selected,
                appearance::satin_ink(ui.ctx(), tone),
            );
            inspector_widgets::paint_glyph(
                ui.painter(),
                tab.rect.center(),
                panel.glyph(),
                ink,
                17.,
            );
            if tab
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(panel.label())
                .clicked()
                && !selected
            {
                action = Some(HeaderAction::Show(panel));
            }
        }
        let action_size = Vec2::splat(height);
        let spacer = ui.available_width() - 2. * action_size.x - gap;
        if preferences.pinned || spacer <= gap {
            ui.add_space(spacer.max(0.));
        } else {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(spacer - gap, height), Sense::hover());
            let grip = ui.interact(rect, Id::new(OVERLAY_ID).with("grip"), Sense::drag());
            grip.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Other,
                    ui.is_enabled(),
                    "Move properties panel",
                )
            });
            let palette = appearance::palette(ui.ctx());
            let ink = if grip.hovered() || grip.dragged() {
                palette.accent
            } else {
                palette.muted
            };
            for y in [-2.5, 2.5] {
                for x in [-5., 0., 5.] {
                    ui.painter()
                        .circle_filled(rect.center() + Vec2::new(x, y), 1.25, ink);
                }
            }
            let cursor = if grip.dragged() {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            };
            grip.on_hover_and_drag_cursor(cursor)
                .on_hover_text("Drag to move properties panel");
        }
        let ink = appearance::palette(ui.ctx()).ink;
        let (pin_icon, pin_label, pin_hint) = if preferences.pinned {
            (
                appearance::Icon::Unpin,
                "Unpin panel",
                "Unpin panel and drag it anywhere on the canvas",
            )
        } else {
            (
                appearance::Icon::Pin,
                "Pin panel",
                "Pin panel to dock beside the canvas",
            )
        };
        let pin = ui.add_sized(
            action_size,
            egui::Button::new("").selected(preferences.pinned),
        );
        pin.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                ui.is_enabled(),
                preferences.pinned,
                pin_label,
            )
        });
        appearance::icon(ui.painter(), pin.rect.center(), pin_icon, ink, 16.);
        if pin
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(pin_hint)
            .clicked()
        {
            action = Some(HeaderAction::TogglePin);
        }

        let close = ui.add_sized(action_size, egui::Button::new(""));
        close.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                ui.is_enabled(),
                "Close properties",
            )
        });
        appearance::icon(
            ui.painter(),
            close.rect.center(),
            appearance::Icon::Close,
            ink,
            16.,
        );
        if close
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text("Close properties")
            .clicked()
        {
            action = Some(HeaderAction::Close);
        }
    });
    ui.separator();
    action
}

pub(super) fn overlay_placement(ui: &egui::Ui) -> (Rect, bool) {
    let workspace = ui.available_rect_before_wrap();
    let ctx = ui.ctx();
    let id = Id::new(OVERLAY_ID);
    let position_id = id.with("position");
    let (width, offset) = ctx.data(|data| {
        let width = data
            .get_temp::<WidthPreference>(Id::new("annotation-inspector-width"))
            .unwrap_or_default()
            .0
            .unwrap_or(MIN_WIDTH);
        (width, data.get_temp::<Vec2>(position_id))
    });
    let inset = (workspace.size().min_elem() * 0.1).clamp(0., 10.);
    let bounds = workspace.shrink(inset);
    let size = Vec2::new(
        width
            .clamp(MIN_WIDTH, MAX_WIDTH)
            .min(bounds.width().max(0.)),
        (bounds.height() * 0.85).clamp(0., 560.),
    );
    let travel = (bounds.size() - size).max(Vec2::ZERO);
    let offset = offset
        .unwrap_or(Vec2::new(travel.x, 0.))
        .clamp(Vec2::ZERO, travel);
    let mut position = offset;
    let mut dragging = false;
    // Read the grip before laying out the floating panel and hit-testing the canvas.
    if ui.is_enabled()
        && !egui::Popup::is_any_open(ctx)
        && let Some(grip) = ctx
            .read_response(id.with("grip"))
            .filter(egui::Response::enabled)
    {
        let delta = if let Some((start, end)) = completed_drag(ui, grip.rect)
            .filter(|(start, _)| ctx.layer_id_at(*start) == Some(grip.layer_id))
        {
            Some(end - start)
        } else if grip.drag_started() {
            Some(ui.input(|i| {
                i.pointer
                    .press_origin()
                    .zip(i.pointer.interact_pos())
                    .map_or(Vec2::ZERO, |(start, pos)| pos - start)
            }))
        } else if grip.dragged() || grip.drag_stopped() {
            Some(ui.input(|i| i.pointer.delta()))
        } else {
            None
        };
        if let Some(delta) = delta {
            dragging = true;
            if delta != Vec2::ZERO {
                position = (offset + delta).clamp(Vec2::ZERO, travel);
                ctx.data_mut(|data| data.insert_temp(position_id, position));
            }
        }
    }
    (Rect::from_min_size(bounds.min + position, size), dragging)
}

pub(super) fn overlay<R>(ui: &egui::Ui, rect: Rect, contents: impl FnOnce(&mut egui::Ui) -> R) {
    let palette = crate::appearance::palette(ui.ctx());
    let enabled = ui.is_enabled();
    let opacity = ui.opacity();
    egui::Area::new(Id::new(OVERLAY_ID))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .default_size(rect.size())
        .movable(false)
        .sense(Sense::hover())
        .constrain_to(rect)
        .show(ui.ctx(), |ui| {
            if !enabled {
                ui.disable();
            }
            ui.set_opacity(opacity);
            ui.set_clip_rect(rect);
            let frame = egui::Frame::new()
                .fill(palette.paper)
                .corner_radius(10)
                .stroke(Stroke::new(1., palette.border))
                .inner_margin(PANEL_MARGIN);
            let size = (rect.size() - frame.total_margin().sum()).max(Vec2::ZERO);
            frame.show(ui, |ui| {
                ui.set_min_size(size);
                ui.set_max_size(size);
                contents(ui);
            });
        });
}

#[derive(Clone, Copy, Default)]
struct WidthPreference(Option<f32>);

fn completed_drag(ui: &egui::Ui, rect: Rect) -> Option<(Pos2, Pos2)> {
    // A native press, motion and release can all arrive between two repaints.
    ui.input(|i| {
        let mut origin = None;
        let mut completed = None;
        for event in &i.events {
            if let egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                ..
            } = event
            {
                if *pressed {
                    origin = rect.contains(*pos).then_some(*pos);
                } else if let Some(start) = origin.take()
                    && pos.distance(start) > 3.
                {
                    completed = Some((start, *pos));
                }
            }
        }
        completed
    })
}

pub(super) fn dock<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let id = Id::new("annotation-inspector-width");
    let divider_id = id.with("divider");
    let workspace = ui.available_rect_before_wrap();
    // Leave a useful canvas even in a small window or at a large UI scale.
    let max = (workspace.width() - 240.).clamp(MIN_WIDTH, MAX_WIDTH);
    let min = MIN_WIDTH;
    let fit = min;
    let mut preference = ui
        .ctx()
        .data(|d| d.get_temp::<WidthPreference>(id))
        .unwrap_or_default();

    // Consume this frame's interaction before laying out either side, so
    // the canvas and inspector move together without a frame of overlap.
    if let Some(response) = ui
        .ctx()
        .read_response(divider_id)
        .filter(|r| r.enabled() && ui.is_enabled())
    {
        if !egui::Popup::is_any_open(ui.ctx())
            && let Some((_, pointer)) = completed_drag(ui, response.rect)
        {
            preference.0 = Some((workspace.right() - pointer.x).clamp(min, max));
            response.request_focus();
        } else if response.double_clicked() {
            preference.0 = None;
        } else if (response.dragged() || response.drag_stopped())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            preference.0 = Some((workspace.right() - pointer.x).clamp(min, max));
        }
        if response.has_focus() {
            for (key, direction) in [(Key::ArrowLeft, 1.), (Key::ArrowRight, -1.)] {
                let step = ui.input_mut(|i| {
                    if i.consume_key(egui::Modifiers::SHIFT, key) {
                        40.
                    } else if i.consume_key(egui::Modifiers::NONE, key) {
                        10.
                    } else {
                        0.
                    }
                });
                if step != 0. {
                    preference.0 =
                        Some((preference.0.unwrap_or(fit) + direction * step).clamp(min, max));
                }
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Home)) {
                preference.0 = None;
            }
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, preference));
    let width = preference.0.unwrap_or(fit).clamp(min, max);
    let panel = egui::SidePanel::right("annotation-inspector")
        .resizable(false) // The divider below also supports reset and keyboard input.
        .exact_width(width)
        .show_separator_line(false)
        .frame(
            egui::Frame::side_top_panel(ui.style())
                .fill(crate::appearance::palette(ui.ctx()).paper)
                .inner_margin(PANEL_MARGIN),
        )
        .show_inside(ui, contents);

    let edge = panel.response.rect.left();
    let grab = Rect::from_min_max(
        Pos2::new(edge - 5., workspace.top()),
        Pos2::new(edge + 5., workspace.bottom()),
    );
    let response = ui
        .interact(grab, divider_id, Sense::click_and_drag())
        .on_hover_cursor(CursorIcon::ResizeHorizontal)
        .on_hover_text("Drag to resize · Double-click to fit\nArrow keys resize · Home resets");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            "Resize properties panel; double-click to fit",
        )
    });
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if response.has_focus() {
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                divider_id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    ..Default::default()
                },
            )
        });
    }
    let palette = crate::appearance::palette(ui.ctx());
    let active = response.hovered() || response.dragged() || response.has_focus();
    ui.painter().line_segment(
        [
            Pos2::new(edge, workspace.top()),
            Pos2::new(edge, workspace.bottom()),
        ],
        Stroke::new(
            if active { 2. } else { 1. },
            if active {
                palette.accent
            } else {
                palette.border
            },
        ),
    );
    // A subtle grip makes resizing discoverable without adding a toolbar control.
    ui.painter().rect_filled(
        Rect::from_center_size(Pos2::new(edge, workspace.center().y), Vec2::new(3., 28.)),
        2.,
        if active {
            palette.accent
        } else {
            palette.border
        },
    );
    panel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::{AnnotationKind, Document, EditHistory},
        editor::{EditorState, Tool},
    };
    use egui::Color32;

    #[test]
    fn divider_resizes_and_resets_without_drawing_on_the_canvas() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let texture = ctx.load_texture(
            "inspector-test",
            egui::ColorImage::filled([1, 1], Color32::WHITE),
            Default::default(),
        );
        let mut editor = EditorState {
            tool: Tool::Pen,
            inspector: State::pinned(Panel::Styles),
            ..Default::default()
        };
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(900, 600)),
            annotations: vec![],
            geometry: Default::default(),
        };
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        let mut preview = Default::default();
        let mut time = 0.;
        let mut frame = |events, size: Vec2| {
            time += 0.05;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        editor.ui(ui, &mut document, &mut history, &texture, &mut preview);
                    });
                },
            );
            egui::containers::panel::PanelState::load(&ctx, Id::new("annotation-inspector"))
                .unwrap()
                .rect
        };
        let size = Vec2::new(1100., 720.);
        for _ in 0..3 {
            frame(vec![], size);
        }
        let fitted = frame(vec![], size);
        let at = fitted.left_center();
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(at), button(at, true)], size);
        let to = at - Vec2::new(140., 0.);
        frame(vec![egui::Event::PointerMoved(to)], size);
        frame(vec![], size);
        frame(vec![button(to, false)], size);
        let wider = frame(vec![], size);
        assert!(
            (wider.width() - fitted.width() - 140.).abs() < 2.,
            "{fitted:?} -> {wider:?}"
        );
        // A smaller window constrains the width; enlarging it restores the user's choice.
        let small = frame(vec![], Vec2::new(700., 720.));
        assert!(small.left() >= 240.);
        assert!((frame(vec![], size).width() - wider.width()).abs() < 1.);
        let at = wider.left_center();
        for _ in 0..2 {
            frame(vec![egui::Event::PointerMoved(at), button(at, true)], size);
            frame(vec![button(at, false)], size);
        }
        let reset = frame(vec![], size);
        assert!(
            (reset.width() - fitted.width()).abs() < 1.,
            "{reset:?} vs {fitted:?}"
        );
        // Rapid input may arrive between repaints, including the final release.
        let at = reset.left_center();
        let to = at - Vec2::new(80., 0.);
        frame(
            vec![
                egui::Event::PointerMoved(at),
                button(at, true),
                egui::Event::PointerMoved(to),
                button(to, false),
            ],
            size,
        );
        let rapid = frame(vec![], size);
        assert!((rapid.width() - reset.width() - 80.).abs() < 2.);
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let keyboard = frame(vec![key(Key::ArrowLeft, egui::Modifiers::SHIFT)], size);
        assert!((keyboard.width() - rapid.width() - 40.).abs() < 2.);
        let keyboard = frame(vec![key(Key::ArrowLeft, egui::Modifiers::NONE)], size);
        assert!((keyboard.width() - rapid.width() - 50.).abs() < 2.);
        let home = frame(vec![key(Key::Home, egui::Modifiers::NONE)], size);
        assert!((home.width() - fitted.width()).abs() < 1.);
        assert_eq!(document.edit_state(), before);
        assert!(!history.can_undo());
        assert!(editor.gesture.is_none());
    }
    struct Workspace {
        ctx: egui::Context,
        editor: EditorState,
        document: Document,
        history: EditHistory,
        texture: egui::TextureHandle,
        preview: crate::privacy_preview::PrivacyPreview,
        time: f64,
        size: Vec2,
        image_controls: bool,
    }

    impl Workspace {
        fn new() -> Self {
            let ctx = egui::Context::default();
            crate::appearance::apply(&ctx);
            ctx.enable_accesskit();
            let texture = ctx.load_texture(
                "inspector-source",
                egui::ColorImage::filled([1, 1], Color32::WHITE),
                Default::default(),
            );
            let document = Document {
                image: std::sync::Arc::new(image::RgbaImage::new(1200, 800)),
                annotations: vec![],
                geometry: Default::default(),
            };
            let history = EditHistory::new(document.edit_state());
            let mut this = Self {
                ctx,
                editor: EditorState::default(),
                document,
                history,
                texture,
                preview: Default::default(),
                time: 0.,
                size: Vec2::new(1100., 740.),
                image_controls: false,
            };
            this.editor.activate_tool(Tool::Arrow);
            this.settle();
            this
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
            self.time += 0.03;
            self.ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                    time: Some(self.time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::TopBottomPanel::top("test-ribbon").show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            self.editor
                                .toolbar(ui, &mut self.document, &mut self.history)
                        });
                    });
                    if self.image_controls {
                        egui::TopBottomPanel::bottom("test-image-controls").show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                self.editor.image_controls(
                                    ui,
                                    &mut self.document,
                                    &mut self.history,
                                )
                            });
                        });
                    }
                    egui::CentralPanel::default().show(ctx, |ui| {
                        self.editor.ui(
                            ui,
                            &mut self.document,
                            &mut self.history,
                            &self.texture,
                            &mut self.preview,
                        );
                    });
                },
            )
        }

        fn settle(&mut self) -> egui::FullOutput {
            self.frame(vec![]);
            self.frame(vec![]);
            self.frame(vec![])
        }

        fn click(&mut self, label: &str) {
            let output = self.settle();
            let nodes = &output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes;
            let at = nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.label() == Some(label))
                        .then(|| node.bounds())
                        .flatten()
                        .map(|r| {
                            egui::pos2(((r.x0 + r.x1) * 0.5) as f32, ((r.y0 + r.y1) * 0.5) as f32)
                        })
                })
                .unwrap_or_else(|| panic!("Missing control {label}"));
            for pressed in [true, false] {
                self.frame(vec![egui::Event::PointerMoved(at), pointer(at, pressed)]);
            }
            self.settle();
        }

        fn right_click(&mut self, label: &str) {
            let output = self.settle();
            let bounds = output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, n)| (n.label() == Some(label)).then(|| n.bounds()).flatten())
                .unwrap();
            let at = egui::pos2(
                ((bounds.x0 + bounds.x1) * 0.5) as f32,
                ((bounds.y0 + bounds.y1) * 0.5) as f32,
            );
            for pressed in [true, false] {
                self.frame(vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        pressed,
                        button: egui::PointerButton::Secondary,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            self.settle();
        }

        fn choose_panel(&mut self, panel: Panel) {
            if !self.editor.inspector.preferences.expanded {
                self.click("Properties");
            }
            self.click(&format!("Show {} panel", panel.label()));
        }

        fn source_quad(&mut self) -> Vec<(Pos2, Pos2)> {
            let output = self.settle();
            self.ctx
                .tessellate(output.shapes, output.pixels_per_point)
                .into_iter()
                .filter_map(|primitive| match primitive.primitive {
                    egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == self.texture.id() => {
                        Some(
                            mesh.vertices
                                .into_iter()
                                .map(|v| (v.pos, v.uv))
                                .collect::<Vec<_>>(),
                        )
                    }
                    _ => None,
                })
                .flatten()
                .collect()
        }
    }

    fn pointer(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn ribbon_panels_pin_and_unpin_without_changing_artwork_or_overlay_canvas_geometry() {
        let mut workspace = Workspace::new();
        let before = workspace.document.edit_state();
        let original = workspace.source_quad();
        assert_eq!(original.len(), 4);
        let zoom = workspace.editor.zoom;
        assert!(
            egui::containers::panel::PanelState::load(&workspace.ctx, Id::new("inspector-rail"))
                .is_none()
        );
        assert!(!workspace.editor.inspector.preferences.expanded);
        for panel in Panel::ALL {
            workspace.choose_panel(panel);
            assert!(workspace.editor.inspector.preferences.expanded);
            assert!(workspace.editor.inspector.is(panel));
            assert_eq!(
                workspace.source_quad(),
                original,
                "{} moved/rescaled the canvas",
                panel.label()
            );
            assert_eq!(workspace.editor.zoom, zoom);
        }
        workspace.click("Pin panel");
        assert!(workspace.editor.inspector.preferences.pinned);
        assert_ne!(workspace.source_quad(), original);
        workspace.click("Close properties");
        assert!(!workspace.editor.inspector.preferences.expanded);
        assert_eq!(workspace.source_quad(), original);
        workspace.click("Properties");
        assert_eq!(
            workspace.editor.inspector.preferences,
            Preferences {
                panel: Panel::Effects,
                pinned: true,
                expanded: true,
            }
        );
        workspace.click("Unpin panel");
        assert!(!workspace.editor.inspector.preferences.pinned);
        assert_eq!(workspace.source_quad(), original);
        workspace.click("Properties");
        assert!(!workspace.editor.inspector.preferences.expanded);
        assert!(workspace.editor.inspector.is(Panel::Effects));
        assert_eq!(workspace.source_quad(), original);
        assert_eq!(workspace.document.edit_state(), before);
        assert!(!workspace.history.can_undo());
    }

    #[test]
    fn inspector_header_switches_panels_in_one_row_when_floating_or_docked() {
        let mut workspace = Workspace::new();
        let before = workspace.document.edit_state();
        workspace.click("Properties");
        for pinned in [false, true] {
            let pin = if pinned { "Unpin panel" } else { "Pin panel" };
            let labels = Panel::ALL.map(|panel| format!("Show {} panel", panel.label()));
            let output = workspace.settle();
            let nodes = output.platform_output.accesskit_update.unwrap().nodes;
            let bounds = |label: &str| {
                nodes
                    .iter()
                    .find_map(|(_, node)| {
                        (node.label() == Some(label))
                            .then(|| node.bounds())
                            .flatten()
                    })
                    .unwrap_or_else(|| panic!("Missing header control {label}"))
            };
            let row = labels
                .iter()
                .map(|label| bounds(label))
                .chain([bounds(pin), bounds("Close properties")])
                .collect::<Vec<_>>();
            assert!(row.windows(2).all(|pair| pair[0].x1 <= pair[1].x0));
            let row_center = row[0].y0 + row[0].y1;
            assert!(
                row.iter()
                    .all(|rect| (rect.y0 + rect.y1 - row_center).abs() < 2.),
                "Header controls should share a vertical center: {row:?}"
            );

            let canvas = workspace.source_quad();
            for (panel, label) in Panel::ALL.into_iter().zip(labels) {
                workspace.click(&label);
                assert_eq!(
                    workspace.editor.inspector.preferences,
                    Preferences {
                        panel,
                        pinned,
                        expanded: true
                    }
                );
                assert_eq!(workspace.source_quad(), canvas);
            }
            if !pinned {
                workspace.click("Pin panel");
            }
        }
        assert_eq!(workspace.document.edit_state(), before);
        assert!(!workspace.history.can_undo());
    }

    #[test]
    fn overlay_controls_and_fast_pen_input_do_not_reach_the_image_underneath() {
        let mut workspace = Workspace::new();
        workspace.click("Properties");
        let output = workspace.settle();
        let mut swatches: Vec<_> = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, n)| {
                n.label()
                    .filter(|label| label.ends_with("quick style"))
                    .and_then(|_| n.bounds())
            })
            .collect();
        swatches.sort_by(|a, b| a.y0.total_cmp(&b.y0).then_with(|| a.x0.total_cmp(&b.x0)));
        assert_eq!(swatches.len(), 4);
        assert!(
            swatches[..4]
                .iter()
                .all(|r| (r.y0 - swatches[0].y0).abs() < 1.)
        );
        assert!(swatches.iter().all(|r| (r.y1 - r.y0 - 36.).abs() < 1.));
        let overlay = workspace
            .ctx
            .memory(|m| m.area_rect(Id::new("inspector-overlay")))
            .unwrap();
        assert!(
            swatches
                .iter()
                .all(|r| r.x0 >= f64::from(overlay.left()) && r.x1 <= f64::from(overlay.right()))
        );
        let before = workspace.document.edit_state();
        workspace.click("Arrow · Blue quick style");
        assert_eq!(workspace.editor.color, [40, 117, 219, 255]);
        assert_eq!(workspace.editor.stroke_width, 4.);
        assert_eq!(workspace.document.edit_state(), before);
        workspace.editor.activate_tool(Tool::Pen);
        workspace.settle();
        let overlay = workspace
            .ctx
            .memory(|memory| memory.area_rect(Id::new("inspector-overlay")))
            .unwrap();
        let start = overlay.min + Vec2::new(16., 16.);
        let end = start + Vec2::new(70., 0.);
        workspace.frame(vec![
            egui::Event::PointerMoved(start),
            pointer(start, true),
            egui::Event::PointerMoved(end),
            pointer(end, false),
        ]);
        assert_eq!(workspace.document.edit_state(), before);
        assert!(workspace.editor.gesture.is_none());
        assert!(!workspace.history.can_undo());
        workspace.editor.area = Some(Rect::from_min_size(egui::pos2(20., 20.), Vec2::splat(60.)));
        workspace.frame(vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(!workspace.editor.inspector.preferences.expanded);
        assert!(
            workspace.editor.area.is_some(),
            "closing properties must not also deselect pixels"
        );
    }

    #[test]
    fn quick_style_tabs_save_apply_update_and_clear_without_resizing_the_canvas() {
        let mut workspace = Workspace::new();
        workspace.click("Properties");
        let image = workspace.source_quad();
        let artwork = workspace.document.edit_state();
        workspace.click("Quick styles 2");
        workspace.click("Arrow · Green quick style");
        let green = workspace.editor.color;
        let output = workspace.settle();
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        assert_eq!(
            nodes
                .iter()
                .filter(|(_, n)| n.label().is_some_and(|l| l.ends_with("quick style")))
                .count(),
            4
        );
        assert!(
            !nodes
                .iter()
                .any(|(_, n)| n.label() == Some("Arrow · Red quick style"))
        );
        workspace.click("Quick styles 3");
        for slot in 1..=4 {
            workspace.click(&format!("Save current Arrow style {slot}"));
        }
        workspace.click("Quick styles 1");
        workspace.click("Arrow · Red quick style");
        let red = workspace.editor.color;
        workspace.click("Quick styles 3");
        workspace.click("Apply custom Arrow style 1");
        assert_eq!(workspace.editor.color, green);
        workspace.click("Quick styles 1");
        workspace.click("Arrow · Red quick style");
        workspace.click("Quick styles 3");
        workspace.right_click("Apply custom Arrow style 1");
        workspace.click("Update from current style");
        workspace.click("Apply custom Arrow style 2");
        assert_eq!(workspace.editor.color, green);
        workspace.click("Apply custom Arrow style 1");
        assert_eq!(workspace.editor.color, red);
        workspace.right_click("Apply custom Arrow style 1");
        workspace.click("Clear saved style");
        let output = workspace.settle();
        assert!(
            output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .any(|(_, n)| n.label() == Some("Save current Arrow style 1"))
        );
        assert!(
            workspace
                .editor
                .custom_quick_styles(crate::document::AnnotationKind::Arrow)[0]
                .is_none()
        );
        assert!(workspace.editor.take_tool_preferences_change().is_some());
        assert_eq!(workspace.source_quad(), image);
        assert_eq!(workspace.document.edit_state(), artwork);
        assert!(!workspace.history.can_undo());
    }

    #[test]
    fn styles_default_on_restore_and_document_reset_preserving_pin_and_tool_appearances() {
        let mut workspace = Workspace::new();
        workspace.click("Properties");
        workspace.click("Arrow · Blue quick style");
        workspace.choose_panel(Panel::Layers);
        workspace.click("Pin panel");
        let tools = workspace.editor.take_tool_preferences_change().unwrap();
        let inspector = workspace
            .editor
            .take_inspector_preferences_change()
            .unwrap();
        assert_eq!(
            inspector,
            Preferences {
                panel: Panel::Layers,
                pinned: true,
                expanded: true
            }
        );
        let settings = crate::storage::Settings {
            editor_tools: tools.clone(),
            editor_inspector: inspector,
            ..Default::default()
        };
        let restored: crate::storage::Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        let mut editor = EditorState::default();
        editor.set_tool_preferences(restored.editor_tools);
        editor.set_inspector_preferences(restored.editor_inspector);
        editor.reset();
        assert_eq!(
            editor.inspector.preferences,
            Preferences {
                panel: Panel::Styles,
                ..inspector
            }
        );
        assert_eq!(editor.tool_preferences, tools);
        editor.activate_tool(Tool::Arrow);
        assert_eq!(editor.color, [40, 117, 219, 255]);
        assert_eq!(editor.stroke_width, 4.);
        editor.inspector.close();
        editor.reset();
        assert_eq!(
            editor.take_inspector_preferences_change(),
            Some(Preferences {
                panel: Panel::Styles,
                expanded: false,
                ..inspector
            })
        );
        let legacy: crate::storage::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.editor_inspector, Preferences::default());
    }

    #[test]
    fn each_panel_stays_inside_a_small_workspace_at_every_text_size() {
        for scale in [1., 1.15, 1.3] {
            let mut workspace = Workspace::new();
            // UI points shrink as the native text/UI scale grows.
            workspace.size = Vec2::new(980., 640.) / scale;
            for panel in Panel::ALL {
                workspace.editor.inspector.show(panel);
                workspace.settle();
                let rect = workspace
                    .ctx
                    .memory(|memory| memory.area_rect(Id::new("inspector-overlay")))
                    .unwrap();
                assert!(
                    Rect::from_min_size(Pos2::ZERO, workspace.size).contains_rect(rect),
                    "{panel:?}, {scale}: {rect:?}"
                );
                let output = workspace.settle();
                let nodes = output.platform_output.accesskit_update.unwrap().nodes;
                assert!(
                    nodes
                        .iter()
                        .any(|(_, n)| n.label() == Some("Close properties"))
                );
                assert!(nodes.iter().any(|(_, n)| n.label() == Some("Pin panel")));
            }
        }
    }
    #[test]
    fn narrow_ribbon_keeps_overflow_tools_and_all_panels_reachable() {
        let mut workspace = Workspace::new();
        workspace.size = Vec2::new(400., 640.);
        let output = workspace.settle();
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        let bounds = |label| {
            nodes
                .iter()
                .find_map(|(_, n)| (n.label() == Some(label)).then(|| n.bounds()).flatten())
                .unwrap()
        };
        assert!(bounds("Properties").x0 >= bounds("More tools").x1);
        assert!(bounds("Properties").x1 <= f64::from(workspace.size.x));
        assert!(!nodes.iter().any(|(_, n)| n.label() == Some("Fill")));
        workspace.click("More tools");
        workspace.click("Fill");
        assert_eq!(workspace.editor.tool, Tool::Fill);
        for (label, tool) in [("Pen", Tool::Pen), ("Magnify", Tool::Magnify)] {
            let output = workspace.settle();
            assert!(
                !output
                    .platform_output
                    .accesskit_update
                    .unwrap()
                    .nodes
                    .iter()
                    .any(|(_, n)| n.label() == Some(label))
            );
            workspace.click("More tools");
            workspace.click(label);
            assert_eq!(workspace.editor.tool, tool);
        }
        for panel in [Panel::Layers, Panel::Presets, Panel::Effects] {
            workspace.choose_panel(panel);
            assert!(workspace.editor.inspector.is(panel));
            assert!(workspace.editor.inspector.preferences.expanded);
        }
        workspace.choose_panel(Panel::Styles);
        assert!(workspace.editor.inspector.is(Panel::Styles));
        assert!(workspace.editor.inspector.preferences.expanded);
        workspace.click("Close properties");
        workspace.click("Properties");
        assert!(workspace.editor.inspector.is(Panel::Styles));
        assert!(workspace.editor.inspector.preferences.expanded);
    }

    #[test]
    fn unpinned_inspector_drags_without_moving_or_editing_the_canvas() {
        let mut workspace = Workspace::new();
        let before = workspace.document.edit_state();
        let canvas = workspace.source_quad();
        let overlay = |workspace: &Workspace| {
            workspace
                .ctx
                .memory(|m| m.area_rect(Id::new(OVERLAY_ID)))
                .unwrap()
        };
        let grip = |workspace: &Workspace| {
            workspace
                .ctx
                .read_response(Id::new(OVERLAY_ID).with("grip"))
                .unwrap()
                .rect
                .center()
        };
        workspace.click("Properties");
        workspace.click("Pin panel");
        assert_ne!(workspace.source_quad(), canvas);
        workspace.click("Unpin panel");
        let initial = overlay(&workspace);
        let start = grip(&workspace);
        let delta = Vec2::new(-330., 44.);
        let end = start + delta;
        workspace.frame(vec![egui::Event::PointerMoved(start), pointer(start, true)]);
        assert_eq!(
            overlay(&workspace),
            initial,
            "pressing the grip must not move it"
        );
        workspace.frame(vec![egui::Event::PointerMoved(end)]);
        assert_eq!(overlay(&workspace), initial.translate(delta));
        workspace.frame(vec![pointer(end, false)]);
        assert_eq!(overlay(&workspace), initial.translate(delta));
        assert_eq!(workspace.source_quad(), canvas);

        workspace.click("Close properties");
        workspace.click("Properties");
        assert_eq!(overlay(&workspace), initial.translate(delta));

        let start = grip(&workspace);
        let end = egui::pos2(0., workspace.size.y - 1.);
        workspace.frame(vec![
            egui::Event::PointerMoved(start),
            pointer(start, true),
            egui::Event::PointerMoved(end),
            pointer(end, false),
        ]);
        workspace.settle();
        let moved = overlay(&workspace);
        assert!(moved.left() < initial.left() - 500.);
        assert!(moved.top() > initial.top() + delta.y);
        assert!(Rect::from_min_size(Pos2::ZERO, workspace.size).contains_rect(moved));
        assert!(workspace.editor.inspector.preferences.expanded);
        assert_eq!(workspace.source_quad(), canvas);
        assert_eq!(workspace.document.edit_state(), before);
        assert!(workspace.editor.gesture.is_none());
        assert!(!workspace.history.can_undo());
    }

    #[test]
    fn floating_annotation_controls_do_not_cover_the_open_inspector() {
        let mut workspace = Workspace::new();
        workspace.click("Properties");
        let mut arrow = workspace.editor.new_annotation(
            crate::document::AnnotationKind::Arrow,
            crate::document::Point { x: 950., y: 250. },
        );
        arrow.end = crate::document::Point { x: 1150., y: 400. };
        workspace.document.annotations.push(arrow);
        workspace.editor.select(Some(0), &workspace.document);
        workspace.settle();
        for offset in [None, Some(Vec2::ZERO)] {
            if let Some(offset) = offset {
                workspace.ctx.data_mut(|d| {
                    d.insert_temp(Id::new(OVERLAY_ID).with("position"), offset);
                });
                workspace.settle();
            }
            let (overlay, bar) = workspace.ctx.memory(|m| {
                (
                    m.area_rect(Id::new(OVERLAY_ID)).unwrap(),
                    m.area_rect(
                        Id::new("selected-object-properties")
                            .with(crate::document::AnnotationKind::Arrow as u8)
                            .with(1_usize),
                    )
                    .unwrap(),
                )
            });
            assert!(
                !bar.intersects(overlay),
                "Floating controls {bar:?} cover inspector {overlay:?}"
            );
        }
    }
    #[test]
    fn object_styles_shortcut_opens_styles_without_editing_or_deselecting() {
        for multiple in [false, true] {
            let mut w = Workspace::new();
            let mut arrow = w.editor.new_annotation(
                crate::document::AnnotationKind::Arrow,
                crate::document::Point { x: 200., y: 280. },
            );
            arrow.end = crate::document::Point { x: 500., y: 400. };
            w.document.annotations = vec![arrow.clone(), arrow];
            w.editor
                .set_selection(if multiple { vec![0, 1] } else { vec![0] }, &w.document);
            w.editor.inspector.select(Panel::Layers);
            w.editor.inspector.close();
            let before = w.document.edit_state();
            let selected = w.editor.selected_indices(&w.document);
            let color = w.editor.color;
            w.click("Open Styles");
            assert!(w.editor.inspector.is(Panel::Styles));
            assert!(w.editor.inspector.preferences.expanded);
            assert_eq!(w.editor.selected_indices(&w.document), selected);
            assert_eq!(w.editor.color, color);
            assert_eq!(w.document.edit_state(), before);
            assert!(!w.history.can_undo());
        }
    }

    #[test]
    fn customizer_reorders_unpinned_tools_and_overflow_uses_that_order() {
        let mut w = Workspace::new();
        w.editor.set_toolbar_order(Vec::new());
        w.click("More tools");
        w.click("Customize toolbar…");
        w.click("Move Connected Arrows later");
        assert!(
            !w.editor
                .toolbar_pins
                .iter()
                .any(|v| v == "Connected Arrows")
        );
        w.editor.toolbar_customizer = false;
        w.click("More tools");
        let output = w.settle();
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        let top = |label| {
            nodes
                .iter()
                .find_map(|(_, n)| (n.label() == Some(label)).then(|| n.bounds()).flatten())
                .unwrap()
                .y0
        };
        assert!(top("Smart Erase / Repair") < top("Connected Arrows"));
        w.click("Connected Arrows");
        assert_eq!(w.editor.tool, Tool::ConnectedArrow);
    }
    #[test]
    fn canvas_resize_menu_repeats_steps_presets_and_opens_detailed_dialog_with_shortcut() {
        fn open_menu(w: &mut Workspace) {
            w.frame(vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            w.settle();
            let quad = w.source_quad();
            let rect = quad.iter().fold(Rect::NOTHING, |r, (p, _)| {
                r.union(Rect::from_min_max(*p, *p))
            });
            let at = rect.center();
            for pressed in [true, false] {
                w.frame(vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            w.settle();
        }
        let mut w = Workspace::new();
        w.editor.inspector.close();
        open_menu(&mut w);
        w.click("Increase image size by 10%");
        assert_eq!(w.document.output_size(), [1320, 880]);
        assert!(w.editor.fit, "quick resize must retain Fit mode");
        assert_eq!(w.editor.resize_feedback.unwrap().0, [1320, 880]);
        w.editor.fit = false;
        w.editor.zoom = 0.5;
        w.click("Increase image size by 10%");
        assert_eq!(w.document.output_size(), [1452, 968]);
        assert!(!w.editor.fit);
        assert_eq!(w.editor.zoom, 0.5, "manual zoom remains unchanged");
        assert_eq!(w.editor.resize_feedback.unwrap().0, [1452, 968]);
        w.click("Decrease image size by 10%");
        assert_eq!(w.document.output_size(), [1307, 871]);
        w.editor.resize_feedback.as_mut().unwrap().1 =
            std::time::Instant::now() - std::time::Duration::from_secs(1);
        w.settle();
        assert!(
            w.editor.resize_feedback.is_none(),
            "resize feedback disappears without changing the image"
        );
        for (label, size) in [
            // Each new dialog starts from the current output, including rounding.
            ("25%", [327, 218]),
            ("50%", [164, 109]),
            ("125%", [205, 136]),
            ("150%", [308, 204]),
            ("Set to original", [1200, 800]),
        ] {
            open_menu(&mut w);
            let menu = w.settle();
            assert!(
                !menu
                    .platform_output
                    .accesskit_update
                    .unwrap()
                    .nodes
                    .iter()
                    .any(|(_, node)| node.label() == Some("Resize presets"))
            );
            w.click("Resize image…");
            let before = w.document.edit_state();
            w.click(label);
            assert_eq!(w.editor.resize.as_ref().unwrap().size, size);
            assert_eq!(w.document.edit_state(), before);
            w.click("Apply resize");
            assert!(w.editor.resize.is_none());
            assert_eq!(w.document.output_size(), size);
        }
        open_menu(&mut w);
        w.click("Resize image…");
        assert!(w.editor.resize.is_some());
        let before_cancel = w.document.edit_state();
        w.click("25%");
        w.click("Cancel");
        assert!(w.editor.resize.is_none());
        assert_eq!(w.document.edit_state(), before_cancel);
        // The icon groups share rows and retain their original actions.
        open_menu(&mut w);
        let output = w.settle();
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        let bounds = |label| {
            nodes
                .iter()
                .find_map(|(_, n)| (n.label() == Some(label)).then(|| n.bounds()).flatten())
                .unwrap()
        };
        for labels in [
            vec!["Rotate left", "Rotate right", "Rotate 180°"],
            vec!["Flip horizontal", "Flip vertical"],
            vec!["Select all", "Select all annotations", "Deselect"],
        ] {
            let row = bounds(labels[0]);
            for label in &labels[1..] {
                assert!((bounds(label).y0 - row.y0).abs() < 1.0);
            }
        }
        for (label, turns) in [("Rotate left", 3), ("Rotate right", 0), ("Rotate 180°", 2)] {
            open_menu(&mut w);
            w.click(label);
            assert_eq!(w.document.geometry.quarter_turns, turns);
        }
        for label in ["Flip horizontal", "Flip vertical"] {
            open_menu(&mut w);
            let before = w.document.geometry;
            w.click(label);
            assert_ne!(w.document.geometry, before);
            assert!(w.history.undo(&mut w.document));
            assert_eq!(w.document.geometry, before);
        }
        let quad = w.source_quad();
        let rect = quad.iter().fold(Rect::NOTHING, |r, (p, _)| {
            r.union(Rect::from_min_max(*p, *p))
        });
        let at = rect.center();
        for pressed in [true, false] {
            w.frame(vec![egui::Event::PointerMoved(at), pointer(at, pressed)]);
        }
        let before = w.document.edit_state();
        w.frame(vec![egui::Event::Key {
            key: Key::R,
            physical_key: Some(Key::R),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift: true,
                ..egui::Modifiers::COMMAND
            },
        }]);
        assert!(w.editor.resize.is_some());
        assert_eq!(w.document.edit_state(), before);
        w.click("Cancel");
        w.document.annotations = vec![
            crate::markup_tests::object(AnnotationKind::Rectangle, [10., 10.], [60., 60.]),
            crate::markup_tests::object(AnnotationKind::Arrow, [80., 80.], [180., 160.]),
        ];
        w.history = EditHistory::new(w.document.edit_state());
        let before_selection = w.document.edit_state();
        open_menu(&mut w);
        w.click("Select all");
        assert!(w.editor.area.is_some());
        open_menu(&mut w);
        w.click("Select all annotations");
        assert_eq!(w.editor.selected_indices(&w.document), vec![0, 1]);
        assert!(w.editor.area.is_none());
        open_menu(&mut w);
        w.click("Deselect");
        assert!(w.editor.selected_indices(&w.document).is_empty());
        assert!(w.editor.area.is_none());
        assert_eq!(w.document.edit_state(), before_selection);
        assert!(!w.history.can_undo());
    }
    #[test]
    fn image_history_buttons_and_shortcuts_work_after_controls_and_preserve_modal_drafts() {
        fn key(w: &mut Workspace, redo: bool) {
            let modifiers = egui::Modifiers {
                shift: redo,
                ..egui::Modifiers::COMMAND
            };
            for pressed in [true, false] {
                w.frame(vec![egui::Event::Key {
                    key: Key::Z,
                    physical_key: Some(Key::Z),
                    pressed,
                    repeat: false,
                    modifiers,
                }]);
            }
            w.settle();
        }
        let mut w = Workspace::new();
        w.image_controls = true;
        w.settle();
        let initial = w.document.edit_state();
        w.click("Image tools · crop, resize, rotate");
        w.click("Rotate right");
        let rotated = w.document.edit_state();
        assert_eq!(rotated.geometry.quarter_turns, 1);
        w.history.mark_saved(&rotated);
        key(&mut w, false);
        assert_eq!(w.document.edit_state(), initial);
        key(&mut w, true);
        assert_eq!(w.document.edit_state(), rotated);
        w.click("Undo");
        assert_eq!(w.document.edit_state(), initial);
        w.click("Redo");
        assert_eq!(w.document.edit_state(), rotated);
        w.click("Fit canvas to window");
        key(&mut w, false);
        assert_eq!(
            w.document.edit_state(),
            initial,
            "toolbar focus must not block image undo"
        );
        key(&mut w, true);
        w.click("Image tools · crop, resize, rotate");
        w.click("Resize image…");
        w.click("50%");
        key(&mut w, false);
        assert_eq!(
            w.document.edit_state(),
            rotated,
            "a modal draft owns its keyboard input"
        );
        assert_eq!(w.editor.resize.as_ref().unwrap().size, [400, 600]);
        w.click("Cancel");
        w.click("Undo");
        assert_eq!(w.document.edit_state(), initial);
    }
}
