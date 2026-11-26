//! In-place canvas text editing with a compact formatting bar and atomic undo.
use super::*;
use crate::appearance::{self, TileTone};
use crate::text_style::Alignment;

pub(super) struct TextDraft {
    pub(super) index: usize,
    before: EditState,
    focus: bool,
    pub(super) canvas: Option<(Pos2, f32, Rect)>,
}

impl TextDraft {
    pub(super) fn focus_editor(&mut self, response: &egui::Response, text: &str) {
        if !std::mem::take(&mut self.focus) {
            return;
        }
        response.request_focus();
        if let Some(mut state) = egui::TextEdit::load_state(&response.ctx, response.id) {
            use egui::text::{CCursor, CCursorRange};

            let end = CCursor::new(text.chars().count());
            let range = if self.index < self.before.annotations.len() {
                CCursorRange::one(end)
            } else {
                // New objects can show placeholder text. Typing should replace it.
                CCursorRange::two(CCursor::new(0), end)
            };
            state.cursor.set_char_range(Some(range));
            state.store(&response.ctx, response.id);
        }
    }
}

impl EditorState {
    pub(super) fn open_text_editor(
        &mut self,
        index: usize,
        before: EditState,
        document: &Document,
    ) {
        if document.annotations.get(index).is_none_or(|a| {
            a.locked() || !matches!(a.kind, AnnotationKind::Text | AnnotationKind::Callout)
        }) {
            return;
        }
        self.select(Some(index), document);
        self.text_editor_generation = self.text_editor_generation.wrapping_add(1);
        self.text_dialog = Some(TextDraft {
            index,
            before,
            focus: true,
            canvas: None,
        });
        self.focus_text = false;
    }
    pub(super) fn finish_text_editor(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
        apply: bool,
    ) {
        let Some(draft) = self.text_dialog.take() else {
            return;
        };
        let mut removed = false;
        let mut remember_style = false;
        if apply {
            // Empty text is not an invisible object; cancel a new blank annotation.
            removed = document
                .annotations
                .get(draft.index)
                .is_some_and(|a| a.kind == AnnotationKind::Text && a.text.trim().is_empty());
            if removed {
                document.annotations.remove(draft.index);
            }
            let indices = if removed { vec![] } else { vec![draft.index] };
            if let Err(error) = self.extend_for_annotations(document, &indices) {
                document.annotations = draft.before.annotations;
                document.geometry = draft.before.geometry;
                self.image_error = Some(error);
            } else {
                // Opening old text or changing only its words must not replace
                // the drawing defaults. Persist formatting only after a valid
                // commit; canceled drafts never affect the next annotation.
                remember_style = !removed
                    && document
                        .annotations
                        .get(draft.index)
                        .is_some_and(|annotation| {
                            draft
                                .before
                                .annotations
                                .get(draft.index)
                                .is_none_or(|before| {
                                    annotation.color != before.color
                                        || annotation.text_color != before.text_color
                                        || annotation.font_size != before.font_size
                                        || annotation.text_style != before.text_style
                                        || annotation.callout_style != before.callout_style
                                })
                        });
                history.commit(draft.before, document);
            }
        } else {
            document.annotations = draft.before.annotations;
            document.geometry = draft.before.geometry;
        }
        if !self.draws_objects() {
            self.tool = Tool::Select;
        }
        self.select(
            (!removed && draft.index < document.annotations.len()).then_some(draft.index),
            document,
        );
        if remember_style && let Some(annotation) = document.annotations.get(draft.index) {
            self.remember_appearance(annotation.kind);
        }
        self.focus_canvas = true;
    }
    pub(super) fn text_editor_window(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(mut draft) = self.text_dialog.take() else {
            return;
        };
        if document
            .annotations
            .get(draft.index)
            .is_some_and(|a| a.text_style.path.is_some())
        {
            self.path_text_dialog(ctx, draft, document, history);
            return;
        }
        let Some((origin, scale, viewport)) = draft.canvas else {
            self.text_dialog = Some(draft);
            return;
        };
        let Some(annotation) = document.annotations.get(draft.index) else {
            return;
        };
        let bounds = mapped_bounds(
            if annotation.kind == AnnotationKind::Callout {
                Rect::from_two_pos(
                    egui::pos2(annotation.start.x, annotation.start.y),
                    egui::pos2(annotation.end.x, annotation.end.y),
                )
            } else {
                render::annotation_bounds(annotation)
            },
            document,
        );
        let screen = Rect::from_min_max(
            origin + bounds.min.to_vec2() * scale,
            origin + bounds.max.to_vec2() * scale,
        );
        // Map font size with the document's resize, independently of viewport zoom.
        let x = output_point(
            Point {
                x: annotation.start.x + 1.,
                y: annotation.start.y,
            },
            document,
        );
        let at = output_point(annotation.start, document);
        let density = (x - at).length().max(0.01) * scale;
        let metrics = render::editable_text_layout(annotation).ok();
        let text_position = metrics
            .as_ref()
            .map(|m| {
                origin
                    + output_point(
                        Point {
                            x: m.origin.x,
                            y: m.origin.y,
                        },
                        document,
                    )
                    .to_vec2()
                        * scale
            })
            .unwrap_or(screen.min);
        let annotation = &mut document.annotations[draft.index];
        let first_frame = draft.focus;
        let popup_was_open = egui::Popup::is_any_open(ctx);
        let mut apply = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
        let mut cancel =
            !popup_was_open && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let font_size = (metrics
            .as_ref()
            .map_or(annotation.font_size, |m| m.font_size)
            * density)
            .max(1.);
        let ink = if annotation.kind == AnnotationKind::Callout {
            annotation.text_color
        } else {
            annotation.color
        };
        let color = Color32::from_rgba_unmultiplied(ink[0], ink[1], ink[2], ink[3]);
        let style = annotation.text_style.clone();
        let (format, new_fonts) = ctx.fonts(|fonts| {
            let definitions = fonts.definitions();
            (
                style.format(font_size, color, definitions),
                style.with_system_font(definitions),
            )
        });
        if let Some(definitions) = new_fonts {
            ctx.set_fonts(definitions);
            ctx.request_repaint();
        }
        let callout = annotation.kind == AnnotationKind::Callout;
        let position = text_position;
        let width = metrics
            .as_ref()
            .map_or(screen.width(), |m| m.size.x * density)
            .max(font_size * 0.6)
            .min(viewport.width().max(1.));
        let wrap_width = metrics
            .as_ref()
            .map_or(f32::INFINITY, |m| m.wrap_width * density);
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _available: f32| {
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = wrap_width;
            job.halign = match style.alignment {
                Alignment::Left => egui::Align::Min,
                Alignment::Center => egui::Align::Center,
                Alignment::Right => egui::Align::Max,
            };
            job.append(text.as_str(), 0., format.clone());
            normalize_text_galley(ui.fonts_mut(|fonts| fonts.layout_job(job)))
        };
        let text = egui::Area::new(egui::Id::new("inline-text-editor"))
            .order(egui::Order::Foreground)
            .fixed_pos(position)
            .constrain(false)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_clip_rect(viewport);
                ui.visuals_mut().selection.bg_fill =
                    Color32::from_rgba_unmultiplied(70, 140, 220, 45);
                ui.visuals_mut().selection.stroke.color = color;
                let response = ui.add(
                    egui::TextEdit::multiline(&mut annotation.text)
                        .id(egui::Id::new((
                            "focused-annotation-text",
                            self.text_editor_generation,
                        )))
                        .font(format.font_id.clone())
                        .text_color(color)
                        .frame(false)
                        .margin(0.)
                        .desired_rows(1)
                        .desired_width(width)
                        .char_limit(16_384)
                        .layouter(&mut layouter),
                );
                ui.painter().rect_stroke(
                    response.rect.expand(3.),
                    0.,
                    Stroke::new(1., Color32::from_gray(55)),
                    StrokeKind::Outside,
                );
                draft.focus_editor(&response, &annotation.text);
                response
            });
        // Use the actual text widget bounds: font metrics and UI scaling can
        // make them taller than the annotation's raster bounds.
        let bar_rect = inline_toolbar_rect(
            if callout {
                screen
            } else {
                text.inner.rect.expand(3.)
            },
            viewport,
            if callout { 5 } else { 4 },
        );
        // A formatting change can resize the text. Keep the controls under the
        // pointer while their popup is open.
        let bar_id = egui::Id::new("inline-text-properties");
        let bar_position = ctx
            .memory(|memory| memory.area_rect(bar_id))
            .filter(|_| popup_was_open && !first_frame)
            .map_or(bar_rect.min, |rect| rect.min);
        let bar = egui::Area::new(bar_id)
            .order(egui::Order::Foreground)
            .fixed_pos(bar_position)
            .constrain(false)
            .movable(false)
            .show(ctx, |ui| {
                object_properties::quick_frame(ui)
                    .fill(appearance::surface(ui.ctx()))
                    .inner_margin(2)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        ui.spacing_mut().button_padding = Vec2::splat(3.);
                        ui.spacing_mut().interact_size = Vec2::new(30., 28.);
                        ui.horizontal(|ui| {
                            let font = inline_control(ui, "Text formatting", |p, r, ink| {
                                p.text(
                                    r.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "Aa",
                                    egui::FontId::proportional(16.),
                                    ink,
                                );
                            });
                            inline_popup(
                                ui,
                                &font,
                                FONT_POPUP_WIDTH,
                                font_popup_alignment(&font, viewport),
                                |ui| {
                                    if font_controls(ui, annotation) {
                                        text.inner.request_focus();
                                    }
                                },
                            );
                            inline_color_button(ui, annotation);
                            if callout {
                                let shape = inline_control(ui, "Callout shape", |p, r, ink| {
                                    crate::callout_style::paint_icon(
                                        p,
                                        r.shrink(3.),
                                        annotation.callout_style,
                                        ink,
                                    );
                                });
                                egui::Popup::menu(&shape).show(|ui| {
                                    crate::callout_style::picker(ui, &mut annotation.callout_style);
                                });
                            }
                            let align = annotation.text_style.alignment;
                            let alignment = inline_control(ui, "Text alignment", |p, r, ink| {
                                paint_alignment(p, r, align, ink)
                            });
                            egui::Popup::menu(&alignment).show(|ui| {
                                ui.horizontal(|ui| {
                                    for (value, label) in [
                                        (Alignment::Left, "Align left"),
                                        (Alignment::Center, "Align center"),
                                        (Alignment::Right, "Align right"),
                                    ] {
                                        let button = inline_control(ui, label, |p, r, ink| {
                                            if annotation.text_style.alignment == value {
                                                p.rect_filled(
                                                    r.shrink(1.),
                                                    2,
                                                    crate::appearance::palette(p.ctx())
                                                        .accent
                                                        .gamma_multiply(0.15),
                                                );
                                            }
                                            paint_alignment(p, r, value, ink);
                                        });
                                        if button.clicked() {
                                            annotation.text_style.alignment = value;
                                            ui.close();
                                        }
                                    }
                                });
                            });
                            let more =
                                inline_control(ui, "Finish or cancel text editing", |p, r, ink| {
                                    for dx in [-5., 0., 5.] {
                                        p.circle_filled(r.center() + Vec2::new(dx, 0.), 1.35, ink);
                                    }
                                });
                            egui::Popup::menu(&more).show(|ui| {
                                if canvas_menu::item(
                                    ui,
                                    "Finish editing",
                                    canvas_menu::MenuIcon::App(DialogIcon::Check),
                                    true,
                                )
                                .on_hover_text("Command/Ctrl+Enter")
                                .clicked()
                                {
                                    apply = true;
                                    ui.close();
                                }
                                if canvas_menu::item(
                                    ui,
                                    "Cancel editing",
                                    canvas_menu::MenuIcon::App(DialogIcon::Close),
                                    true,
                                )
                                .on_hover_text("Esc")
                                .clicked()
                                {
                                    cancel = true;
                                    ui.close();
                                }
                            });
                        });
                    });
            });
        // Formatting popovers own their clicks. A canvas click outside the text
        // commits without starting an accidental selection/drawing gesture.
        let outside = !first_frame
            && !popup_was_open
            && !egui::Popup::is_any_open(ctx)
            && ctx.input(|i| {
                i.pointer.primary_pressed()
                    && i.pointer.interact_pos().is_some_and(|p| {
                        viewport.contains(p)
                            && !text.response.rect.expand(5.).contains(p)
                            && !bar.response.rect.contains(p)
                    })
            });
        self.text_dialog = Some(draft);
        if apply || cancel || outside {
            self.finish_text_editor(document, history, !cancel);
            if outside {
                self.clear_selection();
            }
            ctx.request_repaint();
        }
    }
}
/// TextEdit expects top-left-local glyph coordinates even for centered/right
/// paragraphs. Keep row alignment, caret hit-testing and the border in agreement.
fn normalize_text_galley(mut galley: std::sync::Arc<egui::Galley>) -> std::sync::Arc<egui::Galley> {
    let shift = -galley.rect.min.to_vec2();
    if shift != Vec2::ZERO {
        let g = std::sync::Arc::make_mut(&mut galley);
        for row in &mut g.rows {
            row.pos += shift;
        }
        g.rect = g.rect.translate(shift);
        g.mesh_bounds = g.mesh_bounds.translate(shift);
    }
    galley
}

