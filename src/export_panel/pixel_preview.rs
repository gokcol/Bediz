//! Inspect finished pixels at one image pixel per display pixel.

use crate::{
    capture_size::OutputSize,
    document::Document,
    export::{self, ExportOptions},
};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2, emath::GuiRounding};
use image::RgbaImage;
use std::{sync::mpsc, thread::JoinHandle, time::Duration};

type PixelReply = Result<RgbaImage, String>;

#[derive(Default)]
pub(super) struct PixelPreview {
    open: bool,
    output_size: Option<OutputSize>,
    worker: Option<JoinHandle<()>>,
    result: Option<mpsc::Receiver<PixelReply>>,
    image: Option<PixelReply>,
    origin: Option<Vec2>,
    tile: Option<Tile>,
}

#[derive(PartialEq, Eq)]
struct Sample {
    origin: [u32; 2],
    size: [u32; 2],
    options: ExportOptions,
}

struct Tile {
    sample: Sample,
    texture: egui::TextureHandle,
}

impl PixelPreview {
    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    pub(super) fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open = true;
        }
    }

    pub(super) fn close(&mut self) {
        self.open = false;
        self.clear_pixels();
    }

    fn clear_pixels(&mut self) {
        self.output_size = None;
        self.result = None;
        self.image = None;
        self.origin = None;
        self.tile = None;
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui, document: &Document, options: ExportOptions) {
        if !self.open {
            return;
        }
        if self.output_size != Some(options.output_size) {
            self.clear_pixels();
            self.output_size = Some(options.output_size);
        }
        self.poll(ui.ctx(), document, options.output_size);
        ui.add_space(4.);
        ui.small("Drag to inspect · 1 image pixel per display pixel");
        let (rect, drag) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 108.), egui::Sense::drag());
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 4, ui.visuals().extreme_bg_color);
        let image = match &self.image {
            Some(Ok(image)) => image,
            Some(Err(error)) => {
                ui.weak(error);
                return;
            }
            None => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Preparing preview…",
                    egui::TextStyle::Body.resolve(ui.style()),
                    ui.visuals().weak_text_color(),
                );
                return;
            }
        };

        let pixels_per_point = ui.ctx().pixels_per_point();
        let size = [
            ((rect.width() * pixels_per_point).floor() as u32).clamp(1, image.width()),
            ((rect.height() * pixels_per_point).floor() as u32).clamp(1, image.height()),
        ];
        let travel = egui::vec2(
            (image.width() - size[0]) as f32,
            (image.height() - size[1]) as f32,
        );
        let origin = self.origin.get_or_insert(travel * 0.5);
        let pointer_delta = if drag.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            drag.drag_delta() * pixels_per_point
        } else {
            if drag.hovered() && travel != Vec2::ZERO {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            Vec2::ZERO
        };
        *origin = (*origin - pointer_delta).clamp(Vec2::ZERO, travel);
        let sample = Sample {
            origin: [origin.x.round() as u32, origin.y.round() as u32],
            size,
            options,
        };
        if self.tile.as_ref().is_none_or(|tile| tile.sample != sample) {
            let pixels = (0..size[1])
                .flat_map(|y| (0..size[0]).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let [r, g, b, a] = export::preview_pixel(
                        image,
                        options,
                        sample.origin[0] + x,
                        sample.origin[1] + y,
                    )
                    .0;
                    Color32::from_rgba_unmultiplied(r, g, b, a)
                })
                .collect();
            let color = egui::ColorImage::new([size[0] as usize, size[1] as usize], pixels);
            let texture = match self.tile.take() {
                Some(mut tile) => {
                    tile.texture.set(color, egui::TextureOptions::NEAREST);
                    tile.texture
                }
                None => ui.ctx().load_texture(
                    "Saved image pixels",
                    color,
                    egui::TextureOptions::NEAREST,
                ),
            };
            self.tile = Some(Tile { sample, texture });
        }
        if let Some(tile) = &self.tile {
            let size = egui::vec2(size[0] as f32, size[1] as f32) / pixels_per_point;
            let position = (rect.center() - size * 0.5).round_to_pixels(pixels_per_point);
            let sample_rect = Rect::from_min_size(position, size);
            crate::editor::paint_checkerboard(&painter.with_clip_rect(sample_rect), sample_rect);
            painter.image(
                tile.texture.id(),
                sample_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                Color32::WHITE,
            );
        }
    }

    fn poll(&mut self, ctx: &egui::Context, document: &Document, size: OutputSize) {
        if let Some(result) = &self.result {
            let reply = match result.try_recv() {
                Ok(reply) => reply,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err("Preview could not be prepared.".into())
                }
            };
            self.image = Some(reply);
            self.result = None;
            self.worker = None;
        }
        if self.image.is_some() {
            return;
        }
        // Dropping a stale reply immediately releases its image. Let its worker
        // finish before starting another large flatten or resize.
        if self.worker.as_ref().is_some_and(|job| !job.is_finished()) {
            ctx.request_repaint_after(Duration::from_millis(40));
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let document = document.clone();
        let ctx = ctx.clone();
        self.worker = Some(std::thread::spawn(move || {
            let _ = tx.send(export::resized_pixels(&document, size));
            ctx.request_repaint();
        }));
        self.result = Some(rx);
    }
}
