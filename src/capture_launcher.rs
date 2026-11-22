//! Resident capture launcher: a native child window, independent of the editor.
use super::*;
use crate::capture::{CaptureDestination, CaptureMode};

const W: f32 = 468.;
const SIZE: Vec2 = Vec2::new(W, 280.);
const NOTCH: f32 = 8.;
const FIELD_X: f32 = 72.;
const FIELD_W: f32 = 250.;

/// The active Capture Now theme, shared by every control painted in the panel.
fn look(ctx: &egui::Context) -> crate::capture_panel::CaptureLook {
    ctx.data(|data| {
        data.get_temp::<crate::capture_panel::CaptureTheme>(egui::Id::new("capture-theme"))
    })
    .unwrap_or_default()
    .look()
}
fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("resident-capture-panel")
}

pub(super) struct Launcher {
    position: Pos2,
    focused: bool,
    request_focus: bool,
}

fn panel_position(screen: Rect, anchor: Rect, size: Vec2) -> Pos2 {
    let x = (anchor.center().x - size.x * 0.5).clamp(
        screen.left() + 8.,
        (screen.right() - size.x - 8.).max(screen.left() + 8.),
    );
    let below = anchor.bottom() + 8.;
    let y = if below + size.y <= screen.bottom() - 8. {
        below
    } else {
        anchor.top() - size.y - 8.
    };
    egui::pos2(
        x,
        y.clamp(
            screen.top() + 8.,
            (screen.bottom() - size.y - 8.).max(screen.top() + 8.),
        ),
    )
}

fn position(displays: &[DisplayInfo], tray: Option<Rect>, zoom: f32) -> Pos2 {
    let display = displays
        .iter()
        .find(|d| {
            let Some(tray) = tray else {
                return false;
            };
            let p = if cfg!(target_os = "macos") {
                tray.center() / d.scale_factor.max(1.)
            } else {
                tray.center()
            };
            Rect::from_min_size(
                egui::pos2(d.x as f32, d.y as f32),
                Vec2::new(d.width as f32, d.height as f32),
            )
            .contains(p)
        })
        .or_else(|| displays.iter().find(|d| d.is_primary))
        .or(displays.first());
    let Some(d) = display else {
        return egui::pos2(20., 40.);
    };
    let divisor = if cfg!(target_os = "macos") {
        zoom
    } else {
        d.scale_factor.max(1.) * zoom
    };
    let screen = Rect::from_min_size(
        egui::pos2(d.x as f32, d.y as f32) / divisor,
        Vec2::new(d.width as f32, d.height as f32) / divisor,
    );
    let tray_divisor = d.scale_factor.max(1.) * zoom;
    let anchor = tray
        .map(|r| Rect::from_min_max(r.min / tray_divisor, r.max / tray_divisor))
        .unwrap_or_else(|| {
            Rect::from_min_size(screen.right_top() - Vec2::new(40., -8.), Vec2::splat(24.))
        });
    panel_position(screen, anchor, SIZE)
}

