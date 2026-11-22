use super::*;
use image::ImageFormat;
use std::{io::Cursor, sync::Mutex};

const MAX_ARTWORK: usize = 32 * 1024 * 1024;
const FRAME_EDITOR: &str = "Annotate This Frame";

#[derive(Clone, Copy)]
pub(super) struct Request {
    id: u64,
    time: i64,
    available: i64,
    editing: bool,
}

pub(super) struct Reference {
    request: Request,
    document: Document,
    history: EditHistory,
    editor: EditorState,
    texture: egui::TextureHandle,
    preview: PrivacyPreview,
    seconds: f64,
    error: Option<String>,
    sending: bool,
    needs_focus: bool,
    confirm_cancel: bool,
}

pub(super) struct Artwork {
    bytes: Vec<u8>,
    composite: bool,
    objects: Option<Vec<u8>>,
}

struct Bridge {
    ctx: egui::Context,
    tx: mpsc::Sender<WorkEvent>,
}

static BRIDGE: Mutex<Option<Bridge>> = Mutex::new(None);

type OpenReference = extern "C" fn(u64, *const u8, usize, i64, i64, *const u8, usize) -> bool;
type ApplyArtwork = extern "C" fn(*const u8, usize, *mut u8, usize, u32, u32) -> bool;

unsafe extern "C" {
    fn bediz_video_frame_bridge(open: OpenReference, apply: ApplyArtwork);
    fn bediz_video_frame_result(
        id: u64,
        bytes: *const u8,
        size: usize,
        duration: i64,
        kind: i32,
        objects: *const u8,
        objects_size: usize,
    ) -> bool;
}

pub(super) fn connect(ctx: &egui::Context, tx: mpsc::Sender<WorkEvent>) {
    if let Ok(mut bridge) = BRIDGE.lock() {
        *bridge = Some(Bridge {
            ctx: ctx.clone(),
            tx,
        });
    }
    // SAFETY: Registered on the main thread before opening the AppKit player.
    unsafe { bediz_video_frame_bridge(open_reference, apply_artwork) };
}

extern "C" fn open_reference(
    id: u64,
    bytes: *const u8,
    length: usize,
    time: i64,
    available: i64,
    objects: *const u8,
    objects_size: usize,
) -> bool {
    if bytes.is_null()
        || length == 0
        || length > MAX_ARTWORK
        || available <= 0
        || objects_size > MAX_ARTWORK
        || (objects_size > 0 && objects.is_null())
    {
        return false;
    }
    let Ok(bridge) = BRIDGE.lock() else {
        return false;
    };
    let Some(bridge) = bridge.as_ref() else {
        return false;
    };
    let tx = bridge.tx.clone();
    let ctx = bridge.ctx.clone();
    // SAFETY: Swift retains the PNG bytes for the duration of this callback.
    let png = unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec();
    let objects = if objects_size == 0 {
        None
    } else {
        // SAFETY: The original object data is held by Swift during the same callback.
        Some(unsafe { std::slice::from_raw_parts(objects, objects_size) }.to_vec())
    };
    let request = Request {
        id,
        time,
        available,
        editing: objects.is_some(),
    };
    std::thread::spawn(move || {
        let result = decode_reference(&png, objects.as_deref());
        let _ = tx.send(WorkEvent::VideoFrameOpened(request, result));
        crate::runtime::wake_root(&ctx);
    });
    true
}

fn decode_reference(png: &[u8], objects: Option<&[u8]>) -> Result<Document, String> {
    let image = image::load_from_memory_with_format(png, ImageFormat::Png)
        .map(image::DynamicImage::into_rgba8)
        .map_err(|error| format!("Could not open the video frame: {error}"))?;
    let annotations = match objects {
        Some(objects) => serde_json::from_slice(objects)
            .map_err(|error| format!("Could not open the original annotations: {error}"))?,
        None => Vec::new(),
    };
    let document = Document {
        image: Arc::new(image),
        annotations,
        geometry: Default::default(),
    };
    document::validate_document(&document)?;
    Ok(document)
}

