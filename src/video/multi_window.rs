//! Window settings and the same output geometry for preview and both recorders.
use super::{Icon, VideoTarget, appearance, capture, load_texture, window_picker::WindowPicker};
use eframe::egui::{self, Color32, Pos2, Rect, vec2};
use serde::Serialize;
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub(super) const WINDOW_LIMIT: usize = 2;
pub(crate) const WINDOW_COMPOSITION_SIZE: [u32; 2] = [3840, 2160];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Layout {
    #[default]
    SideBySide,
    Stacked,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Detail {
    #[default]
    Original,
    Medium,
    Low,
}

impl Detail {
    fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Medium => "720 px short edge",
            Self::Low => "480 px short edge",
        }
    }

    fn limit(self) -> u32 {
        match self {
            Self::Original => 0,
            Self::Medium => 720,
            Self::Low => 480,
        }
    }
}

struct Source {
    id: u32,
    name: String,
    cursor: bool,
    fill: bool,
    detail: Detail,
    preview: Option<Result<egui::TextureHandle, String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub(in crate::app) struct WindowTile {
    pub id: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub cursor: bool,
    pub fill: bool,
    pub detail: u32,
}

#[cfg(any(target_os = "windows", test))]
impl WindowTile {
    pub(super) fn blit_nv12(self, width: u32, height: u32, source: &[u8], target: &mut [u8]) {
        let (width, height) = (width as usize, height as usize);
        let (tile_width, tile_height) = (self.width as usize, self.height as usize);
        let x = self.x as usize;
        let y = self.y as usize;
        let (luma, chroma) = target.split_at_mut(width * height);
        let (tile_luma, tile_chroma) = source.split_at(tile_width * tile_height);
        for (plane, rows, top) in [(luma, tile_luma, y), (chroma, tile_chroma, y / 2)] {
            for (row, pixels) in rows.chunks_exact(tile_width).enumerate() {
                let start = (top + row) * width + x;
                plane[start..start + tile_width].copy_from_slice(pixels);
            }
        }
    }
}

type Previews = Vec<(u32, Result<image::RgbaImage, String>)>;

pub(in crate::app) struct WindowSetup {
    sources: Vec<Source>,
    layout: Layout,
    split: u32,
    previews: Option<Receiver<Previews>>,
    refresh: bool,
    pub(super) picker: WindowPicker,
    pub single: Option<u32>,
    pub picker_target: Option<VideoTarget>,
    pub initial_display: Option<u32>,
}

impl Default for WindowSetup {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            layout: Layout::default(),
            split: 50,
            previews: None,
            refresh: false,
            picker: WindowPicker::default(),
            single: None,
            picker_target: None,
            initial_display: None,
        }
    }
}

impl WindowSetup {
    pub(super) fn panel_height(&self) -> f32 {
        if self.sources.is_empty() { 130. } else { 490. }
    }

    pub fn ready(&self) -> bool {
        self.sources.len() >= 2
    }

    pub fn contains(&self, id: u32) -> bool {
        self.sources.iter().any(|source| source.id == id)
    }

    pub fn open_picker(&mut self, target: VideoTarget) {
        self.picker_target = Some(target);
        let selected = if target == VideoTarget::Window {
            self.single.into_iter().collect()
        } else {
            self.sources.iter().map(|source| source.id).collect()
        };
        self.picker.open(selected);
    }

    pub(super) fn apply_pick(
        &mut self,
        windows: &[capture::WindowInfo],
        cursor: bool,
        multiple: bool,
    ) {
        if !multiple {
            self.single = self.picker.selected.first().copied();
            return;
        }
        let mut previous = std::mem::take(&mut self.sources);
        self.sources = self
            .picker
            .selected
            .iter()
            .filter_map(|id| {
                if let Some(index) = previous.iter().position(|source| source.id == *id) {
                    return Some(previous.remove(index));
                }
                let window = windows.iter().find(|window| window.id == *id)?;
                Some(Source {
                    id: *id,
                    name: window.name.clone(),
                    cursor,
                    fill: false,
                    detail: Detail::default(),
                    preview: self.picker.texture(*id).cloned().map(Ok),
                })
            })
            .collect();
    }

    pub fn reset(&mut self) {
        self.sources.clear();
        self.layout = Layout::default();
        self.split = 50;
        self.previews = None;
        self.refresh = false;
        self.single = None;
        self.picker_target = None;
    }

    pub fn tiles(&self, width: u32, height: u32) -> Vec<WindowTile> {
        grid(width, height, self.sources.len(), self.layout, self.split)
            .into_iter()
            .zip(&self.sources)
            .map(|([x, y, width, height], source)| WindowTile {
                id: source.id,
                x,
                y,
                width,
                height,
                cursor: source.cursor,
                fill: source.fill,
                detail: source.detail.limit(),
            })
            .collect()
    }

