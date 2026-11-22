use super::*;
use std::sync::mpsc;

// Rebase a lasso when rounding to source pixels or clipping its selection to a crop.
// Clip polygon edges rather than clamping vertices, which changes concave regions.
fn remap_mask(mask: &[Point], from: Rect, to: Rect) -> Result<Vec<Point>, String> {
    if mask.is_empty() {
        return Ok(Vec::new());
    }
    let mut polygon: Vec<Pos2> = mask
        .iter()
        .map(|p| {
            let absolute = from.min + egui::vec2(p.x, p.y) * from.size();
            let relative = (absolute - to.min) / to.size();
            egui::pos2(relative.x, relative.y)
        })
        .collect();
    for edge in 0..4 {
        let distance = |p: Pos2| match edge {
            0 => p.x,
            1 => 1. - p.x,
            2 => p.y,
            _ => 1. - p.y,
        };
        let input = std::mem::take(&mut polygon);
        let Some(&last) = input.last() else {
            break;
        };
        let mut previous = last;
        for current in input {
            let a = distance(previous);
            let b = distance(current);
            if (a >= 0.) != (b >= 0.) {
                polygon.push(previous + (current - previous) * (a / (a - b)));
            }
            if b >= 0. {
                polygon.push(current);
            }
            previous = current;
        }
    }
    if polygon.len() < 3 {
        return Err("The lasso does not overlap the visible image.".into());
    }
    Ok(polygon
        .into_iter()
        .map(|p| Point {
            x: p.x.clamp(0., 1.),
            y: p.y.clamp(0., 1.),
        })
        .collect())
}

pub(super) struct FillState {
    color: [u8; 4],
    tolerance: u8,
    global: bool,
    edge: i8,
    pending: Option<Pending>,
    pub(super) error: Option<String>,
}
struct Pending {
    before: EditState,
    index: Option<usize>,
    receiver: mpsc::Receiver<Result<Annotation, String>>,
}
impl Default for FillState {
    fn default() -> Self {
        Self {
            color: [46, 137, 221, 255],
            tolerance: 15,
            global: false,
            edge: 0,
            pending: None,
            error: None,
        }
    }
}
impl FillState {
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some()
    }
}
impl EditorState {
    #[cfg(test)]
    pub(crate) fn hold_fill_for_test(
        &mut self,
        document: &Document,
    ) -> mpsc::Sender<Result<Annotation, String>> {
        let (sender, receiver) = mpsc::channel();
        self.fill_tool.pending = Some(Pending {
            before: document.edit_state(),
            index: None,
            receiver,
        });
        sender
    }

    pub(super) fn restore_fill_preferences(&mut self) {
        let preferences = &self.tool_preferences.fill;
        self.fill_tool.color = preferences.color;
        self.fill_tool.tolerance = preferences.tolerance;
        self.fill_tool.global = preferences.global;
        self.fill_tool.edge = preferences.edge;
    }

    pub(super) fn remember_sampled_fill(&mut self, color: [u8; 4]) {
        self.fill_tool.color = color;
        self.remember_fill_preferences();
    }

    fn remember_fill_preferences(&mut self) {
        let preferences = tool_appearance::FillPreferences {
            color: self.fill_tool.color,
            tolerance: self.fill_tool.tolerance,
            global: self.fill_tool.global,
            edge: self.fill_tool.edge,
        };
        if self.tool_preferences.fill != preferences {
            self.tool_preferences.fill = preferences;
            self.tool_preferences_changed = true;
        }
    }

