//! The capture selector owns webcam placement. The recorder owns the file
//! and native streams until finalization completes.
use super::*;
#[path = "video/options.rs"]
mod options;
pub(crate) use options::{VideoPreferences, VideoTarget, preferences_ui};
#[path = "video/setup.rs"]
mod setup;
pub(super) use setup::{VideoPanel, show_video_panel};
#[cfg(target_os = "windows")]
#[path = "video/windows.rs"]
mod windows;
#[cfg(target_os = "macos")]
use std::{
    ffi::{CStr, CString, c_char, c_void},
    hash::{BuildHasher, RandomState},
    os::unix::ffi::OsStrExt,
    ptr::NonNull,
};
use std::{path::Path, time::Instant};

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum Webcam {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
    Free,
    Random,
}

#[cfg(target_os = "macos")]
impl Webcam {
    const ALL: [Self; 6] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
        Self::Free,
        Self::Random,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "Top left",
            Self::TopRight => "Top right",
            Self::BottomLeft => "Bottom left",
            Self::BottomRight => "Bottom right",
            Self::Free => "Free position",
            Self::Random => "Random position",
        }
    }

    fn position(self, free: Vec2, random: Vec2) -> Vec2 {
        match self {
            Self::TopLeft => Vec2::ZERO,
            Self::TopRight => Vec2::X,
            Self::BottomLeft => Vec2::Y,
            Self::BottomRight => Vec2::splat(1.),
            Self::Free => free,
            Self::Random => random,
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BubbleSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[cfg(target_os = "macos")]
impl BubbleSize {
    fn ratio(self) -> f32 {
        match self {
            Self::Small => 0.16,
            Self::Medium => 0.22,
            Self::Large => 0.30,
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Default)]
struct CameraFraming {
    size: BubbleSize,
    mirrored: bool,
}

#[cfg(target_os = "macos")]
pub(super) struct WebcamSetup {
    pub enabled: bool,
    pub mode: Webcam,
    framing: CameraFraming,
    free: Vec2,
    random: Vec2,
    drag_offset: Option<Vec2>,
    preview: Option<WebcamPreview>,
    pub preview_error: Option<String>,
    preview_texture: Option<egui::TextureHandle>,
    preview_pixels: Vec<u8>,
    preview_refresh: Option<Instant>,
}

#[cfg(target_os = "macos")]
impl Default for WebcamSetup {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: Webcam::default(),
            framing: CameraFraming::default(),
            free: Vec2::splat(0.5),
            random: Vec2::splat(0.5),
            drag_offset: None,
            preview: None,
            preview_error: None,
            preview_texture: None,
            preview_pixels: Vec::new(),
            preview_refresh: None,
        }
    }
}

#[cfg(target_os = "macos")]
impl WebcamSetup {
    fn choose_position(&mut self, mode: Webcam) {
        self.mode = mode;
        self.drag_offset = None;
        if mode == Webcam::Random {
            let bits = RandomState::new().hash_one("bediz-webcam-position");
            self.random =
                Vec2::new(bits as u32 as f32, (bits >> 32) as u32 as f32) / u32::MAX as f32;
        }
    }

    fn bubble_geometry(&self, rect: Rect) -> (Pos2, Vec2, f32) {
        // Match Recorder.swift; x and y express travel inside the selected
        // frame, with the circle kept clear of its edges at all positions.
        let smaller = rect.width().min(rect.height());
        let diameter = smaller * self.framing.size.ratio();
        let inset = smaller * 0.035;
        (
            rect.min + Vec2::splat(inset + diameter * 0.5),
            rect.size() - Vec2::splat(2. * inset + diameter),
            diameter * 0.5,
        )
    }

    pub fn drag(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        bar: Rect,
        events: &mut Vec<egui::Event>,
    ) {
        if !self.enabled || self.mode != Webcam::Free {
            return;
        }
        let (origin, travel, radius) = self.bubble_geometry(rect);
        let bubble = origin + travel * self.free;
        if ctx
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|p| p.distance(bubble) <= radius && !bar.contains(p))
            || self.drag_offset.is_some()
        {
            ctx.set_cursor_icon(if self.drag_offset.is_some() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
        events.retain(|event| match *event {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                ..
            } if pos.distance(bubble) <= radius && !bar.contains(pos) => {
                self.drag_offset = Some(bubble - pos);
                false
            }
            egui::Event::PointerMoved(pos) => {
                let Some(offset) = self.drag_offset else {
                    return true;
                };
                self.free = ((pos + offset - origin) / travel).clamp(Vec2::ZERO, Vec2::splat(1.));
                false
            }
            egui::Event::PointerButton {
                button: egui::PointerButton::Primary,
                pressed: false,
                ..
            }
            | egui::Event::PointerGone
            | egui::Event::WindowFocused(false)
                if self.drag_offset.take().is_some() =>
            {
                false
            }
            _ => true,
        });
    }

    pub fn paint(&self, painter: &egui::Painter, rect: Rect) {
        if !self.enabled {
            return;
        }
        let position = self.native_position();
        let (origin, travel, radius) = self.bubble_geometry(rect);
        let center = origin + travel * position;
        let palette = appearance::palette(painter.ctx());
        self.paint_circle(
            painter,
            Rect::from_center_size(center, Vec2::splat(radius * 2.)),
            palette.navy,
            Color32::from_white_alpha(190),
        );
    }

    fn paint_circle(&self, painter: &egui::Painter, rect: Rect, fill: Color32, border: Color32) {
        let center = rect.center();
        let radius = rect.width() * 0.5;
        if let Some(texture) = &self.preview_texture {
            let mut mesh = egui::Mesh::with_texture(texture.id());
            // Match the circular crop in the encoded MP4.
            mesh.vertices.push(egui::epaint::Vertex {
                pos: center,
                uv: egui::pos2(0.5, 0.5),
                color: Color32::WHITE,
            });
            for i in 0..=48 {
                let offset = Vec2::angled(std::f32::consts::TAU * i as f32 / 48.);
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: center + offset * radius,
                    uv: egui::pos2(0.5, 0.5) + offset * 0.5,
                    color: Color32::WHITE,
                });
                if i < 48 {
                    mesh.add_triangle(0, i + 1, i + 2);
                }
            }
            painter.add(egui::Shape::mesh(mesh));
        } else {
            painter.circle_filled(center, radius, fill);
            appearance::icon(
                painter,
                center,
                Icon::Video,
                border,
                (radius * 1.12).min(28.),
            );
        }
        painter.circle_stroke(center, radius, Stroke::new(1.4, border));
    }

    fn native_position(&self) -> Vec2 {
        self.mode.position(self.free, self.random)
    }

    pub fn update_preview(&mut self, ctx: &egui::Context, active: bool, demo: bool) {
        if !self.enabled || !active || demo {
            self.preview = None;
            self.preview_error = None;
            self.preview_texture = None;
            self.preview_refresh = None;
            return;
        }
        let preview = self.preview.get_or_insert_with(WebcamPreview::new);
        let (status, message) = preview.status();
        self.preview_error = (status < 0).then_some(message);
        let delay = Duration::from_millis(if status < 0 { 500 } else { 80 });
        ctx.request_repaint_after_for(delay, egui::ViewportId::ROOT);
        if status > 0 {
            self.refresh_preview_texture(ctx);
        } else if status < 0 {
            self.preview_texture = None;
        }
    }
}

