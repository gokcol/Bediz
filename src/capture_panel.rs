//! Capture home: visual targets, compact options and the resident Capture Now entry point.
use crate::{
    appearance,
    capture::{CaptureDestination, CaptureMode, DisplayInfo},
    selection::CaptureShape,
    storage::Settings,
};
use eframe::egui::{self, Color32, Rect, Stroke, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Capture,
    EditHotkey,
    Panel,
    RefreshDisplays,
    ChooseFolder,
    Library,
    Background,
    Permission,
}
#[derive(Default)]
pub struct Response {
    pub changed: bool,
    pub action: Option<Action>,
}
pub struct Availability {
    pub capture: bool,
    pub background: bool,
    pub permission_needed: bool,
    pub demo: bool,
}
pub(crate) const CAPTURE_PANEL_SHORTCUT_HEIGHT: f32 = 49.;
/// Capture Now has its own look, independent of the editor's GUI finish.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureTheme {
    #[default]
    Midnight,
    Ember,
    Aurora,
    Porcelain,
}

/// Everything the Capture Now panel paints with for one theme.
#[derive(Clone, Copy)]
pub struct CaptureLook {
    pub palette: appearance::Palette,
    /// Top of the surface gradient; `palette.paper` is its bottom.
    pub top: Color32,
    /// Track of a switch that is off.
    pub off: Color32,
    pub dark: bool,
}

impl CaptureTheme {
    pub const ALL: [Self; 4] = [Self::Midnight, Self::Ember, Self::Aurora, Self::Porcelain];
    pub fn label(self) -> &'static str {
        match self {
            Self::Midnight => "Midnight",
            Self::Ember => "Ember",
            Self::Aurora => "Aurora",
            Self::Porcelain => "Porcelain",
        }
    }
    pub fn look(self) -> CaptureLook {
        // ink, muted, accent, recess, surface, highlight, hover, border, base, accent top, top, off
        let colors: [u32; 12] = match self {
            Self::Midnight => [
                0xf3f5fc, 0xacb5d4, 0x4f63f0, 0x151930, 0x1f2444, 0xcad1ff, 0x323b6e, 0x40497a,
                0x343c68, 0x7c8cff, 0x313968, 0x464f7a,
            ],
            Self::Ember => [
                0xf6f3ef, 0xbdb6ad, 0xdd5a22, 0x1a1b1e, 0x292a2f, 0xffcfb0, 0x44454c, 0x55565e,
                0x494a51, 0xff9458, 0x3e3f46, 0x696a72,
            ],
            Self::Aurora => [
                0xeefaf7, 0xa5c6c3, 0x0e9486, 0x0d2327, 0x153238, 0xa9f1e4, 0x23505a, 0x2f5f68,
                0x285058, 0x3dd0b9, 0x22494f, 0x4a7078,
            ],
            Self::Porcelain => [
                0x1f2637, 0x66708a, 0x2b65d9, 0xe4e9f4, 0xedf1f9, 0x2b65d9, 0xd8e3fb, 0xc0cadd,
                0xdbe2ef, 0x5a8df2, 0xffffff, 0xa9b4c8,
            ],
        };
        let c = colors.map(|hex| Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8));
        CaptureLook {
            palette: appearance::Palette {
                ink: c[0],
                muted: c[1],
                accent: c[2],
                navy: c[3],
                paper: c[4],
                ginger: c[5],
                cream: c[6],
                border: c[7],
                base: c[8],
                accent_top: c[9],
                surface: c[10],
            },
            top: c[10],
            off: c[11],
            dark: self != Self::Porcelain,
        }
    }
}

/// Scope a Capture Now theme to `ui` and its popups; the editor theme is untouched.
pub fn apply_look(ui: &mut egui::Ui, look: &CaptureLook) {
    let p = look.palette;
    let v = ui.visuals_mut();
    *v = if look.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = Some(p.ink);
    v.weak_text_color = Some(p.muted);
    v.window_fill = p.paper;
    v.panel_fill = p.paper;
    v.window_stroke = Stroke::new(1., p.border);
    v.extreme_bg_color = p.navy;
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1., Color32::WHITE);
    for (widget, fill, stroke) in [
        (&mut v.widgets.inactive, p.navy, p.border),
        (&mut v.widgets.hovered, p.cream, p.accent_top),
        (&mut v.widgets.active, p.cream, p.accent_top),
        (&mut v.widgets.open, p.navy, p.accent_top),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::new(1., stroke);
        widget.fg_stroke = Stroke::new(1., p.ink);
        widget.corner_radius = egui::CornerRadius::same(8);
        widget.expansion = 0.;
    }
}

pub fn display_label(index: usize, display: &DisplayInfo) -> String {
    format!(
        "Display {}{}",
        index + 1,
        if display.is_primary { " · Main" } else { "" }
    )
}
pub fn selected_display_label(settings: &Settings, displays: &[DisplayInfo]) -> String {
    if settings.capture_all_displays {
        "Any display".into()
    } else {
        displays
            .iter()
            .enumerate()
            .find(|(_, d)| Some(d.id) == settings.display_id)
            .map(|(i, d)| display_label(i, d))
            .unwrap_or_else(|| "No display available".into())
    }
}

pub fn display_choices(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    displays: &[DisplayInfo],
) -> bool {
    let before = (settings.capture_all_displays, settings.display_id);
    if ui
        .selectable_label(settings.capture_all_displays, "Any display")
        .on_hover_text("Start capture, then select on any connected screen")
        .clicked()
    {
        settings.capture_all_displays = true;
    }
    for (i, display) in displays.iter().enumerate() {
        if ui
            .selectable_label(
                !settings.capture_all_displays && settings.display_id == Some(display.id),
                display_label(i, display),
            )
            .on_hover_text(format!(
                "{} · {} × {} · {:.0}%",
                display.name,
                display.width,
                display.height,
                display.scale_factor * 100.
            ))
            .clicked()
        {
            settings.capture_all_displays = false;
            settings.display_id = Some(display.id);
        }
    }
    before != (settings.capture_all_displays, settings.display_id)
}

