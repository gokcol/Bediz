use super::*;
use crate::{appearance, geometry::CanvasRect, page_correction::PageCorrection};
use egui::Shape;

pub(super) struct PageDraft {
    page: PageCorrection,
    preview: Option<(PageCorrection, egui::TextureHandle)>,
}

impl EditorState {
    pub(super) fn open_page_correction(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        self.finish_gesture(document, history);
        self.cancel_image_tools();
        self.page_correction = Some(PageDraft {
            page: document.geometry.page.unwrap_or_else(|| {
                PageCorrection::for_crop(document.geometry.crop_rect(original_size(document)))
            }),
            preview: None,
        });
    }

    pub(super) fn page_correction_dialog(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
        original: &egui::TextureHandle,
    ) {
        let Some(draft) = self.page_correction.as_mut() else {
            return;
        };
        let size = original_size(document);
        let mut open = true;
        let response = Dialog::new(
            "Correct photographed page",
            "Drag each handle to a corner of the page.",
            DialogIcon::Edit,
            860.0,
        )
        .modal_closable(ctx, egui::Id::new("correct-page"), &mut open, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10., 8.);
            let columns = (ui.available_width() - 10.) / 2.;
            let height = (ctx.content_rect().height() - 240.).clamp(130., 430.);
            let panel = egui::vec2(columns, height);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.strong("Original photo");
                    draft.draw_corners(ui, original, size, panel);
                });
                ui.vertical(|ui| {
                    ui.strong("Corrected page");
                    draft.draw_preview(ui, document, size, panel);
                });
            });
            ui.horizontal(|ui| {
                appearance::checkbox(ui, &mut draft.page.clean_paper, "Clean paper")
                    .on_hover_text("Even out shadows on the paper while retaining colored ink");
                if ui.button("Reset corners").clicked() {
                    draft.page.corners =
                        PageCorrection::for_crop(ImageGeometry::default().crop_rect(size)).corners;
                }
            });
            let candidate = corrected_geometry(document, draft.page);
            if let Err(error) = &candidate {
                ui.colored_label(crate::dialogs::DANGER, error);
            }
            let mut apply = false;
            let mut cancel = false;
            ui.horizontal(|ui| {
                apply = appearance::action_button(
                    ui,
                    "Apply correction",
                    DialogIcon::Check,
                    candidate.is_ok(),
                    true,
                )
                .clicked();
                cancel = ui.button("Cancel").clicked();
            });
            (apply.then_some(candidate.ok()).flatten(), cancel)
        });
        let (apply, cancel) = response.inner;
        if !open || cancel {
            self.page_correction = None;
        } else if let Some(geometry) = apply
            && self.apply_geometry(document, history, geometry)
        {
            self.page_correction = None;
            self.clear_selection();
            self.deselect_pixels();
        }
    }
}

