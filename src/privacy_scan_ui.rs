//! Review suggestions before adding editable privacy layers to the image.
use super::*;
use crate::privacy_scan::{Finding, Kind};
use egui::{Align2, FontId};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const MAX_FINDINGS: usize = 256;
const REVIEW_LIMIT: Duration = Duration::from_secs(40);
static SCANNING: AtomicBool = AtomicBool::new(false);

pub(super) struct Review {
    selection: Option<Rect>,
    use_selection: bool,
    faces: bool,
    text: bool,
    solid: bool,
    snapshot: Option<Arc<Document>>,
    job: Option<Job>,
    ready: Option<ReviewImage>,
    error: Option<String>,
}

struct Job {
    receiver: mpsc::Receiver<Result<Scanned, String>>,
    canceled: Arc<AtomicBool>,
    started: Instant,
}

struct ScanPermit;

impl Drop for ScanPermit {
    fn drop(&mut self) {
        SCANNING.store(false, Ordering::Release);
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.canceled.store(true, Ordering::Release);
    }
}

struct Scanned {
    preview: egui::ColorImage,
    items: Vec<ReviewItem>,
    errors: Vec<String>,
    limited: bool,
}

struct ReviewImage {
    texture: egui::TextureHandle,
    items: Vec<ReviewItem>,
    errors: Vec<String>,
    limited: bool,
}

struct ReviewItem {
    kind: Kind,
    preview: Rect,
    source: Rect,
    checked: bool,
}

#[derive(Clone, Copy)]
enum Action {
    Scan,
    Apply,
    Close,
}

impl EditorState {
    pub(super) fn privacy_scan_button(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) -> bool {
        let clicked = crate::appearance::action_button(
            ui,
            "Find private details…",
            DialogIcon::Shield,
            !self.locked_reference && self.privacy_scan.is_none(),
            false,
        )
        .on_hover_text("Find faces, contact details, card numbers, and labeled private values")
        .clicked();
        if clicked {
            self.finish_gesture(document, history);
            let selection = self.pixel_area(document).map(|(rect, _)| rect);
            self.privacy_scan = Some(Review {
                selection,
                use_selection: selection.is_some(),
                faces: true,
                text: true,
                solid: false,
                snapshot: None,
                job: None,
                ready: None,
                error: None,
            });
        }
        clicked
    }

    pub(super) fn privacy_scan_dialog(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(mut review) = self.privacy_scan.take() else {
            return;
        };
        review.poll(ctx, document);
        let dialog = Dialog::new(
            "Find private details",
            "Scan and review on this device",
            DialogIcon::Shield,
            680.,
        )
        .modal(ctx, egui::Id::new("privacy-scan"), |ui| review.contents(ui));
        if dialog.should_close() || matches!(dialog.inner, Some(Action::Close)) {
            return;
        }
        match dialog.inner {
            Some(Action::Scan) => review.scan(ctx, document),
            Some(Action::Apply) if review.matches_document(document) => {
                if let Some(result) = &review.ready {
                    match self.apply_privacy_findings(
                        document,
                        history,
                        &result.items,
                        review.solid,
                    ) {
                        Ok(()) => return,
                        Err(error) => review.error = Some(error),
                    }
                }
            }
            _ => {}
        }
        self.privacy_scan = Some(review);
    }

    fn apply_privacy_findings(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
        items: &[ReviewItem],
        solid: bool,
    ) -> Result<(), String> {
        let count = items.iter().filter(|item| item.checked).count();
        let first = document.annotations.len();
        if count == 0 {
            return Ok(());
        }
        if first + count > crate::document::MAX_ANNOTATIONS {
            return Err("This project has too many annotations for the selected matches.".into());
        }
        let before = document.edit_state();
        let kind = if solid {
            AnnotationKind::Rectangle
        } else {
            AnnotationKind::Blur
        };
        document
            .annotations
            .extend(items.iter().filter(|item| item.checked).map(|item| {
                let mut annotation = self.new_annotation(kind, from_pos(item.source.min));
                annotation.end = from_pos(item.source.max);
                annotation.layer.name = format!(
                    "{} {}",
                    item.kind.label(),
                    if solid { "cover" } else { "blur" }
                );
                annotation.text.clear();
                annotation.color = [0, 0, 0, 255];
                annotation.fill = solid;
                annotation.stroke_width = 0.1;
                annotation.effect_size = (item.source.size().min_elem() / 3.)
                    .round()
                    .clamp(16., crate::document::MAX_EFFECT_SIZE as f32)
                    as u32;
                annotation
            }));
        if let Err(error) = crate::document::validate_document(document) {
            document.annotations.truncate(first);
            return Err(error);
        }
        history.commit(before, document);
        self.activate_tool(Tool::Select);
        self.set_selection((first..document.annotations.len()).collect(), document);
        self.image_error = None;
        Ok(())
    }
}

