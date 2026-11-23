use super::windows_media::Media;
use crate::appearance::{self, Icon};
use eframe::egui;
use std::{path::Path, time::Duration};
use windows::{
    Win32::{
        Foundation::{HANDLE, HWND, RECT, RPC_E_CHANGED_MODE},
        Media::MediaFoundation::*,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            Threading::SetEvent,
        },
        UI::{Shell::UrlCreateFromPathW, WindowsAndMessaging::*},
    },
    core::{BSTR, HSTRING, Interface, PCWSTR, PWSTR, Result, w},
};

#[windows::core::implement(IMFMediaEngineNotify)]
struct Notify(egui::Context);

impl IMFMediaEngineNotify_Impl for Notify_Impl {
    fn EventNotify(&self, event: u32, param: usize, _: u32) -> Result<()> {
        if event == MF_MEDIA_ENGINE_EVENT_NOTIFYSTABLESTATE.0 as u32 {
            // SAFETY: Media Engine passes a live event handle for this notification.
            unsafe { SetEvent(HANDLE(param as *mut _))? };
        } else if event != MF_MEDIA_ENGINE_EVENT_TIMEUPDATE.0 as u32 {
            self.0.request_repaint();
        }
        Ok(())
    }
}

struct Com(bool);

impl Com {
    fn new() -> Result<Self> {
        // SAFETY: This balances successful initialization on Bediz's UI thread.
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self(false));
        }
        result.ok()?;
        Ok(Self(true))
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: This instance stays on the thread that initialized COM.
            unsafe { CoUninitialize() };
        }
    }
}

struct Surface(HWND);

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: This owns the playback child, created on the current UI thread.
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

pub(in crate::app) struct Player {
    engine: IMFMediaEngine,
    surface: Surface,
    _media: Media,
    _com: Com,
    placement: Option<[i32; 4]>,
    speed: f64,
}