#[cfg(target_os = "macos")]
struct WebcamPreview(NonNull<c_void>);

#[cfg(target_os = "macos")]
impl WebcamPreview {
    fn new() -> Self {
        // SAFETY: Native preview creation and use are on the AppKit main thread.
        Self(NonNull::new(unsafe { bediz_webcam_create() }).expect("Retained Swift preview"))
    }

    fn show(&self, target: Rect, position: Vec2, window: Option<u32>, framing: CameraFraming) {
        // SAFETY: The native preview owns one retained reference until Drop.
        unsafe {
            bediz_webcam_show(
                self.0.as_ptr(),
                f64::from(target.min.x),
                f64::from(target.min.y),
                f64::from(target.width()),
                f64::from(target.height()),
                f64::from(position.x),
                f64::from(position.y),
                window.unwrap_or(0),
                f64::from(framing.size.ratio()),
                i32::from(framing.mirrored),
            );
        }
    }

    fn status(&self) -> (i32, String) {
        let mut message = [0 as c_char; 512];
        // SAFETY: The probe writes a terminated string within this buffer.
        let status =
            unsafe { bediz_webcam_status(self.0.as_ptr(), message.as_mut_ptr(), message.len()) };
        let message = unsafe { CStr::from_ptr(message.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        (status, message)
    }
}

#[cfg(target_os = "macos")]
impl Drop for WebcamPreview {
    fn drop(&mut self) {
        // SAFETY: Our owned preview is always closed on the AppKit main thread.
        unsafe { bediz_webcam_release(self.0.as_ptr()) };
    }
}

#[cfg(target_os = "macos")]
struct LiveWebcam {
    preview: WebcamPreview,
    target: Rect,
    position: Vec2,
    window: Option<u32>,
    framing: CameraFraming,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn bediz_webcam_create() -> *mut c_void;
    fn bediz_webcam_show(
        handle: *const c_void,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        camera_x: f64,
        camera_y: f64,
        window: u32,
        size_ratio: f64,
        mirrored: i32,
    );
    fn bediz_webcam_status(handle: *const c_void, message: *mut c_char, capacity: usize) -> i32;
    fn bediz_webcam_copy_preview(
        handle: *const c_void,
        rgba: *mut u8,
        capacity: usize,
        side: usize,
        mirrored: i32,
    ) -> i32;
    fn bediz_webcam_release(handle: *const c_void);
    fn bediz_record_start(
        path: *const c_char,
        display: u32,
        window: u32,
        cursor: i32,
        webcam: *const c_void,
        camera_x: f64,
        camera_y: f64,
        camera_size: f64,
        camera_mirrored: i32,
        output_width: u32,
        output_height: u32,
        bits_per_second: u32,
        max_seconds: u32,
        disk_reserve: u64,
        audio_source: u32,
        audio_sample_rate: u32,
        audio_bits_per_second: u32,
        voice_processing: i32,
        countdown_seconds: u32,
    ) -> *mut c_void;
    fn bediz_record_status(handle: *mut c_void, out: *mut c_char, capacity: usize) -> i32;
    fn bediz_record_command(handle: *mut c_void, command: i32);
    fn bediz_record_release(handle: *mut c_void);
    fn bediz_play_video(
        path: *const c_char,
        render_artwork: extern "C" fn(*const c_char, *const c_char) -> bool,
        edit: bool,
    ) -> bool;
    fn bediz_video_layout(host: *mut c_void, x: f64, y: f64, w: f64, h: f64, pause: bool);
    fn bediz_video_save_project();
    fn bediz_video_prepare_to_quit() -> bool;
    fn bediz_record_thumbnail(
        movie: *const c_char,
        poster: *const c_char,
        width: isize,
        height: isize,
        dimensions: *mut u32,
    ) -> i32;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Starting,
    Countdown,
    Recording,
    Paused,
    Saving,
    Finished,
    Canceled,
    Failed,
}

#[repr(i32)]
#[derive(Clone, Copy)]
enum Command {
    Pause = 1,
    Resume = 2,
    Stop = 3,
}

pub(super) struct Recording {
    #[cfg(target_os = "macos")]
    native: NonNull<c_void>,
    #[cfg(target_os = "windows")]
    native: windows::Recorder,
    staging: tempfile::TempDir,
    destination: PathBuf,
    position: Pos2,
    return_hidden: bool,
    placement_frames: u8,
    active_started: Option<Instant>,
    elapsed: Duration,
    stopping: bool,
    #[cfg(target_os = "macos")]
    webcam: Option<LiveWebcam>,
}

#[cfg(target_os = "macos")]
impl Drop for Recording {
    fn drop(&mut self) {
        // SAFETY: The pointer owns one retained Swift recorder. Its native
        // queues keep themselves alive while finishing, independently of Rust.
        unsafe { bediz_record_release(self.native.as_ptr()) };
    }
}

impl Recording {
    fn new(
        settings: &Settings,
        screen: &mut Selection,
        region: selection::PixelRect,
        window: Option<u32>,
        position: Pos2,
    ) -> Result<Self, String> {
        if region.width < 16 || region.height < 16 {
            return Err("Choose a window or display at least 16 × 16 pixels to record.".into());
        }
        let root = settings.output_dir.join("Recordings");
        options::check_start_space(&root)?;
        let plan = settings.video.plan(region.width, region.height);
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("Could not create the Recordings folder: {error}"))?;
        let staging = tempfile::Builder::new()
            .prefix(".bediz-recording-")
            .tempdir_in(&root)
            .map_err(|error| format!("Could not prepare a temporary recording: {error}"))?;
        let destination = root.join(format!(
            "Recording {}.mp4",
            Local::now().format("%Y-%m-%d %H-%M-%S-%3f")
        ));
        #[cfg(target_os = "macos")]
        let (native, webcam) = {
            let path = CString::new(staging.path().join("recording.mp4").as_os_str().as_bytes())
                .map_err(|_| "The recording path contains an invalid character.")?;
            let camera = screen.webcam.native_position();
            let display = &screen.display;
            let framing = screen.webcam.framing;
            let webcam = screen.webcam.enabled.then(|| LiveWebcam {
                preview: screen
                    .webcam
                    .preview
                    .take()
                    .unwrap_or_else(WebcamPreview::new),
                target: Rect::from_min_size(
                    egui::pos2(display.x as f32, display.y as f32),
                    egui::vec2(display.width as f32, display.height as f32),
                ),
                position: camera,
                window,
                framing,
            });
            let webcam_ptr = webcam.as_ref().map_or(std::ptr::null(), |webcam| {
                webcam.preview.0.as_ptr().cast_const()
            });
            // SAFETY: Swift copies the UTF-8 path while this call is active, and
            // returns a retained handle for the UI to poll and release.
            let native = unsafe {
                bediz_record_start(
                    path.as_ptr(),
                    screen.display.id,
                    window.unwrap_or(0),
                    i32::from(settings.include_pointer),
                    webcam_ptr,
                    f64::from(camera.x),
                    f64::from(camera.y),
                    f64::from(framing.size.ratio()),
                    i32::from(framing.mirrored),
                    plan.width,
                    plan.height,
                    plan.bits_per_second,
                    plan.max_seconds,
                    options::DISK_RESERVE,
                    plan.audio_source,
                    plan.audio_sample_rate,
                    plan.audio_bits_per_second,
                    i32::from(plan.voice_processing),
                    plan.countdown_seconds,
                )
            };
            let native = NonNull::new(native).ok_or("The native recorder could not be started.")?;
            (native, webcam)
        };
        #[cfg(target_os = "windows")]
        let native = windows::Recorder::start(
            staging.path().join("recording.mp4"),
            &screen.display,
            window,
            settings.include_pointer,
            plan,
        )?;
        Ok(Self {
            native,
            staging,
            destination,
            position,
            return_hidden: screen.return_hidden,
            placement_frames: 0,
            active_started: None,
            elapsed: Duration::ZERO,
            stopping: false,
            #[cfg(target_os = "macos")]
            webcam,
        })
    }

    #[cfg(target_os = "macos")]
    fn status(&self) -> (Status, String) {
        let mut message = [0 as c_char; 1024];
        // SAFETY: The recorder is retained, and the caller owns this buffer.
        let status = unsafe {
            bediz_record_status(self.native.as_ptr(), message.as_mut_ptr(), message.len())
        };
        let status = match status {
            0 => Status::Starting,
            1 => Status::Recording,
            2 => Status::Paused,
            3 => Status::Saving,
            4 => Status::Finished,
            5 => Status::Canceled,
            6 => Status::Countdown,
            _ => Status::Failed,
        };
        // The zeroed buffer and the Swift bridge both guarantee termination.
        let message = unsafe { CStr::from_ptr(message.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        (status, message)
    }

    #[cfg(target_os = "windows")]
    fn status(&self) -> (Status, String) {
        self.native.status()
    }

    fn control(&mut self, command: Command) {
        if self.stopping {
            return;
        }
        self.stopping = matches!(command, Command::Stop);
        // SAFETY: The recorder enqueues the command before returning.
        #[cfg(target_os = "macos")]
        unsafe {
            bediz_record_command(self.native.as_ptr(), command as i32)
        };
        #[cfg(target_os = "windows")]
        self.native.control(command);
    }

    fn duration(&mut self, status: Status) -> Duration {
        if status == Status::Recording && !self.stopping {
            self.elapsed
                + self
                    .active_started
                    .get_or_insert_with(Instant::now)
                    .elapsed()
        } else {
            if let Some(started) = self.active_started.take() {
                self.elapsed += started.elapsed();
            }
            self.elapsed
        }
    }

    fn publish(&self) -> Result<(), String> {
        // Publish a finalized MP4 atomically and never replace an existing recording.
        let source = self.staging.path().join("recording.mp4");
        #[cfg(target_os = "macos")]
        let result = std::fs::hard_link(source, &self.destination);
        #[cfg(target_os = "windows")]
        let result = windows::publish(&source, &self.destination);
        result.map_err(|error| format!("Could not save {}: {error}", self.destination.display()))
    }

    pub fn focus(&mut self, ctx: &egui::Context) {
        // Keep the current Space even if the active app is in native full screen.
        #[cfg(target_os = "macos")]
        crate::capture_window::reveal_after_paint(&title());
        #[cfg(target_os = "windows")]
        ctx.send_viewport_cmd_to(viewport(), ViewportCommand::Focus);
        crate::runtime::wake_root(ctx);
    }
}

fn title() -> String {
    format!("{} Capture — Recording", crate::branding::NAME)
}

fn viewport() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("bediz-recording-controller")
}

pub(super) fn window_size(screen: &Selection, id: u32) -> Option<[u32; 2]> {
    let window = screen.windows.iter().find(|w| w.id == id)?;
    #[cfg(target_os = "macos")]
    let scale = f64::from(screen.image.width()) / f64::from(screen.display.width);
    #[cfg(target_os = "windows")]
    let scale = 1.;
    Some([
        (f64::from(window.width) * scale).round() as u32,
        (f64::from(window.height) * scale).round() as u32,
    ])
}

impl MyImageApp {
    pub(super) fn start_window_recording(
        &mut self,
        ctx: &egui::Context,
        screen: &mut Selection,
        id: u32,
    ) {
        if let Some([width, height]) = window_size(screen, id) {
            let region = selection::PixelRect {
                x: 0,
                y: 0,
                width,
                height,
            };
            self.start_recording(ctx, screen, region, Some(id));
        } else {
            self.error = Some("The window chosen for recording is no longer available.".into());
            self.restore_window(ctx, false);
        }
    }

    pub(super) fn start_recording(
        &mut self,
        ctx: &egui::Context,
        screen: &mut Selection,
        region: selection::PixelRect,
        window: Option<u32>,
    ) {
        let display = &screen.display;
        let scale = ctx.zoom_factor()
            * if cfg!(target_os = "windows") {
                display.scale_factor
            } else {
                1.
            };
        let w = display.width as f32 / scale;
        let h = display.height as f32 / scale;
        let selection_bottom = (region.y + region.height) as f32 / screen.image.height() as f32 * h;
        let y = if selection_bottom + 68. <= h {
            selection_bottom + 8.
        } else {
            (h - 82.).max(32.)
        };
        let result = Recording::new(
            &self.settings,
            screen,
            region,
            window,
            Pos2::new(
                display.x as f32 / scale + ((w - 230.) / 2.).max(8.),
                display.y as f32 / scale + y,
            ),
        );
        match result {
            Ok(recording) => {
                self.recording = Some(recording);
                crate::runtime::wake_root(ctx);
            }
            Err(error) => {
                self.error = Some(error);
                self.restore_window(ctx, false);
            }
        }
    }

    pub(super) fn recording_ui(&mut self, ctx: &egui::Context) {
        let Some(recording) = &mut self.recording else {
            return;
        };
        let (status, message) = recording.status();
        if matches!(status, Status::Finished | Status::Failed | Status::Canceled) {
            let recording = self.recording.take().unwrap();
            crate::capture_window::forget_overlay_reveals();
            ctx.send_viewport_cmd_to(viewport(), ViewportCommand::Close);
            match status {
                Status::Finished => match recording.publish() {
                    Ok(()) => {
                        self.remember_recent(recording.destination.clone());
                        self.status = format!(
                            "Saved {}",
                            recording
                                .destination
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                        );
                        if !message.is_empty() {
                            self.status.push_str(" · ");
                            self.status.push_str(&message);
                        }
                        self.restore_window(ctx, false);
                        self.open_video_path(ctx, recording.destination.clone());
                    }
                    Err(error) => {
                        let staging = recording.staging.path().to_owned();
                        // Keep the finished MP4 available if the final publish
                        // failed (e.g. the destination volume became read-only).
                        let mut recording = recording;
                        recording.staging.disable_cleanup(true);
                        self.error = Some(format!(
                            "{error}. The finished video is at {}.",
                            staging.join("recording.mp4").display()
                        ));
                        self.restore_window(ctx, false);
                    }
                },
                Status::Failed => {
                    self.error = Some(message);
                    self.restore_window(ctx, false);
                }
                _ => {
                    self.status = "Recording canceled".into();
                    self.restore_window(ctx, recording.return_hidden);
                }
            }
            return;
        }
        let placing = recording.placement_frames < 3;
        let first_reveal = recording.placement_frames == 3;
        #[cfg(target_os = "macos")]
        if recording.stopping || status == Status::Saving {
            recording.webcam = None;
        } else if let Some(webcam) = &recording.webcam {
            webcam.preview.show(
                webcam.target,
                webcam.position,
                webcam.window,
                webcam.framing,
            );
        }
        let time = recording.duration(status).as_secs();
        let look = self.settings.capture_theme.look();
        let title = title();
        crate::capture_window::prepare();
        ctx.show_viewport_immediate(
            viewport(),
            egui::ViewportBuilder::default()
                .with_title(&title)
                .with_position(recording.position)
                .with_inner_size([230., 48.])
                .with_resizable(false)
                .with_taskbar(false)
                .with_decorations(false)
                .with_has_shadow(false)
                .with_transparent(cfg!(target_os = "macos"))
                .with_window_level(if cfg!(target_os = "windows") {
                    WindowLevel::AlwaysOnTop
                } else {
                    WindowLevel::Normal
                })
                .with_active(false)
                .with_visible(cfg!(target_os = "windows") && !placing),
            |child, _| {
                if child.input(|i| {
                    i.viewport().close_requested()
                        || (matches!(status, Status::Starting | Status::Countdown)
                            && i.key_pressed(egui::Key::Escape))
                }) {
                    child.send_viewport_cmd(ViewportCommand::CancelClose);
                    recording.control(Command::Stop);
                }
                if placing {
                    child.send_viewport_cmd(ViewportCommand::OuterPosition(recording.position));
                    #[cfg(target_os = "windows")]
                    if !self.demo {
                        crate::capture_window::win32::exclude_from_recording(&title);
                    }
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(child, |ui| {
                        egui::Frame::new()
                            .fill(look.palette.paper)
                            .stroke(Stroke::new(1., look.palette.border))
                            .corner_radius(10.)
                            .inner_margin(egui::Margin::symmetric(9, 6))
                            .show(ui, |ui| {
                                crate::capture_panel::apply_look(ui, &look);
                                ui.set_min_width(210.);
                                ui.horizontal(|ui| {
                                    let live = status == Status::Recording && !recording.stopping;
                                    let red = Color32::from_rgb(231, 56, 59);
                                    let (dot, _) = ui.allocate_exact_size(
                                        Vec2::splat(12.),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().circle_filled(
                                        dot.center(),
                                        4.,
                                        if live { red } else { look.palette.muted },
                                    );
                                    let message = if recording.stopping || status == Status::Saving
                                    {
                                        "Saving…".into()
                                    } else if status == Status::Countdown {
                                        message.clone()
                                    } else if status == Status::Starting {
                                        "Starting…".into()
                                    } else {
                                        format!(
                                            "{:02}:{:02}:{:02}",
                                            time / 3600,
                                            time / 60 % 60,
                                            time % 60
                                        )
                                    };
                                    ui.add_sized(
                                        [93., 30.],
                                        egui::Label::new(
                                            egui::RichText::new(message)
                                                .monospace()
                                                .color(look.palette.ink),
                                        ),
                                    );
                                    let pause = status == Status::Paused;
                                    ui.add_enabled_ui(
                                        matches!(status, Status::Recording | Status::Paused)
                                            && !recording.stopping,
                                        |ui| {
                                            if appearance::icon_button(
                                                ui,
                                                if pause {
                                                    "Resume recording"
                                                } else {
                                                    "Pause recording"
                                                },
                                                if pause { Icon::Play } else { Icon::Pause },
                                            )
                                            .clicked()
                                            {
                                                recording.control(if pause {
                                                    Command::Resume
                                                } else {
                                                    Command::Pause
                                                });
                                            }
                                        },
                                    );
                                    ui.add_enabled_ui(
                                        status != Status::Saving && !recording.stopping,
                                        |ui| {
                                            if appearance::icon_button(
                                                ui,
                                                if matches!(
                                                    status,
                                                    Status::Starting | Status::Countdown
                                                ) {
                                                    "Cancel recording"
                                                } else {
                                                    "Stop and save recording"
                                                },
                                                Icon::Stop,
                                            )
                                            .clicked()
                                            {
                                                recording.control(Command::Stop);
                                            }
                                        },
                                    );
                                });
                            });
                    });
                if first_reveal {
                    #[cfg(target_os = "macos")]
                    crate::capture_window::reveal_after_paint(&title);
                    #[cfg(target_os = "windows")]
                    child.send_viewport_cmd(ViewportCommand::Visible(true));
                }
            },
        );
        recording.placement_frames = recording.placement_frames.saturating_add(1);
        if placing {
            crate::runtime::wake_root(ctx);
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

pub(super) fn is_video_path(path: &Path) -> bool {
    crate::library::is_recording(path)
        || path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bedizvideo"))
}

#[cfg(target_os = "macos")]
pub(super) fn play(path: &Path, edit: bool) -> Result<bool, String> {
    if !is_video_path(path) {
        return Err("Choose an MP4, MOV or Bediz video project.".into());
    }
    if !path.is_file() {
        return Err(format!(
            "This recording is no longer available at {}.",
            path.display()
        ));
    }
    let native_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| "The video path contains an invalid character.")?;
    // SAFETY: This runs on AppKit's main thread. Swift copies the path synchronously.
    Ok(unsafe { bediz_play_video(native_path.as_ptr(), render_artwork, edit) })
}

#[cfg(target_os = "macos")]
pub(super) fn layout(frame: &eframe::Frame, ctx: &egui::Context, rect: Option<Rect>, pause: bool) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let bounds = rect.and_then(|rect| {
        let handle = frame.window_handle().ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        let scale = ctx.pixels_per_point() / ctx.native_pixels_per_point().unwrap_or(1.);
        Some((handle.ns_view.as_ptr(), rect * scale))
    });
    // SAFETY: The raw handle belongs to eframe's live AppKit view. The call
    // runs on the main thread and AppKit owns the attached player subview.
    unsafe {
        match bounds {
            Some((host, rect)) => bediz_video_layout(
                host,
                f64::from(rect.min.x),
                f64::from(rect.min.y),
                f64::from(rect.width()),
                f64::from(rect.height()),
                pause,
            ),
            None => bediz_video_layout(std::ptr::null_mut(), 0., 0., 0., 0., pause),
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) fn save_project() {
    // SAFETY: Called from the root AppKit event loop with its player attached.
    unsafe { bediz_video_save_project() };
}

#[cfg(target_os = "windows")]
pub(super) use windows::{
    Player as WindowsPlayer, publish as publish_video, render_mp4, video_frame,
};

#[cfg(target_os = "windows")]
impl MyImageApp {
    pub(super) fn layout_windows_video(
        &mut self,
        ctx: &egui::Context,
        frame: &eframe::Frame,
        bounds: Option<Rect>,
    ) {
        use ::windows::Win32::Foundation::HWND;
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        if bounds.is_some()
            && self.video_open_pending
            && let Ok(handle) = frame.window_handle()
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
            && let Some(path) = &self.video_path
        {
            self.video_open_pending = false;
            let path = self.windows_timeline.player_file(path);
            match WindowsPlayer::open(HWND(handle.hwnd.get() as *mut _), &path, ctx) {
                Ok(player) => self.video_player = Some(player),
                Err(error) => {
                    self.error = Some(format!("Could not open this video in Bediz: {error}"))
                }
            }
            ctx.request_repaint();
        }
        if let Some(player) = &mut self.video_player {
            player.layout(bounds, ctx.pixels_per_point(), self.video_bounds.is_none());
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn thumbnail(
    path: &Path,
    width: u32,
    height: u32,
) -> Result<crate::library::Thumbnail, String> {
    windows::thumbnail(path, width, height)
}

#[cfg(target_os = "macos")]
pub(super) fn prepare_to_quit() -> bool {
    // SAFETY: The root app invokes this on AppKit's main thread.
    unsafe { bediz_video_prepare_to_quit() }
}

#[cfg(target_os = "macos")]
extern "C" fn render_artwork(input: *const c_char, output: *const c_char) -> bool {
    if input.is_null() || output.is_null() {
        return false;
    }
    // SAFETY: Swift keeps both null-terminated path strings alive for the call.
    let input = Path::new(std::ffi::OsStr::from_bytes(unsafe {
        CStr::from_ptr(input).to_bytes()
    }));
    let output = Path::new(std::ffi::OsStr::from_bytes(unsafe {
        CStr::from_ptr(output).to_bytes()
    }));
    crate::document::load_project(input)
        .and_then(|document| crate::render::flatten(&document))
        .is_ok_and(|artwork| {
            artwork
                .save_with_format(output, image::ImageFormat::Png)
                .is_ok()
        })
}

#[cfg(target_os = "macos")]
pub(crate) fn thumbnail(
    path: &Path,
    width: u32,
    height: u32,
) -> Result<crate::library::Thumbnail, String> {
    let staging = tempfile::tempdir().map_err(|error| error.to_string())?;
    let poster = staging.path().join("preview.jpg");
    let input = CString::new(path.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    let output = CString::new(poster.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    let mut original_size = [0; 2];
    // SAFETY: Both paths and the two writable dimensions live through this synchronous call.
    let decoded = unsafe {
        bediz_record_thumbnail(
            input.as_ptr(),
            output.as_ptr(),
            width as isize,
            height as isize,
            original_size.as_mut_ptr(),
        )
    };
    if decoded == 0 {
        return Err("Could not read the recording preview.".into());
    }
    let image = image::open(poster)
        .map(image::DynamicImage::into_rgba8)
        .map_err(|error| error.to_string())?;
    Ok(crate::library::Thumbnail {
        image,
        original_size,
    })
}

#[cfg(target_os = "macos")]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webcam_corner_presets_and_custom_position_stay_inside_the_capture() {
        let bounds = Rect::from_min_size(Pos2::new(50., 180.), Vec2::new(420., 780.));
        let free = Vec2::new(0.37, 0.81);
        let random = Vec2::new(0.55, 0.14);
        let mut setup = WebcamSetup::default();
        assert!(setup.enabled);
        for size in [BubbleSize::Small, BubbleSize::Medium, BubbleSize::Large] {
            setup.framing.size = size;
            let (origin, travel, radius) = setup.bubble_geometry(bounds);
            assert!((radius * 2. - bounds.width() * size.ratio()).abs() < 0.001);
            for (mode, anchor) in [
                (Webcam::TopLeft, Vec2::ZERO),
                (Webcam::TopRight, Vec2::X),
                (Webcam::BottomLeft, Vec2::Y),
                (Webcam::BottomRight, Vec2::splat(1.)),
                (Webcam::Free, free),
                (Webcam::Random, random),
            ] {
                assert_eq!(mode.position(free, random), anchor);
                let bubble =
                    Rect::from_center_size(origin + travel * anchor, Vec2::splat(radius * 2.));
                assert!(bounds.contains_rect(bubble));
            }
        }
    }

    #[test]
    fn dragging_the_webcam_consumes_pointer_events_and_clamps_to_the_region() {
        let ctx = egui::Context::default();
        let mut setup = WebcamSetup {
            enabled: true,
            mode: Webcam::Free,
            ..WebcamSetup::default()
        };
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(500., 300.));
        let (origin, travel, _) = setup.bubble_geometry(rect);
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let start = origin + travel * setup.free;
        let outside = Rect::from_min_size(rect.right_bottom(), Vec2::splat(40.));
        let mut events = vec![
            pointer(start, true),
            egui::Event::PointerMoved(Pos2::new(560., -18.)),
            pointer(Pos2::new(560., -18.), false),
        ];
        setup.drag(&ctx, rect, outside, &mut events);
        assert_eq!(setup.native_position(), Vec2::X);
        assert!(events.is_empty());
        assert!(setup.drag_offset.is_none());
    }
}
