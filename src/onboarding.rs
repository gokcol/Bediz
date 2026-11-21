//! Calico welcome and compact native shortcut guide.
use crate::appearance::{self, Icon, Palette, TileTone};
use crate::dialogs;
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke, Vec2};

pub const VERSION: u8 = 2;

const PANEL_INSET: i8 = 19;

pub fn show(
    ctx: &egui::Context,
    open: &mut bool,
    calico: &egui::TextureHandle,
    shortcut: &crate::desktop::shortcut::CaptureShortcut,
) -> bool {
    if !*open {
        return false;
    }
    let palette = appearance::palette(ctx);
    let screen = ctx.content_rect();
    let width = (screen.width() - 32.).clamp(220., 650.);
    let response = egui::Modal::new(egui::Id::new("quick_start"))
        .frame(
            Frame::new()
                .fill(palette.paper)
                .stroke(Stroke::new(1., palette.border))
                .corner_radius(19)
                .shadow(egui::Shadow {
                    offset: [0, 11],
                    blur: 30,
                    spread: 0,
                    color: Color32::from_black_alpha(53),
                }),
        )
        .backdrop_color(Color32::from_black_alpha(85))
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let header = welcome_header(ui, width, palette);
            ui.painter().hline(
                header.response.rect.x_range(),
                header.response.rect.bottom(),
                Stroke::new(1., palette.border),
            );

            let footer_height = if width >= 390. { 70. } else { 102. };
            let body_height =
                (screen.height() - header.response.rect.height() - footer_height - 40.).max(44.);
            let create = Frame::new()
                .inner_margin(Margin::symmetric(PANEL_INSET, 0))
                .show(ui, |ui| {
                    ui.set_width(width - 2. * f32::from(PANEL_INSET));
                    ui.add_space(14.);
                    egui::ScrollArea::vertical()
                        .id_salt("quick_start_body")
                        .max_height(body_height)
                        .min_scrolled_height(0.)
                        .auto_shrink([false, true])
                        .show(ui, |ui| welcome_body(ui, calico, shortcut, palette));
                    ui.add_space(10.);
                    ui.add(egui::Separator::default().spacing(1.));
                    ui.add_space(10.);
                    let create = ui
                        .horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(12., 6.);
                            let clicked =
                                dialogs::primary(ui, "Let's create", Icon::Edit).clicked();
                            ui.label(
                                RichText::new("Your calico App menu keeps this guide handy.")
                                    .size(12.)
                                    .color(palette.muted),
                            );
                            clicked
                        })
                        .inner;
                    ui.add_space(13.);
                    create
                })
                .inner;
            header.inner || create
        });
    let dismissed = response.inner || ctx.input(|i| i.key_pressed(egui::Key::Escape));
    *open = !dismissed;
    dismissed
}

fn welcome_header(ui: &mut egui::Ui, width: f32, palette: Palette) -> egui::InnerResponse<bool> {
    Frame::new()
        .fill(palette.surface)
        .corner_radius(CornerRadius {
            nw: 18,
            ne: 18,
            sw: 0,
            se: 0,
        })
        .inner_margin(Margin::symmetric(PANEL_INSET, 12))
        .show(ui, |ui| {
            let inner_width = width - 2. * f32::from(PANEL_INSET);
            ui.set_width(inner_width);
            ui.spacing_mut().item_spacing = egui::vec2(11., 2.);
            ui.horizontal(|ui| {
                let (badge, _) = ui.allocate_exact_size(Vec2::splat(32.), egui::Sense::hover());
                ui.painter().rect_filled(badge, 10, palette.cream);
                appearance::icon(
                    ui.painter(),
                    badge.center(),
                    Icon::Camera,
                    welcome_accent(palette),
                    17.,
                );
                let text_width = inner_width - 32. - 30. - 22.;
                ui.allocate_ui_with_layout(
                    egui::vec2(text_width, 0.),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(text_width);
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("Welcome to {}", crate::branding::NAME))
                                    .size(17.)
                                    .strong()
                                    .color(palette.ink),
                            )
                            .wrap(),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new("Your compact capture and annotation studio.")
                                    .size(12.)
                                    .color(palette.muted),
                            )
                            .wrap(),
                        );
                    },
                );
                let close = ui.add_sized(
                    [30., 30.],
                    egui::Button::new(RichText::new("×").size(20.).color(palette.muted))
                        .frame(false),
                );
                close.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Close dialog")
                });
                close.on_hover_text("Close").clicked()
            })
            .inner
        })
}