impl MyImageApp {
    pub(super) fn close_capture_launcher(&mut self, ctx: &egui::Context) {
        if self.capture_launcher.take().is_some() {
            ctx.send_viewport_cmd_to(viewport_id(), ViewportCommand::Visible(false));
            ctx.request_repaint();
        }
    }
    pub(super) fn toggle_capture_launcher(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "macos")]
        if let Some(recording) = &mut self.recording {
            recording.focus(ctx);
            return;
        }
        if self.capture_launcher.is_some() {
            self.close_capture_launcher(ctx);
            return;
        }
        if self.selection.is_some()
            || self.scroll_session.is_some()
            || self.pending_capture.is_some()
            || self.screen_text.open
        {
            return;
        }
        if let Some(desktop) = &self.desktop {
            desktop.reveal_application();
        }
        self.capture_launcher = Some(Launcher {
            position: position(
                &self.displays,
                self.desktop.as_ref().and_then(|d| d.tray_bounds()),
                ctx.zoom_factor(),
            ),
            focused: false,
            request_focus: true,
        });
        ctx.request_repaint();
    }
    pub(super) fn capture_launcher_ui(&mut self, ctx: &egui::Context) {
        let Some(launcher) = &mut self.capture_launcher else {
            return;
        };
        let mut action = None;
        let mut changed = false;
        let enabled = !self.displays.is_empty()
            && !self.busy
            && self.selection.is_none()
            && self.pending_capture.is_none()
            && self.scroll_session.is_none()
            && !self.recovery.prompt
            && self.capture_hotkey_editor.is_none()
            && !self.guide_open
            && !self.about_open
            && !self.updates.open
            && !self.appearance_open
            && self.pending_action.is_none()
            && self.file_dialog.is_none()
            && !self.save_as_panel.open
            && !self.editor.has_modal();
        let enabled = enabled && !self.screen_text.open;
        let builder = egui::ViewportBuilder::default()
            .with_title(format!("{} Capture Now", crate::branding::NAME))
            .with_inner_size(SIZE)
            .with_position(launcher.position)
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_taskbar(false)
            .with_has_shadow(true)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_active(true);
        ctx.show_viewport_immediate(viewport_id(), builder, |child, class| {
            if launcher.request_focus {
                child.send_viewport_cmd(ViewportCommand::Focus);
                launcher.request_focus = false;
            }
            let focused = child.input(|i| i.viewport().focused.unwrap_or(i.focused));
            let escape = child.input(|i| i.key_pressed(egui::Key::Escape));
            if child.input(|i| i.viewport().close_requested())
                || (!egui::Popup::is_any_open(child) && (escape || (launcher.focused && !focused)))
            {
                action = Some(Action::Close);
            }
            launcher.focused |= focused;
            let mut content = |ui: &mut egui::Ui| {
                let result = launcher_content(ui, &mut self.settings, &self.displays, enabled);
                changed |= result.0;
                if result.1.is_some() {
                    action = result.1;
                }
            };
            if class == egui::ViewportClass::Embedded {
                egui::Window::new("Capture Now")
                    .resizable(false)
                    .show(child, &mut content);
            } else {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(child, &mut content);
            }
            crate::pointer_style::paint(child, self.settings.pointer_style);
        });
        if changed {
            self.persist_settings();
        }
        if let Some(action) = action {
            self.close_capture_launcher(ctx);
            match action {
                Action::Capture => self.begin_capture(ctx),
                Action::EditHotkey => self.open_capture_hotkey(ctx),
                Action::Close => {}
                Action::Editor | Action::Library | Action::Settings => {
                    let event = match action {
                        Action::Editor => DesktopEvent::Show,
                        Action::Library => DesktopEvent::Library,
                        _ => DesktopEvent::CaptureSettings,
                    };
                    let _ = self.desktop_tx.send(event);
                    ctx.request_repaint();
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Action {
    Capture,
    EditHotkey,
    Editor,
    Library,
    Settings,
    Close,
}

/// Help for the control under the pointer is shown in the panel header rather than
/// in a popup, so it can never cover the controls of this small window.
fn hint(response: &egui::Response, text: &str) {
    if (response.hovered() || response.has_focus()) && !text.is_empty() {
        response
            .ctx
            .data_mut(|data| data.insert_temp(egui::Id::new("capture-hint"), text.to_owned()));
    }
}

fn switch(ui: &mut egui::Ui, rect: Rect, value: &mut bool, label: &str, help: &str) -> bool {
    let id = ui.id().with(label);
    let mut response = ui.interact(rect, id, egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label)
    });
    let p = look(ui.ctx()).palette;
    let t = ui.ctx().animate_bool_with_time(id.with("on"), *value, 0.12);
    let track = Rect::from_min_size(
        egui::pos2(rect.left(), rect.center().y - 8.),
        Vec2::new(28., 16.),
    );
    let off = look(ui.ctx()).off;
    appearance::gradient(
        ui.painter(),
        track,
        8.,
        off.lerp_to_gamma(p.accent_top, t),
        off.lerp_to_gamma(p.accent, t),
    );
    let center = egui::pos2(
        egui::lerp(track.left() + 8.0..=track.right() - 8., t),
        track.center().y,
    );
    ui.painter().circle_filled(
        center + Vec2::new(0., 1.),
        6.5,
        Color32::from_black_alpha(50),
    );
    ui.painter().circle_filled(center, 6., Color32::WHITE);
    ui.painter().text(
        egui::pos2(track.right() + 8., rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.),
        if *value || response.hovered() {
            p.ink
        } else {
            p.muted
        },
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand2(Vec2::new(3., 0.)),
            6,
            Stroke::new(1.5, p.ginger),
            egui::StrokeKind::Inside,
        );
    }
    hint(&response, help);
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .changed()
}

/// One recessed track with a sliding accent pill: the whole segment is the target.
fn segmented(
    ui: &mut egui::Ui,
    track: Rect,
    name: &str,
    items: &[(&str, Option<Icon>, &str)],
    selected: Option<usize>,
) -> Option<usize> {
    let p = look(ui.ctx()).palette;
    ui.painter().rect(
        track,
        8,
        p.navy,
        Stroke::new(1., p.border),
        egui::StrokeKind::Inside,
    );
    let inner = track.shrink(2.);
    let width = inner.width() / items.len() as f32;
    let cell = |i: f32| {
        Rect::from_min_size(
            inner.min + Vec2::new(i * width, 0.),
            Vec2::new(width, inner.height()),
        )
    };
    if let Some(selected) = selected {
        let at = ui
            .ctx()
            .animate_value_with_time(ui.id().with(name), selected as f32, 0.12);
        appearance::satin_with_palette(ui.painter(), cell(at), 6., true, false, p);
    }
    let mut clicked = None;
    let mut focused = None;
    for (i, (label, icon, hint)) in items.iter().enumerate() {
        let rect = cell(i as f32);
        let is_selected = selected == Some(i);
        let response = ui.interact(rect, ui.id().with(label), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                ui.is_enabled(),
                is_selected,
                *label,
            )
        });
        let color = if is_selected {
            Color32::WHITE
        } else if response.hovered() {
            p.ink
        } else {
            p.muted
        };
        if !is_selected && response.hovered() {
            ui.painter().rect_filled(rect.shrink(1.), 6, p.cream);
        }
        let text =
            ui.painter()
                .layout_no_wrap((*label).into(), egui::FontId::proportional(12.), color);
        let gap = if icon.is_some() { 20. } else { 0. };
        let left = rect.center().x - (text.size().x + gap) * 0.5;
        if let Some(icon) = icon {
            appearance::icon(
                ui.painter(),
                egui::pos2(left + 7., rect.center().y),
                *icon,
                color,
                14.,
            );
        }
        ui.painter().galley(
            egui::pos2(left + gap, rect.center().y - text.size().y * 0.5),
            text,
            color,
        );
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                6,
                Stroke::new(1.5, p.ginger),
                egui::StrokeKind::Inside,
            );
        }
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        self::hint(&response, hint);
        if response.clicked() {
            clicked = Some(i);
        }
        if response.has_focus() {
            focused = Some(i);
        }
    }
    if let Some(i) = focused
        && let Some(target) = appearance::arrow_target(ui, ui.id().with(items[i].0), i, items.len())
    {
        ui.memory_mut(|memory| memory.request_focus(ui.id().with(items[target].0)));
        clicked = Some(target);
    }
    clicked
}