impl Review {
    fn matches_document(&self, document: &Document) -> bool {
        self.snapshot.as_ref().is_some_and(|snapshot| {
            Arc::ptr_eq(&snapshot.image, &document.image)
                && snapshot.geometry == document.geometry
                && snapshot.annotations == document.annotations
        })
    }

    fn scan(&mut self, ctx: &egui::Context, document: &Document) {
        if SCANNING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.error = Some("The previous scan is still stopping. Try again shortly.".into());
            return;
        }
        let permit = ScanPermit;
        let snapshot = Arc::new(document.clone());
        let selection = self.selection.filter(|_| self.use_selection);
        let faces = self.faces;
        let text = self.text;
        let canceled = Arc::new(AtomicBool::new(false));
        let worker_cancel = canceled.clone();
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        self.snapshot = Some(snapshot.clone());
        self.ready = None;
        self.error = None;
        self.job = Some(Job {
            receiver,
            canceled,
            started: Instant::now(),
        });
        if let Err(error) = std::thread::Builder::new()
            .name("bediz-privacy-scan".into())
            .spawn(move || {
                let result = scan_image(&snapshot, selection, faces, text, worker_cancel.clone());
                drop(permit);
                if !worker_cancel.load(Ordering::Acquire) {
                    let _ = sender.send(result);
                    crate::runtime::wake_root(&ctx);
                }
            })
        {
            self.job = None;
            self.error = Some(format!("Could not start the scan: {error}"));
        }
    }

    fn poll(&mut self, ctx: &egui::Context, document: &Document) {
        if self.snapshot.is_some() && !self.matches_document(document) {
            self.job = None;
            self.ready = None;
            self.snapshot = None;
            self.selection = None;
            self.use_selection = false;
            self.error =
                Some("The image changed. Run a fresh scan before applying any changes.".into());
        }
        let Some(job) = &self.job else {
            return;
        };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) if job.started.elapsed() < REVIEW_LIMIT => {
                ctx.request_repaint_after(Duration::from_millis(250));
                return;
            }
            Err(mpsc::TryRecvError::Empty) => {
                Err("Scanning took too long. Try a smaller selection.".into())
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("Scanning stopped. Try again.".into()),
        };
        self.job = None;
        match result {
            Ok(scanned) => {
                self.ready = Some(ReviewImage {
                    texture: ctx.load_texture(
                        "privacy-scan-preview",
                        scanned.preview,
                        egui::TextureOptions::LINEAR,
                    ),
                    items: scanned.items,
                    errors: scanned.errors,
                    limited: scanned.limited,
                })
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let running = self.job.is_some();
        let mut scope_changed = false;
        ui.add_enabled_ui(!running, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Scan");
                scope_changed |= ui
                    .selectable_value(&mut self.use_selection, false, "Whole image")
                    .changed();
                scope_changed |= ui
                    .add_enabled_ui(self.selection.is_some(), |ui| {
                        ui.selectable_value(&mut self.use_selection, true, "Selected area")
                            .on_hover_text("Scan the rectangle around your pixel selection")
                    })
                    .inner
                    .changed();
            });
            ui.horizontal_wrapped(|ui| {
                scope_changed |=
                    crate::appearance::checkbox(ui, &mut self.faces, "Faces").changed();
                scope_changed |=
                    crate::appearance::checkbox(ui, &mut self.text, "Sensitive text").changed();
            });
        });
        if scope_changed {
            self.ready = None;
            self.error = None;
        }
        if self.ready.is_none() && !running {
            ui.weak("Looks for email addresses, phone numbers, payment cards, and labeled private fields.");
        }
        if running {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Scanning visible pixels…");
            });
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(ready) = &mut self.ready {
            ready.show(ui);
            if !ready.items.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.strong("Apply as");
                    ui.selectable_value(&mut self.solid, false, "Blur");
                    ui.selectable_value(&mut self.solid, true, "Solid cover");
                });
                ui.weak("Review for missed details. For text that needs to stay hidden, use Solid cover and share a flattened image. The editable project keeps the original pixels.");
            }
        }
        let count = self.ready.as_ref().map_or(0, |ready| {
            ready.items.iter().filter(|item| item.checked).count()
        });
        dialogs::actions(ui, |ui| {
            if dialogs::secondary(ui, "Close", DialogIcon::Close).clicked() {
                Some(Action::Close)
            } else if dialogs::button(
                ui,
                if self.ready.is_some() {
                    "Scan again"
                } else {
                    "Scan"
                },
                DialogIcon::Scan,
                if count > 0 {
                    dialogs::Action::Secondary
                } else {
                    dialogs::Action::Primary
                },
                !running && (self.faces || self.text),
            )
            .clicked()
            {
                Some(Action::Scan)
            } else if count > 0
                && dialogs::primary(ui, &format!("Apply to {count}"), DialogIcon::Check).clicked()
            {
                Some(Action::Apply)
            } else {
                None
            }
        })
    }
}

