use super::*;
use crate::appearance::TileTone;
use egui::{Align, Align2, FontId, Layout, Rect, Stroke, StrokeKind, UiBuilder, Vec2, pos2, vec2};

#[derive(Clone, Copy)]
enum HomeAction {
    Capture,
    Record,
    OpenImage,
    OpenVideo,
    Paste,
    Create,
}

impl HomeAction {
    const OTHER: [Self; 4] = [Self::OpenImage, Self::OpenVideo, Self::Paste, Self::Create];

    fn presentation(self) -> (&'static str, Icon, TileTone) {
        match self {
            Self::Capture => ("Capture image", Icon::Scan, TileTone::Blue),
            Self::Record => ("Record video", Icon::Video, TileTone::Clay),
            Self::OpenImage => ("Open image", Icon::Open, TileTone::Clay),
            Self::OpenVideo => ("Open video", Icon::Play, TileTone::Teal),
            Self::Paste => ("Paste image", Icon::Copy, TileTone::Blue),
            Self::Create => ("Create", Icon::Create, TileTone::Violet),
        }
    }
}

impl MyImageApp {
    pub(super) fn home_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if ui.is_enabled()
            && !self.busy
            && ui.input_mut(|input| {
                let paste = input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Paste(_)));
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Paste(_)));
                paste
            })
        {
            self.paste_new_image(ctx);
        }

        let top_space = ((ui.available_height() - 330.) * 0.24).clamp(16., 62.);
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt("capture-studio")
            .show(ui, |ui| {
                let width = (ui.available_width() - 24.).clamp(1., 920.);
                ui.add_space(top_space);
                ui.vertical_centered(|ui| {
                    ui.allocate_ui_with_layout(
                        vec2(width, 0.),
                        Layout::top_down(Align::Min),
                        |ui| {
                            action = capture_hero(ui, width, self.app_icon.id());
                            ui.add_space(6.);
                            let columns = if width >= 680. { 4 } else { 2 };
                            let gap = 9.;
                            let button_width = ((width + gap) / columns as f32 - gap).max(1.);
                            for row in HomeAction::OTHER.chunks(columns) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = gap;
                                    for &item in row {
                                        if home_button(ui, item, button_width, false) {
                                            action = Some(item);
                                        }
                                    }
                                });
                            }
                            ui.add_space(3.);
                            ui.vertical_centered(|ui| {
                                ui.weak("Drop an image or video anywhere");
                                if self.document.is_some()
                                    && appearance::toolbar_action_button(
                                        ui,
                                        "Continue editing",
                                        Icon::Edit,
                                        true,
                                        false,
                                    )
                                    .clicked()
                                {
                                    self.view = View::Editor;
                                }
                            });
                        },
                    );
                });
                ui.add_space(16.);
            });

        match action {
            Some(HomeAction::Capture) => {
                if !self.settings.capture_mode.makes_image() {
                    self.settings.capture_mode = capture::CaptureMode::Region;
                    self.persist_settings();
                }
                self.view = View::Capture;
            }
            Some(HomeAction::Record) => self.show_video_capture(),
            Some(HomeAction::OpenImage) => self.open_image(ctx),
            Some(HomeAction::OpenVideo) => self.open_video(ctx),
            Some(HomeAction::Paste) => self.paste_new_image(ctx),
            Some(HomeAction::Create) => self.view = View::Create,
            None => {}
        }
    }
}

fn capture_hero(ui: &mut egui::Ui, width: f32, mascot: egui::TextureId) -> Option<HomeAction> {
    let wide = width >= 660.;
    let height = if wide { 234. } else { 310. };
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
    let p = appearance::palette(ui.ctx());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let teal = appearance::satin_ink(ui.ctx(), TileTone::Teal);
    let (start, end) = if p.is_dark() {
        (
            p.navy.lerp_to_gamma(p.accent, 0.28),
            p.navy.lerp_to_gamma(teal, 0.20),
        )
    } else {
        (
            p.surface.lerp_to_gamma(p.accent, 0.13),
            p.surface.lerp_to_gamma(teal, 0.08),
        )
    };
    appearance::gradient(&painter, rect, 16., start, end);
    painter.rect_stroke(rect, 16, Stroke::new(1., p.border), StrokeKind::Inside);

    let art = if wide {
        Rect::from_min_max(
            pos2(rect.left() + width * 0.57, rect.top() + 18.),
            rect.max - vec2(24., 14.),
        )
    } else {
        Rect::from_center_size(
            pos2(rect.center().x, rect.top() + 75.),
            vec2((width - 32.).clamp(1., 320.), 128.),
        )
    };
    paint_capture_art(&painter, art, mascot, p, teal);

    let copy = Rect::from_min_max(
        rect.min
            + if wide {
                vec2(30., 41.)
            } else {
                vec2(18., 151.)
            },
        pos2(
            if wide {
                art.left() - 8.
            } else {
                rect.right() - 18.
            },
            rect.bottom() - 12.,
        ),
    );
    let mut content = ui.new_child(
        UiBuilder::new()
            .id_salt("capture-hero-actions")
            .max_rect(copy)
            .layout(Layout::top_down(Align::Min)),
    );
    content.spacing_mut().item_spacing = vec2(8., 7.);
    content.heading(
        egui::RichText::new("Capture your screen")
            .size(if wide { 34. } else { 27. })
            .strong(),
    );
    content.weak("Take a screenshot or record a video.");
    content.add_space(7.);
    content
        .horizontal_wrapped(|ui| {
            if home_button(ui, HomeAction::Capture, 168., true) {
                Some(HomeAction::Capture)
            } else if home_button(ui, HomeAction::Record, 160., false) {
                Some(HomeAction::Record)
            } else {
                None
            }
        })
        .inner
}

