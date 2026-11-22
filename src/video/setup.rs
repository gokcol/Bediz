//! Direct controls drawn in the capture overlay, above the selection surface.
use super::*;
use crate::{appearance::TileTone, capture_panel::CaptureLook};

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub(in crate::app) enum VideoPanel {
    Recording,
    Camera,
}

impl VideoPanel {
    fn title(self) -> &'static str {
        match self {
            Self::Recording => "Recording options",
            Self::Camera => "Presenter camera",
        }
    }
}

pub(in crate::app) fn show_video_panel(
    ctx: &egui::Context,
    screen: &mut Selection,
    settings: &mut Settings,
    toolbar: Rect,
) -> bool {
    let Some(panel) = screen.video_panel else {
        return false;
    };
    let look = settings.capture_theme.look();
    let viewport = ctx.content_rect().shrink(10.);
    let width = viewport.width().clamp(180., 344.);
    let above = toolbar.top() - viewport.top();
    let below = viewport.bottom() - toolbar.bottom();
    let place_above = above >= 410. || above >= below;
    let space = if place_above { above } else { below };
    let x = (toolbar.center().x - width / 2.).clamp(
        viewport.left(),
        (viewport.right() - width).max(viewport.left()),
    );
    let (anchor, pivot) = if place_above {
        (Pos2::new(x, toolbar.top() - 8.), egui::Align2::LEFT_BOTTOM)
    } else {
        (Pos2::new(x, toolbar.bottom() + 8.), egui::Align2::LEFT_TOP)
    };
    let view = egui::Area::new(egui::Id::new((
        "video-setup-panel",
        screen.display.id,
        panel,
    )))
    .order(egui::Order::Foreground)
    .movable(false)
    .default_size(egui::vec2(width, (space - 8.).clamp(126., 504.)))
    .pivot(pivot)
    .fixed_pos(anchor)
    .constrain_to(viewport)
    .show(ctx, |ui| {
        crate::capture_panel::apply_look(ui, &look);
        egui::Frame::new()
            .fill(look.palette.paper)
            .stroke(Stroke::new(1., look.palette.border))
            .corner_radius(13.)
            .inner_margin(12)
            .shadow(egui::Shadow {
                offset: [0, 6],
                blur: 18,
                spread: 0,
                color: Color32::from_black_alpha(108),
            })
            .show(ui, |ui| {
                ui.set_width(width - 24.);
                ui.spacing_mut().item_spacing = egui::vec2(6., 5.);
                let close = panel_header(ui, panel, &look);
                ui.separator();
                let changed = egui::ScrollArea::vertical()
                    .max_height((space - 72.).clamp(54., 430.))
                    .auto_shrink([false, true])
                    .show(ui, |ui| match panel {
                        VideoPanel::Recording => {
                            let dimensions = match settings.video.target {
                                VideoTarget::Window => {
                                    screen.window_id.and_then(|id| window_size(screen, id))
                                }
                                VideoTarget::FullScreen => {
                                    Some([screen.image.width(), screen.image.height()])
                                }
                            };
                            preferences_ui(
                                ui,
                                &mut settings.video,
                                &settings.output_dir,
                                dimensions,
                                &look,
                            )
                        }
                        VideoPanel::Camera => {
                            let changed = screen.webcam.setup_ui(ui, &look);
                            settings.video.presenter_camera = screen.webcam.enabled;
                            changed
                        }
                    })
                    .inner;
                (changed, close)
            })
            .inner
    });
    let pressed_outside = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer.interact_pos().is_some_and(|point| {
                !toolbar.contains(point) && !view.response.rect.contains(point)
            })
    });
    let (changed, close) = view.inner;
    if close || pressed_outside {
        screen.video_panel = None;
    }
    changed
}

fn panel_header(ui: &mut egui::Ui, panel: VideoPanel, look: &CaptureLook) -> bool {
    ui.horizontal(|ui| {
        let (tile, _) = ui.allocate_exact_size(Vec2::splat(30.), egui::Sense::hover());
        let (icon, tone) = match panel {
            VideoPanel::Recording => (Icon::Settings, TileTone::Blue),
            VideoPanel::Camera => (Icon::Camera, TileTone::Teal),
        };
        appearance::sculpted_icon(ui.painter(), tile.center(), icon, tone, 28., None);
        ui.label(
            egui::RichText::new(panel.title())
                .color(look.palette.ink)
                .strong()
                .size(17.),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            super::super::capture_session::capture_icon(ui, "Close setup panel", Icon::Close, false)
                .clicked()
        })
        .inner
    })
    .inner
}