impl ReviewImage {
    fn show(&mut self, ui: &mut egui::Ui) {
        for error in &self.errors {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if self.items.is_empty() {
            ui.strong(if self.errors.is_empty() {
                "No matches found"
            } else {
                "No matches to review"
            });
            return;
        }
        ui.strong(format!("{} suggestions", self.items.len()));
        if self.limited {
            ui.weak(
                "Only the first 256 suggestions are shown. Scan smaller areas to review the rest.",
            );
        }
        let size = self.texture.size_vec2();
        let scale = (ui.available_width() / size.x).min(236. / size.y);
        let image = ui.add(egui::Image::new(&self.texture).fit_to_exact_size(size * scale));
        for (index, item) in self.items.iter_mut().enumerate() {
            let map = |p: Pos2| image.rect.min + p.to_vec2() * image.rect.size();
            let rect = Rect::from_min_max(map(item.preview.min), map(item.preview.max));
            let response = ui.interact(rect, image.id.with(index), Sense::click());
            if response.clicked() {
                item.checked = !item.checked;
            }
            let color = if item.checked {
                Color32::from_rgb(34, 220, 190)
            } else {
                Color32::from_gray(174)
            };
            ui.painter().rect_filled(
                rect,
                2.,
                color.gamma_multiply(if response.hovered() { 0.42 } else { 0.22 }),
            );
            ui.painter().rect_stroke(
                rect,
                2.,
                Stroke::new(3., Color32::from_rgb(25, 39, 48)),
                egui::StrokeKind::Outside,
            );
            ui.painter()
                .rect_stroke(rect, 2., Stroke::new(1.5, color), egui::StrokeKind::Inside);
            let label = (index + 1).to_string();
            let badge = Rect::from_min_size(
                egui::pos2(rect.left(), (rect.top() - 14.).max(image.rect.top())),
                egui::vec2(8. + 7. * label.len() as f32, 14.),
            );
            ui.painter()
                .rect_filled(badge, 3., Color32::from_rgb(25, 39, 48));
            ui.painter().text(
                badge.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::monospace(11.),
                color,
            );
            response.on_hover_text(item.kind.label());
        }
        ui.add_space(4.);
        egui::ScrollArea::vertical()
            .id_salt("review-private-detail-list")
            .max_height(118.)
            .show(ui, |ui| {
                for (index, item) in self.items.iter_mut().enumerate() {
                    crate::appearance::checkbox(
                        ui,
                        &mut item.checked,
                        format!("{}   {}", index + 1, item.kind.label()),
                    );
                }
            });
    }
}

fn scan_image(
    document: &Document,
    selection: Option<Rect>,
    faces: bool,
    text: bool,
    canceled: Arc<AtomicBool>,
) -> Result<Scanned, String> {
    if canceled.load(Ordering::Acquire) {
        return Err("Scan canceled.".into());
    }
    let original = original_size(document);
    let crop = selection
        .map(|region| crate::image_clipboard::copy_bounds(document, region))
        .transpose()?;
    let pixels = crate::image_clipboard::copy_pixels_masked(
        document,
        selection.map(|region| (region, Vec::new())),
        None,
    )?;
    if canceled.load(Ordering::Acquire) {
        return Err("Scan canceled.".into());
    }
    let size = Vec2::new(pixels.width() as f32, pixels.height() as f32);
    let scale = (752. / size.max_elem()).min(1.);
    let preview = image::imageops::thumbnail(
        &pixels,
        (size.x * scale).round().max(1.) as u32,
        (size.y * scale).round().max(1.) as u32,
    );
    let preview = egui::ColorImage::from_rgba_unmultiplied(
        [preview.width() as usize, preview.height() as usize],
        preview.as_raw(),
    );
    let findings = crate::ocr::scan_privacy(pixels, faces, text, canceled)?;
    let origin = crop.map_or(Pos2::ZERO, |crop| egui::pos2(crop.x as f32, crop.y as f32));
    let target = document.geometry.view_rect(original);
    let target = Rect::from_min_size(
        egui::pos2(target.x as f32, target.y as f32),
        egui::vec2(target.width as f32, target.height as f32),
    );
    let target = selection.map_or(target, |selection| target.intersect(selection));
    let mut items: Vec<_> = findings
        .matches
        .into_iter()
        .filter_map(|finding| place_finding(finding, document, origin, size, target))
        .collect();
    items.sort_by(|a, b| {
        a.preview
            .top()
            .total_cmp(&b.preview.top())
            .then_with(|| a.preview.left().total_cmp(&b.preview.left()))
    });
    let limited = items.len() > MAX_FINDINGS;
    items.truncate(MAX_FINDINGS);
    Ok(Scanned {
        preview,
        items,
        errors: findings.errors,
        limited,
    })
}

fn place_finding(
    finding: Finding,
    document: &Document,
    origin: Pos2,
    size: Vec2,
    scope: Rect,
) -> Option<ReviewItem> {
    if !finding.bounds.is_finite() || !finding.bounds.is_positive() {
        return None;
    }
    let map = |p: Pos2| origin + p.to_vec2() * size;
    let rect = Rect::from_min_max(map(finding.bounds.min), map(finding.bounds.max));
    let padding = if finding.kind == Kind::Face {
        egui::vec2(rect.width() * 0.13, rect.height() * 0.17).max(Vec2::splat(3.))
    } else {
        Vec2::splat((rect.height() * 0.18).clamp(2., 24.))
    };
    let visible = rect
        .expand2(padding)
        .intersect(Rect::from_min_size(origin, size));
    if !visible.is_positive() {
        return None;
    }
    let original = original_size(document);
    let source = [
        visible.left_top(),
        visible.right_top(),
        visible.right_bottom(),
        visible.left_bottom(),
    ]
    .into_iter()
    .map(|p| point(document.geometry.inverse_point(from_pos(p), original)))
    .fold(Rect::NOTHING, |rect, p| {
        rect.union(Rect::from_min_max(p, p))
    });
    let source = Rect::from_min_max(source.min.floor(), source.max.ceil()).intersect(scope);
    if !source.is_finite() || !source.is_positive() {
        return None;
    }
    let normalized = |p: Pos2| ((p - origin) / size).to_pos2();
    Some(ReviewItem {
        kind: finding.kind,
        preview: Rect::from_min_max(normalized(visible.min), normalized(visible.max)),
        source,
        checked: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_suggestions_use_transformed_selection_coordinates_and_apply_in_one_undo_step() {
        let mut document = Document {
            image: Arc::new(image::RgbaImage::from_pixel(
                160,
                100,
                image::Rgba([255; 4]),
            )),
            annotations: Vec::new(),
            geometry: ImageGeometry {
                crop: Some(CropRect {
                    x: 11,
                    y: 7,
                    width: 130,
                    height: 80,
                }),
                quarter_turns: 1,
                flip_horizontal: true,
                resize: Some([160, 260]),
                ..Default::default()
            },
        };
        let selection = Rect::from_min_max(egui::pos2(27., 19.), egui::pos2(105., 61.));
        let crop = crate::image_clipboard::copy_bounds(&document, selection).unwrap();
        let origin = egui::pos2(crop.x as f32, crop.y as f32);
        let size = egui::vec2(crop.width as f32, crop.height as f32);
        let finding = Finding {
            kind: Kind::Email,
            bounds: Rect::from_min_max(egui::pos2(0.12, 0.1), egui::pos2(0.33, 0.38)),
        };
        let item = place_finding(finding, &document, origin, size, selection).unwrap();
        assert!(selection.contains_rect(item.source));
        let checked_center = origin + finding.bounds.center().to_vec2() * size;
        let face = Finding {
            kind: Kind::Face,
            bounds: Rect::from_min_max(egui::pos2(0.12, 0.65), egui::pos2(0.31, 0.86)),
        };
        let checked_face = place_finding(face, &document, origin, size, selection).unwrap();
        let face_center = origin + face.bounds.center().to_vec2() * size;
        let other = Finding {
            kind: Kind::Face,
            bounds: Rect::from_min_max(egui::pos2(0.6, 0.55), egui::pos2(0.9, 0.8)),
        };
        let mut unchecked = place_finding(other, &document, origin, size, selection).unwrap();
        unchecked.checked = false;
        let unchecked_center = origin + other.bounds.center().to_vec2() * size;

        let mut editor = EditorState::default();
        let mut history = EditHistory::new(document.edit_state());
        editor
            .apply_privacy_findings(
                &mut document,
                &mut history,
                &[item, checked_face, unchecked],
                true,
            )
            .unwrap();
        assert_eq!(document.annotations.len(), 2);
        let rendered = render::flatten(&document).unwrap();
        assert_eq!(
            rendered
                .get_pixel(face_center.x as u32, face_center.y as u32)
                .0,
            [0, 0, 0, 255]
        );
        assert_eq!(
            rendered
                .get_pixel(checked_center.x as u32, checked_center.y as u32)
                .0,
            [0, 0, 0, 255]
        );
        assert_eq!(
            rendered
                .get_pixel(unchecked_center.x as u32, unchecked_center.y as u32)
                .0,
            [255; 4]
        );
        history.undo(&mut document);
        assert!(document.annotations.is_empty());
        assert!(!history.can_undo());
    }
}