/// Fixed-size cells keep symbols on one baseline regardless of global button padding.
fn inline_control(
    ui: &mut egui::Ui,
    label: &str,
    paint: impl FnOnce(&egui::Painter, Rect, Color32),
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(30., 28.), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let palette = appearance::palette(ui.ctx());
    let ink = appearance::satin_button(
        ui,
        rect.shrink(1.),
        &response,
        open,
        if open {
            appearance::satin_ink(ui.ctx(), TileTone::Blue)
        } else {
            palette.ink
        },
    );
    paint(ui.painter(), rect, ink);
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}
fn paint_alignment(p: &egui::Painter, rect: Rect, alignment: Alignment, ink: Color32) {
    for (row, width) in [17., 11., 17., 11.].into_iter().enumerate() {
        let x = match alignment {
            Alignment::Left => rect.center().x - 8.5,
            Alignment::Center => rect.center().x - width / 2.,
            Alignment::Right => rect.center().x + 8.5 - width,
        };
        let y = rect.center().y - 6. + row as f32 * 4.;
        p.line_segment(
            [egui::pos2(x, y), egui::pos2(x + width, y)],
            Stroke::new(1.5, ink),
        );
    }
}
fn inline_toolbar_rect(text: Rect, viewport: Rect, controls: usize) -> Rect {
    let size = Vec2::new(controls as f32 * 30. + 6., 34.);
    let x = (text.right() + 10.).clamp(
        viewport.left(),
        (viewport.right() - size.x).max(viewport.left()),
    );
    let y = if text.top() - size.y - 10. >= viewport.top() {
        text.top() - size.y - 10.
    } else {
        (text.bottom() + 10.).min((viewport.bottom() - size.y).max(viewport.top()))
    };
    Rect::from_min_size(egui::pos2(x, y), size)
}
const FONT_POPUP_WIDTH: f32 = 210.;
const COLOR_POPUP_WIDTH: f32 = 232.;