fn complete(request: Request, duration: i64, artwork: Option<&Artwork>) -> bool {
    let (ptr, len, kind) = artwork.map_or((std::ptr::null(), 0, 0), |artwork| {
        (
            artwork.bytes.as_ptr(),
            artwork.bytes.len(),
            if artwork.composite { 2 } else { 1 },
        )
    });
    let (objects, size) = artwork
        .and_then(|a| a.objects.as_ref())
        .map_or((std::ptr::null(), 0), |bytes| (bytes.as_ptr(), bytes.len()));
    // SAFETY: Called from the main UI thread; Swift copies both buffers before returning.
    unsafe { bediz_video_frame_result(request.id, ptr, len, duration, kind, objects, size) }
}

fn encode_artwork(mut document: Document, editing: bool) -> Result<Artwork, String> {
    if document.geometry != Default::default() {
        return Err("Frame annotations need the original video dimensions.".into());
    }
    if !editing
        && !document
            .annotations
            .iter()
            .any(document::Annotation::visible)
    {
        return Err("Add or show an annotation before sending it to the video.".into());
    }
    crate::document::validate_document(&document)?;
    let composite = document
        .annotations
        .iter()
        .any(|a| a.visible() && a.kind.needs_composite());
    let objects = serde_json::to_vec(&document.annotations).map_err(|error| error.to_string())?;
    let (bytes, objects) = if composite {
        (objects, None)
    } else {
        document.image = Arc::new(image::RgbaImage::new(
            document.image.width(),
            document.image.height(),
        ));
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(render::flatten(&document)?)
            .write_to(&mut png, ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        (png.into_inner(), Some(objects))
    };
    if bytes.len() > MAX_ARTWORK
        || objects
            .as_ref()
            .is_some_and(|bytes| bytes.len() > MAX_ARTWORK)
    {
        return Err("This set of annotations exceeds the video artwork limit of 32 MB.".into());
    }
    Ok(Artwork {
        bytes,
        composite,
        objects,
    })
}

extern "C" fn apply_artwork(
    program: *const u8,
    program_len: usize,
    rgba: *mut u8,
    rgba_len: usize,
    width: u32,
    height: u32,
) -> bool {
    if program.is_null()
        || rgba.is_null()
        || program_len == 0
        || program_len > MAX_ARTWORK
        || width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || rgba_len != width as usize * height as usize * 4
    {
        return false;
    }
    // SAFETY: Swift retains disjoint program and bitmap buffers during this callback.
    let program = unsafe { std::slice::from_raw_parts(program, program_len) };
    let rgba = unsafe { std::slice::from_raw_parts_mut(rgba, rgba_len) };
    let Ok(annotations) = serde_json::from_slice(program) else {
        return false;
    };
    let Some(image) = image::RgbaImage::from_raw(width, height, rgba.to_vec()) else {
        return false;
    };
    let document = Document {
        image: Arc::new(image),
        annotations,
        geometry: Default::default(),
    };
    let Ok(result) = render::flatten(&document) else {
        return false;
    };
    rgba.copy_from_slice(result.as_raw());
    true
}

fn viewport() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("video-frame-annotation")
}

fn clock(time: i64) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        time / 3_600_000,
        time / 60_000 % 60,
        time / 1_000 % 60,
        time % 1_000
    )
}

impl MyImageApp {
    pub(super) fn open_frame_reference(
        &mut self,
        ctx: &egui::Context,
        request: Request,
        document: Result<Document, String>,
    ) {
        let document = match document {
            Ok(document) => document,
            Err(error) => {
                self.error = Some(error);
                complete(request, 0, None);
                return;
            }
        };
        if self.frame_reference.is_some() || self.quitting {
            complete(request, 0, None);
            return;
        }
        let texture = load_texture(ctx, "video-frame-reference", &document.image);
        let history = EditHistory::new(document.edit_state());
        let mut editor = EditorState::with_presets(storage::preset_library_path(self.demo));
        editor.set_tool_preferences(self.settings.editor_tools.clone());
        editor.set_toolbar(
            self.settings.editor_toolbar.clone(),
            self.settings.toolbar_layout_version,
        );
        editor.set_toolbar_order(self.settings.editor_tool_order.clone());
        editor.lock_reference();
        self.frame_reference = Some(Reference {
            request,
            document,
            history,
            editor,
            texture,
            preview: Default::default(),
            seconds: (if request.editing {
                request.available
            } else {
                request.available.min(3_000)
            }) as f64
                / 1_000.0,
            error: None,
            sending: false,
            needs_focus: true,
            confirm_cancel: false,
        });
        ctx.request_repaint();
    }