fn welcome_body(
    ui: &mut egui::Ui,
    calico: &egui::TextureHandle,
    shortcut: &crate::desktop::shortcut::CaptureShortcut,
    palette: Palette,
) {
    ui.spacing_mut().item_spacing.y = 8.;
    Frame::new()
        .fill(palette.cream.lerp_to_gamma(palette.paper, 0.36))
        .corner_radius(13)
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::vec2(14., 4.);
            ui.horizontal(|ui| {
                ui.add(egui::Image::new(calico).fit_to_exact_size(Vec2::splat(62.)));
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new("Small paws. Big ideas.")
                                .size(26.)
                                .strong()
                                .color(palette.ink),
                        )
                        .wrap(),
                    );
                    ui.label(
                        RichText::new("Capture it. Make it yours.")
                            .size(14.)
                            .color(palette.muted),
                    );
                });
            });
        });
    let capture_tip = format!(
        "{}  ·  Change it by clicking the hotkey in Capture Now.",
        shortcut.label()
    );
    for (icon, detail, tone, title, body, tip) in [
        (
            Icon::Camera,
            Icon::Scan,
            TileTone::Blue,
            "Catch a moment",
            "Capture a region, a window, or your whole screen.",
            capture_tip.as_str(),
        ),
        (
            Icon::View,
            Icon::Pencil,
            TileTone::Teal,
            "Make your mark",
            "Add arrows, text and stamps. Click an object to style it.",
            "Drag a selection to move pixels; use its handles to adjust the boundary.",
        ),
        (
            Icon::Layers,
            Icon::Check,
            TileTone::Clay,
            "Keep it editable. Share it anywhere.",
            "Each image saves automatically as one editable .myimage project. Use Save As to create an image or PDF to share.",
            "Right-click a selection for actions, including Delete.",
        ),
    ] {
        step_card(ui, (icon, detail, tone), title, body, tip, palette);
    }
    Frame::new()
        .fill(palette.cream)
        .corner_radius(11)
        .inner_margin(11)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::vec2(10., 3.);
            ui.horizontal_top(|ui| {
                let accent = welcome_accent(palette);
                let (badge, _) = ui.allocate_exact_size(Vec2::splat(26.), egui::Sense::hover());
                ui.painter().rect_filled(badge, 8, palette.surface);
                appearance::icon(ui.painter(), badge.center(), Icon::Shield, accent, 15.);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("A little peace of mind")
                            .size(13.)
                            .strong()
                            .color(accent),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new("Edits save to your project automatically. Recovery also protects unfinished work after an unexpected exit.")
                                .size(13.)
                                .color(palette.ink),
                        )
                        .wrap(),
                    );
                });
            });
        });
    shortcuts(ui);
}

fn welcome_accent(palette: Palette) -> Color32 {
    if palette.is_dark() {
        palette.ginger
    } else {
        palette.accent
    }
}

fn step_card(
    ui: &mut egui::Ui,
    (icon, detail, tone): (Icon, Icon, TileTone),
    title: &str,
    body: &str,
    tip: &str,
    palette: Palette,
) {
    Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1., palette.border.gamma_multiply(0.75)))
        .corner_radius(13)
        .inner_margin(Margin::symmetric(13, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::vec2(12., 4.);
            ui.horizontal_top(|ui| {
                let (slot, _) = ui.allocate_exact_size(egui::vec2(56., 55.), egui::Sense::hover());
                appearance::sculpted_icon(
                    ui.painter(),
                    slot.left_top() + Vec2::splat(25.),
                    icon,
                    tone,
                    50.,
                    Some(detail),
                );
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(title).size(15.).strong().color(palette.ink),
                        )
                        .wrap(),
                    );
                    ui.add(
                        egui::Label::new(RichText::new(body).size(13.).color(palette.ink)).wrap(),
                    );
                    ui.add(
                        egui::Label::new(RichText::new(tip).size(11.5).color(palette.muted)).wrap(),
                    );
                });
            });
        });
}

fn shortcuts(ui: &mut egui::Ui) {
    ui.collapsing("Shortcuts & little tricks", |ui| {
        let key = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl"
        };
        let column_width = ((ui.available_width() - 16.) / 2.).max(80.);
        egui::Grid::new("shortcut_reference")
            .min_col_width(column_width)
            .max_col_width(column_width)
            .spacing(egui::vec2(16., 4.))
            .show(ui, |ui| {
                for (keys, action) in [
                    (format!("{key} + S"), "Save editable project"),
                    (
                        format!("{key} + Shift + S"),
                        "Save As: project, image or PDF",
                    ),
                    (format!("{key} + C / X / V"), "Copy / cut / paste"),
                    (
                        format!("{key} + A"),
                        "Select all in the focused image or collection",
                    ),
                    (format!("{key} + Z / Shift + Z"), "Undo / redo"),
                    (
                        "Delete / Backspace".into(),
                        "Delete the selected area or layers",
                    ),
                    ("Escape".into(), "Cancel a selection or operation"),
                    ("Pinch / Ctrl + scroll".into(), "Zoom around the pointer"),
                    (
                        "Ctrl / ⌘ + click".into(),
                        "Select several Library or Recents items",
                    ),
                    (
                        "Right-click image".into(),
                        "Selection, image and canvas actions",
                    ),
                ] {
                    ui.label(RichText::new(keys).small().strong());
                    ui.add(egui::Label::new(RichText::new(action).small()).wrap());
                    ui.end_row();
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::shortcut::CaptureShortcut;

    #[test]
    fn welcome_keeps_dismissal_controls_reachable_in_a_short_window() {
        for finish in [
            appearance::GuiFinish::CobaltSatin,
            appearance::GuiFinish::GraphiteNight,
        ] {
            check_dismissal_controls(finish);
        }
    }

    fn check_dismissal_controls(finish: appearance::GuiFinish) {
        let ctx = egui::Context::default();
        appearance::apply(&ctx);
        appearance::set_finish(&ctx, finish);
        ctx.enable_accesskit();
        let calico = ctx.load_texture(
            "welcome-test-calico",
            egui::ColorImage::filled([1, 1], Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        let shortcut = CaptureShortcut::default();
        let mut open = true;
        let mut frame = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(654., 426.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    show(ctx, &mut open, &calico, &shortcut);
                },
            )
        };
        let _ = frame(vec![]);
        let _ = frame(vec![]);
        let nodes = frame(vec![])
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes;
        for label in ["Close dialog", "Let's create"] {
            let bounds = nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.label() == Some(label))
                        .then(|| node.bounds())
                        .flatten()
                })
                .unwrap_or_else(|| panic!("Missing {label}"));
            assert!(bounds.x0 >= 0. && bounds.x1 <= 654.);
            assert!(bounds.y0 >= 0. && bounds.y1 <= 426.);
        }
        let _ = frame(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(!open);
    }
}
