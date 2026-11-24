use crate::{
    appearance::{self, Icon},
    capture_panel::CaptureLook,
};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MIB: u64 = 1024 * 1024;
pub(super) const DISK_RESERVE: u64 = 256 * MIB;
const MIN_START_SPACE: u64 = DISK_RESERVE + 16 * MIB;
const MAX_MINUTES: u32 = 240;
const MIN_FPS: f64 = 1.;
const MAX_FPS: f64 = 240.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

impl FrameRate {
    pub const DEFAULT: Self = Self {
        numerator: 30,
        denominator: 1,
    };

    fn new(fps: f64) -> Self {
        if !(MIN_FPS..=MAX_FPS).contains(&fps) {
            return Self::DEFAULT;
        }
        let numerator = (fps * 1_000.).round() as u32;
        let (mut a, mut b) = (numerator, 1_000);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Self {
            numerator: numerator / a,
            denominator: 1_000 / a,
        }
    }

    fn fps(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }

    fn label(self) -> String {
        format!("{} fps", self.fps())
    }

    #[cfg(any(target_os = "windows", test))]
    pub fn frame_time(self, frame: i64, ticks_per_second: i64) -> i64 {
        frame * ticks_per_second * i64::from(self.denominator) / i64::from(self.numerator)
    }

    #[cfg(any(target_os = "windows", test))]
    pub fn frame_at(self, ticks: i64, ticks_per_second: i64) -> i64 {
        ticks * i64::from(self.numerator) / (ticks_per_second * i64::from(self.denominator))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum Quality {
    Height240,
    Height360,
    Height480,
    Height640,
    Compact,
    #[default]
    Balanced,
    Cinema2K,
    Qhd,
    High,
}

impl Quality {
    const ALL: [Self; 9] = [
        Self::Height240,
        Self::Height360,
        Self::Height480,
        Self::Height640,
        Self::Compact,
        Self::Balanced,
        Self::Cinema2K,
        Self::Qhd,
        Self::High,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Height240 => "240p",
            Self::Height360 => "360p",
            Self::Height480 => "480p",
            Self::Height640 => "640p",
            Self::Compact => "720p",
            Self::Balanced => "1080p",
            Self::Cinema2K => "2K DCI",
            Self::Qhd => "1440p QHD",
            Self::High => "4K UHD",
        }
    }

    fn settings(self) -> (u32, u32, f64) {
        match self {
            Self::Height240 => (426, 240, 0.075),
            Self::Height360 => (640, 360, 0.075),
            Self::Height480 => (854, 480, 0.075),
            Self::Height640 => (1138, 640, 0.075),
            Self::Compact => (1280, 720, 0.075),
            Self::Balanced => (1920, 1080, 0.11),
            Self::Cinema2K => (2048, 1080, 0.12),
            Self::Qhd => (2560, 1440, 0.13),
            Self::High => (3840, 2160, 0.15),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum VideoTarget {
    #[default]
    #[serde(alias = "Region")]
    Window,
    FullScreen,
    MultipleWindows,
}

impl VideoTarget {
    pub(crate) fn mode(self) -> crate::capture::CaptureMode {
        use crate::capture::CaptureMode;
        match self {
            Self::Window | Self::MultipleWindows => CaptureMode::Window,
            Self::FullScreen => CaptureMode::FullScreen,
        }
    }

    pub(crate) fn ui(&mut self, ui: &mut egui::Ui, look: &CaptureLook) -> bool {
        appearance::segmented_with_palette(
            ui,
            "video-target",
            self,
            &[
                (
                    Self::Window,
                    "Window",
                    Some(Icon::View),
                    "Record a single window",
                ),
                (
                    Self::MultipleWindows,
                    "Two windows",
                    Some(Icon::Layers),
                    "Record two windows together in one video",
                ),
                (
                    Self::FullScreen,
                    "Full screen",
                    Some(Icon::Fit),
                    "Record this entire display",
                ),
            ],
            look.palette,
        )
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum AudioSource {
    #[default]
    Off,
    System,
    Microphone,
}

impl AudioSource {
    const ALL: [Self; 3] = [Self::Off, Self::System, Self::Microphone];

    fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::System => "System audio",
            Self::Microphone => "Microphone",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum AudioQuality {
    Compact,
    #[default]
    Standard,
    High,
}

impl AudioQuality {
    const ALL: [Self; 3] = [Self::Compact, Self::Standard, Self::High];

    fn rate(self, sample_rate: AudioSampleRate) -> u32 {
        let rates = sample_rate.bitrates();
        1_000 * rates[self as usize]
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum AudioSampleRate {
    Hz8000 = 8_000,
    Hz12000 = 12_000,
    Hz16000 = 16_000,
    Hz22050 = 22_050,
    Hz24000 = 24_000,
    Hz32000 = 32_000,
    Hz44100 = 44_100,
    #[default]
    Hz48000 = 48_000,
}

impl AudioSampleRate {
    #[cfg(target_os = "macos")]
    const ALL: [Self; 8] = [
        Self::Hz8000,
        Self::Hz12000,
        Self::Hz16000,
        Self::Hz22050,
        Self::Hz24000,
        Self::Hz32000,
        Self::Hz44100,
        Self::Hz48000,
    ];

    #[cfg(target_os = "windows")]
    const ALL: [Self; 2] = [Self::Hz44100, Self::Hz48000];

    #[cfg(target_os = "macos")]
    fn bitrates(self) -> [u32; 3] {
        match self {
            Self::Hz8000 => [16, 24, 32],
            Self::Hz12000 => [24, 32, 48],
            Self::Hz16000 => [32, 48, 64],
            Self::Hz22050 | Self::Hz24000 | Self::Hz32000 => [48, 64, 96],
            Self::Hz44100 | Self::Hz48000 => [64, 96, 128],
        }
    }

    #[cfg(target_os = "windows")]
    fn bitrates(self) -> [u32; 3] {
        [96, 128, 160]
    }

    fn normalized(self) -> Self {
        if Self::ALL.contains(&self) {
            self
        } else {
            Self::default()
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Hz8000 => "8 kHz",
            Self::Hz12000 => "12 kHz",
            Self::Hz16000 => "16 kHz",
            Self::Hz22050 => "22.05 kHz",
            Self::Hz24000 => "24 kHz",
            Self::Hz32000 => "32 kHz",
            Self::Hz44100 => "44.1 kHz",
            Self::Hz48000 => "48 kHz",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct VideoPreferences {
    pub target: VideoTarget,
    #[cfg(target_os = "macos")]
    pub presenter_camera: bool,
    quality: Quality,
    frame_rate: f64,
    max_minutes: u32,
    audio_source: AudioSource,
    audio_quality: AudioQuality,
    audio_sample_rate: AudioSampleRate,
    #[cfg(target_os = "macos")]
    reduce_noise: bool,
    countdown_seconds: u32,
}

impl Default for VideoPreferences {
    fn default() -> Self {
        Self {
            target: VideoTarget::default(),
            #[cfg(target_os = "macos")]
            presenter_camera: true,
            quality: Quality::Balanced,
            frame_rate: FrameRate::DEFAULT.fps(),
            max_minutes: 30,
            audio_source: Default::default(),
            audio_quality: Default::default(),
            audio_sample_rate: Default::default(),
            #[cfg(target_os = "macos")]
            reduce_noise: false,
            countdown_seconds: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct RecordingPlan {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub bits_per_second: u32,
    pub fallback_bits_per_second: u32,
    pub max_seconds: u32,
    pub audio_source: u32,
    pub audio_sample_rate: u32,
    pub audio_bits_per_second: u32,
    #[cfg(target_os = "macos")]
    pub voice_processing: bool,
    pub countdown_seconds: u32,
}

impl VideoPreferences {
    pub(super) fn plan(self, width: u32, height: u32) -> RecordingPlan {
        let (long_edge, short_edge, bits_per_pixel) = self.quality.settings();
        let scale = (f64::from(long_edge) / f64::from(width.max(height)))
            .min(f64::from(short_edge) / f64::from(width.min(height)))
            .min(1.);
        let even = |pixels| ((f64::from(pixels) * scale).floor() as u32) & !1;
        let width = even(width).max(16);
        let height = even(height).max(16);
        let frame_rate = FrameRate::new(self.frame_rate);
        let bitrate = |rate: FrameRate| {
            (f64::from(width * height) * rate.fps() * bits_per_pixel)
                .round()
                .clamp(160_000., 32_000_000.) as u32
        };
        RecordingPlan {
            width,
            height,
            frame_rate,
            bits_per_second: bitrate(frame_rate),
            fallback_bits_per_second: bitrate(FrameRate::DEFAULT),
            max_seconds: self.max_minutes.clamp(1, MAX_MINUTES) * 60,
            audio_source: self.audio_source as u32,
            audio_sample_rate: self.audio_sample_rate.normalized() as u32,
            #[cfg(target_os = "macos")]
            voice_processing: self.audio_source == AudioSource::Microphone && self.reduce_noise,
            countdown_seconds: self.countdown_seconds.min(10),
            audio_bits_per_second: if self.audio_source == AudioSource::Off {
                0
            } else {
                self.audio_quality.rate(self.audio_sample_rate)
            },
        }
    }
}

impl RecordingPlan {
    #[cfg(target_os = "windows")]
    pub(super) fn fallback(self) -> Self {
        Self {
            frame_rate: FrameRate::DEFAULT,
            bits_per_second: self.fallback_bits_per_second,
            ..self
        }
    }

    fn bytes_per_second(&self) -> u64 {
        // Leave headroom for file metadata and variable-rate encoding.
        u64::from(self.bits_per_second + self.audio_bits_per_second) * 110 / 800
    }

    fn estimated_bytes(&self) -> u64 {
        self.bytes_per_second() * u64::from(self.max_seconds)
    }

    fn recommended_seconds(&self, free: u64) -> u64 {
        if free < MIN_START_SPACE {
            return 0;
        }
        free.saturating_sub(DISK_RESERVE) / self.bytes_per_second()
    }
}

pub(super) fn check_start_space(root: &Path) -> Result<(), String> {
    if available_space(root).is_some_and(|free| free < MIN_START_SPACE) {
        return Err("The recording drive has less than 272 MB free. Free some space or choose another save folder.".into());
    }
    Ok(())
}

pub(super) fn available_space(root: &Path) -> Option<u64> {
    disk_available(root.ancestors().find(|path| path.is_dir())?)
}

#[cfg(target_os = "macos")]
fn disk_available(path: &Path) -> Option<u64> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: The path is terminated and statvfs initializes stats on success.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return None;
    }
    let stats = unsafe { stats.assume_init() };
    Some(u64::from(stats.f_bavail).saturating_mul(stats.f_frsize))
}

#[cfg(target_os = "windows")]
fn disk_available(path: &Path) -> Option<u64> {
    use ::windows::{Win32::Storage::FileSystem::GetDiskFreeSpaceExW, core::PCWSTR};
    use std::os::windows::ffi::OsStrExt;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut bytes = 0;
    // SAFETY: The null-terminated path and output storage live for the call.
    unsafe { GetDiskFreeSpaceExW(PCWSTR(path.as_ptr()), Some(&mut bytes), None, None) }.ok()?;
    Some(bytes)
}

#[derive(Clone)]
struct DiskSample {
    root: PathBuf,
    time: Instant,
    free: Option<u64>,
}

fn sampled_space(ui: &egui::Ui, root: &Path) -> Option<u64> {
    let key = egui::Id::new("recording-available-space");
    if let Some(sample) = ui.ctx().data(|data| data.get_temp::<DiskSample>(key))
        && sample.root == root
        && sample.time.elapsed() < Duration::from_secs(3)
    {
        return sample.free;
    }
    let free = available_space(root);
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            key,
            DiskSample {
                root: root.to_owned(),
                time: Instant::now(),
                free,
            },
        );
    });
    free
}

fn space_label(bytes: u64) -> String {
    if bytes >= 1024 * MIB {
        format!("{:.1} GB", bytes as f64 / (1024 * MIB) as f64)
    } else {
        format!("{} MB", bytes.div_ceil(MIB))
    }
}

fn frame_rate_ui(ui: &mut egui::Ui, fps: &mut f64, look: &CaptureLook) -> bool {
    let previous = *fps;
    let rate = FrameRate::new(*fps);
    *fps = rate.fps();
    ui.horizontal(|ui| {
        heading(ui, look, Icon::Timer, "Frame rate");
        let edit_id = ui.id().with("custom-recording-frame-rate");
        let reset_edit = |ui: &egui::Ui| ui.data_mut(|data| data.remove::<String>(edit_id));
        egui::ComboBox::from_id_salt("recording-frame-rate")
            .selected_text(rate.label())
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show_ui(ui, |ui| {
                for preset in [15., 30., 60., 120.] {
                    let label = if preset == 30. { "30 fps (default)".into() } else { format!("{preset} fps") };
                    if ui.selectable_label(*fps == preset, label).clicked() {
                        *fps = preset;
                        reset_edit(ui);
                        ui.close();
                        return;
                    }
                }
                ui.separator();
                ui.label("Custom frame rate");
                ui.horizontal(|ui| {
                    let mut input = ui.data_mut(|data| data.get_temp::<String>(edit_id))
                        .unwrap_or_else(|| rate.fps().to_string());
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut input)
                            .hint_text("1–240 fps")
                            .desired_width(72.),
                    );
                    if ui.button("Apply").clicked()
                        || (field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)))
                    {
                        *fps = parse_frame_rate(&input);
                        reset_edit(ui);
                        ui.close();
                    } else if field.changed() {
                        ui.data_mut(|data| data.insert_temp(edit_id, input));
                    }
                });
            })
            .response
            .on_hover_text("Choose a preset or enter a custom rate from 1 to 240 fps. Falls back to 30 fps when unavailable.");
    });
    *fps != previous
}

fn parse_frame_rate(input: &str) -> f64 {
    let value = input.trim().replace(',', ".").parse().unwrap_or(f64::NAN);
    FrameRate::new(value).fps()
}

pub(crate) fn preferences_ui(
    ui: &mut egui::Ui,
    prefs: &mut VideoPreferences,
    root: &Path,
    region: Option<[u32; 2]>,
    look: &CaptureLook,
) -> bool {
    let quality = prefs.quality;
    ui.horizontal(|ui| {
        heading(ui, look, Icon::Video, "Resolution");
        egui::ComboBox::from_id_salt("recording-quality")
            .selected_text(prefs.quality.label())
            .show_ui(ui, |ui| {
                ui.set_min_width(124.);
                for choice in Quality::ALL {
                    let (width, height, _) = choice.settings();
                    ui.selectable_value(&mut prefs.quality, choice, choice.label())
                        .on_hover_text(format!(
                            "Up to {width} × {height}. Keeps the target’s aspect ratio."
                        ));
                }
            });
    });
    let mut changed = quality != prefs.quality;
    changed |= frame_rate_ui(ui, &mut prefs.frame_rate, look);
    ui.horizontal(|ui| {
        ui.label("Maximum recording time");
        changed |= ui
            .add(
                egui::DragValue::new(&mut prefs.max_minutes)
                    .range(1..=MAX_MINUTES)
                    .suffix(" min"),
            )
            .changed();
    });
    ui.separator();
    changed |= ui
        .horizontal_wrapped(|ui| {
            heading(ui, look, Icon::Microphone, "Audio");
            appearance::segmented_with_palette(
                ui,
                "recording-audio-source",
                &mut prefs.audio_source,
                &AudioSource::ALL.map(|source| (source, source.label(), None, "")),
                look.palette,
            )
        })
        .inner;
    if prefs.audio_source != AudioSource::Off {
        let old_sample_rate = prefs.audio_sample_rate;
        prefs.audio_sample_rate = old_sample_rate.normalized();
        let old_quality = prefs.audio_quality;
        ui.horizontal_wrapped(|ui| {
            ui.label("Sample rate");
            egui::ComboBox::from_id_salt("recording-audio-sample-rate")
                .selected_text(prefs.audio_sample_rate.label())
                .show_ui(ui, |ui| {
                    for sample_rate in AudioSampleRate::ALL {
                        ui.selectable_value(
                            &mut prefs.audio_sample_rate,
                            sample_rate,
                            sample_rate.label(),
                        );
                    }
                });
            ui.label("AAC bitrate");
            let label = |quality: AudioQuality| {
                format!("{} kbps", quality.rate(prefs.audio_sample_rate) / 1_000)
            };
            egui::ComboBox::from_id_salt("recording-audio-quality")
                .selected_text(label(prefs.audio_quality))
                .show_ui(ui, |ui| {
                    for quality in AudioQuality::ALL {
                        ui.selectable_value(&mut prefs.audio_quality, quality, label(quality));
                    }
                });
        });
        changed |= old_sample_rate != prefs.audio_sample_rate || old_quality != prefs.audio_quality;
    }
    #[cfg(target_os = "macos")]
    if prefs.audio_source == AudioSource::Microphone {
        changed |= ui
            .checkbox(&mut prefs.reduce_noise, "Reduce background noise")
            .on_hover_text("Use macOS voice processing for the microphone.")
            .changed();
    }
    ui.separator();
    changed |= ui
        .horizontal_wrapped(|ui| {
            heading(ui, look, Icon::Timer, "Countdown");
            appearance::segmented_with_palette(
                ui,
                "recording-countdown",
                &mut prefs.countdown_seconds,
                &[
                    (0, "Off", None, ""),
                    (3, "3 s", None, ""),
                    (5, "5 s", None, ""),
                    (10, "10 s", None, ""),
                ],
                look.palette,
            )
        })
        .inner;
    ui.separator();
    heading(ui, look, Icon::Save, "Recording size");
    let free = sampled_space(ui, root);
    if let Some([width, height]) = region {
        let plan = prefs.plan(width, height);
        ui.label(format!(
            "{} × {} · {} · about {} at {} min",
            plan.width,
            plan.height,
            plan.frame_rate.label(),
            space_label(plan.estimated_bytes()),
            plan.max_seconds / 60
        ));
        if let Some(free) = free {
            let recommended = plan.recommended_seconds(free);
            let lower_quality = Quality::ALL.into_iter().rev().find(|&quality| {
                quality < prefs.quality
                    && VideoPreferences { quality, ..*prefs }
                        .plan(width, height)
                        .recommended_seconds(free)
                        >= u64::from(plan.max_seconds)
            });
            if recommended < u64::from(plan.max_seconds) {
                let note = if let Some(quality) = lower_quality {
                    let alternative = VideoPreferences { quality, ..*prefs }.plan(width, height);
                    format!(
                        "Try {} for {} min (about {}).",
                        quality.label(),
                        alternative.max_seconds / 60,
                        space_label(alternative.estimated_bytes())
                    )
                } else if recommended < 60 {
                    "Free up space or choose another save drive.".into()
                } else {
                    format!(
                        "Set the limit to {} min or lower, or choose a lower quality.",
                        recommended / 60
                    )
                };
                ui.colored_label(egui::Color32::from_rgb(218, 105, 56), note);
            } else {
                ui.weak("There is space for the full recording estimate.");
            }
        }
    } else {
        ui.weak("Choose a recording target to estimate the MP4 size.");
    }
    match free {
        Some(bytes) => ui.weak(format!("{} available · 256 MB reserve", space_label(bytes))),
        None => ui.weak("Free space could not be checked on this drive."),
    }
    .on_hover_text(root.join("Recordings").display().to_string());
    changed
}

pub(super) fn heading(ui: &mut egui::Ui, look: &CaptureLook, icon: Icon, label: &str) {
    ui.horizontal(|ui| {
        let (tile, _) = ui.allocate_exact_size(egui::Vec2::splat(22.), egui::Sense::hover());
        ui.painter().rect_filled(tile, 6., look.palette.cream);
        appearance::icon(ui.painter(), tile.center(), icon, look.palette.ginger, 14.);
        ui.label(egui::RichText::new(label).strong());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_frame_rate_drives_recording_timing_bitrate_and_disk_estimate() {
        let defaults = VideoPreferences::default();
        let baseline = defaults.plan(1280, 720);
        let fast = VideoPreferences {
            frame_rate: 120.,
            ..defaults
        }
        .plan(1280, 720);
        assert_eq!(fast.frame_rate.numerator, 120);
        assert_eq!(fast.frame_rate.denominator, 1);
        assert_eq!(fast.bits_per_second, baseline.bits_per_second * 4);
        assert_eq!(fast.fallback_bits_per_second, baseline.bits_per_second);
        assert!(fast.estimated_bytes() > baseline.estimated_bytes());
        let fractional = FrameRate::new(parse_frame_rate(" 23,976 "));
        assert_eq!(fractional.label(), "23.976 fps");
        assert_eq!(fractional.frame_time(2_997, 10_000_000), 1_250_000_000);
        assert_eq!(fractional.frame_at(1_250_000_000, 10_000_000), 2_997);

        for invalid in ["", "NaN", "-1", "0", "241", "invalid"] {
            assert_eq!(
                FrameRate::new(parse_frame_rate(invalid)),
                FrameRate::DEFAULT
            );
        }
        let legacy: VideoPreferences = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.plan(1280, 720).frame_rate, FrameRate::DEFAULT);
    }

    #[test]
    fn saved_region_recording_preferences_open_as_window_capture() {
        let preferences: VideoPreferences =
            serde_json::from_str(r#"{"target":"Region","max_minutes":15}"#).unwrap();
        assert_eq!(preferences.target, VideoTarget::Window);
        assert_eq!(preferences.max_minutes, 15);
        assert_eq!(preferences.plan(1920, 1080).audio_sample_rate, 48_000);
        #[cfg(target_os = "macos")]
        {
            assert!(preferences.presenter_camera);
            let turned_off: VideoPreferences =
                serde_json::from_str(r#"{"presenter_camera":false}"#).unwrap();
            assert!(!turned_off.presenter_camera);
        }
    }

    #[test]
    fn resolution_keeps_portrait_orientation_and_small_regions_are_not_upscaled() {
        let prefs = VideoPreferences::default();
        let small = prefs.plan(321, 241);
        assert_eq!((small.width, small.height), (320, 240));
        let portrait = prefs.plan(2160, 3840);
        assert_eq!((portrait.width, portrait.height), (1080, 1920));
        assert!(portrait.estimated_bytes() > small.estimated_bytes());
    }

    #[test]
    fn disk_estimate_preserves_the_reserve_and_duration_is_bounded() {
        let plan = VideoPreferences {
            max_minutes: u32::MAX,
            ..Default::default()
        }
        .plan(1920, 1080);
        assert_eq!(plan.max_seconds, 240 * 60);
        assert_eq!(plan.recommended_seconds(DISK_RESERVE), 0);
        let free = DISK_RESERVE + plan.estimated_bytes();
        assert_eq!(plan.recommended_seconds(free), u64::from(plan.max_seconds));
    }

    #[test]
    fn compact_recording_uses_the_selected_resolution_and_audio_rate() {
        let prefs = VideoPreferences {
            quality: Quality::Height640,
            audio_source: AudioSource::Microphone,
            audio_sample_rate: AudioSampleRate::Hz8000,
            ..Default::default()
        };
        let plan = prefs.plan(2560, 1440);
        assert_eq!((plan.width, plan.height), (1136, 640));
        #[cfg(target_os = "macos")]
        assert_eq!(
            (plan.audio_sample_rate, plan.audio_bits_per_second),
            (8_000, 24_000)
        );
        #[cfg(target_os = "windows")]
        assert_eq!(
            (plan.audio_sample_rate, plan.audio_bits_per_second),
            (48_000, 128_000)
        );
    }
}