    pub(super) fn frame_artwork_ready(
        &mut self,
        ctx: &egui::Context,
        request: Request,
        duration: i64,
        result: Result<Artwork, String>,
    ) {
        let Some(reference) = &mut self.frame_reference else {
            return;
        };
        if reference.request.id != request.id {
            return;
        }
        reference.sending = false;
        match result {
            Ok(artwork) if complete(request, duration, Some(&artwork)) => {
                self.frame_reference = None;
                ctx.send_viewport_cmd_to(viewport(), ViewportCommand::Close);
            }
            Ok(_) => {
                reference.error = Some(
                    "The video player could not receive these annotations. Try again or cancel."
                        .into(),
                )
            }
            Err(error) => reference.error = Some(error),
        }
    }

    pub(super) fn focus_frame_reference(&mut self, ctx: &egui::Context) -> bool {
        if self.frame_reference.is_some() {
            ctx.send_viewport_cmd_to(viewport(), ViewportCommand::Focus);
            true
        } else {
            false
        }
    }

    pub(super) fn frame_reference_ui(&mut self, ctx: &egui::Context) {
        let Some(reference) = &mut self.frame_reference else {
            return;
        };
        let title = if reference.request.editing {
            "Edit Frame Annotations"
        } else {
            FRAME_EDITOR
        };
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([1080., 650.])
            .with_min_inner_size([680., 420.])
            .with_active(true);
        let mut send = false;
        let mut cancel = false;
        ctx.show_viewport_immediate(viewport(), builder, |child, class| {
            if std::mem::take(&mut reference.needs_focus) {
                child.send_viewport_cmd(ViewportCommand::Focus);
            }
            if child.input(|ui| ui.viewport().close_requested()) {
                child.send_viewport_cmd(ViewportCommand::CancelClose);
                if !reference.sending {
                    reference.confirm_cancel = true;
                }
            }
            let mut content = |ui: &mut egui::Ui| {
                let busy = reference.sending;
                let can_send = !busy && !reference.confirm_cancel && !reference.editor.has_modal();
                ui.horizontal(|ui| {
                    let (icon, _) =
                        ui.allocate_exact_size(egui::vec2(22., 22.), egui::Sense::hover());
                    appearance::icon(
                        ui.painter(),
                        icon.center(),
                        Icon::Video,
                        appearance::palette(ui.ctx()).accent,
                        16.,
                    );
                    ui.label(egui::RichText::new("Locked reference").strong());
                    ui.monospace(clock(reference.request.time));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        send |= appearance::action_button(
                            ui,
                            "Send to Video",
                            Icon::Check,
                            can_send,
                            true,
                        )
                        .clicked();
                        if appearance::action_button(ui, "Cancel", Icon::Close, !busy, false)
                            .clicked()
                        {
                            reference.confirm_cancel = true;
                        }
                        if reference.request.editing {
                            ui.monospace(clock(reference.request.available))
                                .on_hover_text("Uses the layer's current timeline duration.");
                        } else {
                            ui.add_enabled(
                                can_send,
                                egui::DragValue::new(&mut reference.seconds)
                                    .range(0.001..=reference.request.available as f64 / 1000.0)
                                    .speed(0.05)
                                    .fixed_decimals(3)
                                    .suffix(" s"),
                            )
                            .on_hover_text("How long the annotations appear in the video");
                        }
                        ui.label("Duration");
                    });
                });
                if let Some(error) = &reference.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                if busy {
                    ui.weak("Preparing annotations…");
                }
                ui.separator();
                appearance::input_scope(ui, can_send, !reference.editor.has_dialog(), |ui| {
                    ui.horizontal(|ui| {
                        reference.editor.toolbar(
                            ui,
                            &mut reference.document,
                            &mut reference.history,
                        )
                    });
                });
                ui.separator();
                ui.add_enabled_ui(!busy && !reference.confirm_cancel, |ui| {
                    reference.editor.ui(
                        ui,
                        &mut reference.document,
                        &mut reference.history,
                        &reference.texture,
                        &mut reference.preview,
                    );
                });
                if reference.confirm_cancel {
                    let modal = egui::Modal::new(egui::Id::new("discard-frame-annotations")).show(
                        ui.ctx(),
                        |ui| {
                            ui.heading("Cancel frame annotations?");
                            ui.horizontal(|ui| {
                                if appearance::action_button(
                                    ui,
                                    "Keep editing",
                                    Icon::Back,
                                    true,
                                    false,
                                )
                                .clicked()
                                {
                                    reference.confirm_cancel = false;
                                }
                                cancel |= appearance::action_button(
                                    ui,
                                    "Discard annotations",
                                    Icon::Trash,
                                    true,
                                    true,
                                )
                                .clicked();
                            });
                        },
                    );
                    if modal.should_close() {
                        reference.confirm_cancel = false;
                    }
                }
            };
            if class == egui::ViewportClass::Embedded {
                egui::Window::new(title)
                    .default_size([900., 560.])
                    .show(child, &mut content);
            } else {
                egui::CentralPanel::default().show(child, content);
            }
        });
        if cancel {
            complete(reference.request, 0, None);
            self.frame_reference = None;
            ctx.send_viewport_cmd_to(viewport(), ViewportCommand::Close);
        } else if send {
            reference
                .editor
                .finish_gesture(&mut reference.document, &mut reference.history);
            reference.sending = true;
            reference.error = None;
            let request = reference.request;
            let duration = if request.editing {
                request.available
            } else {
                ((reference.seconds * 1000.0).round() as i64).clamp(1, request.available)
            };
            let document = reference.document.clone();
            let tx = self.work_tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(WorkEvent::VideoFrameArtwork(
                    request,
                    duration,
                    encode_artwork(document, request.editing),
                ));
                crate::runtime::wake_root(&ctx);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn annotation(kind: &str) -> document::Annotation {
        serde_json::from_value(serde_json::json!({
            "kind": kind,
            "start": {"x": 4.0, "y": 2.0},
            "end": {"x": 18.0, "y": 12.0},
            "color": [240, 30, 45, 255],
            "stroke_width": 2.0,
            "fill": true,
            "text": "",
            "font_size": 12.0,
            "effect_size": 6
        }))
        .unwrap()
    }

    #[test]
    fn frame_handoff_rasterizes_only_artwork_and_preserves_original_pixels() {
        let background = Arc::new(RgbaImage::from_pixel(24, 24, Rgba([20, 120, 30, 255])));
        let objects = vec![annotation("Rectangle"), annotation("Arrow")];
        let doc = Document {
            image: background.clone(),
            annotations: objects.clone(),
            geometry: Default::default(),
        };
        let result = encode_artwork(doc, false).unwrap();
        assert!(!result.composite);
        let pixels = image::load_from_memory(&result.bytes).unwrap().to_rgba8();
        assert_eq!(pixels.dimensions(), background.dimensions());
        assert_eq!(pixels.get_pixel(22, 18).0[3], 0);
        assert_eq!(pixels.get_pixel(10, 6).0, [240, 30, 45, 255]);
        assert_eq!(background.get_pixel(10, 6).0, [20, 120, 30, 255]);
        let mut reference = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8((*background).clone())
            .write_to(&mut reference, ImageFormat::Png)
            .unwrap();
        let reopened = decode_reference(reference.get_ref(), result.objects.as_deref()).unwrap();
        assert_eq!(reopened.annotations, objects);
        assert_eq!(reopened.image, background);
    }

    #[test]
    fn sampling_tools_are_sent_as_commands_over_each_current_video_frame() {
        let reference = Arc::new(RgbaImage::from_pixel(24, 24, Rgba([255, 255, 255, 255])));
        let result = encode_artwork(
            Document {
                image: reference.clone(),
                annotations: vec![annotation("Pixelate")],
                geometry: Default::default(),
            },
            false,
        )
        .unwrap();
        assert!(result.composite);
        let mut current =
            RgbaImage::from_fn(24, 24, |x, y| Rgba([(x * 9) as u8, (y * 11) as u8, 0, 255]));
        let original = current.clone();
        let pixels = current.as_mut();
        assert!(apply_artwork(
            result.bytes.as_ptr(),
            result.bytes.len(),
            pixels.as_mut_ptr(),
            pixels.len(),
            24,
            24
        ));
        assert_eq!(current.get_pixel(20, 21), original.get_pixel(20, 21));
        assert_ne!(current.get_pixel(5, 4), original.get_pixel(5, 4));
        assert_ne!(current.get_pixel(5, 4), reference.get_pixel(5, 4));
    }
}