    fn update_previews(&mut self, ctx: &egui::Context, demo: bool) {
        if let Some(pending) = &self.previews {
            match pending.try_recv() {
                Ok(result) => {
                    for (id, image) in result {
                        if let Some(source) = self.sources.iter_mut().find(|source| source.id == id)
                        {
                            source.preview = Some(
                                image.map(|image| load_texture(ctx, "window-preview", &image)),
                            );
                        }
                    }
                    self.previews = None;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.previews = None;
                    for source in self
                        .sources
                        .iter_mut()
                        .filter(|source| source.preview.is_none())
                    {
                        source.preview = Some(Err("Could not load this preview".into()));
                    }
                }
            }
        }
        let ids: Vec<_> = self
            .sources
            .iter()
            .filter(|source| self.refresh || source.preview.is_none())
            .map(|source| source.id)
            .collect();
        self.refresh = false;
        if ids.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        self.previews = Some(rx);
        std::thread::spawn(move || {
            let snapshots = ids
                .into_iter()
                .map(|id| {
                    let image = if demo {
                        let colors = [[28, 117, 195, 255], [17, 139, 114, 255]];
                        Ok(image::RgbaImage::from_pixel(
                            480,
                            270,
                            image::Rgba(colors[id as usize % 2]),
                        ))
                    } else {
                        capture::window_thumbnail(id, 480)
                    };
                    (id, image)
                })
                .collect();
            let _ = tx.send(snapshots);
            crate::runtime::wake_root(&ctx);
        });
    }

    pub(super) fn ui(&mut self, ui: &mut egui::Ui, output: [u32; 2], demo: bool) -> bool {
        let change = ui
            .horizontal_wrapped(|ui| {
                ui.label(format!("{} of {WINDOW_LIMIT} selected", self.sources.len()));
                let change = ui.button("Change windows").clicked();
                if ui
                    .add_enabled(
                        !self.sources.is_empty() && self.previews.is_none(),
                        egui::Button::new("Refresh previews"),
                    )
                    .clicked()
                {
                    self.refresh = true;
                }
                change
            })
            .inner;
        if self.sources.is_empty() {
            return change;
        }

        self.update_previews(ui.ctx(), demo);
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.layout, Layout::SideBySide, "Side by side");
            ui.selectable_value(&mut self.layout, Layout::Stacked, "Stacked");
            if self.sources.len() == 2 {
                ui.add(
                    egui::Slider::new(&mut self.split, 30..=70)
                        .text("First window share")
                        .suffix("%"),
                );
            }
        });
        let tiles = self.tiles(output[0], output[1]);
        self.paint_preview(ui, output, &tiles);

        let mut move_up = None;
        let mut remove = None;
        for (index, source) in self.sources.iter_mut().enumerate() {
            ui.push_id(source.id, |ui| {
                ui.separator();
                ui.horizontal(|ui| {
                    let name_width = (ui.available_width() - 74.).max(40.);
                    ui.add_sized(
                        [name_width, 23.],
                        egui::Label::new(
                            egui::RichText::new(format!("{} · {}", index + 1, source.name))
                                .strong(),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&source.name);
                    if ui
                        .add_enabled_ui(index > 0, |ui| {
                            super::super::capture_session::capture_icon(
                                ui,
                                "Move window earlier",
                                Icon::Up,
                                false,
                            )
                        })
                        .inner
                        .clicked()
                    {
                        move_up = Some(index);
                    }
                    if super::super::capture_session::capture_icon(
                        ui,
                        "Remove window",
                        Icon::Close,
                        false,
                    )
                    .clicked()
                    {
                        remove = Some(index);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut source.fill, false, "Fit");
                    ui.selectable_value(&mut source.fill, true, "Fill")
                        .on_hover_text("Crop the edges to fill this window’s tile");
                    ui.weak("Detail");
                    egui::ComboBox::from_id_salt("window-detail")
                        .selected_text(source.detail.label())
                        .show_ui(ui, |ui| {
                            for detail in [Detail::Original, Detail::Medium, Detail::Low] {
                                ui.selectable_value(&mut source.detail, detail, detail.label());
                            }
                        })
                        .response
                        .on_hover_text("Source detail in this tile. Output resolution is shared.");
                    ui.checkbox(&mut source.cursor, "Pointer");
                });
            });
        }
        if let Some(index) = remove {
            self.sources.remove(index);
        } else if let Some(index) = move_up {
            self.sources.swap(index, index - 1);
        }
        change
    }

    fn paint_preview(&self, ui: &mut egui::Ui, output: [u32; 2], tiles: &[WindowTile]) {
        let aspect = output[0] as f32 / output[1] as f32;
        let width = ui.available_width();
        let height = (width / aspect).min(158.);
        let (container, _) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
        let rect = Rect::from_center_size(container.center(), vec2(height * aspect, height));
        let painter = ui.painter().with_clip_rect(rect);
        let palette = appearance::palette(ui.ctx());
        painter.rect_filled(rect, 6, Color32::BLACK);
        let scale = rect.width() / output[0] as f32;
        for (source, tile) in self.sources.iter().zip(tiles) {
            let cell = Rect::from_min_size(
                rect.min + vec2(tile.x as f32, tile.y as f32) * scale,
                vec2(tile.width as f32, tile.height as f32) * scale,
            );
            match &source.preview {
                Some(Ok(texture)) => {
                    let source_size = texture.size_vec2();
                    let ratio = cell.size() / source_size;
                    let factor = if source.fill {
                        ratio.max_elem()
                    } else {
                        ratio.min_elem()
                    };
                    let drawn = Rect::from_center_size(cell.center(), source_size * factor);
                    painter
                        .with_clip_rect(cell.intersect(ui.clip_rect()))
                        .image(
                            texture.id(),
                            drawn,
                            Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                            Color32::WHITE,
                        );
                }
                result => {
                    let message = if let Some(Err(error)) = result {
                        ui.interact(
                            cell,
                            ui.id().with(("preview-error", source.id)),
                            egui::Sense::hover(),
                        )
                        .on_hover_text(error);
                        "Preview unavailable"
                    } else {
                        "Loading preview…"
                    };
                    painter.with_clip_rect(cell.intersect(ui.clip_rect())).text(
                        cell.center(),
                        egui::Align2::CENTER_CENTER,
                        message,
                        egui::FontId::proportional(11.),
                        Color32::WHITE,
                    );
                }
            }
            painter.rect_stroke(
                cell,
                0,
                egui::Stroke::new(1., palette.border),
                egui::StrokeKind::Inside,
            );
        }
    }
}

