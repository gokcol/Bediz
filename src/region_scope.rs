//! Scope for a rectangular pixel selection. Object selection still works on whole objects.
use super::annotation_layers::LayerTarget;
use super::*;
use crate::document::LayerRegionEdit;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RegionLayer {
    Image,
    Object(usize),
}

#[derive(Clone, Default)]
enum Scope {
    #[default]
    Visible,
    Active,
    Checked(Vec<RegionLayer>),
}

#[derive(Clone, Default)]
pub(super) struct RegionScope {
    scope: Scope,
    active: Option<usize>,
}

impl RegionLayer {
    fn label(self, document: &Document) -> String {
        match self {
            Self::Image => "Base image".into(),
            Self::Object(index) => LayerTarget::Annotation(index).label(document),
        }
    }

    fn available(self, document: &Document) -> bool {
        match self {
            Self::Image => true,
            Self::Object(index) => document
                .annotations
                .get(index)
                .is_some_and(|a| a.visible() && !a.locked() && a.supports_region_edits()),
        }
    }
}

fn layer_scope_row(
    ui: &mut egui::Ui,
    document: &Document,
    layer: Option<RegionLayer>,
    active: bool,
    selected: bool,
) -> egui::Response {
    use crate::appearance::{self, Icon};
    let label = match layer {
        Some(layer) if active => format!("Active layer · {}", layer.label(document)),
        Some(layer) => layer.label(document),
        None => "Visible image".into(),
    };
    let checkbox = layer.is_some() && !active;
    let available = layer.is_none_or(|layer| layer.available(document));
    ui.add_enabled_ui(available, |ui| {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 27.), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                if checkbox {
                    egui::WidgetType::Checkbox
                } else {
                    egui::WidgetType::RadioButton
                },
                ui.is_enabled(),
                selected,
                &label,
            )
        });
        let palette = appearance::palette(ui.ctx());
        let painter = ui.painter();
        let accent = appearance::satin_ink(ui.ctx(), appearance::TileTone::Blue);
        let ink = if available {
            palette.ink
        } else {
            palette.muted
        };
        if selected && !checkbox {
            painter.rect_filled(rect, 5., accent.gamma_multiply(0.12));
        } else if response.hovered() || response.has_focus() {
            painter.rect_filled(rect, 5., palette.cream);
        }
        if response.has_focus() {
            painter.rect_stroke(rect, 5., Stroke::new(1., accent), StrokeKind::Inside);
        }
        let badge = Rect::from_center_size(
            rect.left_center() + egui::vec2(15., 0.),
            egui::Vec2::splat(21.),
        );
        painter.rect_filled(badge, 4., palette.base);
        match layer {
            Some(RegionLayer::Object(index)) => {
                if let Some(annotation) = document.annotations.get(index) {
                    annotation_layers::paint_layer_badge(ui, badge, annotation);
                }
            }
            Some(RegionLayer::Image) => {
                appearance::icon(painter, badge.center(), Icon::View, accent, 15.);
            }
            None => inspector_widgets::paint_glyph(
                painter,
                badge.center(),
                inspector_widgets::Glyph::Stack,
                accent,
                15.,
            ),
        }
        let text_rect = Rect::from_min_max(
            rect.left_top() + egui::vec2(32., 0.),
            rect.right_bottom() - egui::vec2(29., 0.),
        );
        painter.with_clip_rect(text_rect).text(
            text_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            &label,
            egui::FontId::proportional(12.),
            ink,
        );
        let mark = rect.right_center() - egui::vec2(13., 0.);
        if checkbox {
            let square = Rect::from_center_size(mark, egui::Vec2::splat(14.));
            painter.rect_stroke(
                square,
                3.,
                Stroke::new(1., if selected { accent } else { palette.muted }),
                StrokeKind::Inside,
            );
        }
        if selected {
            appearance::icon(
                painter,
                mark,
                Icon::Check,
                accent,
                if checkbox { 12. } else { 15. },
            );
        }
        response
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(label)
    })
    .inner
    .on_disabled_hover_text("Choose a visible, unlocked image or object layer")
}