impl Player {
    pub(in crate::app) fn open(parent: HWND, path: &Path, ctx: &egui::Context) -> Result<Self> {
        let com = Com::new()?;
        let media = Media::new()?;
        let notify: IMFMediaEngineNotify = Notify(ctx.clone()).into();
        let attributes = media.attributes()?;
        let path = HSTRING::from(std::path::absolute(path)?.as_os_str());
        let mut url = vec![0u16; path.len() * 9 + 24];
        let mut length = url.len() as u32;
        // SAFETY: The path is NUL-terminated and the URL buffer has room for escaping.
        unsafe {
            UrlCreateFromPathW(
                PCWSTR(path.as_ptr()),
                PWSTR(url.as_mut_ptr()),
                &mut length,
                0,
            )?
        };
        let source = BSTR::from_wide(&url[..length as usize]);
        // SAFETY: The child window belongs to the current UI thread. Media Engine
        // retains the notification interface and copies the filename during setup.
        unsafe {
            let surface = Surface(CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                WS_CHILD | WS_CLIPSIBLINGS | WINDOW_STYLE(4), // SS_BLACKRECT
                0,
                0,
                1,
                1,
                Some(parent),
                None,
                None,
                None,
            )?);
            attributes.SetUINT64(&MF_MEDIA_ENGINE_PLAYBACK_HWND, surface.0.0 as u64)?;
            attributes.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, &notify)?;
            let factory: IMFMediaEngineClassFactory =
                CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER)?;
            let engine = factory.CreateInstance(0, &attributes)?;
            let player = Self {
                engine,
                surface,
                _media: media,
                _com: com,
                placement: None,
                speed: 1.0,
            };
            player.engine.SetSource(&source)?;
            player.engine.Load()?;
            Ok(player)
        }
    }

    pub(in crate::app) fn layout(&mut self, target: Option<egui::Rect>, scale: f32, pause: bool) {
        let placement = target.filter(|rect| rect.is_positive()).map(|rect| {
            let rect = rect * scale;
            [
                rect.min.x.round() as i32,
                rect.min.y.round() as i32,
                rect.width().round().max(1.) as i32,
                rect.height().round().max(1.) as i32,
            ]
        });
        // SAFETY: Both COM and the child handle are owned by the UI thread.
        unsafe {
            if self.placement != placement {
                if let Some([x, y, width, height]) = placement {
                    let _ = SetWindowPos(
                        self.surface.0,
                        None,
                        x,
                        y,
                        width,
                        height,
                        SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
                    );
                    if let Ok(engine) = self.engine.cast::<IMFMediaEngineEx>() {
                        let dst = RECT {
                            left: 0,
                            top: 0,
                            right: width,
                            bottom: height,
                        };
                        let _ = engine.UpdateVideoStream(None, Some(&dst), None);
                    }
                } else {
                    let _ = ShowWindow(self.surface.0, SW_HIDE);
                }
                self.placement = placement;
            }
            if pause && !self.engine.IsPaused().as_bool() {
                let _ = self.engine.Pause();
            }
        }
    }

    pub(in crate::app) fn controls(&mut self, ui: &mut egui::Ui) {
        // SAFETY: This player is only accessed from the root UI thread.
        unsafe {
            let duration = self.engine.GetDuration();
            let ready = duration.is_finite() && duration > 0.;
            let ended = self.engine.IsEnded().as_bool();
            let playing = ready && !ended && !self.engine.IsPaused().as_bool();
            let time = self.engine.GetCurrentTime();
            let mut position = if time.is_finite() { time.max(0.) } else { 0. };
            let error = self.engine.GetError().is_ok();
            appearance::input_scope(ui, ready && !error, true, |ui| {
                let label = if playing { "Pause" } else { "Play" };
                let icon = if playing { Icon::Pause } else { Icon::Play };
                ui.horizontal(|ui| {
                    if appearance::icon_button(ui, label, icon).clicked() {
                        if playing {
                            let _ = self.engine.Pause();
                        } else {
                            if ended {
                                let _ = self.engine.SetCurrentTime(0.);
                            }
                            let _ = self.engine.Play();
                        }
                    }
                    let muted = self.engine.GetMuted().as_bool();
                    if appearance::action_button(
                        ui,
                        if muted { "Unmute" } else { "Mute" },
                        Icon::Volume,
                        true,
                        false,
                    )
                    .clicked()
                    {
                        let _ = self.engine.SetMuted(!muted);
                    }
                    egui::ComboBox::from_id_salt("video-playback-speed")
                        .width(56.)
                        .selected_text(format!("{}×", self.speed))
                        .show_ui(ui, |ui| {
                            for speed in [0.5, 0.75, 1., 1.25, 1.5, 2.] {
                                if ui
                                    .selectable_value(&mut self.speed, speed, format!("{speed}×"))
                                    .changed()
                                {
                                    let _ = self.engine.SetPlaybackRate(speed);
                                }
                            }
                        });
                    ui.monospace(clock(position));
                    ui.spacing_mut().slider_width = (ui.available_width() - 110.).max(60.);
                    if ui
                        .add(
                            egui::Slider::new(
                                &mut position,
                                0. ..=if ready { duration } else { 1. },
                            )
                            .show_value(false),
                        )
                        .on_hover_text("Playback timeline")
                        .changed()
                    {
                        let _ = self.engine.SetCurrentTime(position);
                    }
                    ui.monospace(clock(duration));
                });
            });
            if error {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "Bediz couldn’t play this video. The file may use an unsupported codec.",
                );
            } else if !ready {
                ui.weak("Opening video…");
            }
            if playing {
                ui.ctx().request_repaint_after(Duration::from_millis(80));
            }
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // SAFETY: Shut down the engine before dropping the child window or Media Foundation.
        let _ = unsafe { self.engine.Shutdown() };
    }
}

fn clock(seconds: f64) -> String {
    let ms = if seconds.is_finite() {
        (seconds * 1_000.) as u64
    } else {
        0
    };
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1_000 % 60,
        ms % 1_000
    )
}