impl WebcamSetup {
    pub(super) fn refresh_preview_texture(&mut self, ctx: &egui::Context) {
        const REFRESH: Duration = Duration::from_millis(80);
        if self
            .preview_refresh
            .is_some_and(|last| last.elapsed() < REFRESH)
        {
            return;
        }
        self.preview_refresh = Some(Instant::now());
        const SIDE: usize = 160;
        let preview = self.preview.get_or_insert_with(WebcamPreview::new);
        self.preview_pixels.resize(SIDE * SIDE * 4, 0);
        // SAFETY: The preview is retained and the buffer stays writable for this synchronous call.
        let ready = unsafe {
            bediz_webcam_copy_preview(
                preview.0.as_ptr(),
                self.preview_pixels.as_mut_ptr(),
                self.preview_pixels.len(),
                SIDE,
                i32::from(self.framing.mirrored),
            ) != 0
        };
        if ready {
            let image =
                egui::ColorImage::from_rgba_unmultiplied([SIDE, SIDE], &self.preview_pixels);
            if let Some(texture) = &mut self.preview_texture {
                texture.set(image, egui::TextureOptions::LINEAR);
            } else {
                self.preview_texture =
                    Some(ctx.load_texture("presenter-camera", image, egui::TextureOptions::LINEAR));
            }
        }
    }

    fn setup_ui(&mut self, ui: &mut egui::Ui, look: &CaptureLook) -> bool {
        let enabled_changed = ui
            .horizontal_wrapped(|ui| {
                let changed = ui
                    .checkbox(&mut self.enabled, "Include presenter camera")
                    .changed();
                ui.checkbox(&mut self.framing.mirrored, "Mirror camera");
                changed
            })
            .inner;
        egui::Frame::new()
            .fill(look.palette.navy)
            .corner_radius(10.)
            .inner_margin(9)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(94.), egui::Sense::hover());
                    self.paint_circle(ui.painter(), rect, look.palette.paper, look.palette.muted);
                    ui.vertical_centered_justified(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.;
                        ui.strong("Live preview");
                        ui.weak(if !self.enabled {
                            "Enable the camera to preview your video."
                        } else if let Some(error) = &self.preview_error {
                            error
                        } else if self.preview_texture.is_some() {
                            "Your camera is ready."
                        } else {
                            "Starting camera…"
                        });
                        ui.add_space(2.);
                        ui.label(egui::RichText::new("Size").size(11.));
                        appearance::segmented_with_palette(
                            ui,
                            "camera-size",
                            &mut self.framing.size,
                            &[
                                (BubbleSize::Small, "Small", None, ""),
                                (BubbleSize::Medium, "Medium", None, ""),
                                (BubbleSize::Large, "Large", None, ""),
                            ],
                            look.palette,
                        );
                    });
                });
            });
        ui.add_space(3.);
        ui.horizontal_wrapped(|ui| {
            for mode in Webcam::ALL {
                let response = ui.add_sized(
                    [36., 30.],
                    egui::Button::new("").selected(mode == self.mode),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        ui.is_enabled(),
                        mode == self.mode,
                        mode.label(),
                    )
                });
                let p = ui.painter();
                let c = response.rect.center();
                let ink = ui
                    .style()
                    .interact_selectable(&response, mode == self.mode)
                    .fg_stroke
                    .color;
                if matches!(mode, Webcam::Free | Webcam::Random) {
                    let icon = if mode == Webcam::Free {
                        Icon::Resize
                    } else {
                        Icon::Refresh
                    };
                    appearance::icon(p, c, icon, ink, 16.);
                } else {
                    let frame = Rect::from_center_size(c, egui::vec2(20., 15.));
                    p.rect_stroke(frame, 2., Stroke::new(1., ink), egui::StrokeKind::Inside);
                    let place = frame.shrink(4.);
                    let point = place.min + place.size() * mode.position(Vec2::ZERO, Vec2::ZERO);
                    p.circle_filled(point, 2.5, ink);
                }
                let hint = if mode == Webcam::Free {
                    "Free position · Drag the camera bubble inside the chosen window or display"
                } else {
                    mode.label()
                };
                if response.on_hover_text(hint).clicked() {
                    self.choose_position(mode);
                }
            }
        });
        enabled_changed
    }
}