    pub(super) fn paint_fill_cursor(&self, ctx: &egui::Context, tip: Pos2) {
        crate::pointer_style::custom_tool(ctx, tip + Vec2::new(26., -10.));
        // Screen-space artwork stays legible at every image zoom. The droplet's
        // bottom tip is the actual fill seed, rather than the bucket's center.
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("fill-pointer"),
        ));
        let ink = Color32::from_rgb(28, 31, 36);
        let p = |x, y| tip + egui::vec2(x, y);
        let bucket = vec![p(6., -16.), p(17., -27.), p(28., -16.), p(17., -5.)];
        painter.add(egui::Shape::convex_polygon(
            bucket,
            Color32::WHITE,
            Stroke::new(4., Color32::WHITE),
        ));
        let color = self.fill_tool.color;
        for (x, y) in [(14., -19.), (17., -16.)] {
            painter.rect_filled(
                Rect::from_min_size(p(x, y), Vec2::splat(3.)),
                0.,
                Color32::from_gray(160),
            );
        }
        painter.add(egui::Shape::convex_polygon(
            vec![p(6., -16.), p(17., -27.), p(28., -16.), p(17., -5.)],
            Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]),
            Stroke::new(1.5, ink),
        ));
        let handle = vec![p(11., -19.), p(11., -29.), p(18., -29.), p(22., -24.)];
        for stroke in [Stroke::new(4., Color32::WHITE), Stroke::new(1.5, ink)] {
            painter.add(egui::Shape::line(handle.clone(), stroke));
            painter.line_segment([p(6., -16.), p(28., -16.)], stroke);
        }
        let drop = vec![
            p(3., -12.),
            p(-1., -6.),
            p(-1., -3.),
            tip,
            p(4., -3.),
            p(5., -6.),
        ];
        painter.add(egui::Shape::convex_polygon(
            drop.clone(),
            Color32::WHITE,
            Stroke::new(4., Color32::WHITE),
        ));
        painter.add(egui::Shape::convex_polygon(
            drop,
            Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]),
            Stroke::new(1.5, ink),
        ));
        if color[3] == 0 {
            painter.line_segment([p(13., -12.), p(21., -20.)], Stroke::new(2., ink));
        }
    }

    pub(super) fn activate_fill(&mut self, document: &mut Document, history: &mut EditHistory) {
        self.cancel_fill_picker();
        self.finish_gesture(document, history);
        // Preserve a rectangle/lasso when switching from Select to Fill.
        if self.area.is_none()
            && self.selected.is_some()
            && self.style_kind(document) != Some(AnnotationKind::Fill)
        {
            self.area = self.selection_region(document);
            self.area_mask = self
                .selected
                .and_then(|i| document.annotations.get(i))
                .filter(|a| a.kind.needs_composite())
                .map(|a| a.area_mask.clone())
                .unwrap_or_default();
            self.pixel_mask = self
                .selected
                .and_then(|i| document.annotations.get(i))
                .and_then(|a| a.pixel_mask.clone());
        }
        if self.style_kind(document) != Some(AnnotationKind::Fill) {
            self.clear_selection();
        }
        self.tool = Tool::Fill;
        if self.selected.is_none() {
            self.restore_fill_preferences();
        }
        self.fill_tool.error = None;
        self.inspector.select(InspectorPanel::Styles);
    }
    pub(super) fn sync_fill_selection(&mut self, annotation: &Annotation) {
        if let Some(region) = &annotation.fill_region {
            self.fill_tool.color = annotation.color;
            self.fill_tool.tolerance = region.tolerance;
            self.fill_tool.global = region.global;
            self.fill_tool.edge = annotation.fill_edge;
        }
    }
    pub(super) fn poll_fill(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(pending) = &self.fill_tool.pending else {
            return;
        };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(75));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("The fill worker stopped. Please try again.".into())
            }
        };
        let pending = self.fill_tool.pending.take().unwrap();
        if document.edit_state() != pending.before {
            self.fill_tool.error =
                Some("The image changed while preparing the fill. Please try again.".into());
            return;
        }
        match result {
            Ok(annotation) => {
                let index = pending.index.unwrap_or(document.annotations.len());
                if pending.index.is_some() {
                    document.annotations[index] = annotation;
                } else {
                    document.annotations.push(annotation);
                }
                if let Err(error) = crate::document::validate_document(document) {
                    document.annotations = pending.before.annotations;
                    self.fill_tool.error = Some(error);
                    return;
                }
                history.commit(pending.before, document);
                // A bucket click finishes a fill; the color controls should then
                // configure the next one. Only an explicit region update keeps
                // the existing fill selected for further editing.
                self.select(pending.index, document);
                self.fill_tool.error = None;
                self.focus_canvas = true;
            }
            Err(error) => self.fill_tool.error = Some(error),
        }
    }
    fn start_fill_job(
        &mut self,
        ctx: &egui::Context,
        document: &Document,
        mut annotation: Annotation,
        seed: Point,
        index: Option<usize>,
    ) {
        if self.fill_tool.busy() {
            return;
        }
        let old_bounds = Rect::from_min_max(point(annotation.start), point(annotation.end));
        let bounds = Rect::from_min_max(old_bounds.min.floor(), old_bounds.max.ceil());
        annotation.area_mask = match remap_mask(&annotation.area_mask, old_bounds, bounds) {
            Ok(mask) => mask,
            Err(error) => {
                self.fill_tool.error = Some(error);
                return;
            }
        };
        annotation.start = from_pos(bounds.min);
        annotation.end = from_pos(bounds.max);
        annotation.fill_edge = self.fill_tool.edge;
        let mut source = document.clone();
        if let Some(index) = index {
            source.annotations.truncate(index);
        }
        let tolerance = self.fill_tool.tolerance;
        let global = self.fill_tool.global;
        let (sender, receiver) = mpsc::channel();
        self.fill_tool.pending = Some(Pending {
            before: document.edit_state(),
            index,
            receiver,
        });
        self.fill_tool.error = None;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                annotation.fill_region = Some(crate::canvas::fill_region(
                    &source,
                    &annotation,
                    seed,
                    tolerance,
                    global,
                )?);
                Ok(annotation)
            }))
            .unwrap_or_else(|_| Err("The fill could not be prepared for this image.".into()));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
    pub(super) fn place_fill(&mut self, ctx: &egui::Context, at: Point, document: &Document) {
        if self.selected.is_some() {
            self.restore_fill_preferences();
        }
        let crop = document.geometry.canvas_rect(original_size(document));
        let canvas = Rect::from_min_size(
            egui::pos2(crop.x as f32, crop.y as f32),
            egui::vec2(crop.width as f32, crop.height as f32),
        );
        let region = self.area.unwrap_or(canvas).intersect(canvas);
        if !region.contains(point(at)) {
            self.fill_tool.error = Some(
                "Click inside the selected area. Use Clear selection to fill elsewhere.".into(),
            );
            return;
        }
        let mut annotation =
            self.new_annotation(AnnotationKind::Fill, from_pos(region.min.floor()));
        annotation.end = from_pos(region.max.ceil());
        annotation.color = self.fill_tool.color;
        annotation.text.clear();
        annotation.layer.name = if annotation.color[3] == 0 {
            "Transparent fill"
        } else {
            "Color fill"
        }
        .into();
        annotation.area_mask = match remap_mask(
            &self.area_mask,
            self.area.unwrap_or(canvas),
            Rect::from_min_max(point(annotation.start), point(annotation.end)),
        ) {
            Ok(mask) => mask,
            Err(error) => {
                self.fill_tool.error = Some(error);
                return;
            }
        };
        if self.area.is_some() {
            annotation.pixel_mask = self.pixel_mask.clone();
        }
        self.start_fill_job(ctx, document, annotation, at, None);
    }
    pub(super) fn set_fill_color(
        &mut self,
        color: [u8; 4],
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        self.fill_tool.color = color;
        if let Some(index) = self.selected
            && self.additional.is_empty()
            && document
                .annotations
                .get(index)
                .is_some_and(|a| a.kind == AnnotationKind::Fill && !a.locked() && a.color != color)
        {
            let before = document.edit_state();
            document.annotations[index].color = color;
            self.color = color;
            self.record_properties(before, document, history);
        }
        self.remember_fill_preferences();
    }

    pub(super) fn fill_panel(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let selected = self.selected.filter(|&i| {
            document
                .annotations
                .get(i)
                .is_some_and(|a| a.kind == AnnotationKind::Fill)
                && self.additional.is_empty()
        });
        let enabled = selected.is_none_or(|i| !document.annotations[i].locked());
        ui.strong(if selected.is_some() {
            "Selected fill"
        } else {
            "Next fill"
        });
        ui.add_enabled_ui(enabled, |ui| {
            let old_options = (self.fill_tool.tolerance, self.fill_tool.global, self.fill_tool.edge);
            let mut color = self.fill_tool.color;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                for (name, swatch) in [
                    ("Red", [225, 62, 61, 255]),
                    ("Blue", [46, 137, 221, 255]),
                    ("Teal", [0, 137, 137, 255]),
                    ("Yellow", [255, 215, 45, 255]),
                    ("White", [255; 4]),
                    ("Black", [27, 35, 48, 255]),
                    ("Transparent", [0; 4]),
                ] {
                    let (rect, response) = ui.allocate_exact_size(Vec2::splat(23.), Sense::click());
                    let chosen = color == swatch;
                    let palette = crate::appearance::palette(ui.ctx());
                    if swatch[3] == 0 {
                        paint_checkerboard(ui.painter(), rect.shrink(3.));
                    } else {
                        ui.painter().circle_filled(rect.center(), 7.5, Color32::from_rgb(swatch[0], swatch[1], swatch[2]));
                    }
                    ui.painter().circle_stroke(
                        rect.center(),
                        if chosen || response.has_focus() { 10. } else { 8. },
                        Stroke::new(1., if chosen || response.has_focus() { palette.accent } else { palette.border }),
                    );
                    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, ui.is_enabled(), chosen, name));
                    if response.on_hover_text(name).clicked() {
                        color = swatch;
                    }
                }
                ui.scope(|ui| {
                    ui.spacing_mut().interact_size.x = 23.;
                    ui.color_edit_button_srgba_unmultiplied(&mut color).on_hover_text("Custom fill color");
                });
                let picking = self.fill_picker_active();
                let (rect, pick) = ui.allocate_exact_size(Vec2::splat(23.), Sense::click());
                pick.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), picking, "Fill eyedropper"));
                let ink = crate::appearance::satin_button(ui, rect, &pick, picking,
                    crate::appearance::satin_ink(ui.ctx(), crate::appearance::TileTone::Teal));
                super::precision_tools::paint_icon(ui.painter(), rect.center(), Tool::ColorInspector, ink, Stroke::new(1.2, ink), 0.65);
                if pick.on_hover_text("Pick a fill color from the image · Esc to cancel").clicked() {
                    self.toggle_fill_picker();
                }
            });
            if self.fill_picker_active() {
                ui.small("Click the image to pick a color · Esc to cancel");
            }
            let mut opacity = color[3] as f32 / 2.55;
            if object_properties::slider(ui, &mut opacity, 0.0..=100.0, "Opacity", "%") {
                color[3] = (opacity * 2.55).round() as u8;
            }
            object_properties::slider(ui, &mut self.fill_tool.tolerance, 0..=100, "Tolerance", "%");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.fill_tool.global, false, "Connected")
                    .on_hover_text("Fill matching pixels that touch the clicked region");
                ui.selectable_value(&mut self.fill_tool.global, true, "All matching")
                    .on_hover_text("Fill matching pixels throughout the image or selection");
            });
            ui.horizontal(|ui| {
                ui.label("Edge").on_hover_text("Grow or shrink the fill boundary in image pixels");
                let limit = crate::fill::MAX_EDGE;
                for (step, glyph, label) in [(-1, "−", "Shrink fill edge"), (1, "+", "Grow fill edge")] {
                    let edge = self.fill_tool.edge + step;
                    let button = ui.add_enabled((-limit..=limit).contains(&edge), egui::Button::new(glyph));
                    button.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, button.enabled(), label));
                    if button.on_hover_text(label).clicked() {
                        self.fill_tool.edge = edge;
                    }
                }
                let edge = self.fill_tool.edge;
                let label = match edge.cmp(&0) {
                    std::cmp::Ordering::Less => format!("Shrink {} px", edge.unsigned_abs()),
                    std::cmp::Ordering::Equal => "0 px".into(),
                    std::cmp::Ordering::Greater => format!("Grow {edge} px"),
                };
                ui.monospace(label);
            });
            if color != self.fill_tool.color {
                self.set_fill_color(color, document, history);
            } else if old_options != (self.fill_tool.tolerance, self.fill_tool.global, self.fill_tool.edge) {
                self.remember_fill_preferences();
            }
            if let Some(index) = selected {
                use crate::appearance::{Icon, icon_button};
                ui.horizontal(|ui| {
                    if icon_button(ui, "Update fill region", Icon::Refresh)
                        .on_hover_text("Recalculate the fill using current tolerance, matching and edge settings")
                        .clicked()
                    {
                        let annotation = &document.annotations[index];
                        let region = annotation.fill_region.as_ref().unwrap();
                        let seed = Point {
                            x: annotation.start.x + (annotation.end.x - annotation.start.x) * region.seed.x,
                            y: annotation.start.y + (annotation.end.y - annotation.start.y) * region.seed.y,
                        };
                        self.start_fill_job(ui.ctx(), document, annotation.clone(), seed, Some(index));
                    }
                    if icon_button(ui, "Open layers", Icon::Layers).clicked() {
                        self.inspector.show(InspectorPanel::Layers);
                    }
                    if icon_button(ui, "Remove fill", Icon::Trash).clicked() {
                        self.delete(document, history);
                    }
                });
            }
            if self.area.is_some() {
                ui.horizontal(|ui| {
                    ui.weak("Selection only");
                    if crate::appearance::icon_button(ui, "Clear selection", crate::appearance::Icon::Close).clicked() {
                        self.deselect_pixels();
                        self.clear_selection();
                        self.tool = Tool::Fill;
                        self.fill_tool.error = None;
                    }
                });
            }
        });
        if !enabled {
            ui.weak("Unlock this fill in Layers to edit it.");
        }
        if self.fill_tool.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Finding the fill boundary…");
            });
        }
        if let Some(error) = &self.fill_tool.error {
            ui.colored_label(Color32::DARK_RED, error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(60, 50, image::Rgba([255; 4]))),
            annotations: vec![],
            geometry: Default::default(),
        }
    }
    fn finish(e: &mut EditorState, ctx: &egui::Context, d: &mut Document, h: &mut EditHistory) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while e.fill_tool.busy() {
            e.poll_fill(ctx, d, h);
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(e.fill_tool.error.is_none(), "{:?}", e.fill_tool.error);
    }

    fn click_fill_control(
        label: &str,
        e: &mut EditorState,
        ctx: &egui::Context,
        d: &mut Document,
        h: &mut EditHistory,
    ) {
        ctx.enable_accesskit();
        let mut frame = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(500., 900.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| e.fill_panel(ui, d, h));
                },
            )
        };
        let output = frame(vec![]);
        let bounds = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .find_map(|(_, node)| {
                (node.label() == Some(label))
                    .then(|| node.bounds())
                    .flatten()
            })
            .unwrap_or_else(|| panic!("Missing fill control: {label}"));
        let at = egui::pos2(
            ((bounds.x0 + bounds.x1) * 0.5) as f32,
            ((bounds.y0 + bounds.y1) * 0.5) as f32,
        );
        for pressed in [true, false] {
            frame(vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]);
        }
    }

    #[test]
    fn next_fill_color_does_not_recolor_completed_fills_or_add_an_undo_edit() {
        let ctx = egui::Context::default();
        let mut e = EditorState::default();
        let mut d = doc();
        d.image = std::sync::Arc::new(image::RgbaImage::from_fn(60, 50, |x, _| {
            image::Rgba(if x == 30 { [0, 0, 0, 255] } else { [255; 4] })
        }));
        let mut h = EditHistory::new(d.edit_state());
        e.activate_fill(&mut d, &mut h);
        click_fill_control("Red", &mut e, &ctx, &mut d, &mut h);
        e.place_fill(&ctx, Point { x: 10., y: 10. }, &d);
        finish(&mut e, &ctx, &mut d, &mut h);
        let first = d.edit_state();
        assert_eq!(d.annotations[0].color, [225, 62, 61, 255]);

        click_fill_control("Blue", &mut e, &ctx, &mut d, &mut h);
        assert_eq!(
            d.edit_state(),
            first,
            "Choosing the next color changed the completed fill"
        );
        assert_eq!(e.fill_tool.color, [46, 137, 221, 255]);
        assert!(e.selected.is_none());
        assert_eq!(e.tool, Tool::Fill);
        h.undo(&mut d);
        assert!(
            d.annotations.is_empty(),
            "Changing a tool color added an undo edit"
        );
        h.redo(&mut d);
        assert_eq!(d.edit_state(), first);

        e.place_fill(&ctx, Point { x: 40., y: 10. }, &d);
        finish(&mut e, &ctx, &mut d, &mut h);
        let both = d.edit_state();
        let flat = crate::render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(10, 10), &image::Rgba([225, 62, 61, 255]));
        assert_eq!(flat.get_pixel(40, 10), &image::Rgba([46, 137, 221, 255]));
        click_fill_control("Transparent", &mut e, &ctx, &mut d, &mut h);
        assert_eq!(e.fill_tool.color[3], 0);
        assert_eq!(d.edit_state(), both);
        h.undo(&mut d);
        assert_eq!(d.edit_state(), first);
        h.redo(&mut d);
        assert_eq!(d.edit_state(), both);

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("independent-fills.myimage");
        crate::document::save_project(&d, &path).unwrap();
        assert_eq!(
            crate::document::load_project(&path).unwrap().edit_state(),
            both
        );
    }

    #[test]
    fn explicitly_selected_fills_still_recolor_and_update_but_new_fills_leave_edit_mode() {
        let ctx = egui::Context::default();
        let mut e = EditorState::default();
        let mut d = doc();
        let mut h = EditHistory::new(d.edit_state());
        e.activate_fill(&mut d, &mut h);
        e.place_fill(&ctx, Point { x: 10., y: 10. }, &d);
        finish(&mut e, &ctx, &mut d, &mut h);
        let first = d.edit_state();
        e.select(Some(0), &d);
        click_fill_control("Red", &mut e, &ctx, &mut d, &mut h);
        assert_eq!(d.annotations[0].color, [225, 62, 61, 255]);
        h.undo(&mut d);
        assert_eq!(d.edit_state(), first);
        h.redo(&mut d);
        click_fill_control("Update fill region", &mut e, &ctx, &mut d, &mut h);
        finish(&mut e, &ctx, &mut d, &mut h);
        assert_eq!(e.selected, Some(0));
        assert_eq!(d.annotations.len(), 1);
        let previous = d.annotations[0].clone();
        e.place_fill(&ctx, Point { x: 20., y: 20. }, &d);
        finish(&mut e, &ctx, &mut d, &mut h);
        assert!(e.selected.is_none());
        click_fill_control("Blue", &mut e, &ctx, &mut d, &mut h);
        assert_eq!(d.annotations[0], previous);
        assert_eq!(d.annotations[1].color, previous.color);
    }
    #[test]
    fn switching_to_fill_preserves_area_and_async_fill_is_one_edit() {
        let ctx = egui::Context::default();
        let mut e = EditorState::default();
        let mut d = doc();
        let mut h = EditHistory::new(d.edit_state());
        let area = Rect::from_min_max(egui::pos2(10., 10.), egui::pos2(40., 40.));
        e.area = Some(area);
        e.area_mask = vec![
            Point { x: 0., y: 0. },
            Point { x: 1., y: 0. },
            Point { x: 0., y: 1. },
        ];
        e.activate_fill(&mut d, &mut h);
        assert_eq!(e.area, Some(area));
        assert_eq!(e.area_mask.len(), 3);
        e.place_fill(&ctx, Point { x: 15., y: 15. }, &d);
        assert!(e.has_modal());
        finish(&mut e, &ctx, &mut d, &mut h);
        assert!(!e.has_modal());
        assert_eq!(d.annotations.len(), 1);
        assert_eq!(d.annotations[0].area_mask, e.area_mask);
        let flat = crate::render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(15, 15), &image::Rgba(e.fill_tool.color));
        assert_eq!(flat.get_pixel(39, 39), d.image.get_pixel(39, 39));
        h.undo(&mut d);
        assert!(d.annotations.is_empty());
        assert!(!h.can_undo());
        h.redo(&mut d);
        assert_eq!(d.annotations.len(), 1);
        assert!(
            annotation_clipboard::selection_snapshot(&d, &[0])
                .unwrap_err()
                .contains("Fills")
        );
    }
    #[test]
    fn a_lasso_crossing_the_crop_is_clipped_without_changing_its_shape() {
        let ctx = egui::Context::default();
        let mut e = EditorState::default();
        let mut d = doc();
        d.geometry.crop = Some(crate::geometry::CropRect {
            x: 10,
            y: 10,
            width: 40,
            height: 30,
        });
        e.area = Some(Rect::from_min_max(
            egui::pos2(0.2, 0.2),
            egui::pos2(40.2, 40.2),
        ));
        e.area_mask = vec![
            Point { x: 0., y: 0. },
            Point { x: 1., y: 0. },
            Point { x: 0., y: 1. },
        ];
        let mut h = EditHistory::new(d.edit_state());
        e.activate_fill(&mut d, &mut h);
        e.place_fill(&ctx, Point { x: 15., y: 15. }, &d);
        finish(&mut e, &ctx, &mut d, &mut h);
        let flat = crate::render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(5, 5), &image::Rgba(e.fill_tool.color));
        assert_eq!(flat.get_pixel(14, 14), &image::Rgba([255; 4]));
        assert_eq!(flat.get_pixel(25, 5), &image::Rgba([255; 4]));
        assert_eq!(d.annotations[0].start, Point { x: 10., y: 10. });
        assert_eq!(d.annotations[0].end, Point { x: 41., y: 40. });
        assert_eq!(d.image.get_pixel(15, 15), &image::Rgba([255; 4]));
    }
    #[test]
    fn boundary_sampling_includes_editable_lines_below_the_fill() {
        let ctx = egui::Context::default();
        let mut e = EditorState::default();
        let mut d = doc();
        let mut line = e.new_annotation(AnnotationKind::Rectangle, Point { x: 10., y: 10. });
        line.end = Point { x: 40., y: 40. };
        line.color = [0, 0, 0, 255];
        line.fill = false;
        line.stroke_width = 2.;
        d.annotations.push(line);
        let mut h = EditHistory::new(d.edit_state());
        e.activate_fill(&mut d, &mut h);
        e.place_fill(&ctx, Point { x: 20., y: 20. }, &d);
        finish(&mut e, &ctx, &mut d, &mut h);
        let flat = crate::render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(20, 20), &image::Rgba(e.fill_tool.color));
        assert_eq!(flat.get_pixel(45, 20), &image::Rgba([255; 4]));
        let a = d.annotations[1].clone();
        e.fill_tool.global = true;
        e.start_fill_job(&ctx, &d, a, Point { x: 20., y: 20. }, Some(1));
        finish(&mut e, &ctx, &mut d, &mut h);
        assert_eq!(d.annotations.len(), 2);
        assert_eq!(
            crate::render::flatten(&d).unwrap().get_pixel(45, 20),
            &image::Rgba(e.fill_tool.color)
        );
        h.undo(&mut d);
        assert!(!d.annotations[1].fill_region.as_ref().unwrap().global);
        d.annotations[0].start = Point { x: 15., y: 15. };
        e.fill_tool.global = false;
        e.start_fill_job(
            &ctx,
            &d,
            d.annotations[1].clone(),
            Point { x: 20., y: 20. },
            Some(1),
        );
        finish(&mut e, &ctx, &mut d, &mut h);
        let refreshed = crate::render::flatten(&d).unwrap();
        assert_eq!(refreshed.get_pixel(12, 20), &image::Rgba([255; 4]));
        assert_eq!(refreshed.get_pixel(20, 20), &image::Rgba(e.fill_tool.color));
    }

    #[test]
    fn chosen_fill_color_and_options_survive_selection_reset_and_settings_round_trip() {
        let mut editor = EditorState::default();
        editor.fill_tool.color = [80, 140, 220, 120];
        editor.fill_tool.tolerance = 42;
        editor.fill_tool.global = true;
        editor.fill_tool.edge = -2;
        editor.remember_fill_preferences();
        let preferences = editor.take_tool_preferences_change().unwrap();
        let json = serde_json::to_string(&preferences).unwrap();
        let mut old = editor.new_annotation(AnnotationKind::Fill, Point { x: 0., y: 0. });
        old.color = [255, 0, 0, 255];
        old.fill_region = Some(crate::fill::FillRegion {
            seed: Point { x: 0., y: 0. },
            tolerance: 15,
            global: false,
            width: 1,
            height: 1,
            runs: std::sync::Arc::new(vec![[0, 0, 1]]),
        });
        editor.sync_fill_selection(&old);
        assert_eq!(editor.fill_tool.color, old.color);
        assert_eq!(editor.fill_tool.edge, 0);
        assert!(editor.take_tool_preferences_change().is_none());
        editor.reset();
        assert_eq!(editor.fill_tool.color, [80, 140, 220, 120]);
        assert_eq!(editor.fill_tool.tolerance, 42);
        assert!(editor.fill_tool.global);
        assert_eq!(editor.fill_tool.edge, -2);
        let mut reopened = EditorState::default();
        reopened.set_tool_preferences(serde_json::from_str(&json).unwrap());
        assert_eq!(reopened.fill_tool.color, [80, 140, 220, 120]);
        assert_eq!(reopened.fill_tool.tolerance, 42);
        assert!(reopened.fill_tool.global);
        assert_eq!(reopened.fill_tool.edge, -2);
    }
}
