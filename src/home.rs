use super::*;
use crate::{appearance::TileTone, library::EntryKind, library_panel::RecentPreview};
use egui::{Align, Align2, FontId, Layout, Rect, Stroke, StrokeKind, Vec2, pos2, vec2};

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

        let top_space = ((ui.available_height() - 340.) * 0.45).clamp(16., 88.);
        egui::ScrollArea::vertical()
            .id_salt("home-shelf")
            .show(ui, |ui| {
                let width = (ui.available_width() - 24.).clamp(1., 920.);
                let wide = width >= 680.;
                ui.add_space(if wide { top_space } else { 16. });
                ui.vertical_centered(|ui| {
                    ui.allocate_ui_with_layout(
                        vec2(width, 0.),
                        Layout::top_down(Align::Min),
                        |ui| {
                            if wide {
                                let gap = 20.;
                                let shelf = (width - gap) * 0.62;
                                ui.spacing_mut().item_spacing.x = gap;
                                ui.horizontal_top(|ui| {
                                    ui.allocate_ui_with_layout(
                                        vec2(shelf, 0.),
                                        Layout::top_down(Align::Min),
                                        |ui| self.home_recent_work(ui, shelf),
                                    );
                                    self.home_quick_actions(ui, width - shelf - gap);
                                });
                            } else {
                                self.home_recent_work(ui, width);
                                ui.add_space(14.);
                                self.home_quick_actions(ui, width);
                            }
                        },
                    );
                });
                ui.add_space(18.);
            });
    }

    fn home_recent_work(&mut self, ui: &mut egui::Ui, width: f32) {
        let ctx = ui.ctx().clone();
        let items = self.library.recent_previews(
            &ctx,
            &self.settings.output_dir,
            &self.settings.recent_files,
            3,
        );
        self.sync_library_references();

        ui.horizontal(|ui| {
            ui.set_min_height(30.);
            ui.heading(egui::RichText::new("Recent work").size(24.).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if appearance::toolbar_action_button(ui, "Library", Icon::Library, true, false)
                    .on_hover_text("Browse the full library")
                    .clicked()
                {
                    self.view = View::Library;
                }
            });
        });
        let Some((featured, rest)) = items.split_first() else {
            empty_shelf(ui, width);
            return;
        };

        let mut picked = recent_card(ui, featured, width, true).then_some(featured);
        if !rest.is_empty() {
            ui.horizontal(|ui| {
                let gap = 9.;
                ui.spacing_mut().item_spacing.x = gap;
                let card_width = ((width - gap) * 0.5).max(1.);
                for item in rest {
                    if recent_card(ui, item, card_width, false) {
                        picked = Some(item);
                    }
                }
            });
        }
        if let Some(item) = picked {
            let path = &item.entry.path;
            let active = self.project_path.as_ref().or(self.last_path.as_ref());
            if self.document.is_some() && Some(path) == active {
                self.view = View::Editor;
            } else {
                self.library_action(&ctx, LibraryAction::Open(path.clone()));
            }
        }
    }

    fn home_quick_actions(&mut self, ui: &mut egui::Ui, width: f32) {
        use TileTone::{Blue, Clay, Teal, Violet};
        ui.allocate_ui_with_layout(vec2(width, 0.), Layout::top_down(Align::Min), |ui| {
            ui.add_sized(
                [width, 30.],
                egui::Label::new(egui::RichText::new("Quick actions").size(15.).strong())
                    .halign(Align::Min),
            );
            ui.spacing_mut().item_spacing.y = 5.;
            for (title, icon, tone) in [
                ("Capture image", Icon::Scan, Blue),
                ("Record video", Icon::Camera, Clay),
                ("Open image", Icon::Open, Clay),
                ("Open video", Icon::Video, Teal),
                ("Paste image", Icon::Copy, Blue),
                ("Create", Icon::Create, Violet),
            ] {
                let tint = appearance::satin_ink(ui.ctx(), tone);
                let button = shelf_button(ui, title, vec2(width, 41.), tint);
                let rect = button.rect;
                let center = rect.left_center() + vec2(20., 0.);
                let palette = appearance::palette(ui.ctx());
                let painter = ui.painter();
                painter.rect_filled(
                    Rect::from_center_size(center, Vec2::splat(26.)),
                    7,
                    palette.surface.lerp_to_gamma(tint, 0.13),
                );
                appearance::icon(painter, center, icon, tint, 16.);
                painter.text(
                    rect.left_center() + vec2(40., 0.),
                    Align2::LEFT_CENTER,
                    title,
                    FontId::proportional(14.),
                    palette.ink,
                );
                appearance::icon(
                    painter,
                    rect.right_center() - vec2(16., 0.),
                    Icon::Forward,
                    palette.muted,
                    11.,
                );
                if button.clicked() {
                    let ctx = ui.ctx().clone();
                    match icon {
                        Icon::Scan => {
                            if !self.settings.capture_mode.makes_image() {
                                self.settings.capture_mode = capture::CaptureMode::Region;
                                self.persist_settings();
                            }
                            self.view = View::Capture;
                        }
                        Icon::Camera => self.show_video_capture(),
                        Icon::Open => self.open_image(&ctx),
                        Icon::Video => self.open_video(&ctx),
                        Icon::Copy => self.paste_new_image(&ctx),
                        Icon::Create => self.view = View::Create,
                        _ => unreachable!(),
                    }
                }
            }
            if self.document.is_some() {
                ui.add_space(3.);
                if appearance::toolbar_action_button(
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
            }
        });
    }
}