fn home_button(ui: &mut egui::Ui, action: HomeAction, width: f32, primary: bool) -> bool {
    let (label, icon, tone) = action.presentation();
    let response = ui.add(
        egui::Button::new("")
            .frame(false)
            .min_size(vec2(width, 58.)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let p = appearance::palette(ui.ctx());
    let tint = appearance::satin_ink(ui.ctx(), tone);
    let (fill, ink, tile, border) = if primary {
        let (fill, ink) = if p.is_dark() {
            (tint, p.navy)
        } else {
            (p.accent, Color32::WHITE)
        };
        (fill, ink, fill.lerp_to_gamma(ink, 0.16), fill)
    } else {
        (
            p.surface,
            p.ink,
            p.surface.lerp_to_gamma(tint, 0.13),
            p.border,
        )
    };
    let active = response.hovered() || response.has_focus();
    let fill = if response.is_pointer_button_down_on() {
        fill.lerp_to_gamma(ink, 0.14)
    } else if active {
        fill.lerp_to_gamma(ink, 0.07)
    } else {
        fill
    };
    let painter = ui.painter();
    let rect = response.rect;
    painter.rect(
        rect,
        12,
        fill,
        Stroke::new(1., if active { p.accent_top } else { border }),
        StrokeKind::Inside,
    );
    let center = rect.left_center() + vec2(26., 0.);
    painter.rect_filled(Rect::from_center_size(center, Vec2::splat(34.)), 9, tile);
    appearance::icon(painter, center, icon, if primary { ink } else { tint }, 19.);
    painter.text(
        rect.left_center() + vec2(52., 0.),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(15.),
        ink,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn paint_capture_art(
    painter: &egui::Painter,
    rect: Rect,
    mascot: egui::TextureId,
    palette: appearance::Palette,
    teal: Color32,
) {
    let scale = (rect.width() / 365.).min(rect.height() / 190.);
    let size = vec2(260., 132.) * scale;
    let card = |offset: Vec2, angle: f32, front: bool| {
        let center = rect.center() + offset * scale;
        let rotation = egui::emath::Rot2::from_angle(angle);
        let at = |x: f32, y: f32| center + rotation * vec2(x * size.x, y * size.y);
        let corners = |left, top, right, bottom| {
            vec![
                at(left, top),
                at(right, top),
                at(right, bottom),
                at(left, bottom),
            ]
        };
        let panel = corners(-0.5, -0.5, 0.5, 0.5);
        painter.add(egui::Shape::convex_polygon(
            panel
                .iter()
                .map(|&point| point + vec2(0., 7.) * scale)
                .collect(),
            Color32::from_black_alpha(if palette.is_dark() { 65 } else { 24 }),
            Stroke::NONE,
        ));
        painter.add(egui::Shape::convex_polygon(
            panel,
            if front { palette.surface } else { palette.base },
            Stroke::new(1., palette.border),
        ));
        painter.add(egui::Shape::convex_polygon(
            corners(-0.495, -0.495, 0.495, -0.32),
            palette.paper,
            Stroke::NONE,
        ));
        if front {
            for y in [-0.16, -0.03, 0.10, 0.23] {
                painter.line_segment(
                    [at(-0.37, y), at(0.37, y)],
                    Stroke::new(1., palette.border.gamma_multiply(0.52)),
                );
            }
            painter.add(egui::Shape::closed_line(
                corners(-0.37, -0.22, 0.37, 0.32),
                Stroke::new(1.8, teal),
            ));
        }
    };
    card(vec2(0., -17.), 0.16, false);
    card(vec2(-24., 13.), -0.12, true);
    let mascot_rect = Rect::from_min_size(
        rect.center() + vec2(31., -63.) * scale,
        Vec2::splat(150. * scale),
    );
    painter.image(
        mascot,
        mascot_rect,
        Rect::from_min_max(egui::Pos2::ZERO, pos2(1., 1.)),
        Color32::WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_below_the_hero_clear_its_artwork() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for (width, height) in [(890., 234.), (580., 310.)] {
                    let top = ui.next_widget_position().y;
                    capture_hero(ui, width, egui::TextureId::default());
                    assert!(ui.next_widget_position().y >= top + height);
                }
            });
        });
    }
}
