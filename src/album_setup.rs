//! Capture Album template selection and page previews.
use super::*;
use egui::{Align2, Color32, FontId, Sense, StrokeKind, vec2};

impl MyImageApp {
    pub(in crate::app) fn new_album_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if appearance::action_button(ui, "Back to Albums", Icon::Back, true, false).clicked() {
            self.view = View::Albums;
        }
        ui.separator();
        let folder = self.settings.output_dir.join("Albums");
        let mut create = false;
        egui::ScrollArea::vertical()
            .id_salt("create-capture-album")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let width = (ui.available_width() - 24.).clamp(1., 948.);
                ui.add_space(15.);
                ui.vertical_centered(|ui| {
                    ui.allocate_ui_with_layout(
                        vec2(width, 0.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(width);
                            ui.horizontal(|ui| {
                                let (icon, _) =
                                    ui.allocate_exact_size(Vec2::splat(44.), Sense::hover());
                                appearance::sculpted_icon(
                                    ui.painter(),
                                    icon.center(),
                                    Icon::Album,
                                    TileTone::Teal,
                                    39.,
                                    Some(Icon::Capture),
                                );
                                ui.label(
                                    egui::RichText::new("Create a capture album")
                                        .size(25.)
                                        .strong(),
                                );
                            });
                            ui.add_space(9.);
                            template_picker(ui, width, &mut self.albums.layout);
                            ui.add_space(11.);

                            if width >= 610. {
                                ui.columns(2, |columns| {
                                    selected_preview(&mut columns[0], self.albums.layout);
                                    create = album_details(
                                        &mut columns[1],
                                        &mut self.albums.name,
                                        &folder,
                                    );
                                });
                            } else {
                                selected_preview(ui, self.albums.layout);
                                ui.add_space(8.);
                                create = album_details(ui, &mut self.albums.name, &folder);
                            }
                        },
                    );
                });
                ui.add_space(12.);
            });
        if create {
            let name = self.albums.name.clone();
            let layout = self.albums.layout;
            self.album_work(ctx, "Creating album…", move || {
                Event::Opened(album::create(&folder, &name, layout))
            });
        }
    }
}

fn choice(ui: &mut egui::Ui, label: &str, size: Vec2, selected: bool) -> egui::Response {
    let (_, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn template_picker(ui: &mut egui::Ui, width: f32, selected: &mut Layout) {
    const GAP: f32 = 9.;
    let columns = (((width + GAP) / (142. + GAP)).floor() as usize).clamp(1, Layout::ALL.len());
    let card_width = (width - GAP * (columns - 1) as f32) / columns as f32;
    for row in Layout::ALL.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for &layout in row {
                if template_card(ui, layout, *selected == layout, card_width).clicked() {
                    *selected = layout;
                }
            }
        });
    }
}

fn template_card(ui: &mut egui::Ui, layout: Layout, selected: bool, width: f32) -> egui::Response {
    let (_, tone, description) = template_presentation(layout);
    let p = appearance::palette(ui.ctx());
    let accent = appearance::satin_ink(ui.ctx(), tone);
    let response = choice(ui, layout.label(), vec2(width, 116.), selected);
    let rect = response.rect;
    let emphasis = selected || response.hovered() || response.has_focus();
    let painter = ui.painter();
    appearance::gradient(
        painter,
        rect,
        12.,
        p.surface
            .lerp_to_gamma(accent, if emphasis { 0.18 } else { 0.07 }),
        p.surface,
    );
    painter.rect_stroke(
        rect,
        12,
        Stroke::new(
            if selected { 1.7 } else { 1. },
            if emphasis { accent } else { p.border },
        ),
        StrokeKind::Inside,
    );
    let sample = Rect::from_center_size(
        rect.center_top() + vec2(0., 46.),
        vec2((width - 28.).max(1.), 75.),
    );
    page_preview(painter, sample, layout, accent);
    painter.text(
        rect.center_bottom() - vec2(0., 16.),
        Align2::CENTER_CENTER,
        layout.label(),
        FontId::proportional(14.),
        p.ink,
    );
    if selected {
        let mark = rect.right_top() + vec2(-13., 13.);
        painter.circle_filled(mark, 7.5, accent);
        appearance::icon(painter, mark, Icon::Check, p.surface, 9.);
    }
    response.on_hover_text(description)
}

fn setup_card(ui: &egui::Ui) -> egui::Frame {
    let p = appearance::palette(ui.ctx());
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1., p.border))
        .corner_radius(13)
        .inner_margin(14)
}

fn selected_preview(ui: &mut egui::Ui, layout: Layout) {
    let (icon, tone, description) = template_presentation(layout);
    let p = appearance::palette(ui.ctx());
    let accent = appearance::satin_ink(ui.ctx(), tone);
    setup_card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(30.), Sense::hover());
            appearance::sculpted_icon(ui.painter(), rect.center(), icon, tone, 29., None);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(layout.label()).strong());
                ui.small(description);
            });
        });
        ui.add_space(5.);
        let (stage, _) = ui.allocate_exact_size(vec2(ui.available_width(), 154.), Sense::hover());
        let painter = ui.painter();
        appearance::gradient(
            painter,
            stage,
            9.,
            p.base.lerp_to_gamma(accent, 0.13),
            p.paper,
        );
        page_preview(painter, stage.shrink(11.), layout, accent);
        if stage.width() >= 305. {
            painter.text(
                stage.left_bottom() + vec2(9., -8.),
                Align2::LEFT_BOTTOM,
                "PDF preview",
                FontId::proportional(10.),
                p.muted,
            );
        }
    });
}