/// A folder path that reads at a glance: the home folder becomes `~`.
pub fn folder_label(path: &std::path::Path) -> String {
    directories::UserDirs::new()
        .and_then(|dirs| {
            path.strip_prefix(dirs.home_dir())
                .ok()
                .map(|rest| std::path::Path::new("~").join(rest).display().to_string())
        })
        .unwrap_or_else(|| path.display().to_string())
}

pub fn delay_label(seconds: u32) -> String {
    match seconds {
        0 => "No delay".into(),
        1 => "1 second".into(),
        n => format!("{n} seconds"),
    }
}
/// Output scaling never changes selection coordinates or the full-size preview.
pub fn output_size_picker(ui: &mut egui::Ui, size: &mut crate::capture_size::OutputSize) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt("capture-output-size")
        .selected_text(format!("Output: {}", size.label())).width(142.)
        .popup_style(egui::style::StyleModifier::new({ let visuals = ui.visuals().clone(); move |style: &mut egui::Style| style.visuals = visuals.clone() }))
        .show_ui(ui, |ui| {
            for choice in crate::capture_size::OutputSize::ALL { changed |= ui.selectable_value(size, choice, choice.label()).changed(); }
        }).response.on_hover_text("Resize the finished image before saving or copying. Percentages apply to width and height. Selection stays at full resolution.");
    changed
}

pub fn menu_summary(settings: &Settings, _shape: CaptureShape, displays: &[DisplayInfo]) -> String {
    let display = selected_display_label(settings, displays);
    let destination = match settings.capture_mode {
        CaptureMode::Text => "Clipboard",
        CaptureMode::Video => "Recordings",
        _ => settings
            .album_capture
            .selected()
            .map_or(settings.capture_destination.label(), |(_, title)| title),
    };
    format!(
        "{} · {display} · {} · {}",
        settings.capture_mode.label(),
        delay_label(settings.delay_seconds),
        destination,
    )
}

pub(crate) fn destination_icon(destination: CaptureDestination) -> appearance::Icon {
    match destination {
        CaptureDestination::Editor => appearance::Icon::Edit,
        CaptureDestination::Clipboard => appearance::Icon::Paste,
    }
}

pub(crate) fn destination_picker(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    palette: appearance::Palette,
    width: f32,
    compact: bool,
) -> bool {
    enum Pick {
        Standard(CaptureDestination),
        Album(std::path::PathBuf),
    }
    let album = settings.album_capture.selected();
    let destination = settings.capture_destination;
    let (label, icon) = album.map_or(
        (destination.label(), destination_icon(destination)),
        |(_, title)| (title, appearance::Icon::Album),
    );
    let response =
        ui.add(egui::Button::new("").min_size(Vec2::new(width, ui.spacing().interact_size.y)));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::ComboBox,
            ui.is_enabled(),
            format!("Destination: {label}"),
        )
    });
    let rect = response.rect;
    let ink = ui.style().interact(&response).text_color();
    appearance::icon(
        ui.painter(),
        rect.left_center() + Vec2::new(16., 0.),
        icon,
        ink,
        17.,
    );
    if !compact {
        picker_text(ui, rect, label, ink, 31.);
    }
    let arrow = rect.right_center() - Vec2::new(11., 0.);
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            arrow + Vec2::new(-3.5, -1.5),
            arrow + Vec2::new(3.5, -1.5),
            arrow + Vec2::new(0., 2.5),
        ],
        ink,
        Stroke::NONE,
    ));
    let visuals = ui.visuals().clone();
    let popup_width = width.max(218.);
    let mut picked = None;
    egui::Popup::menu(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(
            egui::Frame::popup(ui.style())
                .fill(palette.surface)
                .stroke(Stroke::new(1., palette.border)),
        )
        .style(egui::style::StyleModifier::new(move |style| {
            style.visuals = visuals.clone()
        }))
        .width(popup_width)
        .show(|ui| {
            ui.set_width(popup_width);
            for choice in CaptureDestination::ALL {
                if capture_menu_item(
                    ui,
                    choice.label(),
                    album.is_none() && destination == choice,
                    palette,
                    |painter, center, ink| {
                        appearance::icon(painter, center, destination_icon(choice), ink, 17.);
                    },
                )
                .on_hover_text(choice.hint())
                .clicked()
                {
                    picked = Some(Pick::Standard(choice));
                    ui.close();
                }
            }
            if settings.album_capture.destinations().next().is_some() {
                ui.separator();
                ui.weak("Ready albums");
                let height = (ui.ctx().content_rect().height() * 0.44).clamp(82., 218.);
                egui::ScrollArea::vertical()
                    .max_height(height)
                    .show(ui, |ui| {
                        for (path, title) in settings.album_capture.destinations() {
                            if capture_menu_item(
                                ui,
                                title,
                                album.is_some_and(|(selected, _)| selected == path),
                                palette,
                                |painter, center, ink| {
                                    appearance::icon(
                                        painter,
                                        center,
                                        appearance::Icon::Album,
                                        ink,
                                        17.,
                                    );
                                },
                            )
                            .on_hover_text(format!("{title}\n{}", folder_label(path)))
                            .clicked()
                            {
                                picked = Some(Pick::Album(path.to_owned()));
                                ui.close();
                            }
                        }
                    });
            }
        });
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if album.is_some() {
            format!("Add to {label}")
        } else {
            format!("To {label} · {}", destination.hint())
        });
    match picked {
        Some(Pick::Standard(choice)) => {
            let changed = settings.album_capture.choose(None) || destination != choice;
            settings.capture_destination = choice;
            changed
        }
        Some(Pick::Album(path)) => settings.album_capture.choose(Some(&path)),
        None => false,
    }
}

