//! Windows recorder. COM objects, D3D capture and the MP4 writer stay on one worker.
#[path = "windows_audio.rs"]
mod windows_audio;
#[path = "windows_edit_media.rs"]
mod windows_edit_media;
#[path = "windows_frames.rs"]
mod windows_frames;
#[path = "windows_media.rs"]
mod windows_media;
pub(in crate::app) use windows_edit_media::{reference as video_frame, render_mp4};
#[path = "windows_player.rs"]
mod windows_player;
pub(in crate::app) use windows_player::Player;

use super::{Command, DisplayInfo, Status, options};
use options::RecordingPlan;
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows_audio::Audio;
use windows_frames::{Frames, Target};
use windows_media::{Apartment, FPS, Media, TICKS, Writer};

type State = Arc<Mutex<(Status, String)>>;

pub(super) struct Recorder {
    state: State,
    commands: Sender<Command>,
    worker: JoinHandle<()>,
}

impl Recorder {
    pub(super) fn start(
        movie: PathBuf,
        display: &DisplayInfo,
        window: Option<u32>,
        cursor: bool,
        plan: RecordingPlan,
    ) -> Result<Self, String> {
        let target = Target::new(display, window);
        let state = Arc::new(Mutex::new((Status::Starting, String::new())));
        let (commands, receiver) = channel();
        let updates = Arc::clone(&state);
        let worker = thread::Builder::new()
            .name("bediz-windows-recorder".into())
            .spawn(move || {
                let final_state = match record(&movie, target, cursor, &plan, &updates, &receiver) {
                    Ok(Some(note)) => (Status::Finished, note),
                    Ok(None) => (Status::Canceled, String::new()),
                    Err(error) => (Status::Failed, error),
                };
                *updates.lock().unwrap() = final_state;
            })
            .map_err(|error| format!("Could not start the recorder: {error}"))?;
        Ok(Self {
            state,
            commands,
            worker,
        })
    }

    pub(super) fn status(&self) -> (Status, String) {
        let state = self.state.lock().unwrap().clone();
        if self.worker.is_finished()
            && !matches!(
                state.0,
                Status::Finished | Status::Failed | Status::Canceled
            )
        {
            return (
                Status::Failed,
                "The recording worker stopped unexpectedly.".into(),
            );
        }
        state
    }

    pub(super) fn control(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.control(Command::Stop);
    }
}