impl PageDraft {
    fn draw_corners(
        &mut self,
        ui: &mut egui::Ui,
        texture: &egui::TextureHandle,
        [width, height]: [u32; 2],
        panel: Vec2,
    ) {
        let (bounds, _) = ui.allocate_exact_size(panel, Sense::hover());
        let rect = fitted_rect(bounds, [width, height]);
        let dimensions = egui::vec2(width as f32, height as f32);
        let point_at =
            |[x, y]: [u32; 2]| rect.min + rect.size() * egui::vec2(x as f32, y as f32) / dimensions;
        for (i, label) in ["Top left", "Top right", "Bottom right", "Bottom left"]
            .into_iter()
            .enumerate()
        {
            let point = point_at(self.page.corners[i]);
            let response = ui
                .interact(
                    Rect::from_center_size(point, Vec2::splat(24.)),
                    ui.id().with(("page-corner", i)),
                    Sense::click_and_drag(),
                )
                .on_hover_cursor(egui::CursorIcon::Move)
                .on_hover_text(label);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, label)
            });
            if response.clicked() || response.drag_started() {
                response.request_focus();
            }
            if (response.dragged() || response.drag_stopped())
                && let Some(pos) = response.interact_pointer_pos()
            {
                let pixel = (pos - rect.min) / rect.size() * dimensions;
                self.page.corners[i] = [
                    pixel.x.round().clamp(0., dimensions.x) as u32,
                    pixel.y.round().clamp(0., dimensions.y) as u32,
                ];
            } else if response.has_focus() {
                let delta = ui.input_mut(|input| {
                    let modifiers = input.modifiers;
                    let step: i32 = if modifiers.shift { 10 } else { 1 };
                    let mut axis = |down, up| {
                        step * (input.count_and_consume_key(modifiers, up) as i32
                            - input.count_and_consume_key(modifiers, down) as i32)
                    };
                    [
                        axis(Key::ArrowLeft, Key::ArrowRight),
                        axis(Key::ArrowUp, Key::ArrowDown),
                    ]
                });
                for (axis, limit) in [width, height].into_iter().enumerate() {
                    self.page.corners[i][axis] = self.page.corners[i][axis]
                        .saturating_add_signed(delta[axis])
                        .min(limit);
                }
            }
        }
        let painter = ui.painter();
        painter.image(texture.id(), rect, uv_rect(), Color32::WHITE);
        let mut corners = self.page.corners.map(point_at).to_vec();
        corners.push(corners[0]);
        let accent = appearance::palette(ui.ctx()).accent;
        painter.add(Shape::line(
            corners.clone(),
            Stroke::new(4., Color32::BLACK),
        ));
        painter.add(Shape::line(corners, Stroke::new(2., accent)));
        for (i, point) in self.page.corners.map(point_at).into_iter().enumerate() {
            let focused = ui.memory(|memory| memory.has_focus(ui.id().with(("page-corner", i))));
            painter.circle_filled(point, 7., if focused { accent } else { Color32::WHITE });
            painter.circle_stroke(point, 7., Stroke::new(1.5, Color32::from_gray(30)));
        }
    }

    fn draw_preview(
        &mut self,
        ui: &mut egui::Ui,
        document: &Document,
        size: [u32; 2],
        panel: Vec2,
    ) {
        let (bounds, _) = ui.allocate_exact_size(panel, Sense::hover());
        if self.page.validate(size).is_err() {
            return;
        }
        let crop = self.page.frame(size);
        let rect = fitted_rect(bounds, [crop.width, crop.height]);
        if self
            .preview
            .as_ref()
            .is_none_or(|(page, _)| *page != self.page)
        {
            let preview = self.page.render(
                &document.image,
                canvas_rect(crop),
                [
                    (rect.width() * 1.45).round().clamp(1., 640.) as u32,
                    (rect.height() * 1.45).round().clamp(1., 640.) as u32,
                ],
                None,
            );
            let texture = ui.ctx().load_texture(
                "corrected-page-preview",
                crate::image_loading::color_image(&preview),
                egui::TextureOptions::LINEAR,
            );
            self.preview = Some((self.page, texture));
        }
        if let Some((_, texture)) = &self.preview {
            ui.painter()
                .image(texture.id(), rect, uv_rect(), Color32::WHITE);
        }
    }
}

fn corrected_geometry(document: &Document, page: PageCorrection) -> Result<ImageGeometry, String> {
    let original = original_size(document);
    page.validate(original)?;
    let crop = page.frame(original);
    let mut geometry = document.geometry;
    geometry.page = Some(page);
    geometry.crop = Some(crop);
    if let Some(canvas) = geometry.canvas {
        let page_rect = canvas_rect(crop);
        let left = canvas.x.min(page_rect.x);
        let top = canvas.y.min(page_rect.y);
        let right = (canvas.x + canvas.width as i32).max(page_rect.x + page_rect.width as i32);
        let bottom = (canvas.y + canvas.height as i32).max(page_rect.y + page_rect.height as i32);
        geometry.canvas = Some(CanvasRect {
            x: left,
            y: top,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        });
        geometry.viewport = Some(page_rect);
    }
    geometry.validate(original)?;
    Ok(geometry)
}

fn canvas_rect(crop: CropRect) -> CanvasRect {
    CanvasRect {
        x: crop.x as i32,
        y: crop.y as i32,
        width: crop.width,
        height: crop.height,
    }
}

fn fitted_rect(bounds: Rect, [width, height]: [u32; 2]) -> Rect {
    let size = egui::vec2(width as f32, height as f32);
    let scale = (bounds.shrink(13.).size() / size).min_elem();
    Rect::from_center_size(bounds.center(), size * scale)
}