fn grid(width: u32, height: u32, count: usize, layout: Layout, split: u32) -> Vec<[u32; 4]> {
    if count == 2 {
        let split_at =
            |length: u32| ((length * split.clamp(30, 70) / 100) & !1).clamp(2, length - 2);
        return match layout {
            Layout::SideBySide => {
                let first = split_at(width);
                vec![[0, 0, first, height], [first, 0, width - first, height]]
            }
            Layout::Stacked => {
                let first = split_at(height);
                vec![[0, 0, width, first], [0, first, width, height - first]]
            }
        };
    }
    let columns = (count as f32).sqrt().ceil() as u32;
    let rows = (count as u32).div_ceil(columns.max(1));
    let edge = |length, index, divisions| (length * index / divisions) & !1;
    (0..count as u32)
        .map(|index| {
            let x = edge(width, index % columns, columns);
            let y = edge(height, index / columns, rows);
            let right = edge(width, index % columns + 1, columns);
            let bottom = edge(height, index / columns + 1, rows);
            [x, y, right - x, bottom - y]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_preserves_both_windows_and_places_chroma_at_the_same_offset() {
        let tile = WindowTile {
            id: 1,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            cursor: false,
            fill: false,
            detail: 0,
        };
        let mut canvas = [16; 24];
        canvas[16..].fill(128);
        tile.blit_nv12(4, 4, &[63, 63, 63, 63, 102, 240], &mut canvas);
        WindowTile {
            id: 2,
            x: 2,
            y: 2,
            ..tile
        }
        .blit_nv12(4, 4, &[32, 32, 32, 32, 240, 118], &mut canvas);
        assert_eq!(
            canvas,
            [
                63, 63, 16, 16, 63, 63, 16, 16, 16, 16, 32, 32, 16, 16, 32, 32, 102, 240, 128, 128,
                128, 128, 240, 118,
            ]
        );
    }

    #[test]
    fn layouts_support_ten_sources_without_overlapping_chroma_blocks() {
        for count in 1..=10 {
            for layout in [Layout::SideBySide, Layout::Stacked] {
                let cells = grid(854, 480, count, layout, 65);
                assert_eq!(cells.len(), count);
                for (index, &[x, y, w, h]) in cells.iter().enumerate() {
                    assert!([x, y, w, h].into_iter().all(|value| value % 2 == 0));
                    assert!(w > 0 && h > 0 && x + w <= 854 && y + h <= 480);
                    assert!(
                        cells[..index].iter().all(|&[a, b, c, d]| x + w <= a
                            || a + c <= x
                            || y + h <= b
                            || b + d <= y)
                    );
                }
            }
        }
    }
}
