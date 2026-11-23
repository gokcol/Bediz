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
}

impl VideoTarget {
    pub(crate) fn mode(self) -> crate::capture::CaptureMode {
        use crate::capture::CaptureMode;
        match self {
            Self::Window => CaptureMode::Window,
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

pub(super) struct RecordingPlan {
    pub width: u32,
    pub height: u32,
    pub bits_per_second: u32,
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
        let bits_per_second = (f64::from(width * height) * 30. * bits_per_pixel)
            .round()
            .clamp(160_000., 32_000_000.) as u32;
        RecordingPlan {
            width,
            height,
            bits_per_second,
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
            "{} × {} · 30 fps · about {} at {} min",
            plan.width,
            plan.height,
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