fn font_popup_alignment(anchor: &egui::Response, canvas: Rect) -> egui::RectAlign {
    // Reserve room for the expanded picker before it opens. Its collapsed
    // height can fit below the button even when its scrollable list cannot.
    let expanded_height = crate::text_style::FONT_LIST_HEIGHT + 120.;
    let enough_canvas_above = anchor.rect.top() - canvas.top() >= expanded_height;
    if anchor.ctx.content_rect().bottom() - anchor.rect.bottom() < expanded_height
        || (canvas.bottom() - anchor.rect.bottom() < expanded_height && enough_canvas_above)
    {
        egui::RectAlign::TOP_START
    } else {
        egui::RectAlign::BOTTOM_START
    }
}

fn inline_popup(
    ui: &egui::Ui,
    anchor: &egui::Response,
    width: f32,
    alignment: egui::RectAlign,
    content: impl FnOnce(&mut egui::Ui),
) {
    egui::Popup::menu(anchor)
        .align(alignment)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(object_properties::quick_frame(ui).inner_margin(8))
        .style(|_: &mut egui::Style| {})
        .layout(egui::Layout::top_down(egui::Align::Min))
        .width(width + 18.)
        .gap(6.)
        .show(|ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = egui::vec2(3., 6.);
            ui.spacing_mut().button_padding = egui::vec2(4., 2.);
            ui.spacing_mut().interact_size = egui::vec2(26., 24.);
            content(ui);
        });
}

