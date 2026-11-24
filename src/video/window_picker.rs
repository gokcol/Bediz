use super::{Icon, VideoTarget, appearance, capture, load_texture};
use crate::capture_panel::CaptureLook;
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Stroke, StrokeKind, vec2};
use std::{collections::HashMap, sync::mpsc};

type Preview = (u32, Option<image::RgbaImage>);

#[derive(Default)]
pub(super) struct WindowPicker {
    pub selected: Vec<u32>,
    search: String,
    previews: HashMap<u32, Option<egui::TextureHandle>>,
    pending: Option<mpsc::Receiver<Preview>>,
}

pub(super) enum Action {
    Back,
    UseSelection,
    ChangeTarget(VideoTarget),
}

impl WindowPicker {
    pub fn open(&mut self, selected: Vec<u32>) {
        self.selected = selected;
        self.search.clear();
    }

    pub fn texture(&self, id: u32) -> Option<&egui::TextureHandle> {
        self.previews.get(&id)?.as_ref()
    }

    fn update_previews(&mut self, ctx: &egui::Context, visible: &[u32], demo: bool) {
        if let Some(pending) = &self.pending {
            loop {
                match pending.try_recv() {
                    Ok((id, image)) => {
                        self.previews.insert(
                            id,
                            image.map(|image| load_texture(ctx, "window-picker-thumbnail", &image)),
                        );
                        ctx.request_repaint();
                    }
                    Err(mpsc::TryRecvError::Empty) => return,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.pending = None;
                        break;
                    }
                }
            }
        }
        let ids: Vec<_> = visible
            .iter()
            .copied()
            .filter(|id| !self.previews.contains_key(id))
            .collect();
        if ids.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        self.pending = Some(rx);
        std::thread::spawn(move || {
            for id in ids {
                let image = if demo {
                    let colors = [[28, 117, 195, 255], [17, 139, 114, 255]];
                    Some(image::RgbaImage::from_pixel(
                        320,
                        180,
                        image::Rgba(colors[id as usize % 2]),
                    ))
                } else {
                    capture::window_thumbnail(id, 320).ok()
                };
                if tx.send((id, image)).is_err() {
                    return;
                }
                crate::runtime::wake_root(&ctx);
            }
            drop(tx);
            crate::runtime::wake_root(&ctx);
        });
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        windows: &[capture::WindowInfo],
        target: VideoTarget,
        limit: usize,
        look: &CaptureLook,
        demo: bool,
    ) -> Option<Action> {
        let size = ctx.content_rect().size();
        let preferred_width = match windows.len() {
            0 | 1 => 390.,
            2 => 574.,
            _ => 772.,
        };
        let width = (size.x - 52.).clamp(224., preferred_width);
        let columns = ((width - 14.) / 196.).floor().clamp(1., 3.) as usize;
        let gap = 8.;
        let card_width = (width - 16. - (columns - 1) as f32 * gap) / columns as f32;
        let card_height = (card_width * 0.57).clamp(118., 158.);
        let rows = windows.len().div_ceil(columns).max(1) as f32;
        let gallery_height =
            (card_height * rows + gap * (rows - 1.)).min((size.y - 206.).clamp(88., 436.));
        let multiple = limit > 1;
        let title = if multiple {
            "Choose two windows"
        } else {
            "Choose a window"
        };
        let modal = egui::Modal::new(egui::Id::new("video-window-gallery"))
            .backdrop_color(Color32::from_black_alpha(98))
            .frame(
                egui::Frame::new()
                    .fill(look.palette.paper)
                    .stroke(Stroke::new(1., look.palette.border))
                    .inner_margin(16)
                    .corner_radius(14.)
                    .shadow(egui::Shadow {
                        offset: [0, 6],
                        blur: 18,
                        spread: 0,
                        color: Color32::from_black_alpha(105),
                    }),
            )
            .show(ctx, |ui| {
                crate::capture_panel::apply_look(ui, look);
                ui.set_width(width);
                ui.set_min_height(gallery_height + 134.);
                ui.horizontal(|ui| {
                    let (icon, _) = ui.allocate_exact_size(vec2(32., 32.), egui::Sense::hover());
                    appearance::sculpted_icon(
                        ui.painter(),
                        icon.center(),
                        Icon::View,
                        appearance::TileTone::Teal,
                        30.,
                        None,
                    );
                    ui.heading(title);
                });
                let mut target = target;
                let target_changed = target.ui(ui, look);
                let search_changed = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Filter by window title")
                            .desired_width(width),
                    )
                    .changed();
                let query = self.search.trim().to_lowercase();
                let filtered: Vec<_> = windows
                    .iter()
                    .filter(|window| {
                        query.is_empty() || window.name.to_lowercase().contains(&query)
                    })
                    .collect();
                let mut visible = Vec::new();
                let mut clicked = None;
                if filtered.is_empty() {
                    ui.allocate_ui_with_layout(
                        vec2(width, gallery_height),
                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                        |ui| {
                            ui.weak(if windows.is_empty() {
                                "No open windows to record."
                            } else {
                                "No windows match your search."
                            })
                        },
                    );
                } else {
                    // Bounded viewport keeps lists with many windows responsive
                    // and requests thumbnails only for rows the picker draws.
                    let mut scroll = egui::ScrollArea::vertical()
                        .id_salt("window-cards")
                        .min_scrolled_height(gallery_height)
                        .max_height(gallery_height)
                        .auto_shrink([false, false]);
                    if search_changed {
                        scroll = scroll.vertical_scroll_offset(0.);
                    }
                    scroll.show_rows(
                        ui,
                        card_height,
                        filtered.len().div_ceil(columns),
                        |ui, rows| {
                            for row in rows {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = gap;
                                    for window in filtered.iter().skip(row * columns).take(columns)
                                    {
                                        visible.push(window.id);
                                        let rank =
                                            self.selected.iter().position(|id| *id == window.id);
                                        let response = ui
                                            .push_id(window.id, |ui| {
                                                self.card(
                                                    ui,
                                                    window,
                                                    vec2(card_width, card_height),
                                                    rank,
                                                    look,
                                                )
                                            })
                                            .inner;
                                        if response.clicked() {
                                            clicked = Some(window.id);
                                        }
                                    }
                                });
                            }
                        },
                    );
                }
                self.update_previews(ctx, &visible, demo);
                if let Some(id) = clicked {
                    if !multiple {
                        self.selected = vec![id];
                    } else if let Some(index) =
                        self.selected.iter().position(|selected| *selected == id)
                    {
                        self.selected.remove(index);
                    } else if self.selected.len() < limit {
                        self.selected.push(id);
                    }
                }
                ui.separator();
                let action = ui
                    .horizontal(|ui| {
                        ui.weak(format!("{} of {limit} selected", self.selected.len()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let mut action = None;
                            let label = if multiple {
                                "Arrange windows"
                            } else {
                                "Use window"
                            };
                            if appearance::action_button(
                                ui,
                                label,
                                Icon::View,
                                self.selected.len() == limit,
                                true,
                            )
                            .clicked()
                            {
                                action = Some(Action::UseSelection);
                            }
                            if ui.button("Back").clicked() {
                                action = Some(Action::Back);
                            }
                            action
                        })
                        .inner
                    })
                    .inner;
                if target_changed {
                    Some(Action::ChangeTarget(target))
                } else {
                    action
                }
            });
        modal.inner
    }

    fn card(
        &self,
        ui: &mut egui::Ui,
        window: &capture::WindowInfo,
        size: egui::Vec2,
        selected: Option<usize>,
        look: &CaptureLook,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                ui.is_enabled(),
                selected.is_some(),
                &window.name,
            )
        });
        if ui.is_rect_visible(rect) {
            let p = look.palette;
            let emphasized = selected.is_some() || response.has_focus();
            let edge = if emphasized { p.accent } else { p.border };
            let painter = ui.painter();
            painter.rect(
                rect,
                9,
                if response.hovered() { p.cream } else { p.paper },
                Stroke::new(if emphasized { 2. } else { 1. }, edge),
                StrokeKind::Inside,
            );
            let photo = Rect::from_min_max(rect.min + vec2(6., 6.), rect.max - vec2(6., 28.));
            painter.rect_filled(photo, 5, p.navy);
            if let Some(texture) = self.texture(window.id) {
                let size = texture.size_vec2();
                let ratio = (photo.size() / size).min_elem();
                painter.image(
                    texture.id(),
                    Rect::from_center_size(photo.center(), size * ratio),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                    Color32::WHITE,
                );
            } else {
                appearance::icon(
                    painter,
                    photo.center() - vec2(0., 9.),
                    Icon::View,
                    Color32::from_gray(208),
                    23.,
                );
                painter.text(
                    photo.center() + vec2(0., 15.),
                    Align2::CENTER_CENTER,
                    if self.previews.contains_key(&window.id) {
                        "Preview unavailable"
                    } else {
                        "Loading preview…"
                    },
                    egui::FontId::proportional(11.),
                    Color32::from_gray(218),
                );
            }
            if let Some(index) = selected {
                let center = photo.right_top() + vec2(-13., 13.);
                painter.circle_filled(center, 10., p.accent);
                painter.text(
                    center,
                    Align2::CENTER_CENTER,
                    index + 1,
                    egui::FontId::proportional(12.),
                    Color32::WHITE,
                );
            }
            let text = egui::WidgetText::from(egui::RichText::new(&window.name).color(p.ink));
            let galley = text.into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                rect.width() - 16.,
                egui::TextStyle::Body,
            );
            painter.galley(
                Pos2::new(rect.left() + 8., rect.bottom() - 22.),
                galley,
                p.ink,
            );
        }
        response
            .on_hover_text(&window.name)
            .on_hover_cursor(egui::CursorIcon::PointingHand)
    }
}