pub(crate) fn capture_menu_item(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    palette: appearance::Palette,
    paint_icon: impl FnOnce(&egui::Painter, egui::Pos2, Color32),
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 27.), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    let fill = if selected {
        palette.surface.lerp_to_gamma(palette.accent, 0.27)
    } else if response.hovered() || response.has_focus() {
        palette.cream
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 6, fill);
    paint_icon(
        ui.painter(),
        rect.left_center() + Vec2::new(15., 0.),
        palette.ink,
    );
    picker_text(ui, rect, label, palette.ink, 29.);
    if selected {
        appearance::icon(
            ui.painter(),
            rect.right_center() - Vec2::new(12., 0.),
            appearance::Icon::Check,
            palette.ink,
            11.,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn picker_text(ui: &egui::Ui, rect: Rect, label: &str, ink: Color32, inset: f32) {
    let text = egui::WidgetText::from(egui::RichText::new(label).size(13.).color(ink)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (rect.width() - inset - 23.).max(1.),
        egui::TextStyle::Body,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + inset, rect.center().y - text.size().y * 0.5),
        text,
        ink,
    );
}
fn card(palette: appearance::Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(palette.surface)
        .corner_radius(14)
        .inner_margin(16)
        .stroke(Stroke::new(1., palette.border))
        .shadow(egui::Shadow {
            offset: [0, 3],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(21),
        })
}

pub fn ui(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    _shape: &mut CaptureShape,
    displays: &[DisplayInfo],
    available: Availability,
    cat: Option<egui::TextureId>,
) -> Response {
    let mut result = Response::default();
    egui::ScrollArea::vertical()
        .id_salt("capture-console-scroll")
        // Keep the scrollbar at the workspace edge while the cards stay centered.
        .auto_shrink([false, false])
        .show(ui, |ui| {
            const GAP: f32 = 12.;
            let width = (ui.available_width() - 28.).clamp(1., 880.);
            ui.add_space(14.);
            ui.vertical_centered(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(width, 0.),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing = Vec2::new(GAP, GAP);
                        capture_header(ui, cat, &available, &mut result);

                        let columns = if width >= 660. {
                            6
                        } else if width >= 345. {
                            3
                        } else {
                            2
                        };
                        let tile_width = (width - GAP * (columns - 1) as f32) / columns as f32;
                        for row in CaptureMode::ALL.chunks(columns) {
                            ui.horizontal(|ui| {
                                for &mode in row {
                                    if mode_tile(
                                        ui,
                                        mode,
                                        settings.capture_mode == mode,
                                        tile_width,
                                    )
                                    .clicked()
                                    {
                                        result.changed |= settings.capture_mode != mode;
                                        settings.capture_mode = mode;
                                    }
                                }
                            });
                        }

                        if width >= 680. {
                            let launch_width = width * 0.36;
                            ui.horizontal_top(|ui| {
                                capture_options(
                                    ui,
                                    width - launch_width - GAP,
                                    settings,
                                    displays,
                                    &mut result,
                                );
                                launch_card(
                                    ui,
                                    launch_width,
                                    settings,
                                    available.capture,
                                    &mut result,
                                );
                            });
                        } else {
                            launch_card(ui, width, settings, available.capture, &mut result);
                            capture_options(ui, width, settings, displays, &mut result);
                        }
                        if available.demo {
                            ui.weak("Demo workspace · generated test image");
                        }
                        ui.add_space(6.);
                    },
                );
            });
        });
    result
}

fn capture_header(
    ui: &mut egui::Ui,
    cat: Option<egui::TextureId>,
    available: &Availability,
    result: &mut Response,
) {
    let p = appearance::palette(ui.ctx());
    let wide = ui.available_width() >= 745.;
    let title = |ui: &mut egui::Ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(42.), egui::Sense::hover());
        appearance::gradient(
            ui.painter(),
            rect,
            11.,
            p.navy.lerp_to_gamma(p.accent, 0.42),
            p.navy,
        );
        if let Some(cat) = cat {
            ui.painter().image(
                cat,
                rect.shrink(3.),
                Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                Color32::WHITE,
            );
        } else {
            appearance::icon(
                ui.painter(),
                rect.center(),
                appearance::Icon::Capture,
                Color32::WHITE,
                22.,
            );
        }
        ui.label(
            egui::RichText::new("Capture your screen")
                .size(25.)
                .strong(),
        );
    };
    let mut actions = |ui: &mut egui::Ui| {
        if capture_panel_shortcut(ui, available.background).clicked() {
            result.action = Some(Action::Panel);
        }
        if appearance::toolbar_action_button(
            ui,
            "Keep ready in background",
            appearance::Icon::Power,
            available.background,
            false,
        )
        .clicked()
        {
            result.action = Some(Action::Background);
        }
    };
    if wide {
        ui.horizontal(|ui| {
            title(ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
        });
    } else {
        ui.horizontal_wrapped(|ui| {
            title(ui);
            actions(ui);
        });
    }
    if available.permission_needed {
        egui::Frame::new()
            .fill(p.cream)
            .corner_radius(8)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Allow screen recording to capture other apps.");
                    if ui.small_button("Allow screen capture…").clicked() {
                        result.action = Some(Action::Permission);
                    }
                });
            });
    }
}