fn inline_color_button(ui: &mut egui::Ui, annotation: &mut Annotation) {
    let callout = annotation.kind == AnnotationKind::Callout;
    let target_id = ui.id().with("callout-text-color");
    let mut text = callout && ui.data(|data| data.get_temp(target_id).unwrap_or(false));
    let [r, g, b, a] = if text {
        annotation.text_color
    } else {
        annotation.color
    };
    let swatch = Color32::from_rgba_unmultiplied(r, g, b, a);
    let label = if callout {
        "Callout colors"
    } else {
        "Text color"
    };
    let button = inline_control(ui, label, |p, r, _| {
        let chip = Rect::from_center_size(r.center(), Vec2::splat(17.));
        p.rect_filled(chip, 4, swatch);
        p.rect_stroke(
            chip,
            4,
            Stroke::new(0.7, appearance::palette(p.ctx()).muted),
            StrokeKind::Inside,
        );
    });
    inline_popup(
        ui,
        &button,
        COLOR_POPUP_WIDTH,
        egui::RectAlign::BOTTOM_START,
        |ui| {
            if callout
                && appearance::segmented(
                    ui,
                    "callout-color-target",
                    &mut text,
                    &[
                        (false, "Bubble", None, "Change the callout fill"),
                        (true, "Text", None, "Change the callout text color"),
                    ],
                )
            {
                ui.data_mut(|data| data.insert_temp(target_id, text));
            }
            color_controls(
                ui,
                if text {
                    &mut annotation.text_color
                } else {
                    &mut annotation.color
                },
            );
        },
    );
}

fn color_controls(ui: &mut egui::Ui, color: &mut [u8; 4]) {
    object_properties::palette(ui, color);
    ui.separator();
    ui.collapsing("Custom color", |ui| {
        ui.spacing_mut().slider_width = 132.;
        ui.spacing_mut().item_spacing.y = 4.;
        ui.spacing_mut().interact_size = egui::vec2(21., 19.);
        ui.spacing_mut().button_padding = egui::vec2(2., 1.);
        ui.style_mut().override_font_id = Some(egui::FontId::proportional(11.));
        let mut swatch = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
        if egui::color_picker::color_picker_color32(
            ui,
            &mut swatch,
            egui::color_picker::Alpha::OnlyBlend,
        ) {
            *color = swatch.to_srgba_unmultiplied();
        }
    });
}