fn record(
    movie: &Path,
    target: Target,
    cursor: bool,
    plan: &RecordingPlan,
    state: &State,
    commands: &Receiver<Command>,
) -> Result<Option<String>, String> {
    let update = |status, message| *state.lock().unwrap() = (status, message);
    let _apartment =
        Apartment::new().map_err(|e| format!("Could not initialize Windows recording: {e}"))?;
    let media = Media::new().map_err(|e| format!("Windows Media Foundation is unavailable. Install the Media Feature Pack if you use Windows N. {e}"))?;

    // The still selection has closed. Leave time for Windows to remove its
    // surfaces and for Bediz to paint the excluded recording controls.
    let delay =
        Duration::from_secs(u64::from(plan.countdown_seconds)).max(Duration::from_millis(200));
    let deadline = Instant::now() + delay;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        if plan.countdown_seconds > 0 {
            update(
                Status::Countdown,
                format!("Starting in {}", remaining.as_millis().div_ceil(1_000)),
            );
        }
        match commands.recv_timeout(remaining.min(Duration::from_millis(100))) {
            Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => return Ok(None),
            _ => {}
        }
    }
    update(Status::Starting, String::new());

    let mut frames = Frames::new(target, cursor)
        .map_err(|e| format!("Could not capture the selected window or display: {e}"))?;
    let mut audio = (plan.audio_source != 0).then(|| Audio::new(plan.audio_source == 1, plan.audio_sample_rate))
        .transpose()
        .map_err(|e| format!("Could not open the chosen audio source. Check your Windows audio and microphone settings. {e}"))?;
    let writer = Writer::new(
        &media,
        movie,
        plan,
        audio.as_ref().map(|audio| audio.channels),
    )
    .map_err(|e| format!("Could not start the MP4 encoder. Try a lower resolution. {e}"))?;

    let mut pixels = vec![0; plan.width as usize * plan.height as usize * 3 / 2];
    let mut active = Duration::ZERO;
    let mut started: Option<Instant> = None;
    let mut last_frame = None;
    let mut paused = false;
    let opening = Instant::now();
    let mut space_checked = opening;
    let note = loop {
        match commands.recv_timeout(Duration::from_millis(5)) {
            Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => break String::new(),
            Ok(Command::Pause) if !paused && started.is_some() => {
                active += started.take().unwrap().elapsed();
                if let Some(audio) = &mut audio {
                    audio
                        .pause()
                        .map_err(|e| format!("Could not pause audio: {e}"))?;
                }
                paused = true;
                update(Status::Paused, String::new());
            }
            Ok(Command::Resume) if paused => {
                if let Some(audio) = &mut audio {
                    audio
                        .start(ticks(active))
                        .map_err(|e| format!("Could not resume audio: {e}"))?;
                }
                started = Some(Instant::now());
                paused = false;
                update(Status::Recording, String::new());
            }
            _ => {}
        }
        if frames.closed() {
            if last_frame.is_none() {
                return Err("The selected window or display closed before recording began.".into());
            }
            break "The selected window or display closed".into();
        }
        if paused {
            continue;
        }
        let elapsed = active + started.map_or(Duration::ZERO, |start| start.elapsed());
        if elapsed.as_secs() >= u64::from(plan.max_seconds) {
            break "Recording time limit reached".into();
        }
        if space_checked.elapsed() >= Duration::from_secs(2) {
            space_checked = Instant::now();
            if movie
                .parent()
                .and_then(options::available_space)
                .is_some_and(|bytes| bytes < options::DISK_RESERVE)
            {
                break "Recording stopped to preserve free disk space".into();
            }
        }
        let position = ticks(elapsed) * i64::from(FPS) / TICKS;
        if last_frame.is_none_or(|previous| position > previous) {
            let updated = frames
                .poll(plan.width, plan.height, &mut pixels)
                .map_err(|e| format!("Windows could not read the selected video frame: {e}"))?;
            if updated || last_frame.is_some() {
                writer
                    .video(&pixels, position)
                    .map_err(|e| format!("Could not encode a screen frame: {e}"))?;
                if started.is_none() {
                    if let Some(audio) = &mut audio {
                        audio
                            .start(0)
                            .map_err(|e| format!("Could not start recording audio: {e}"))?;
                    }
                    started = Some(Instant::now());
                    update(Status::Recording, String::new());
                }
                last_frame = Some(position);
            }
        }
        if last_frame.is_none() && opening.elapsed() > Duration::from_secs(8) {
            return Err("Windows did not provide a frame. Make sure the selected window is open and visible.".into());
        }
        if let Some(audio) = &mut audio
            && started.is_some()
        {
            audio
                .poll(&writer, ticks(elapsed))
                .map_err(|e| format!("Could not record audio: {e}"))?;
        }
    };

    let Some(last_frame) = last_frame else {
        return Ok(None);
    };
    update(Status::Saving, String::new());
    drop(frames);
    if let Some(mut audio) = audio {
        audio
            .finish(&writer, (last_frame + 1) * TICKS / i64::from(FPS))
            .map_err(|e| format!("Could not finish recording audio: {e}"))?;
    }
    writer
        .finish()
        .map_err(|e| format!("Could not finalize the MP4 recording: {e}"))?;
    Ok(Some(note))
}

fn ticks(time: Duration) -> i64 {
    (time.as_nanos() / 100) as i64
}

pub(in crate::app) fn publish(source: &Path, destination: &Path) -> io::Result<()> {
    use ::windows::{
        Win32::Storage::FileSystem::{MOVE_FILE_FLAGS, MoveFileExW},
        core::{HSTRING, PCWSTR},
    };

    let source = HSTRING::from(source.as_os_str());
    let destination = HSTRING::from(destination.as_os_str());
    // SAFETY: Both paths are NUL-terminated and are valid until the call returns.
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVE_FILE_FLAGS(0),
        )
    }
    .map_err(io::Error::other)
}

pub(super) fn thumbnail(
    path: &Path,
    width: u32,
    height: u32,
) -> Result<crate::library::Thumbnail, String> {
    windows_media::thumbnail(path, width, height)
        .map_err(|e| format!("Could not read the recording preview: {e}"))
}