pub(crate) fn capture_panel_shortcut(ui: &mut egui::Ui, in_background: bool) -> egui::Response {
    let mac = cfg!(target_os = "macos");
    let p = appearance::palette(ui.ctx());
    let title = if !in_background {
        "Quick access"
    } else if mac {
        "Capture in the menu bar"
    } else {
        "Capture in the system tray"
    };
    let heading = ui
        .painter()
        .layout_no_wrap(title.into(), egui::FontId::proportional(14.), p.ink);
    let action = ui.painter().layout_no_wrap(
        "Open Capture panel".into(),
        egui::FontId::proportional(11.5),
        p.muted,
    );
    let width = heading.size().x.max(action.size().x) + 81.;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, CAPTURE_PANEL_SHORTCUT_HEIGHT),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!("{title}. Open Capture panel"),
        )
    });
    let painter = ui.painter();
    let highlighted = response.hovered() || response.has_focus();
    painter.rect_filled(rect, 9, if highlighted { p.cream } else { p.surface });
    painter.rect_stroke(
        rect,
        9,
        Stroke::new(1., if highlighted { p.accent } else { p.border }),
        egui::StrokeKind::Inside,
    );

    let preview =
        Rect::from_center_size(rect.left_center() + Vec2::new(28., 0.), Vec2::new(40., 25.));
    painter.rect_filled(preview, 5, p.paper);
    painter.rect_stroke(
        preview,
        5,
        Stroke::new(1., p.border),
        egui::StrokeKind::Inside,
    );
    if in_background {
        let band_y = if mac {
            preview.top() + 8.
        } else {
            preview.bottom() - 8.
        };
        let status = Rect::from_center_size(
            egui::pos2(preview.center().x, band_y),
            Vec2::new(preview.width() - 5., 12.),
        );
        painter.rect_filled(status, 3, p.base);
        for offset in [6., 11., 16.] {
            painter.circle_filled(status.left_center() + Vec2::new(offset, 0.), 1., p.muted);
        }
        let camera = status.right_center() - Vec2::new(8., 0.);
        painter.rect_filled(
            Rect::from_center_size(camera, Vec2::splat(15.)),
            4,
            p.accent,
        );
        appearance::icon(
            painter,
            camera,
            appearance::Icon::Capture,
            Color32::WHITE,
            11.,
        );
    } else {
        appearance::icon(
            painter,
            preview.center(),
            appearance::Icon::Capture,
            p.accent,
            16.,
        );
    }

    let text = rect.left_top() + Vec2::new(56., 8.);
    painter.galley(text, heading, p.ink);
    painter.galley(text + Vec2::new(0., 19.), action, p.muted);
    let arrow = rect.right_center() - Vec2::new(12., 0.);
    painter.add(egui::Shape::line(
        vec![
            arrow + Vec2::new(-3., -3.),
            arrow,
            arrow + Vec2::new(-3., 3.),
        ],
        Stroke::new(1.3, p.muted),
    ));
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if in_background && !mac {
        response.on_hover_text("Look for the Bediz camera by the clock or under Show hidden icons.")
    } else {
        response
    }
}

fn capture_options(
    ui: &mut egui::Ui,
    width: f32,
    settings: &mut Settings,
    displays: &[DisplayInfo],
    result: &mut Response,
) {
    let p = appearance::palette(ui.ctx());
    ui.allocate_ui_with_layout(Vec2::new(width, 0.), egui::Layout::top_down(egui::Align::Min), |ui| {
        card(p).show(ui, |ui| {
            ui.set_width((width - 34.).max(1.));
            ui.spacing_mut().item_spacing = Vec2::new(8., 9.);
            ui.spacing_mut().interact_size.y = 28.;
            option_heading(ui, "Capture options", appearance::Icon::Settings);
            ui.horizontal(|ui| {
                let label = selected_display_label(settings, displays);
                egui::ComboBox::from_id_salt("capture-display")
                    .selected_text(label)
                    .width((ui.available_width() - 40.).max(60.))
                    .show_ui(ui, |ui| {
                        result.changed |= display_choices(ui, settings, displays);
                    });
                if appearance::document_icon_button(ui, "Find connected displays", appearance::Icon::Refresh).clicked() {
                    result.action = Some(Action::RefreshDisplays);
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.weak("Timer");
                result.changed |= appearance::segmented(ui, "capture-timer", &mut settings.delay_seconds, &[
                    (0, "Off", None, "Capture immediately"),
                    (3, "3s", None, "Wait 3 seconds before capturing"),
                    (5, "5s", None, "Wait 5 seconds before capturing"),
                    (10, "10s", None, "Wait 10 seconds before capturing"),
                ]);
            });
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(14., 6.);
                result.changed |= ui.add_enabled_ui(settings.capture_mode != CaptureMode::Text, |ui| {
                    appearance::toggle(ui, &mut settings.include_pointer, "Include pointer")
                        .on_hover_text("Show the mouse pointer in the saved capture.")
                        .changed()
                }).inner;
                result.changed |= ui.add_enabled_ui(settings.capture_mode != CaptureMode::Video, |ui| {
                    appearance::toggle(ui, &mut settings.capture_on_release, "Capture immediately after selection")
                        .on_hover_text("Capture when you release a region or click a window. Multiple regions still wait for Capture or Enter.")
                        .changed()
                }).inner;
            });

            match settings.capture_mode {
                CaptureMode::Text => {}
                CaptureMode::Video => {
                    ui.separator();
                    option_heading(ui, "Recording options", appearance::Icon::Video);
                    let look = CaptureLook {
                        palette: p,
                        dark: p.is_dark(),
                        ..settings.capture_theme.look()
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Record");
                        result.changed |= settings.video.target.ui(ui, &look);
                    });
                    use crate::app::video_session::{VideoTarget, WINDOW_COMPOSITION_SIZE, preferences_ui};
                    let dimensions = (settings.video.target == VideoTarget::MultipleWindows)
                        .then_some(WINDOW_COMPOSITION_SIZE);
                    result.changed |= preferences_ui(
                        ui, &mut settings.video, &settings.output_dir, dimensions, &look,
                    );
                    library_destination(ui, settings, result);
                }
                _ => {
                    ui.separator();
                    option_heading(ui, "After capture", appearance::Icon::Edit);
                    ui.horizontal_wrapped(|ui| {
                        if settings.album_capture.destinations().next().is_some() {
                            result.changed |= destination_picker(ui, settings, p, 180., false);
                        } else {
                            let destinations = CaptureDestination::ALL.map(|destination| (
                                destination, destination.label(), Some(destination_icon(destination)), destination.hint()
                            ));
                            result.changed |= appearance::segmented(
                                ui, "capture-destination", &mut settings.capture_destination, &destinations,
                            );
                        }
                        result.changed |= output_size_picker(ui, &mut settings.capture_output_size);
                    });
                    library_destination(ui, settings, result);
                }
            }
        });
    });
}