fn font_controls(ui: &mut egui::Ui, annotation: &mut Annotation) -> bool {
    ui.weak("Typeface");
    let accepted = annotation.text_style.font_face_picker(ui);

    let size = &mut annotation.font_size;
    let max = crate::document::MAX_FONT_SIZE;
    ui.horizontal(|ui| {
        annotation.text_style.emphasis_buttons(ui);
        ui.add_space(8.);
        if ui
            .add_enabled(
                *size > 1.,
                egui::Button::new("−").min_size(egui::vec2(24., 24.)),
            )
            .on_hover_text("Decrease font size")
            .clicked()
        {
            *size = (*size - 1.).max(1.);
        }
        ui.add_sized(
            [55., 24.],
            egui::DragValue::new(size)
                .range(1. ..=max)
                .speed(0.5)
                .max_decimals(1)
                .suffix(" px")
                .update_while_editing(false),
        )
        .on_hover_text("Font size in image pixels. Canvas zoom changes its size on screen. Click to type or drag to adjust.");
        if ui
            .add_enabled(
                *size < max,
                egui::Button::new("+").min_size(egui::vec2(24., 24.)),
            )
            .on_hover_text("Increase font size")
            .clicked()
        {
            *size = (*size + 1.).min(max);
        }
    });
    accepted
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::new(200, 200)),
            annotations: vec![],
            geometry: Default::default(),
        }
    }

    #[test]
    fn inline_popovers_fit_their_controls_near_the_window_edge() {
        for (font, anchor_y) in [(true, 64.), (false, 64.), (true, 660.)] {
            let width = if font {
                FONT_POPUP_WIDTH
            } else {
                COLOR_POPUP_WIDTH
            };
            let ctx = egui::Context::default();
            appearance::apply(&ctx);
            if !font {
                ctx.style_mut(|style| {
                    style.visuals.numeric_color_space = egui::style::NumericColorSpace::Linear;
                });
            }
            let mut annotation = EditorState::default()
                .new_annotation(AnnotationKind::Text, Point { x: 20., y: 20. });
            let mut popup_id = egui::Id::NULL;
            let bounds = Rect::from_min_size(Pos2::ZERO, egui::vec2(1200., 800.));
            let mut expander = None;
            let mut font_option = None;
            for frame in 0..14 {
                let mut events = vec![];
                let target = match frame {
                    3 | 4 => expander,
                    9..=12 => font_option,
                    _ => None,
                };
                if let Some(pos) = target {
                    events.push(egui::Event::PointerMoved(pos));
                    events.push(egui::Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: matches!(frame, 3 | 9 | 11),
                        modifiers: Modifiers::NONE,
                    });
                }
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(bounds),
                        time: Some(f64::from(frame) * 0.1),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::Area::new(egui::Id::new("test-edge-anchor"))
                            .fixed_pos(egui::pos2(1110., anchor_y))
                            .show(ctx, |ui| {
                                let anchor = ui.button("Open");
                                popup_id = egui::Popup::default_response_id(&anchor);
                                if frame == 0 {
                                    egui::Popup::open_id(ctx, popup_id);
                                }
                                let alignment = if font {
                                    font_popup_alignment(&anchor, bounds)
                                } else {
                                    egui::RectAlign::BOTTOM_START
                                };
                                inline_popup(ui, &anchor, width, alignment, |ui| {
                                    if font {
                                        font_controls(ui, &mut annotation);
                                    } else {
                                        color_controls(ui, &mut annotation.color);
                                    }
                                });
                            });
                    },
                );
                let find_label = |label| {
                    output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some(text.pos + text.galley.size() * 0.5)
                        }
                        _ => None,
                    })
                };
                if frame == 2 {
                    if anchor_y > bounds.center().y {
                        let popup = ctx.memory(|memory| memory.area_rect(popup_id)).unwrap();
                        assert!(popup.bottom() < anchor_y, "{popup:?}");
                    }
                    expander = find_label(if font { "Sans serif" } else { "Custom color" });
                    assert!(expander.is_some());
                }
                if font && frame == 8 {
                    font_option = find_label("Crimson Text");
                    assert!(font_option.is_some());
                }
            }
            let popup = ctx.memory(|memory| memory.area_rect(popup_id)).unwrap();
            assert!(bounds.contains_rect(popup), "{popup:?}");
            if anchor_y > bounds.center().y {
                assert!(popup.bottom() < anchor_y, "{popup:?}");
            }
            assert!(
                popup.width() >= width && popup.width() <= width + 19.,
                "{popup:?}"
            );
            if font {
                assert_eq!(
                    annotation.text_style.face,
                    Some(crate::text_style::FontFace::CrimsonText)
                );
                assert!(popup.height() < 295., "{popup:?}");
                assert!(
                    !egui::Popup::is_id_open(&ctx, popup_id),
                    "anchor {anchor_y}"
                );
            } else {
                assert!(popup.height() > 175. && popup.height() < 330., "{popup:?}");
            }
        }
    }
    #[test]
    fn text_session_cancel_and_apply_are_atomic_and_undoable() {
        let mut e = EditorState::default();
        let mut d = document();
        let mut h = EditHistory::new(d.edit_state());
        let before = d.edit_state();
        d.annotations
            .push(e.new_annotation(AnnotationKind::Text, Point { x: 20., y: 20. }));
        e.open_text_editor(0, before, &d);
        d.annotations[0].text = "Draft".into();
        e.finish_text_editor(&mut d, &mut h, false);
        assert!(d.annotations.is_empty());
        assert!(!h.can_undo());
        let before = d.edit_state();
        d.annotations
            .push(e.new_annotation(AnnotationKind::Text, Point { x: 20., y: 20. }));
        e.open_text_editor(0, before, &d);
        d.annotations[0].text = "Applied".into();
        d.annotations[0].text_style.italic = true;
        assert!(e.has_modal());
        assert!(!e.blocks_capture());
        e.toolbar_customizer = true;
        assert!(e.blocks_capture());
        e.toolbar_customizer = false;
        e.finish_gesture(&mut d, &mut h);
        assert!(h.can_undo());
        h.undo(&mut d);
        assert!(d.annotations.is_empty());
        h.redo(&mut d);
        assert_eq!(d.annotations[0].text, "Applied");
        assert!(d.annotations[0].text_style.italic);
        let before = d.edit_state();
        e.open_text_editor(0, before, &d);
        d.annotations[0].text = "Unwanted".into();
        e.finish_text_editor(&mut d, &mut h, false);
        assert_eq!(d.annotations[0].text, "Applied");
    }
    #[test]
    fn committed_inline_formatting_becomes_the_next_text_or_callout_style() {
        for (tool, kind) in [
            (Tool::Text, AnnotationKind::Text),
            (Tool::Callout, AnnotationKind::Callout),
        ] {
            let mut editor = EditorState::default();
            editor.activate_tool(tool);
            let mut document = document();
            let mut annotation = editor.new_annotation(kind, Point { x: 20., y: 20. });
            annotation.end = Point { x: 180., y: 150. };
            annotation.text = "Example".into();
            document.annotations.push(annotation);
            let before = document.edit_state();
            let mut history = EditHistory::new(before.clone());
            editor.open_text_editor(0, before, &document);
            document.annotations[0].color = [17, 88, 140, 190];
            document.annotations[0].text_color = [245, 220, 130, 255];
            document.annotations[0].font_size = 27.;
            document.annotations[0].text_style.monospace = true;
            document.annotations[0].text_style.italic = true;
            document.annotations[0].text_style.alignment = Alignment::Center;
            if kind == AnnotationKind::Callout {
                document.annotations[0].callout_style = crate::callout_style::CalloutStyle::Note;
            }
            editor.finish_text_editor(&mut document, &mut history, true);
            let committed = document.annotations[0].clone();
            editor.activate_tool(Tool::Arrow);
            editor.activate_tool(tool);
            let next = editor.new_annotation(kind, Point { x: 5., y: 7. });
            assert_eq!(next.color, committed.color);
            assert_eq!(next.text_color, committed.text_color);
            assert_eq!(next.font_size, committed.font_size);
            assert_eq!(next.text_style, committed.text_style);
            assert_eq!(next.callout_style, committed.callout_style);
            assert_eq!(next.start, Point { x: 5., y: 7. });
            assert_eq!(next.end, next.start);
            assert!(history.can_undo());
        }
    }

    #[test]
    fn content_only_and_canceled_inline_edits_preserve_chosen_drawing_style() {
        for (apply, change_style) in [(true, false), (false, true)] {
            let mut editor = EditorState::default();
            editor.activate_tool(Tool::Text);
            editor.color = [20, 100, 200, 220];
            editor.font_size = 42.;
            editor.text_style.underline = true;
            editor.remember_appearance(AnnotationKind::Text);
            let chosen = editor.new_annotation(AnnotationKind::Text, Point { x: 1., y: 1. });
            let mut old = chosen.clone();
            old.color = [200, 40, 20, 255];
            old.font_size = 18.;
            old.text_style = Default::default();
            old.text = "Old text".into();
            let mut document = document();
            document.annotations.push(old);
            let before = document.edit_state();
            let mut history = EditHistory::new(before.clone());
            editor.open_text_editor(0, before, &document);
            document.annotations[0].text = "New words".into();
            if change_style {
                document.annotations[0].color = [0, 180, 80, 255];
                document.annotations[0].font_size = 55.;
                document.annotations[0].text_style.italic = true;
            }
            editor.finish_text_editor(&mut document, &mut history, apply);
            editor.activate_tool(Tool::Text);
            let next = editor.new_annotation(AnnotationKind::Text, Point { x: 5., y: 7. });
            assert_eq!(next.color, chosen.color);
            assert_eq!(next.font_size, chosen.font_size);
            assert_eq!(next.text_style, chosen.text_style);
        }
    }

    #[test]
    fn deleting_empty_text_does_not_reveal_an_unrelated_off_canvas_layer() {
        let mut e = EditorState::default();
        let mut d = document();
        let text = e.new_annotation(AnnotationKind::Text, Point { x: 20., y: 20. });
        let mut shape = e.new_annotation(AnnotationKind::Rectangle, Point { x: 250., y: 20. });
        shape.end = Point { x: 320., y: 90. };
        d.annotations = vec![text, shape.clone()];
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        e.open_text_editor(0, before.clone(), &d);
        d.annotations[0].text.clear();
        e.finish_text_editor(&mut d, &mut h, true);
        assert_eq!(d.geometry, before.geometry);
        assert_eq!(d.annotations, vec![shape]);
        assert_eq!(e.selected, None);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
    }
    #[test]
    fn locked_text_does_not_open_for_editing() {
        let mut e = EditorState::default();
        let mut d = document();
        let mut a = e.new_annotation(AnnotationKind::Text, Point { x: 0., y: 0. });
        a.layer.locked = true;
        d.annotations.push(a);
        e.open_text_editor(0, d.edit_state(), &d);
        assert!(e.text_dialog.is_none());
    }
    #[test]
    fn editing_existing_text_appends_while_new_text_replaces_the_placeholder() {
        for existing in [true, false] {
            let ctx = egui::Context::default();
            crate::appearance::apply(&ctx);
            let mut editor = EditorState::default();
            let mut document = document();
            let empty = document.edit_state();
            let mut annotation =
                editor.new_annotation(AnnotationKind::Text, Point { x: 10., y: 10. });
            annotation.text = if existing { "Başlık\nÖzet" } else { "Text" }.into();
            document.annotations.push(annotation);
            let before = if existing {
                document.edit_state()
            } else {
                empty
            };
            let mut history = EditHistory::new(before.clone());
            editor.open_text_editor(0, before, &document);
            editor.text_dialog.as_mut().unwrap().canvas = Some((
                egui::pos2(100., 120.),
                1.,
                Rect::from_min_size(Pos2::ZERO, Vec2::new(1200., 800.)),
            ));
            for events in [vec![], vec![egui::Event::Text(" devam".into())]] {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200., 800.))),
                    events,
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    editor.text_editor_window(ctx, &mut document, &mut history)
                });
            }
            let expected = if existing {
                "Başlık\nÖzet devam"
            } else {
                " devam"
            };
            assert_eq!(document.annotations[0].text, expected);
        }
    }

    #[test]
    fn inline_text_uses_canvas_position_and_apply_shortcut_preserves_content() {
        let context = egui::Context::default();
        crate::appearance::apply(&context);
        let mut editor = EditorState::default();
        let mut document = document();
        let mut annotation = editor.new_annotation(AnnotationKind::Text, Point { x: 10., y: 10. });
        annotation.text = "A long editable line\n".repeat(100);
        document.annotations.push(annotation);
        let content = document.annotations[0].text.clone();
        let original_size = document.annotations[0].font_size;
        let mut history = EditHistory::new(document.edit_state());
        editor.open_text_editor(0, document.edit_state(), &document);
        let generation = editor.text_editor_generation;
        editor.text_dialog.as_mut().unwrap().canvas = Some((
            egui::pos2(100., 120.),
            1.,
            Rect::from_min_max(egui::pos2(20., 40.), egui::pos2(1000., 760.)),
        ));
        let mut font_position = Pos2::ZERO;
        let mut bold_position = Pos2::ZERO;
        let mut size_increase_position = Pos2::ZERO;
        for frame in 0..10 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200., 800.))),
                ..Default::default()
            };
            let click = match frame {
                2 | 3 => Some((font_position, frame == 2)),
                5 | 6 => Some((bold_position, frame == 5)),
                7 | 8 => Some((size_increase_position, frame == 7)),
                _ => None,
            };
            if let Some((pos, pressed)) = click {
                input.events.push(egui::Event::PointerMoved(pos));
                input.events.push(egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                });
            }
            if frame == 9 {
                input.modifiers = Modifiers::COMMAND;
                input.events.push(egui::Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::COMMAND,
                });
            }
            let output = context.run(input, |ctx| {
                editor.text_editor_window(ctx, &mut document, &mut history)
            });
            if frame == 1 {
                let rect = context
                    .memory(|m| m.area_rect(egui::Id::new("inline-text-editor")))
                    .unwrap();
                assert!(
                    (rect.left() - 110.).abs() < 5.,
                    "text must be at canvas x: {rect:?}"
                );
                assert!(
                    (rect.top() - 130.).abs() < 5.,
                    "text must be at canvas y: {rect:?}"
                );
                let bar = context
                    .memory(|m| m.area_rect(egui::Id::new("inline-text-properties")))
                    .unwrap();
                assert!(
                    !bar.intersects(rect),
                    "toolbar overlaps text: {bar:?} {rect:?}"
                );
                assert!(bar.width() <= 126. && bar.height() <= 34., "{bar:?}");
                font_position = egui::pos2(bar.left() + 17., bar.center().y);
                assert!(
                    context
                        .memory(|m| m.area_rect(egui::Id::new("text-properties-window")))
                        .is_none()
                );
            }
            if frame == 4 {
                assert!(egui::Popup::is_any_open(&context));
                let label_position = |label| {
                    output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == label => {
                                Some(text.pos + text.galley.size() * 0.5)
                            }
                            _ => None,
                        })
                        .unwrap()
                };
                bold_position = label_position("B");
                size_increase_position = label_position("+");
            }
            if frame == 8 {
                assert_eq!(document.annotations[0].font_size, original_size + 1.);
                assert!(egui::Popup::is_any_open(&context));
            }
        }
        assert!(editor.text_dialog.is_none());
        assert_eq!(document.annotations[0].text, content);
        assert!(document.annotations[0].text_style.bold);
        assert_eq!(document.annotations[0].font_size, original_size + 1.);
        assert!(history.undo(&mut document));
        assert!(!document.annotations[0].text_style.bold);
        editor.open_text_editor(0, document.edit_state(), &document);
        assert_ne!(editor.text_editor_generation, generation);
    }
    #[test]
    fn callout_text_edits_inside_the_bubble_and_style_changes_are_atomic() {
        for alignment in [Alignment::Left, Alignment::Center, Alignment::Right] {
            let ctx = egui::Context::default();
            crate::appearance::apply(&ctx);
            let mut e = EditorState::default();
            let mut d = document();
            let mut a = crate::callout_style::sample_annotation();
            a.start = Point { x: 20., y: 20. };
            a.end = Point { x: 190., y: 140. };
            a.tail = Some(Point { x: 60., y: 180. });
            a.text = "Hello\nEditable callout".into();
            a.text_style.alignment = alignment;
            d.annotations.push(a);
            let before = d.edit_state();
            let mut h = EditHistory::new(before.clone());
            e.open_text_editor(0, before.clone(), &d);
            e.text_dialog.as_mut().unwrap().canvas = Some((
                egui::pos2(100., 100.),
                1.,
                Rect::from_min_max(egui::pos2(50., 50.), egui::pos2(900., 700.)),
            ));
            for frame in 0..4 {
                if frame == 2 {
                    d.annotations[0].callout_style =
                        crate::callout_style::CalloutStyle::ThoughtCloud;
                }
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 800.))),
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| e.text_editor_window(ctx, &mut d, &mut h));
                if frame == 1 || frame == 3 {
                    let rect = ctx
                        .memory(|m| m.area_rect(egui::Id::new("inline-text-editor")))
                        .unwrap();
                    let metrics = render::editable_text_layout(&d.annotations[0]).unwrap();
                    let expected = egui::pos2(100., 100.) + metrics.origin.to_vec2();
                    assert!(
                        rect.min.distance(expected) < 2.,
                        "{alignment:?} {rect:?} {expected:?}"
                    );
                    assert!(
                        Rect::from_min_max(egui::pos2(120., 120.), egui::pos2(290., 240.))
                            .expand(2.)
                            .contains_rect(rect),
                        "text outside bubble: {rect:?}"
                    );
                    let bar = ctx
                        .memory(|m| m.area_rect(egui::Id::new("inline-text-properties")))
                        .unwrap();
                    assert!(bar.width() <= 156. && bar.height() <= 34., "{bar:?}");
                    assert!(!bar.intersects(rect));
                }
            }
            e.finish_text_editor(&mut d, &mut h, true);
            assert_eq!(d.annotations[0].text, "Hello\nEditable callout");
            assert_eq!(
                d.annotations[0].callout_style,
                crate::callout_style::CalloutStyle::ThoughtCloud
            );
            assert!(h.undo(&mut d));
            assert_eq!(d.edit_state(), before);
            assert!(!h.can_undo());
        }
    }
    #[test]
    fn aligned_glyphs_and_cursor_positions_share_top_left_coordinates() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            for alignment in [egui::Align::Min, egui::Align::Center, egui::Align::Max] {
                let mut job = egui::text::LayoutJob::simple(
                    "Long first line\nShort".into(),
                    egui::FontId::proportional(28.),
                    Color32::RED,
                    300.,
                );
                job.halign = alignment;
                let galley = normalize_text_galley(ctx.fonts_mut(|f| f.layout_job(job)));
                assert_eq!(galley.rect.min, Pos2::ZERO);
                for row in &galley.rows {
                    assert!(row.pos.x >= -0.01);
                }
                let cursor = galley.pos_from_cursor(egui::text::CCursor::new(0));
                assert!(galley.rect.expand(1.).contains(cursor.center()));
            }
        });
    }

    #[test]
    fn inline_typing_and_outside_click_commit_one_undo_step() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let mut e = EditorState::default();
        let mut d = document();
        let mut a = e.new_annotation(AnnotationKind::Text, Point { x: 20., y: 20. });
        a.text = "Original".into();
        d.annotations.push(a);
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        e.open_text_editor(0, before.clone(), &d);
        e.text_dialog.as_mut().unwrap().canvas = Some((
            egui::pos2(100., 100.),
            1.,
            Rect::from_min_max(egui::pos2(50., 50.), egui::pos2(900., 700.)),
        ));
        for frame in 0..4 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200., 800.))),
                ..Default::default()
            };
            if frame == 2 {
                input
                    .events
                    .push(egui::Event::Text(" written on canvas".into()));
            }
            if frame == 3 {
                input
                    .events
                    .push(egui::Event::PointerMoved(egui::pos2(700., 500.)));
                input.events.push(egui::Event::PointerButton {
                    pos: egui::pos2(700., 500.),
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                });
            }
            let _ = ctx.run(input, |ctx| e.text_editor_window(ctx, &mut d, &mut h));
        }
        assert!(e.text_dialog.is_none());
        assert_eq!(d.annotations[0].text, "Original written on canvas");
        assert_eq!(e.selected, None);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
    }
}