fn pearl_icon_button(ui: &mut egui::Ui, rect: Rect, label: &str, icon: Icon) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(label), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let p = look(ui.ctx()).palette;
    let active = response.hovered() || response.has_focus();
    if active {
        ui.painter().rect_filled(rect, 7, p.cream);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            7,
            Stroke::new(1.5, p.ginger),
            egui::StrokeKind::Inside,
        );
    }
    appearance::icon(
        ui.painter(),
        rect.center(),
        icon,
        if active { p.ink } else { p.muted },
        15.,
    );
    hint(&response, label);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn launcher_content(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    displays: &[DisplayInfo],
    enabled: bool,
) -> (bool, Option<Action>) {
    let mut changed = false;
    let mut action = None;
    ui.ctx()
        .data_mut(|data| data.insert_temp(egui::Id::new("capture-theme"), settings.capture_theme));
    let crate::capture_panel::CaptureLook {
        palette: p,
        top: surface_top,
        dark,
        ..
    } = look(ui.ctx());
    let (outer, _) = ui.allocate_exact_size(SIZE, egui::Sense::hover());
    let bounds = Rect::from_min_max(outer.min + Vec2::new(0., NOTCH), outer.max);
    let at = |x, y, w, h| Rect::from_min_size(bounds.min + Vec2::new(x, y), Vec2::new(w, h));
    let body = bounds.shrink(0.5);
    appearance::gradient(ui.painter(), body, 16., surface_top, p.paper);
    ui.painter().rect_stroke(
        body,
        16,
        Stroke::new(1., p.border),
        egui::StrokeKind::Inside,
    );
    let tip = outer.center_top();
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            tip + Vec2::new(-NOTCH, NOTCH + 1.5),
            tip,
            tip + Vec2::new(NOTCH, NOTCH + 1.5),
        ],
        surface_top,
        Stroke::NONE,
    ));
    ui.painter().line(
        vec![
            tip + Vec2::new(-NOTCH, NOTCH),
            tip,
            tip + Vec2::new(NOTCH, NOTCH),
        ],
        Stroke::new(1., p.border),
    );
    for (from, to) in [(16., W * 0.5 - NOTCH - 2.), (W * 0.5 + NOTCH + 2., W - 16.)] {
        ui.painter().line_segment(
            [
                bounds.min + Vec2::new(from, 1.5),
                bounds.min + Vec2::new(to, 1.5),
            ],
            Stroke::new(1., Color32::from_white_alpha(if dark { 26 } else { 200 })),
        );
    }
    // Scope the dropdown and popup colors to this panel; never alter the editor theme.
    crate::capture_panel::apply_look(ui, &look(ui.ctx()));
    ui.spacing_mut().button_padding = Vec2::new(9., 4.);
    ui.spacing_mut().interact_size.y = 26.;
    ui.spacing_mut().item_spacing = Vec2::new(4., 0.);

    // Header: aperture mark, title, status, then quiet shortcuts into the app.
    let mark = bounds.min + Vec2::new(24., 22.);
    ui.painter()
        .circle_filled(mark, 10., p.accent.gamma_multiply(0.35));
    ui.painter()
        .circle_stroke(mark, 6.5, Stroke::new(1.2, p.ginger));
    for i in 0..6 {
        let angle = i as f32 * std::f32::consts::TAU / 6.;
        ui.painter().line_segment(
            [
                mark + Vec2::angled(angle) * 6.5,
                mark + Vec2::angled(angle + 1.4) * 2.8,
            ],
            Stroke::new(1., p.ginger),
        );
    }
    let title = ui.painter().text(
        bounds.min + Vec2::new(42., 22.),
        egui::Align2::LEFT_CENTER,
        "Capture Now",
        egui::FontId::proportional(14.5),
        p.ink,
    );
    let status = if enabled {
        Color32::from_rgb(74, 222, 151)
    } else {
        Color32::from_rgb(244, 183, 86)
    };
    // Last frame's hover help replaces the status; it is consumed so it clears itself.
    let help = ui
        .ctx()
        .data_mut(|data| data.remove_temp::<String>(egui::Id::new("capture-hint")));
    let dot = egui::pos2(title.right() + 14., title.center().y);
    if let Some(help) = help {
        let mut job = egui::text::LayoutJob::simple_singleline(
            help,
            egui::FontId::proportional(11.),
            p.ginger,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(
            W - 4. * 30. - 20. - (dot.x - bounds.left()),
        );
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(
            egui::pos2(dot.x - 4., dot.y - galley.size().y * 0.5),
            galley,
            p.ginger,
        );
    } else {
        ui.painter()
            .circle_filled(dot, 5., status.gamma_multiply(0.25));
        ui.painter().circle_filled(dot, 2.5, status);
        ui.painter().text(
            dot + Vec2::new(9., 0.),
            egui::Align2::LEFT_CENTER,
            if enabled { "Ready" } else { "Busy" },
            egui::FontId::proportional(11.),
            p.muted,
        );
    }
    for (i, (label, icon, next)) in [
        ("Close capture panel", Icon::Close, Action::Close),
        ("More settings", Icon::Settings, Action::Settings),
        ("Library", Icon::Library, Action::Library),
        ("Open Editor", Icon::Edit, Action::Editor),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = at(W - 38. - i as f32 * 30., 9., 26., 26.);
        if pearl_icon_button(ui, rect, label, icon).clicked() {
            action = Some(next);
        }
    }

    // Capture modes share one recessed track; the choice is a filled accent tile.
    let mode_track = at(12., 42., W - 24., 50.);
    ui.painter().rect(
        mode_track,
        11,
        p.navy,
        Stroke::new(1., p.border),
        egui::StrokeKind::Inside,
    );
    let mut modes = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("pearl-modes")
            .max_rect(mode_track.shrink(3.))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    modes.spacing_mut().item_spacing.x = 2.;
    let count = CaptureMode::ALL.len() as f32;
    for mode in CaptureMode::ALL {
        let tile = crate::capture_panel::compact_mode_tile(
            &mut modes,
            mode,
            settings.capture_mode == mode,
            Vec2::new((W - 30. - 2. * (count - 1.)) / count, 44.),
            p,
        );
        hint(
            &tile,
            match mode {
                CaptureMode::Region => "Drag an area or click a window",
                CaptureMode::MultiRegion => "Select several areas, capture together",
                CaptureMode::Window => "Click a window to capture it",
                CaptureMode::FullScreen => "Capture a whole display",
                CaptureMode::Scrolling => "Scroll a window to capture all of it",
                CaptureMode::Text => "Copy the text in a region",
                CaptureMode::Video => "Record a window or a full display to MP4",
            },
        );
        if tile.clicked() {
            changed |= settings.capture_mode != mode;
            settings.capture_mode = mode;
        }
    }

    for (label, y) in [
        ("Display", 115.),
        ("Send to", 148.),
        ("Delay", 181.),
        ("Output", 214.),
    ] {
        ui.painter().text(
            bounds.min + Vec2::new(16., y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.),
            p.muted,
        );
    }
    let mut display_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("pearl-display")
            .max_rect(at(FIELD_X, 102., FIELD_W, 26.))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    display_ui.spacing_mut().combo_width = FIELD_W;
    let name = crate::capture_panel::selected_display_label(settings, displays);
    let popup_visuals = display_ui.visuals().clone();
    egui::ComboBox::from_id_salt("launcher-display")
        .selected_text(egui::RichText::new(name).size(12.))
        .width(FIELD_W)
        .popup_style(egui::style::StyleModifier::new(move |style| {
            style.visuals = popup_visuals.clone()
        }))
        .show_ui(&mut display_ui, |ui| {
            changed |= crate::capture_panel::display_choices(ui, settings, displays);
        })
        .response
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    let send_track = at(FIELD_X, 135., FIELD_W, 26.);
    if !settings.capture_mode.makes_image() {
        let video = settings.capture_mode == CaptureMode::Video;
        ui.painter().rect(
            send_track,
            8,
            p.navy,
            Stroke::new(1., p.border),
            egui::StrokeKind::Inside,
        );
        appearance::icon(
            ui.painter(),
            send_track.left_center() + Vec2::new(16., 0.),
            if video { Icon::Video } else { Icon::TextSize },
            p.ginger,
            15.,
        );
        ui.painter().text(
            send_track.left_center() + Vec2::new(32., 0.),
            egui::Align2::LEFT_CENTER,
            if video {
                "MP4 · Recordings folder"
            } else {
                "Text → clipboard · no image saved"
            },
            egui::FontId::proportional(12.),
            p.ink,
        );
    } else {
        let items = CaptureDestination::ALL.map(|dest| {
            (
                dest.label(),
                Some(if dest == CaptureDestination::Editor {
                    Icon::Edit
                } else {
                    Icon::Copy
                }),
                if dest == CaptureDestination::Editor {
                    "Save to Library and open in Editor"
                } else {
                    "Copy and save to Library"
                },
            )
        });
        let selected = CaptureDestination::ALL
            .iter()
            .position(|dest| *dest == settings.capture_destination);
        if let Some(i) = segmented(ui, send_track, "send-to", &items, selected) {
            changed |= settings.capture_destination != CaptureDestination::ALL[i];
            settings.capture_destination = CaptureDestination::ALL[i];
        }
    }

    const DELAYS: [u32; 4] = [0, 3, 5, 10];
    if let Some(i) = segmented(
        ui,
        at(FIELD_X, 168., FIELD_W, 26.),
        "delay",
        &[
            ("Off", None, "Capture immediately"),
            ("3s", None, "Wait 3 seconds before capturing"),
            ("5s", None, "Wait 5 seconds before capturing"),
            ("10s", None, "Wait 10 seconds before capturing"),
        ],
        DELAYS.iter().position(|s| *s == settings.delay_seconds),
    ) {
        changed |= settings.delay_seconds != DELAYS[i];
        settings.delay_seconds = DELAYS[i];
    }

    let sizes = crate::capture_size::OutputSize::ALL;
    let size_items = sizes.map(|size| {
        (
            if size == crate::capture_size::OutputSize::Original {
                "100%"
            } else {
                size.label()
            },
            None,
            "Resize the finished image before saving or copying",
        )
    });
    ui.add_enabled_ui(settings.capture_mode.makes_image(), |ui| {
        if let Some(i) = segmented(
            ui,
            at(FIELD_X, 201., FIELD_W, 26.),
            "output-size",
            &size_items,
            sizes
                .iter()
                .position(|s| *s == settings.capture_output_size),
        ) {
            changed |= settings.capture_output_size != sizes[i];
            settings.capture_output_size = sizes[i];
        }
    });

    changed |= ui
        .add_enabled_ui(settings.capture_mode != CaptureMode::Text, |ui| {
            switch(
                ui,
                at(16., 234., 134., 26.),
                &mut settings.include_pointer,
                "Include pointer",
                "Show the mouse pointer in the capture",
            )
        })
        .inner;
    changed |= ui
        .add_enabled_ui(settings.capture_mode != CaptureMode::Video, |ui| {
            switch(
                ui,
                at(166., 234., 156., 26.),
                &mut settings.capture_on_release,
                "Capture on release",
                "Capture on release, without confirming",
            )
        })
        .inner;

    ui.painter().line_segment(
        [
            bounds.min + Vec2::new(336., 106.),
            bounds.min + Vec2::new(336., 257.),
        ],
        Stroke::new(1., p.border.gamma_multiply(0.7)),
    );
    let capture_rect = at(356., 102., 92., 92.);
    let capture = ui
        .add_enabled_ui(enabled, |ui| {
            let response = ui.interact(
                capture_rect,
                ui.id().with("capture-now"),
                egui::Sense::click(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    if settings.capture_mode == CaptureMode::Text {
                        "Select region to copy text"
                    } else if settings.capture_mode == CaptureMode::Video {
                        "Set up video"
                    } else {
                        "Capture now"
                    },
                )
            });
            let pressed = response.is_pointer_button_down_on();
            let hover =
                ui.ctx()
                    .animate_bool_with_time(response.id.with("glow"), response.hovered(), 0.15);
            if ui.is_enabled() {
                // A soft halo lifts the one primary action off the surface.
                for (radius, alpha) in [(54., 0.10), (50., 0.16)] {
                    ui.painter().circle_filled(
                        capture_rect.center(),
                        radius + hover * 2.,
                        Color32::from_rgb(255, 86, 74).gamma_multiply(alpha + hover * 0.08),
                    );
                }
                appearance::satin_with_palette(
                    ui.painter(),
                    capture_rect,
                    46.,
                    true,
                    pressed,
                    appearance::Palette {
                        accent: Color32::from_rgb(214, 38, 44),
                        accent_top: Color32::from_rgb(255, 106, 84)
                            .lerp_to_gamma(Color32::from_rgb(255, 136, 112), hover),
                        ..p
                    },
                );
                ui.painter().rect_stroke(
                    capture_rect,
                    46,
                    Stroke::new(1., Color32::from_rgb(255, 150, 132)),
                    egui::StrokeKind::Inside,
                );
            } else {
                ui.painter()
                    .circle_filled(capture_rect.center(), 46., p.base);
            }
            let color = if ui.is_enabled() {
                Color32::WHITE
            } else {
                p.muted
            };
            let nudge = Vec2::new(0., if pressed { 1. } else { 0. });
            appearance::icon(
                ui.painter(),
                capture_rect.center() - Vec2::new(0., 12.) + nudge,
                if settings.capture_mode == CaptureMode::Text {
                    Icon::TextSize
                } else if settings.capture_mode == CaptureMode::Video {
                    Icon::Video
                } else {
                    Icon::Capture
                },
                color,
                24.,
            );
            ui.painter().text(
                capture_rect.center() + Vec2::new(0., 16.) + nudge,
                egui::Align2::CENTER_CENTER,
                if settings.capture_mode == CaptureMode::Text {
                    "Copy text"
                } else if settings.capture_mode == CaptureMode::Video {
                    "Set up video"
                } else {
                    "Capture"
                },
                egui::FontId::proportional(14.),
                color,
            );
            if response.has_focus() {
                ui.painter().rect_stroke(
                    capture_rect.expand(3.),
                    49,
                    Stroke::new(1.5, p.ginger),
                    egui::StrokeKind::Outside,
                );
            }
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        })
        .inner;
    hint(
        &capture,
        if !enabled {
            "Finish the current task in Editor first"
        } else if cfg!(target_os = "macos") {
            "Also ⌃ ⇧ .  or  ⌘ ⇧ ."
        } else {
            "Also Ctrl + Shift + ."
        },
    );
    if capture.clicked() {
        action = Some(Action::Capture);
    }
    // The shortcut reads as raised keycaps: visible at a glance, not a caption.
    let keys = settings.capture_shortcut.keycaps();
    let font = egui::FontId::proportional(if keys.len() > 3 { 10.5 } else { 12.5 });
    let caps: Vec<_> = keys
        .iter()
        .map(|key| {
            ui.painter()
                .layout_no_wrap(key.clone(), font.clone(), p.ink)
        })
        .collect();
    let mut rows: Vec<Vec<_>> = vec![Vec::new()];
    let mut row_width = 0.;
    for cap in caps {
        let width = (cap.size().x + 10.).max(22.);
        if row_width + width > 116. && !rows.last().unwrap().is_empty() {
            rows.push(Vec::new());
            row_width = 0.;
        }
        row_width += width + 4.;
        rows.last_mut().unwrap().push((cap, width));
    }
    let hotkey_rect = Rect::from_center_size(
        egui::pos2(capture_rect.center().x, bounds.top() + 226.),
        Vec2::new(116., 54.),
    );
    let edit = ui.interact(
        hotkey_rect,
        ui.id().with("edit-capture-hotkey"),
        egui::Sense::click(),
    );
    edit.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!(
                "Change capture hotkey: {}",
                settings.capture_shortcut.label()
            ),
        )
    });
    hint(&edit, "Change capture hotkey");
    if edit.clicked() {
        action = Some(Action::EditHotkey);
    }
    let highlighted = edit.hovered() || edit.has_focus();
    edit.on_hover_cursor(egui::CursorIcon::PointingHand);
    for (row, caps) in rows.into_iter().enumerate() {
        let total = caps.iter().map(|(_, width)| width).sum::<f32>() + 4. * (caps.len() - 1) as f32;
        let mut x = capture_rect.center().x - total * 0.5;
        for (cap, width) in caps {
            let key = Rect::from_min_size(
                egui::pos2(x, bounds.top() + 203. + row as f32 * 25.),
                Vec2::new(width, 21.),
            );
            ui.painter()
                .rect_filled(key.translate(Vec2::new(0., 2.)), 5, p.navy);
            appearance::gradient(
                ui.painter(),
                key,
                5.,
                p.base.lerp_to_gamma(p.accent_top, 0.25),
                p.cream,
            );
            ui.painter().rect_stroke(
                key,
                5,
                Stroke::new(
                    if highlighted { 1.5 } else { 1. },
                    if highlighted {
                        p.ginger
                    } else {
                        p.accent_top.gamma_multiply(0.7)
                    },
                ),
                egui::StrokeKind::Inside,
            );
            ui.painter()
                .galley(key.center() - cap.size() * 0.5, cap, p.ink);
            x += width + 4.;
        }
    }
    (changed, action)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launcher_controls_fit_and_update_shared_settings() {
        let ctx = egui::Context::default();
        appearance::apply(&ctx);
        let mut settings = Settings::default();
        let displays = [DisplayInfo {
            id: 1,
            name: "Test".into(),
            x: 0,
            y: 0,
            width: 1470,
            height: 956,
            scale_factor: 2.,
            is_primary: true,
        }];
        settings.display_id = Some(1);
        let mut frame = |events: Vec<egui::Event>, enabled| {
            let mut result = (false, None);
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            result = launcher_content(ui, &mut settings, &displays, enabled);
                        });
                },
            );
            (output, result)
        };
        frame(vec![], true);
        let (output, _) = frame(vec![], true);
        for shape in &output.shapes {
            if let egui::Shape::Text(t) = &shape.shape {
                let rect = Rect::from_min_size(t.pos, t.galley.size());
                assert!(
                    Rect::from_min_size(Pos2::ZERO, SIZE)
                        .expand(1.)
                        .contains_rect(rect),
                    "{} outside panel: {rect:?}",
                    t.galley.job.text
                );
            }
        }
        let center = |output: &egui::FullOutput, label: &str| {
            output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => {
                        Some(Rect::from_min_size(t.pos, t.galley.size()).center())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing {label}"))
        };
        // The quiet header icons keep their full accessible names and actions.
        for (at, expected) in [
            (egui::pos2(353., 30.), Action::Editor),
            (egui::pos2(383., 30.), Action::Library),
            (egui::pos2(413., 30.), Action::Settings),
            (egui::pos2(443., 30.), Action::Close),
            (egui::pos2(402., 213.), Action::EditHotkey),
        ] {
            let event = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(vec![egui::Event::PointerMoved(at), event(true)], true);
            assert_eq!(frame(vec![event(false)], true).1.1, Some(expected));
        }
        let at = center(&output, "Capture");
        let event = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(at), event(true)], false);
        assert_eq!(frame(vec![event(false)], false).1.1, None);
        for label in [
            "Multi-region",
            "Clipboard",
            "Include pointer",
            "Capture on release",
            "5s",
            "50%",
            "Capture",
        ] {
            let (output, _) = frame(vec![], true);
            let at = center(&output, label);
            let event = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(vec![egui::Event::PointerMoved(at), event(true)], true);
            let (_, result) = frame(vec![event(false)], true);
            if label == "Capture" {
                assert_eq!(result.1, Some(Action::Capture));
            } else {
                assert!(result.0, "{label} did not change");
            }
        }
        assert_eq!(settings.capture_mode, CaptureMode::MultiRegion);
        assert_eq!(settings.capture_destination, CaptureDestination::Clipboard);
        assert!(settings.include_pointer && settings.capture_on_release);
        assert_eq!(settings.delay_seconds, 5);
        assert_eq!(
            settings.capture_output_size,
            crate::capture_size::OutputSize::Half
        );
        assert!(
            !ctx.style().visuals.dark_mode,
            "Launcher styling must not change the editor theme"
        );
        assert_eq!(
            appearance::palette(&ctx).paper,
            appearance::GuiFinish::CobaltSatin.palette().paper
        );
    }

    #[test]
    fn text_capture_is_available_without_image_destination_controls() {
        let ctx = egui::Context::default();
        let mut settings = Settings {
            capture_mode: CaptureMode::Text,
            capture_destination: CaptureDestination::Editor,
            include_pointer: true,
            ..Default::default()
        };
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        launcher_content(ui, &mut settings, &[], true);
                    });
            },
        );
        let labels: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(labels.contains(&"Text"));
        assert!(labels.contains(&"Text → clipboard · no image saved"));
        assert!(labels.contains(&"Copy text"));
        assert!(!labels.contains(&"Clipboard"));
        assert_eq!(settings.capture_destination, CaptureDestination::Editor);
        assert!(settings.include_pointer);
    }

    #[test]
    fn every_theme_paints_the_whole_panel_and_leaves_the_editor_theme_alone() {
        let ctx = egui::Context::default();
        appearance::apply(&ctx);
        for theme in crate::capture_panel::CaptureTheme::ALL {
            let mut settings = Settings {
                capture_theme: theme,
                ..Default::default()
            };
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            launcher_content(ui, &mut settings, &[], true);
                        });
                },
            );
            let look = theme.look();
            for label in [
                "Capture Now",
                "Region",
                "Send to",
                "Capture on release",
                "Capture",
            ] {
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => Some(text),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("{theme:?} is missing {label}"));
                assert_ne!(
                    text.fallback_color, look.palette.paper,
                    "{theme:?} paints {label} in its own surface color"
                );
            }
            assert_eq!(settings.capture_theme, theme);
            assert_eq!(
                look.dark,
                theme != crate::capture_panel::CaptureTheme::Porcelain
            );
        }
        assert!(!ctx.style().visuals.dark_mode);
        let restored: Settings = serde_json::from_str(
            &serde_json::to_string(&Settings {
                capture_theme: crate::capture_panel::CaptureTheme::Aurora,
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            restored.capture_theme,
            crate::capture_panel::CaptureTheme::Aurora
        );
        assert_eq!(
            serde_json::from_str::<Settings>("{}")
                .unwrap()
                .capture_theme,
            crate::capture_panel::CaptureTheme::Midnight
        );
    }

    #[test]
    fn hover_help_appears_in_the_header_and_never_as_a_popup_over_controls() {
        let ctx = egui::Context::default();
        appearance::apply(&ctx);
        let mut settings = Settings::default();
        let mut frame = |events: Vec<egui::Event>| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            launcher_content(ui, &mut settings, &[], true);
                        });
                },
            )
        };
        let texts = |output: &egui::FullOutput| -> Vec<(String, Rect)> {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.job.text.clone(),
                        Rect::from_min_size(text.pos, text.galley.size()),
                    )),
                    _ => None,
                })
                .collect()
        };
        let idle = frame(vec![]);
        let idle_texts = texts(&idle);
        assert!(idle_texts.iter().any(|(text, _)| text == "Ready"));
        let switch = idle_texts
            .iter()
            .find(|(text, _)| text == "Capture on release")
            .unwrap()
            .1
            .center();
        frame(vec![egui::Event::PointerMoved(switch)]);
        // Stay long enough for any tooltip delay to have elapsed.
        for _ in 0..3 {
            frame(vec![]);
        }
        let hovered = frame(vec![]);
        let hovered_texts = texts(&hovered);
        let help = hovered_texts
            .iter()
            .find(|(text, _)| text == "Capture on release, without confirming")
            .expect("help is shown in the header");
        assert!(
            help.1.bottom() < 44.,
            "help stays in the header: {:?}",
            help.1
        );
        assert!(!hovered_texts.iter().any(|(text, _)| text == "Ready"));
        assert_eq!(
            hovered_texts.len(),
            idle_texts.len(),
            "no extra popup text is painted over the controls"
        );
        frame(vec![egui::Event::PointerGone]);
        frame(vec![]);
        assert!(
            texts(&frame(vec![]))
                .iter()
                .any(|(text, _)| text == "Ready")
        );
    }

    #[test]
    fn anchored_panel_stays_on_display_and_opens_above_bottom_tray() {
        for screen in [
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1470., 956.)),
            Rect::from_min_size(egui::pos2(-1920., -200.), Vec2::new(1920., 1080.)),
        ] {
            for at in [
                screen.left_top(),
                screen.right_top(),
                screen.left_bottom(),
                screen.right_bottom(),
            ] {
                let anchor = Rect::from_center_size(at, Vec2::splat(24.));
                let rect = Rect::from_min_size(panel_position(screen, anchor, SIZE), SIZE);
                assert!(screen.contains_rect(rect));
                if at.y == screen.bottom() {
                    assert!(rect.bottom() < screen.bottom());
                }
            }
        }
    }
}