fn option_heading(ui: &mut egui::Ui, label: &str, glyph: appearance::Icon) {
    let p = appearance::palette(ui.ctx());
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(23.), egui::Sense::hover());
        ui.painter().rect_filled(rect, 6, p.cream);
        appearance::icon(ui.painter(), rect.center(), glyph, p.ink, 14.);
        ui.label(egui::RichText::new(label).size(15.).strong());
    });
}

fn library_destination(ui: &mut egui::Ui, settings: &Settings, result: &mut Response) {
    let p = appearance::palette(ui.ctx());
    let video = settings.capture_mode == CaptureMode::Video;
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        option_heading(
            ui,
            if video {
                "Save recordings"
            } else {
                "Save to library"
            },
            appearance::Icon::Open,
        );
        if appearance::action_button(ui, "Change…", appearance::Icon::Open, true, false)
            .on_hover_text("Choose where captures are saved")
            .clicked()
        {
            result.action = Some(Action::ChooseFolder);
        }
        if !video
            && appearance::action_button(ui, "Browse", appearance::Icon::Library, true, false)
                .on_hover_text("Open your Library")
                .clicked()
        {
            result.action = Some(Action::Library);
        }
    });
    egui::Frame::new()
        .fill(p.base)
        .corner_radius(7)
        .inner_margin(egui::Margin::symmetric(9, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let folder = if video {
                settings.output_dir.join("Recordings")
            } else {
                settings.output_dir.clone()
            };
            ui.add(
                egui::Label::new(egui::RichText::new(folder_label(&folder)).size(12.5)).truncate(),
            )
            .on_hover_text(folder.display().to_string());
        });
    ui.small(if video {
        "MP4"
    } else {
        "PNG · Organized by year / month / day"
    });
}

fn launch_card(
    ui: &mut egui::Ui,
    width: f32,
    settings: &Settings,
    enabled: bool,
    result: &mut Response,
) {
    let look = settings.capture_theme.look();
    let p = look.palette;
    ui.allocate_ui_with_layout(
        Vec2::new(width, 0.),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            card(appearance::Palette {
                surface: p.paper,
                ..p
            })
            .show(ui, |ui| {
                apply_look(ui, &look);
                ui.set_width((width - 34.).max(1.));
                ui.spacing_mut().item_spacing = Vec2::new(8., 9.);

                capture_preview(ui, settings.capture_mode, &look);
                ui.label(
                    egui::RichText::new(settings.capture_mode.label())
                        .size(23.)
                        .strong(),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(settings.capture_mode.hint())
                            .size(13.)
                            .color(p.muted),
                    )
                    .wrap(),
                );
                let button_size = Vec2::new(ui.available_width(), 42.);
                if ui
                    .add_enabled_ui(enabled, |ui| match settings.capture_mode {
                        CaptureMode::Video => appearance::capture_button_with_icon(
                            ui,
                            "Capture Now",
                            button_size,
                            appearance::Icon::Video,
                        ),
                        CaptureMode::Text => appearance::capture_button_with_icon(
                            ui,
                            "Capture Now",
                            button_size,
                            appearance::Icon::Scan,
                        ),
                        _ => appearance::capture_button(ui, "Capture Now", button_size),
                    })
                    .inner
                    .clicked()
                {
                    result.action = Some(Action::Capture);
                }
                if shortcut_keys(ui, settings, p).clicked() {
                    result.action = Some(Action::EditHotkey);
                }
                let album_detail = settings
                    .album_capture
                    .selected()
                    .map(|(_, name)| format!("Add this capture to {name}."));
                let detail = match settings.capture_mode {
                    CaptureMode::Text => None,
                    CaptureMode::Video => {
                        Some("Position the webcam inside the chosen window or display.")
                    }
                    _ => album_detail
                        .as_deref()
                        .or(Some(settings.capture_destination.hint())),
                };
                if let Some(detail) = detail {
                    ui.separator();
                    ui.add(
                        egui::Label::new(egui::RichText::new(detail).size(12.).color(p.muted))
                            .wrap(),
                    );
                }
            });
        },
    );
}