fn uv_rect() -> Rect {
    Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use std::sync::Arc;

    #[test]
    fn corner_drag_keeps_the_final_release_position() {
        let ctx = egui::Context::default();
        let panel = egui::vec2(326., 226.);
        let original = [300, 200];
        let mut draft = PageDraft {
            page: PageCorrection::for_crop(ImageGeometry::default().crop_rect(original)),
            preview: None,
        };
        let texture = ctx.load_texture(
            "page test",
            egui::ColorImage::filled([1, 1], Color32::WHITE),
            egui::TextureOptions::NEAREST,
        );
        let mut frame = |events| {
            let mut corner = Pos2::ZERO;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, panel * 2.)),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        corner = fitted_rect(
                            Rect::from_min_size(ui.next_widget_position(), panel),
                            original,
                        )
                        .min;
                        draft.draw_corners(ui, &texture, original, panel);
                    });
                },
            );
            corner
        };
        let start = frame(vec![]);
        let moved = start + egui::vec2(30., 20.);
        let released = start + egui::vec2(35., 25.);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
        frame(vec![egui::Event::PointerMoved(moved)]);
        frame(vec![
            egui::Event::PointerMoved(released),
            button(released, false),
        ]);
        assert_eq!(draft.page.corners[0], [35, 25]);
    }
    #[test]
    fn page_correction_round_trips_with_undo_and_existing_canvas_edits() {
        let mut editor = EditorState::default();
        let mut mark = editor.new_annotation(AnnotationKind::Rectangle, Point { x: 25., y: 22. });
        mark.end = Point { x: 44., y: 40. };
        let mut document = Document {
            image: Arc::new(RgbaImage::from_fn(100, 80, |x, y| {
                Rgba([x as u8 + 120, y as u8 + 140, 195, 255])
            })),
            annotations: vec![mark],
            geometry: ImageGeometry {
                canvas: Some(CanvasRect {
                    x: -10,
                    y: -15,
                    width: 130,
                    height: 100,
                }),
                crop: Some(CropRect {
                    x: 8,
                    y: 5,
                    width: 70,
                    height: 50,
                }),
                quarter_turns: 1,
                resize: Some([44, 60]),
                flip_horizontal: true,
                ..Default::default()
            },
        };
        let original = Arc::clone(&document.image);
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        let correction = PageCorrection {
            corners: [[15, 5], [86, 12], [80, 73], [9, 68]],
            clean_paper: true,
        };
        let geometry = corrected_geometry(&document, correction).unwrap();
        assert!(editor.apply_geometry(&mut document, &mut history, geometry));
        assert!(Arc::ptr_eq(&original, &document.image));
        assert_eq!(document.annotations, before.annotations);
        assert_eq!(document.geometry.quarter_turns, 1);
        assert!(document.geometry.flip_horizontal);
        assert_eq!(document.geometry.canvas, before.geometry.canvas);
        let corrected = render::flatten(&document).unwrap();
        assert_eq!(corrected.dimensions(), (44, 60));
        assert!(corrected.pixels().all(|pixel| pixel[3] == 255));
        assert_ne!(
            render::flatten(&Document {
                image: Arc::clone(&original),
                annotations: before.annotations.clone(),
                geometry: before.geometry,
            })
            .unwrap(),
            corrected
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("corrected.myimage");
        crate::document::save_project(&document, &path).unwrap();
        let mut reopened = crate::document::load_project(&path).unwrap();
        assert_eq!(reopened.image, original);
        assert_eq!(reopened.geometry, geometry);
        assert_eq!(render::flatten(&reopened).unwrap(), corrected);
        let thumbnail = render::thumbnail(&reopened, 100, 100).unwrap();
        assert_eq!(thumbnail.dimensions(), (44, 60));
        assert!(thumbnail.pixels().all(|pixel| pixel[3] == 255));
        let mut reopened_history = EditHistory::new(reopened.edit_state());
        editor.open_page_correction(&mut reopened, &mut reopened_history);
        assert_eq!(editor.page_correction.unwrap().page, correction);
        assert!(history.undo(&mut document));
        assert_eq!(document.geometry, before.geometry);
        assert!(history.redo(&mut document));
        assert_eq!(render::flatten(&document).unwrap(), corrected);
    }
}