fn album_details(ui: &mut egui::Ui, name: &mut String, folder: &Path) -> bool {
    setup_card(ui)
        .show(ui, |ui| {
            let p = appearance::palette(ui.ctx());
            ui.set_width(ui.available_width());
            let label = ui.label(egui::RichText::new("Album name").strong());
            ui.add_sized(
                vec2(ui.available_width(), 29.),
                egui::TextEdit::singleline(name)
                    .hint_text("Untitled album")
                    .char_limit(album::TITLE_LIMIT),
            )
            .labelled_by(label.id);
            ui.add_space(8.);
            ui.label(egui::RichText::new("Save to").strong());
            egui::Frame::new()
                .fill(p.base)
                .corner_radius(7)
                .inner_margin(egui::Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.), Sense::hover());
                        appearance::icon(
                            ui.painter(),
                            rect.center(),
                            Icon::Open,
                            appearance::satin_ink(ui.ctx(), TileTone::Clay),
                            16.,
                        );
                        ui.add(
                            egui::Label::new(crate::capture_panel::folder_label(folder)).truncate(),
                        )
                        .on_hover_text(folder.display().to_string());
                    });
                });
            ui.small("One project in Library and Recents");
            ui.add_space(9.);
            appearance::action_button(ui, "Create album", Icon::Create, true, true).clicked()
        })
        .inner
}

fn template_presentation(layout: Layout) -> (Icon, TileTone, &'static str) {
    match layout {
        Layout::Walkthrough => (
            Icon::List,
            TileTone::Blue,
            "One large capture per page, with step notes",
        ),
        Layout::ContactSheet => (
            Icon::Library,
            TileTone::Teal,
            "Four titled captures in a two-column grid",
        ),
        Layout::FieldNotes => (
            Icon::Pencil,
            TileTone::Clay,
            "Two captures per page, each with notes beside it",
        ),
        Layout::Comparison => (
            Icon::Copy,
            TileTone::Violet,
            "Two large captures side by side on a landscape page",
        ),
        Layout::Storyboard => (
            Icon::Film,
            TileTone::Slate,
            "Six captures in sequence on a landscape page",
        ),
    }
}

fn page_preview(painter: &egui::Painter, bounds: Rect, layout: Layout, accent: Color32) {
    let layout = layout.page();
    let page_size = vec2(layout.size[0] as f32, layout.size[1] as f32);
    let scale = (bounds.width() / page_size.x).min(bounds.height() / page_size.y);
    let page = Rect::from_center_size(bounds.center(), page_size * scale);
    let draw_rect = |[x, y, w, h]: [u32; 4]| {
        Rect::from_min_size(
            page.min + vec2(x as f32, y as f32) * scale,
            vec2(w as f32, h as f32) * scale,
        )
    };
    let white = Color32::from_rgb(250, 252, 255);
    let ink = Color32::from_rgb(41, 58, 82);
    let color = ink.lerp_to_gamma(accent, 0.72);
    let tint = white.lerp_to_gamma(color, 0.15);
    painter.add(
        egui::Shadow {
            offset: [0, 2],
            blur: 5,
            spread: 0,
            color: Color32::from_black_alpha(55),
        }
        .as_shape(page, egui::CornerRadius::same(2)),
    );
    painter.rect(
        page,
        2,
        white,
        Stroke::new(1., Color32::from_rgb(201, 212, 226)),
        StrokeKind::Inside,
    );
    painter.rect_filled(
        draw_rect([album::PageLayout::MARGIN, 54, layout.size[0] * 2 / 5, 35]),
        1,
        color,
    );
    for index in 0..layout.entries() {
        let slot = layout.slot(index);
        let [x, y, width, _] = slot.title;
        painter.rect_filled(draw_rect([x, y + 24, width / 2, 22]), 1, ink);

        let capture = draw_rect(slot.image);
        painter.rect_filled(capture, 1, tint);
        let panel = capture.shrink2(capture.size() * 0.12);
        let header = Rect::from_min_size(panel.min, vec2(panel.width(), panel.height() * 0.17));
        painter.rect_filled(panel, 1, white);
        painter.rect_filled(header, 1, color);
        let feature = Rect::from_min_size(
            panel.min + panel.size() * vec2(0.1, 0.32),
            panel.size() * vec2(0.37, 0.53),
        );
        painter.rect_filled(
            feature,
            1,
            tint.lerp_to_gamma(color, 0.2 + (index % 3) as f32 * 0.18),
        );
        for line in 0..3 {
            let row = Rect::from_min_size(
                panel.min + panel.size() * vec2(0.55, 0.36 + line as f32 * 0.19),
                panel.size() * vec2(0.36 - line as f32 * 0.05, 0.08),
            );
            painter.rect_filled(row, 1, color.lerp_to_gamma(white, 0.5));
        }
        let [x, y, width, height] = slot.note;
        let line_gap = (height / 4).min(44);
        for (line, fraction) in [85, 64, 74].into_iter().enumerate() {
            painter.rect_filled(
                draw_rect([
                    x,
                    y + 7 + line as u32 * line_gap,
                    width * fraction / 100,
                    13,
                ]),
                1,
                ink.lerp_to_gamma(white, 0.62),
            );
        }
    }
}