fn shortcut_keys(ui: &mut egui::Ui, settings: &Settings, p: appearance::Palette) -> egui::Response {
    let keys = settings.capture_shortcut.keycaps();
    let caps: Vec<_> = keys
        .into_iter()
        .map(|key| {
            ui.painter()
                .layout_no_wrap(key, egui::FontId::proportional(12.), p.ink)
        })
        .collect();
    let width = caps
        .iter()
        .map(|cap| (cap.size().x + 10.).max(22.) + 4.)
        .sum::<f32>()
        - 4.;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width + 24., 26.), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!(
                "Change capture hotkey: {}",
                settings.capture_shortcut.label()
            ),
        )
    });
    let focused = response.hovered() || response.has_focus();
    let mut x = rect.left();
    for cap in caps {
        let width = (cap.size().x + 10.).max(22.);
        let key = Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(width, 23.));
        ui.painter()
            .rect_filled(key.translate(Vec2::new(0., 2.)), 5, p.navy);
        ui.painter().rect(
            key,
            5,
            if focused {
                p.cream
            } else {
                p.navy.lerp_to_gamma(p.paper, 0.45)
            },
            Stroke::new(1., if focused { p.accent_top } else { p.border }),
            egui::StrokeKind::Inside,
        );
        ui.painter()
            .galley(key.center() - cap.size() * 0.5, cap, p.ink);
        x += width + 4.;
    }
    appearance::icon(
        ui.painter(),
        rect.right_center() - Vec2::new(9., 1.),
        appearance::Icon::Pencil,
        if focused { p.ink } else { p.muted },
        13.,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if cfg!(target_os = "macos") {
            "Change capture hotkey. Also available with ⌃⇧. or ⌘⇧."
        } else {
            "Change capture hotkey. Also available with Ctrl + Shift + ."
        })
}

fn capture_preview(ui: &mut egui::Ui, mode: CaptureMode, look: &CaptureLook) {
    let p = look.palette;
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 94.), egui::Sense::hover());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    appearance::gradient(&painter, rect, 10., look.top, p.navy);
    painter.rect_stroke(
        rect,
        10,
        Stroke::new(1., p.border),
        egui::StrokeKind::Inside,
    );
    let accent = if look.dark { p.ginger } else { p.accent };
    let scale = (rect.width() / 220.).min(1.);
    let at = |x, y| rect.center() + Vec2::new(x, y) * scale;
    let screen = Rect::from_min_max(at(-82., -34.), at(70., 33.));
    painter.rect(
        screen,
        8,
        p.base,
        Stroke::new(1., p.border),
        egui::StrokeKind::Inside,
    );
    let window = Rect::from_min_max(at(-60., -25.), at(45., 22.));
    painter.rect(
        window,
        5,
        p.paper,
        Stroke::new(1., p.border),
        egui::StrokeKind::Inside,
    );
    painter.line_segment([at(-60., -14.), at(45., -14.)], Stroke::new(1., p.border));
    for x in [-53., -47., -41.] {
        painter.circle_filled(at(x, -19.5), 1.35 * scale, p.muted);
    }
    let panel = Rect::from_min_max(at(-52., -7.), at(-29., 15.));
    painter.rect_filled(panel, 3, p.cream);
    for (y, end) in [(-5., 31.), (2., 14.), (9., 25.)] {
        painter.line_segment([at(-20., y), at(end, y)], Stroke::new(2. * scale, p.muted));
    }
    let outline = |bounds| {
        painter.rect_stroke(
            bounds,
            3,
            Stroke::new(1.5, accent),
            egui::StrokeKind::Inside,
        );
    };
    match mode {
        CaptureMode::Region | CaptureMode::Scrolling => {
            let region = Rect::from_min_max(at(-55., -10.), at(29., 17.));
            outline(region);
            for corner in [
                region.left_top(),
                region.right_top(),
                region.left_bottom(),
                region.right_bottom(),
            ] {
                painter.rect_filled(
                    Rect::from_center_size(corner, Vec2::splat(4. * scale)),
                    1,
                    accent,
                );
            }
        }
        CaptureMode::MultiRegion => {
            outline(panel.expand(2. * scale));
            outline(Rect::from_min_max(at(-23., -9.), at(35., 13.)));
        }
        CaptureMode::Window => outline(window),
        CaptureMode::FullScreen => outline(screen),
        CaptureMode::Text => {
            let selection = Rect::from_min_max(at(-23., -10.), at(35., 14.));
            painter.rect_filled(selection, 3, accent.gamma_multiply(0.23));
            outline(selection);
        }
        CaptureMode::Video => {
            outline(window);
            painter.circle_filled(at(35., -19.5), 2.8 * scale, Color32::from_rgb(248, 94, 87));
        }
    }
    let badge = at(69., 23.);
    appearance::sculpted_tile(&painter, badge, mode_tone(mode), 30. * scale);
    mode_icon(
        &painter,
        mode,
        Rect::from_center_size(badge, Vec2::new(18., 15.) * scale),
        Color32::WHITE,
    );
}