fn shelf_button(ui: &mut egui::Ui, label: &str, size: Vec2, accent: Color32) -> egui::Response {
    let response = ui.add(egui::Button::new("").frame(false).min_size(size));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let p = appearance::palette(ui.ctx());
    let hovered = response.hovered() || response.has_focus();
    let tint = if response.is_pointer_button_down_on() {
        0.13
    } else if hovered {
        0.07
    } else {
        0.
    };
    ui.painter().rect(
        response.rect,
        12,
        p.surface.lerp_to_gamma(accent, tint),
        Stroke::new(1., if hovered { accent } else { p.border }),
        StrokeKind::Inside,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn recent_card(ui: &mut egui::Ui, item: &RecentPreview, width: f32, featured: bool) -> bool {
    let path = &item.entry.path;
    let title = path.file_stem().unwrap_or_default().to_string_lossy();
    let recording = item.entry.kind == EntryKind::Recording;
    let tone = if recording {
        TileTone::Teal
    } else {
        TileTone::Blue
    };
    let ink = appearance::satin_ink(ui.ctx(), tone);
    let response = ui
        .push_id(path, |ui| {
            shelf_button(
                ui,
                &title,
                vec2(width, if featured { 140. } else { 102. }),
                ink,
            )
        })
        .inner
        .on_hover_text(path.display().to_string());
    let rect = response.rect;
    let palette = appearance::palette(ui.ctx());
    let painter = ui.painter();
    let image_rect = Rect::from_min_max(
        rect.min + Vec2::splat(8.),
        if featured {
            pos2(rect.left() + rect.width() * 0.58, rect.bottom() - 8.)
        } else {
            rect.max - vec2(8., 30.)
        },
    );
    painter.rect_filled(image_rect, 7, palette.base.lerp_to_gamma(ink, 0.13));
    if let Some(texture) = &item.texture {
        let size = texture.size_vec2();
        let fit = (image_rect.width() / size.x).min(image_rect.height() / size.y);
        let bounds = Rect::from_center_size(image_rect.center(), size * fit);
        let painter = painter.with_clip_rect(bounds.intersect(ui.clip_rect()));
        crate::editor::paint_checkerboard(&painter, bounds);
        painter.image(
            texture.id(),
            bounds,
            Rect::from_min_max(egui::Pos2::ZERO, pos2(1., 1.)),
            Color32::WHITE,
        );
    } else if !recording {
        appearance::icon(painter, image_rect.center(), Icon::View, ink, 22.);
    }
    if recording {
        let center = image_rect.center();
        painter.circle_filled(center, 14., palette.surface);
        appearance::icon(painter, center, Icon::Play, ink, 13.);
    }

    let (title_pos, title_size) = if featured {
        (pos2(image_rect.right() + 12., rect.center().y - 20.), 16.)
    } else {
        (pos2(image_rect.left(), image_rect.bottom() + 5.), 13.)
    };
    let mut title_job = egui::text::LayoutJob::simple_singleline(
        title.into_owned(),
        FontId::proportional(title_size),
        palette.ink,
    );
    title_job.wrap.max_width = (rect.right() - 10. - title_pos.x).max(1.);
    title_job.wrap.max_rows = 1;
    painter.galley(title_pos, painter.layout_job(title_job), palette.ink);
    if featured {
        let kind = match item.entry.kind {
            EntryKind::Recording => "Video",
            EntryKind::Project => "Editable project",
            EntryKind::Image => "Image",
        };
        let detail = item
            .dimensions
            .map_or_else(|| kind.to_owned(), |[w, h]| format!("{kind} · {w} × {h}"));
        painter
            .with_clip_rect(rect.shrink(8.).intersect(ui.clip_rect()))
            .text(
                title_pos + vec2(0., 24.),
                Align2::LEFT_TOP,
                detail,
                FontId::proportional(12.),
                palette.muted,
            );
    }
    response.clicked()
}

fn empty_shelf(ui: &mut egui::Ui, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 251.), egui::Sense::hover());
    let p = appearance::palette(ui.ctx());
    let painter = ui.painter();
    painter.rect(
        rect,
        12,
        p.surface,
        Stroke::new(1., p.border),
        StrokeKind::Inside,
    );
    let center = rect.center();
    let ink = appearance::satin_ink(ui.ctx(), TileTone::Blue);
    let icon_center = center - vec2(0., 34.);
    painter.rect_filled(
        Rect::from_center_size(icon_center, Vec2::splat(46.)),
        12,
        p.surface.lerp_to_gamma(ink, 0.11),
    );
    appearance::icon(painter, icon_center, Icon::Library, ink, 24.);
    for (offset, text, size, color) in [
        (7., "No recent work yet", 16., p.ink),
        (29., "Drop an image or video here.", 13., p.muted),
    ] {
        painter.text(
            center + vec2(0., offset),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(size),
            color,
        );
    }
}