impl RegionScope {
    pub(super) fn for_region(active: Option<usize>) -> Self {
        Self {
            active,
            ..Self::default()
        }
    }

    fn active(&self) -> RegionLayer {
        self.active.map_or(RegionLayer::Image, RegionLayer::Object)
    }

    pub(super) fn layers(&self, document: &Document) -> Result<Option<Vec<RegionLayer>>, String> {
        let layers: Vec<_> = match &self.scope {
            Scope::Visible => return Ok(None),
            Scope::Active => vec![self.active()],
            Scope::Checked(layers) => layers.clone(),
        };
        if layers.is_empty() {
            return Err("Check at least one layer in the region menu.".into());
        }
        if !layers.iter().all(|layer| layer.available(document)) {
            return Err(
                "A selected layer is hidden, locked, or depends on the layers below it.".into(),
            );
        }
        Ok(Some(layers))
    }
}

pub(super) fn copy_source(mut document: Document, layers: &[RegionLayer]) -> Document {
    let base = layers.contains(&RegionLayer::Image);
    if !base {
        document.image = std::sync::Arc::new(image::RgbaImage::new(
            document.image.width(),
            document.image.height(),
        ));
    }
    document.annotations = document
        .annotations
        .into_iter()
        .enumerate()
        .filter_map(|(index, a)| {
            (if a.layer.base_only {
                base
            } else {
                layers.contains(&RegionLayer::Object(index))
            })
            .then_some(a)
        })
        .collect();
    document
}

pub(super) fn bar_id() -> egui::Id {
    egui::Id::new("region-layer-scope")
}

impl EditorState {
    pub(super) fn scoped_layers(
        &self,
        document: &Document,
    ) -> Result<Option<Vec<RegionLayer>>, String> {
        if self.area.is_none()
            || self.selected.is_some()
            || !self.area_mask.is_empty()
            || self.pixel_mask.is_some()
        {
            return Ok(None);
        }
        self.region_scope.layers(document)
    }
    pub(super) fn edit_region_layers(
        &self,
        document: &mut Document,
        layers: &[RegionLayer],
        source: Rect,
        offset: Option<Vec2>,
    ) -> Result<Vec<usize>, String> {
        let mut edited = Vec::with_capacity(layers.len());
        for &layer in layers {
            if !layer.available(document) {
                return Err("Choose a visible, unlocked layer for the region edit.".into());
            }
            match layer {
                RegionLayer::Image => {
                    let dest = source.translate(offset.unwrap_or_default());
                    let kind = if offset.is_some() {
                        AnnotationKind::MoveArea
                    } else {
                        AnnotationKind::DeleteArea
                    };
                    let mut edit = self.new_annotation(kind, from_pos(dest.min));
                    edit.end = from_pos(dest.max);
                    edit.area_source = offset.map(|_| [from_pos(source.min), from_pos(source.max)]);
                    edit.layer.base_only = true;
                    edit.layer.name = if offset.is_some() {
                        "Base image move".into()
                    } else {
                        "Base image delete".into()
                    };
                    edited.push(document.annotations.len());
                    document.annotations.push(edit);
                }
                RegionLayer::Object(index) => {
                    let annotation = &mut document.annotations[index];
                    let source = source.intersect(render::annotation_bounds(annotation));
                    if !source.is_positive() {
                        continue;
                    }
                    let origin = annotation.start;
                    annotation.layer.region_edits.push(LayerRegionEdit {
                        source: [
                            (source.left() - origin.x.round()).floor() as i32,
                            (source.top() - origin.y.round()).floor() as i32,
                            (source.right() - origin.x.round()).ceil() as i32,
                            (source.bottom() - origin.y.round()).ceil() as i32,
                        ],
                        offset: offset.map(|v| [v.x.round() as i32, v.y.round() as i32]),
                    });
                    edited.push(index);
                }
            }
        }
        crate::document::validate_document(document)?;
        Ok(edited)
    }