pub(super) fn mode_icon(painter: &egui::Painter, mode: CaptureMode, rect: Rect, color: Color32) {
    let color = crate::icon_style::color(
        painter.ctx(),
        color,
        match mode {
            CaptureMode::MultiRegion => crate::icon_style::Family::Violet,
            CaptureMode::Scrolling => crate::icon_style::Family::Teal,
            _ => crate::icon_style::Family::Cobalt,
        },
    );
    let stroke = Stroke::new(crate::icon_style::current(painter.ctx()).width(), color);
    match mode {
        CaptureMode::Video => appearance::icon(
            painter,
            rect.center(),
            appearance::Icon::Video,
            color,
            rect.width(),
        ),
        CaptureMode::Text => appearance::icon(
            painter,
            rect.center(),
            appearance::Icon::TextSize,
            color,
            rect.height(),
        ),
        CaptureMode::Region => appearance::icon(
            painter,
            rect.center(),
            appearance::Icon::Fit,
            color,
            rect.height(),
        ),
        CaptureMode::MultiRegion => {
            for offset in [Vec2::ZERO, rect.size() * 0.35] {
                painter.rect_stroke(
                    Rect::from_min_size(rect.min + offset, rect.size() * 0.65),
                    2,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        CaptureMode::Window => {
            painter.rect_stroke(rect, 3, stroke, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    rect.min + Vec2::new(0., 7.),
                    rect.right_top() + Vec2::new(0., 7.),
                ],
                stroke,
            );
            for x in [5., 10.] {
                painter.circle_filled(rect.min + Vec2::new(x, 3.5), 1., color);
            }
        }
        CaptureMode::FullScreen => {
            painter.rect_stroke(
                rect.shrink2(Vec2::new(0., 3.)),
                2,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    rect.center_bottom() + Vec2::new(-8., 1.),
                    rect.center_bottom() + Vec2::new(8., 1.),
                ],
                stroke,
            );
        }
        CaptureMode::Scrolling => {
            painter.rect_stroke(rect, 2, stroke, egui::StrokeKind::Inside);
            let c = rect.center();
            painter.line_segment([c - Vec2::new(0., 8.), c + Vec2::new(0., 8.)], stroke);
            painter.line_segment([c + Vec2::new(-4., 4.), c + Vec2::new(0., 8.)], stroke);
            painter.line_segment([c + Vec2::new(4., 4.), c + Vec2::new(0., 8.)], stroke);
        }
    }
}

fn mode_tone(mode: CaptureMode) -> appearance::TileTone {
    use appearance::TileTone;
    match mode {
        CaptureMode::Region => TileTone::Blue,
        CaptureMode::MultiRegion | CaptureMode::Text => TileTone::Violet,
        CaptureMode::Window | CaptureMode::Scrolling => TileTone::Teal,
        CaptureMode::FullScreen => TileTone::Slate,
        CaptureMode::Video => TileTone::Clay,
    }
}

fn mode_tile(ui: &mut egui::Ui, mode: CaptureMode, selected: bool, width: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 73.), egui::Sense::click());
    mode_widget_info(ui, mode, selected, &response);
    let p = appearance::palette(ui.ctx());
    let tone = mode_tone(mode);
    let ink = appearance::satin_ink(ui.ctx(), tone);
    let fill = if selected {
        0.20
    } else if response.hovered() {
        0.13
    } else {
        0.06
    };
    appearance::gradient(
        ui.painter(),
        rect,
        12.,
        p.surface.lerp_to_gamma(ink, fill),
        p.surface,
    );
    let emphasized = selected || response.hovered() || response.has_focus();
    ui.painter().rect_stroke(
        rect,
        12,
        Stroke::new(
            if selected { 1.7 } else { 1. },
            if emphasized { ink } else { p.border },
        ),
        egui::StrokeKind::Inside,
    );
    let center = rect.center_top() + Vec2::new(0., 25.);
    appearance::sculpted_tile(ui.painter(), center, tone, 34.);
    mode_icon(
        ui.painter(),
        mode,
        Rect::from_center_size(center, Vec2::new(20., 16.)),
        Color32::WHITE,
    );
    ui.painter().text(
        rect.center_bottom() - Vec2::new(0., 16.),
        egui::Align2::CENTER_CENTER,
        mode.label(),
        egui::FontId::proportional(13.),
        p.ink,
    );
    if selected {
        let mark = rect.right_top() + Vec2::new(-12., 12.);
        ui.painter().circle_filled(mark, 6.5, ink);
        appearance::icon(ui.painter(), mark, appearance::Icon::Check, p.surface, 8.);
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(mode.hint())
}

pub(super) fn compact_mode_tile(
    ui: &mut egui::Ui,
    mode: CaptureMode,
    selected: bool,
    size: Vec2,
    palette: appearance::Palette,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    mode_widget_info(ui, mode, selected, &response);
    let p = palette;
    if selected {
        appearance::satin_with_palette(
            ui.painter(),
            rect,
            8.,
            true,
            response.is_pointer_button_down_on(),
            p,
        );
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 8, p.cream);
    }
    let color = if selected {
        Color32::WHITE
    } else if response.hovered() {
        p.ink
    } else {
        p.muted
    };
    mode_icon(
        ui.painter(),
        mode,
        Rect::from_center_size(
            rect.center_top() + Vec2::new(0., size.y * 0.32),
            Vec2::new(19., 15.),
        ),
        color,
    );
    ui.painter().text(
        rect.center_bottom() - Vec2::new(0., size.y * 0.22),
        egui::Align2::CENTER_CENTER,
        mode.label(),
        egui::FontId::proportional(11.5),
        color,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            8,
            Stroke::new(1.5, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    // The compact launcher shows help in its header instead of a popup.
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn mode_widget_info(ui: &egui::Ui, mode: CaptureMode, selected: bool, response: &egui::Response) {
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            mode.label(),
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Pos2;

    #[test]
    fn compact_destination_menu_selects_only_ready_albums_and_can_return_to_editor() {
        use std::path::Path;
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        let first = Path::new("Steps.bedizalbum");
        let second = Path::new("Notes.bedizalbum");
        let paused = Path::new("Paused.bedizalbum");
        settings.album_capture.set_ready(first, "Steps", true);
        settings.album_capture.set_ready(second, "Notes", true);
        settings.album_capture.set_ready(paused, "Paused", false);
        let palette = CaptureTheme::Midnight.look().palette;
        let frame = |settings: &mut Settings, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(470., 275.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add_space(130.);
                        destination_picker(ui, settings, palette, 50., true);
                    });
                },
            )
        };
        let click = |settings: &mut Settings, pos| {
            let button = |pressed| egui::Event::PointerButton {
                pos,
                pressed,
                button: egui::PointerButton::Primary,
                modifiers: egui::Modifiers::NONE,
            };
            frame(settings, vec![egui::Event::PointerMoved(pos), button(true)]);
            frame(settings, vec![button(false)]);
            frame(settings, vec![])
        };
        let text_center = |output: &egui::FullOutput, label| {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    let center = text.pos + text.galley.size() * 0.5;
                    shape.clip_rect.contains(center).then_some(center)
                }
                _ => None,
            })
        };
        let trigger = egui::pos2(33., 149.);
        frame(&mut settings, vec![]);
        for (title, path) in [
            ("Steps", Some(first)),
            ("Notes", Some(second)),
            ("Editor", None),
        ] {
            let output = click(&mut settings, trigger);
            assert!(text_center(&output, "Paused").is_none());
            let row =
                text_center(&output, title).unwrap_or_else(|| panic!("Missing menu row {title}"));
            click(&mut settings, row);
            assert_eq!(
                settings.album_capture.selected().map(|(path, _)| path),
                path
            );
        }
        assert_eq!(settings.capture_destination, CaptureDestination::Editor);
    }

    #[test]
    fn capture_scrollbar_uses_workspace_edge_only_when_content_overflows() {
        for (height, overflowing) in [(400., true), (1000., false)] {
            let ctx = egui::Context::default();
            appearance::apply(&ctx);
            ctx.style_mut(|style| style.animation_time = 0.);
            let mut settings = Settings {
                capture_mode: CaptureMode::Video,
                ..Default::default()
            };
            let mut workspace = Rect::NOTHING;
            let mut output = egui::FullOutput::default();
            for _ in 0..3 {
                output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1240., height),
                        )),
                        events: vec![egui::Event::PointerMoved(egui::pos2(620., 40.))],
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            workspace = ui.available_rect_before_wrap();
                            super::ui(
                                ui,
                                &mut settings,
                                &mut CaptureShape::Rectangle,
                                &[],
                                Availability {
                                    capture: true,
                                    background: true,
                                    permission_needed: false,
                                    demo: false,
                                },
                                None,
                            );
                        });
                    },
                );
            }
            let bar_width = ctx.style().spacing.scroll.bar_width;
            let bars: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.rect.width() > 0.
                            && rect.rect.width() <= bar_width
                            && rect.rect.height() > 100.
                            && rect.fill != Color32::TRANSPARENT =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(!bars.is_empty(), overflowing, "{bars:?} at {height}");
            assert!(
                bars.iter()
                    .all(|bar| (bar.right() - workspace.right()).abs() < 1.),
                "{bars:?} should sit at the right of {workspace:?}"
            );
        }
    }

    #[test]
    fn capture_modes_and_primary_action_are_visible_and_interactive() {
        for (size, capture_theme) in [
            (Vec2::new(940., 480.), CaptureTheme::Midnight),
            (Vec2::new(635., 900.), CaptureTheme::Porcelain),
        ] {
            exercise_capture_console(size, capture_theme);
        }
    }

    fn exercise_capture_console(size: Vec2, capture_theme: CaptureTheme) {
        let ctx = egui::Context::default();
        appearance::apply(&ctx);
        let mut settings = Settings {
            capture_theme,
            ..Default::default()
        };
        let hotkey = settings.capture_shortcut.keycaps().pop().unwrap();
        let mut shape = CaptureShape::Rectangle;
        let mut frame = |events| {
            let mut response = Response::default();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        response = super::ui(
                            ui,
                            &mut settings,
                            &mut shape,
                            &[],
                            Availability {
                                capture: true,
                                background: true,
                                permission_needed: false,
                                demo: false,
                            },
                            None,
                        );
                    });
                },
            );
            (output, response)
        };
        frame(vec![]);
        let (output, _) = frame(vec![]);
        for label in [
            "Region",
            "Window",
            "Multi-region",
            "Full screen",
            "Include pointer",
            "Editor",
            "Clipboard",
            "Capture Now",
            "Open Capture panel",
            if cfg!(target_os = "macos") {
                "Capture in the menu bar"
            } else {
                "Capture in the system tray"
            },
            &hotkey,
        ] {
            let t = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => Some((s.clip_rect, t)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing {label}"));
            assert!(
                t.0.contains_rect(Rect::from_min_size(t.1.pos, t.1.galley.size())),
                "{label} clipped: {:?} within {:?}",
                Rect::from_min_size(t.1.pos, t.1.galley.size()),
                t.0
            );
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
                .unwrap()
        };
        for (label, action) in [
            ("Open Capture panel", Some(Action::Panel)),
            ("Window", None),
            ("Multi-region", None),
            ("Full screen", None),
            ("Region", None),
            ("Include pointer", None),
            ("Capture immediately after selection", None),
            ("Clipboard", None),
            ("Editor", None),
            ("Capture Now", Some(Action::Capture)),
            (hotkey.as_str(), Some(Action::EditHotkey)),
        ] {
            let (output, _) = frame(vec![]);
            let at = center(&output, label);
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                pressed,
                button: egui::PointerButton::Primary,
                modifiers: egui::Modifiers::NONE,
            };
            frame(vec![egui::Event::PointerMoved(at), button(true)]);
            let (_, response) = frame(vec![button(false)]);
            if let Some(action) = action {
                assert_eq!(
                    response.action,
                    Some(action),
                    "{label} at {at:?} in {size:?}"
                );
            } else {
                assert!(
                    response.changed,
                    "{label} did not change the capture options"
                );
            }
        }
        assert_eq!(settings.capture_mode, CaptureMode::Region);
        assert!(settings.include_pointer);
    }
}