    pub(super) fn region_scope_bar(
        &mut self,
        ui: &egui::Ui,
        document: &Document,
        origin: Pos2,
        scale: f32,
    ) {
        let Some(region) = self.area else { return };
        if self.tool != Tool::Area
            || self.area_drag.is_some()
            || self.gesture.is_some()
            || !self.area_mask.is_empty()
            || self.pixel_mask.is_some()
            || self.selected.is_some()
            || self.has_dialog()
            || self.precision_active()
        {
            return;
        }
        let bounds = mapped_bounds(region, document);
        let selection = Rect::from_min_max(
            origin + bounds.min.to_vec2() * scale,
            origin + bounds.max.to_vec2() * scale,
        );
        let viewport = ui.clip_rect().shrink(6.);
        if !selection.intersects(viewport) || viewport.width() < 164. || viewport.height() < 44. {
            return;
        }
        let id = bar_id();
        let old = ui.ctx().memory(|m| m.area_rect(id));
        let size = old.map_or(egui::vec2(172., 36.), |r| r.size());
        let x = (selection.center().x - size.x / 2.).clamp(
            viewport.min.x,
            (viewport.max.x - size.x).max(viewport.min.x),
        );
        let below = selection.bottom() + 9.;
        let y = if below + size.y <= viewport.bottom() {
            below
        } else {
            (selection.top() - size.y - 9.).max(viewport.top())
        };
        let opacity = ui.opacity();
        egui::Area::new(id)
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(x, y))
            .movable(false)
            .enabled(ui.is_enabled())
            .show(ui.ctx(), |ui| {
                ui.set_opacity(opacity);
                object_properties::quick_frame(ui).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let label = match &self.region_scope.scope {
                            Scope::Visible => "Visible image".to_owned(),
                            Scope::Active => "Active layer".to_owned(),
                            Scope::Checked(layers) => format!("{} checked", layers.len()),
                        };
                        let ink = crate::appearance::palette(ui.ctx()).ink;
                        let button = inspector_widgets::tile(
                            ui,
                            "Region layers",
                            inspector_widgets::Glyph::Stack,
                            egui::vec2(28., 27.),
                            false,
                            false,
                        )
                        .on_hover_text("Choose the layers affected by the region");
                        egui::Popup::menu(&button)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| self.region_scope_menu(ui, document, region));
                        ui.label(egui::RichText::new(label).color(ink).strong().size(12.));
                        ui.weak(format!(
                            "{} × {}",
                            bounds.width().round() as u32,
                            bounds.height().round() as u32
                        ));
                    });
                });
            });
    }

    fn region_scope_menu(&mut self, ui: &mut egui::Ui, document: &Document, region: Rect) {
        ui.set_width(235.);
        ui.spacing_mut().item_spacing.y = 4.;
        ui.strong("Region layers");
        if layer_scope_row(
            ui,
            document,
            None,
            false,
            matches!(self.region_scope.scope, Scope::Visible),
        )
        .clicked()
        {
            self.region_scope.scope = Scope::Visible;
            self.focus_canvas = true;
            ui.close();
        }
        let active = self.region_scope.active();
        if layer_scope_row(
            ui,
            document,
            Some(active),
            true,
            matches!(self.region_scope.scope, Scope::Active),
        )
        .clicked()
        {
            self.region_scope.scope = Scope::Active;
            self.focus_canvas = true;
            ui.close();
        }
        ui.separator();
        ui.label(egui::RichText::new("Choose layers").strong().size(12.));
        let rows: Vec<_> = document
            .annotations
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, a)| {
                a.visible() && !a.layer.base_only && render::annotation_bounds(a).intersects(region)
            })
            .map(|(i, _)| RegionLayer::Object(i))
            .chain(std::iter::once(RegionLayer::Image))
            .collect();
        let mut checked = match &self.region_scope.scope {
            Scope::Checked(layers) => layers.clone(),
            Scope::Active => vec![active],
            Scope::Visible => rows
                .iter()
                .copied()
                .filter(|r| r.available(document))
                .collect(),
        };
        egui::ScrollArea::vertical()
            .max_height(190.)
            .show(ui, |ui| {
                for layer in rows {
                    let included = checked.contains(&layer);
                    if layer_scope_row(ui, document, Some(layer), false, included).clicked() {
                        if included {
                            checked.retain(|&item| item != layer);
                        } else {
                            checked.push(layer);
                        }
                        self.region_scope.scope = Scope::Checked(checked.clone());
                    }
                }
            });
        ui.separator();
        if inspector_widgets::tile(
            ui,
            "Done",
            inspector_widgets::Glyph::App(crate::appearance::Icon::Check),
            egui::vec2(52., 36.),
            false,
            true,
        )
        .clicked()
        {
            self.focus_canvas = true;
            ui.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(editor: &EditorState) -> Document {
        let rectangle = |start, end, color| {
            let mut annotation = editor.new_annotation(AnnotationKind::Rectangle, start);
            annotation.end = end;
            annotation.color = color;
            annotation.fill = true;
            annotation.stroke_width = 1.;
            annotation
        };
        Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(80, 60, image::Rgba([255; 4]))),
            annotations: vec![
                rectangle(
                    Point { x: 10., y: 10. },
                    Point { x: 32., y: 30. },
                    [220, 20, 20, 255],
                ),
                rectangle(
                    Point { x: 20., y: 15. },
                    Point { x: 29., y: 25. },
                    [20, 20, 220, 255],
                ),
            ],
            geometry: Default::default(),
        }
    }

    #[test]
    fn checked_layer_move_leaves_the_excluded_object_and_survives_reopening() {
        let mut editor = EditorState::default();
        let mut document = fixture(&editor);
        let prior = document.edit_state();
        let mut history = EditHistory::new(prior.clone());
        editor.area = Some(Rect::from_min_max(egui::pos2(8., 8.), egui::pos2(30., 33.)));
        editor.region_scope.scope = Scope::Checked(vec![RegionLayer::Object(0)]);
        editor.begin_area_drag(Point { x: 18., y: 20. }, &mut document);
        editor.update_area_drag(Point { x: 53., y: 20. }, &mut document);
        editor.finish_area_drag(&mut document, &mut history);

        let pixels = render::flatten(&document).unwrap();
        assert_eq!(pixels.get_pixel(15, 20).0, [255; 4]);
        assert_eq!(pixels.get_pixel(23, 20).0, [20, 20, 220, 255]);
        assert_eq!(pixels.get_pixel(50, 20).0, [220, 20, 20, 255]);
        assert_eq!(document.annotations[1], prior.annotations[1]);
        assert_eq!(document.annotations[0].kind, AnnotationKind::Rectangle);
        assert_eq!(
            hit_document_annotation(&document, Point { x: 50., y: 20. }, 1.),
            Some(0)
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("moved-region.myimage");
        crate::document::save_project(&document, &path).unwrap();
        let reopened = crate::document::load_project(&path).unwrap();
        assert_eq!(reopened.edit_state(), document.edit_state());
        assert_eq!(render::flatten(&reopened).unwrap(), pixels);
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), prior);
    }

    #[test]
    fn active_base_delete_leaves_objects_and_checked_copy_omits_unchecked_layers() {
        let mut editor = EditorState::default();
        let mut document = fixture(&editor);
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        editor.area = Some(Rect::from_min_max(egui::pos2(6., 6.), egui::pos2(35., 32.)));
        editor.region_scope.scope = Scope::Active;
        editor.delete_current_selection(&mut document, &mut history);
        let pixels = render::flatten(&document).unwrap();
        assert_eq!(pixels.get_pixel(7, 7).0, [0; 4]);
        assert_eq!(pixels.get_pixel(15, 20).0, [220, 20, 20, 255]);
        assert_eq!(pixels.get_pixel(23, 20).0, [20, 20, 220, 255]);
        assert_eq!(&document.annotations[..2], before.annotations.as_slice());
        let isolated = copy_source(document.clone(), &[RegionLayer::Object(0)]);
        let isolated = render::flatten(&isolated).unwrap();
        assert_eq!(isolated.get_pixel(4, 4).0, [0; 4]);
        assert_eq!(isolated.get_pixel(23, 20).0, [220, 20, 20, 255]);
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), before);
    }
}
