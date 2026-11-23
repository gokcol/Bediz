//! Native decoding and MP4 rendering for the Windows video workspace.
use super::{
    super::{super::windows_project::Project, options::RecordingPlan},
    windows_frames::FrameMap,
    windows_media::{Apartment, FPS, Media, TICKS, Writer},
};
use crate::{document::Document, render};
use image::{Rgba, RgbaImage};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use windows::{
    Win32::{Foundation::E_FAIL, Media::MediaFoundation::*},
    core::{Error, GUID, Result as NativeResult},
};

const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
const AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
const ALL: u32 = MF_SOURCE_READER_ALL_STREAMS.0 as u32;
const MEDIA_SOURCE: u32 = MF_SOURCE_READER_MEDIASOURCE.0 as u32;
const PER_MS: i64 = TICKS / 1000;
const AUDIO_RATE: i64 = 48_000;
const AUDIO_CHANNELS: u32 = 2;
const AUDIO_BYTES: usize = 4;
const MAX_EDGE: u32 = 4096;

fn fail(message: &str) -> Error {
    Error::new(E_FAIL, message)
}

struct Track {
    reader: IMFSourceReader,
    stream: u32,
    ended: bool,
}

impl Track {
    fn select(reader: IMFSourceReader, stream: u32, format: &IMFMediaType) -> NativeResult<Self> {
        // SAFETY: The COM interfaces belong to this worker. Only the selected
        // stream is decoded so unconsumed streams cannot accumulate samples.
        unsafe {
            reader.SetStreamSelection(ALL, false)?;
            reader.SetStreamSelection(stream, true)?;
            reader.SetCurrentMediaType(stream, None, format)?;
        }
        Ok(Self {
            reader,
            stream,
            ended: false,
        })
    }

    fn seek(&mut self, time: i64) -> NativeResult<()> {
        // SAFETY: An owning signed-integer PROPVARIANT supplies a position in 100 ns units.
        unsafe {
            self.reader
                .SetCurrentPosition(&GUID::zeroed(), &time.into())?
        };
        self.ended = false;
        Ok(())
    }

    fn next(&mut self) -> NativeResult<Option<(i64, IMFSample)>> {
        while !self.ended {
            let (mut flags, mut time, mut sample) = (0, 0, None);
            // SAFETY: Output pointers remain valid during the synchronous read.
            unsafe {
                self.reader.ReadSample(
                    self.stream,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut time),
                    Some(&mut sample),
                )?
            };
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                return Err(fail("The source video could not be decoded."));
            }
            self.ended = flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0;
            if let Some(sample) = sample {
                return Ok(Some((time, sample)));
            }
        }
        Ok(None)
    }
}

struct Video {
    track: Track,
    dimensions: [u32; 2],
    stride: i32,
    rotation: u32,
    next: Option<(i64, IMFSample)>,
    current: Option<(i64, Arc<RgbaImage>)>,
}

impl Video {
    fn open(media: &Media, path: &Path) -> NativeResult<Self> {
        let reader = media.reader(path, true)?;
        // SAFETY: The reader exposes immutable attributes for its native video stream.
        let rotation = unsafe {
            reader
                .GetNativeMediaType(VIDEO, 0)?
                .GetUINT32(&MF_MT_VIDEO_ROTATION)
        }
        .unwrap_or_default();
        let track = Track::select(
            reader,
            VIDEO,
            &media.media_type(&MFMediaType_Video, &MFVideoFormat_RGB32)?,
        )?;
        // SAFETY: The output format was set above and its local COM object is retained.
        let format = unsafe { track.reader.GetCurrentMediaType(VIDEO)? };
        let packed = unsafe { format.GetUINT64(&MF_MT_FRAME_SIZE)? };
        let dimensions = [(packed >> 32) as u32, packed as u32];
        if dimensions.iter().any(|&edge| edge == 0 || edge > MAX_EDGE) {
            return Err(fail(
                "Video editing supports frame dimensions up to 4096 pixels per side.",
            ));
        }
        let stride = media.rgb_stride(&format, dimensions[0])?;
        Ok(Self {
            track,
            dimensions,
            stride,
            rotation,
            next: None,
            current: None,
        })
    }

    fn duration_ms(&self) -> Option<i64> {
        // SAFETY: The presentation duration attribute belongs to the source reader.
        let duration = unsafe {
            self.track
                .reader
                .GetPresentationAttribute(MEDIA_SOURCE, &MF_PD_DURATION)
        }
        .ok()?;
        i64::try_from(u64::try_from(&duration).ok()?)
            .ok()
            .map(|ticks| ticks / PER_MS)
    }

    fn seek(&mut self, time: i64) -> NativeResult<()> {
        self.track.seek(time)?;
        self.next = None;
        self.current = None;
        Ok(())
    }

    fn frame(&mut self, time: i64) -> NativeResult<Arc<RgbaImage>> {
        let mut chosen = None;
        loop {
            if self.next.is_none() {
                self.next = self.track.next()?;
            }
            if self.next.as_ref().is_none_or(|(stamp, _)| *stamp > time) {
                break;
            }
            chosen = self.next.take();
        }
        if self.current.is_none() && chosen.is_none() {
            chosen = self.next.take();
        }
        if let Some((stamp, sample)) = chosen {
            self.current = Some((stamp, Arc::new(self.image(&sample)?)));
        }
        self.current
            .as_ref()
            .map(|(_, frame)| frame.clone())
            .ok_or_else(|| fail("The video has no readable frame at this time."))
    }

    fn image(&self, sample: &IMFSample) -> NativeResult<RgbaImage> {
        let [width, height] = self.dimensions;
        let pitch = self.stride.unsigned_abs() as usize;
        let image = with_sample(sample, |bytes| {
            if pitch < width as usize * 4 || bytes.len() < pitch * height as usize {
                return Err(fail("The source frame layout changed during decoding."));
            }
            Ok(RgbaImage::from_fn(width, height, |x, y| {
                let row = if self.stride < 0 { height - 1 - y } else { y };
                let i = row as usize * pitch + x as usize * 4;
                Rgba([bytes[i + 2], bytes[i + 1], bytes[i], 255])
            }))
        })?;
        Ok(match self.rotation {
            90 => image::imageops::rotate90(&image),
            180 => image::imageops::rotate180(&image),
            270 => image::imageops::rotate270(&image),
            _ => image,
        })
    }
}

struct Audio {
    track: Track,
    packet: Option<(i64, Vec<u8>)>,
}

impl Audio {
    fn open(media: &Media, path: &Path) -> NativeResult<Option<Self>> {
        let reader = media.reader(path, false)?;
        // SAFETY: These media types are owned by this worker. Request 48 kHz
        // stereo PCM so Media Foundation handles source resampling and downmix.
        unsafe {
            match reader.GetNativeMediaType(AUDIO, 0) {
                Err(error) if error.code() == MF_E_INVALIDSTREAMNUMBER => return Ok(None),
                result => {
                    result?;
                }
            }
            let format = media.media_type(&MFMediaType_Audio, &MFAudioFormat_PCM)?;
            format.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, AUDIO_RATE as u32)?;
            format.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, AUDIO_CHANNELS)?;
            format.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            format.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, AUDIO_BYTES as u32)?;
            format.SetUINT32(
                &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
                AUDIO_RATE as u32 * AUDIO_BYTES as u32,
            )?;
            Ok(Some(Self {
                track: Track::select(reader, AUDIO, &format)?,
                packet: None,
            }))
        }
    }

    fn seek(&mut self, time: i64) -> NativeResult<()> {
        self.track.seek(time)?;
        self.packet = None;
        Ok(())
    }

    fn copy(&mut self, mut source: i64, buffer: &mut [u8]) -> NativeResult<()> {
        buffer.fill(0);
        let first = source;
        let end = source + (buffer.len() / AUDIO_BYTES) as i64;
        while source < end {
            if self.packet.is_none() {
                let Some((stamp, sample)) = self.track.next()? else {
                    break;
                };
                self.packet = Some((
                    to_pcm(stamp),
                    with_sample(&sample, |bytes| Ok(bytes.to_vec()))?,
                ));
            }
            let Some((start, bytes)) = &self.packet else {
                break;
            };
            let packet_end = start + (bytes.len() / AUDIO_BYTES) as i64;
            if *start >= end {
                break;
            }
            if packet_end > source {
                let from = source.max(*start);
                let to = packet_end.min(end);
                let read = (from - *start) as usize * AUDIO_BYTES;
                let write = (from - first) as usize * AUDIO_BYTES;
                let count = (to - from) as usize * AUDIO_BYTES;
                buffer[write..write + count].copy_from_slice(&bytes[read..read + count]);
                source = to;
            }
            if source >= packet_end {
                self.packet = None;
            }
        }
        Ok(())
    }
}

fn to_pcm(ticks: i64) -> i64 {
    (i128::from(ticks) * i128::from(AUDIO_RATE) + i128::from(TICKS / 2))
        .div_euclid(i128::from(TICKS)) as i64
}

fn with_sample<T>(
    sample: &IMFSample,
    inspect: impl FnOnce(&[u8]) -> NativeResult<T>,
) -> NativeResult<T> {
    // SAFETY: The contiguous buffer stays locked while its reported bytes are borrowed.
    unsafe {
        let buffer = sample.ConvertToContiguousBuffer()?;
        let (mut pointer, mut length) = (std::ptr::null_mut(), 0);
        buffer.Lock(&mut pointer, None, Some(&mut length))?;
        let result = if pointer.is_null() || length == 0 {
            Err(fail("Media Foundation returned an empty media sample."))
        } else {
            inspect(std::slice::from_raw_parts(pointer, length as usize))
        };
        buffer.Unlock()?;
        result
    }
}

pub(in crate::app) fn reference(path: &Path, position: i64) -> Result<Arc<RgbaImage>, String> {
    let read = || -> NativeResult<_> {
        let _apartment = Apartment::new()?;
        let media = Media::new()?;
        let mut video = Video::open(&media, path)?;
        video.seek(position * PER_MS)?;
        video.frame(position * PER_MS)
    };
    read().map_err(|e| format!("Could not open the paused frame: {e}"))
}

struct Composite {
    active: Vec<usize>,
    overlay: Option<Arc<RgbaImage>>,
    effects: bool,
}

impl Composite {
    fn new() -> Self {
        Self {
            active: Vec::new(),
            overlay: None,
            effects: false,
        }
    }

    fn apply(
        &mut self,
        project: &Project,
        frame: Arc<RgbaImage>,
        time: i64,
    ) -> Result<Arc<RgbaImage>, String> {
        let active = project
            .layers
            .iter()
            .enumerate()
            .filter_map(|(index, layer)| layer.contains(time).then_some(index))
            .collect::<Vec<_>>();
        if self.active != active {
            self.active = active;
            self.effects = self
                .active
                .iter()
                .flat_map(|&index| &project.layers[index].annotations)
                .any(|a| a.visible() && a.needs_composite());
            self.overlay = if self.active.is_empty() || self.effects {
                None
            } else {
                let base = Arc::new(RgbaImage::new(frame.width(), frame.height()));
                Some(self.composite_layers(project, base)?)
            };
        }
        if self.effects {
            return self.composite_layers(project, frame);
        }
        if let Some(layer) = &self.overlay {
            let mut result = (*frame).clone();
            image::imageops::overlay(&mut result, layer.as_ref(), 0, 0);
            Ok(Arc::new(result))
        } else {
            Ok(frame)
        }
    }

    fn composite_layers(
        &self,
        project: &Project,
        mut frame: Arc<RgbaImage>,
    ) -> Result<Arc<RgbaImage>, String> {
        // Object and group identities are local to the frame in which they were created.
        for &index in &self.active {
            frame = Arc::new(render::flatten(&Document {
                image: frame,
                annotations: project.layers[index].annotations.clone(),
                geometry: Default::default(),
            })?);
        }
        Ok(frame)
    }
}

pub(in crate::app) fn render_mp4(
    project: &Project,
    destination: &Path,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> Result<(), String> {
    project.validate()?;
    let _apartment = Apartment::new().map_err(|e| e.to_string())?;
    let media =
        Media::new().map_err(|e| format!("Windows Media Foundation is unavailable: {e}"))?;
    let mut video = Video::open(&media, &project.source)
        .map_err(|e| format!("Could not decode the source video: {e}"))?;
    if video.duration_ms().is_some_and(|duration| {
        project
            .clips
            .last()
            .is_some_and(|clip| clip.end > duration + 50)
    }) {
        return Err("The source video is shorter than the saved timeline. Restore the original source video.".into());
    }
    let mut audio = Audio::open(&media, &project.source)
        .map_err(|e| format!("Could not decode the source audio: {e}"))?;
    let first = project.clips[0].start * PER_MS;
    video.seek(first).map_err(|e| e.to_string())?;
    let initial = video.frame(first).map_err(|e| e.to_string())?;
    let (width, height) = initial.dimensions();
    let plan = RecordingPlan {
        width: (width & !1).max(16),
        height: (height & !1).max(16),
        bits_per_second: (width * height * 4).clamp(1_000_000, 32_000_000),
        max_seconds: 0,
        audio_source: 0,
        audio_sample_rate: AUDIO_RATE as u32,
        audio_bits_per_second: 160_000,
        countdown_seconds: 0,
    };
    let writer = Writer::new(
        &media,
        destination,
        &plan,
        audio.as_ref().map(|_| AUDIO_CHANNELS),
    )
    .map_err(|e| format!("Could not start the MP4 encoder: {e}"))?;
    let mapping = FrameMap::new(width, height, plan.width, plan.height);
    let mut pixels = vec![0; plan.width as usize * plan.height as usize * 3 / 2];
    let mut samples = Vec::new();
    let mut compositor = Composite::new();
    let mut out = 0;
    for clip in &project.clips {
        let start = clip.start * PER_MS;
        let duration = clip.duration() * PER_MS;
        video.seek(start).map_err(|e| e.to_string())?;
        if let Some(audio) = &mut audio {
            audio.seek(start).map_err(|e| e.to_string())?;
        }
        let mut frame = 0;
        loop {
            let local = frame * TICKS / i64::from(FPS);
            if local >= duration {
                break;
            }
            if cancel.load(Ordering::Relaxed) {
                return Err("Video rendering canceled.".into());
            }
            let next = (((frame + 1) * TICKS) / i64::from(FPS)).min(duration);
            let image = video
                .frame(start + local)
                .map_err(|e| format!("Could not decode a timeline frame: {e}"))?;
            let image = compositor.apply(project, image, (start + local) / PER_MS)?;
            mapping.rgba_to_nv12(&image, &mut pixels);
            writer
                .video_at(&pixels, out + local, next - local)
                .map_err(|e| format!("Could not write a video frame: {e}"))?;
            if let Some(audio) = &mut audio {
                let first = to_pcm(out + local);
                let frames = to_pcm(out + next) - first;
                if frames > 0 {
                    samples.resize(frames as usize * AUDIO_BYTES, 0);
                    audio
                        .copy(to_pcm(start + local), &mut samples)
                        .map_err(|e| format!("Could not read timeline audio: {e}"))?;
                    writer
                        .audio(
                            &samples,
                            first * TICKS / AUDIO_RATE,
                            (first + frames) * TICKS / AUDIO_RATE - first * TICKS / AUDIO_RATE,
                        )
                        .map_err(|e| format!("Could not write timeline audio: {e}"))?;
                }
            }
            frame += 1;
            progress.store(((out + next) / PER_MS) as u64, Ordering::Relaxed);
        }
        out += duration;
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Video rendering canceled.".into());
    }
    writer
        .finish()
        .map_err(|e| format!("Could not finalize the MP4: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::windows_project::{Clip, Layer};

    #[test]
    fn native_windows_edit_exports_trimmed_video_audio_and_timed_objects() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        let edited = dir.path().join("edited.mp4");
        let file = dir.path().join("editable.bedizvideo");
        let _apartment = Apartment::new().unwrap();
        let media = Media::new().unwrap();
        let plan = RecordingPlan {
            width: 64,
            height: 64,
            bits_per_second: 700_000,
            max_seconds: 0,
            audio_source: 0,
            audio_sample_rate: 48_000,
            audio_bits_per_second: 128_000,
            countdown_seconds: 0,
        };
        let writer = Writer::new(&media, &source, &plan, Some(2)).unwrap();
        let mut nv12 = vec![128u8; 64 * 64 * 3 / 2];
        nv12[..64 * 64].fill(60);
        let audio = (0..3200)
            .flat_map(|n| (if n % 96 < 48 { 9000i16 } else { -9000i16 }).to_le_bytes())
            .collect::<Vec<_>>();
        for frame in 0..60 {
            writer.video(&nv12, frame).unwrap();
            let start = frame * TICKS / 30;
            writer
                .audio(&audio, start, (frame + 1) * TICKS / 30 - start)
                .unwrap();
        }
        writer.finish().unwrap();
        let paused = reference(&source, 340).unwrap();
        assert_eq!(paused.dimensions(), (64, 64));
        let shape = serde_json::from_value(serde_json::json!({
            "kind": "Rectangle",
            "start": { "x": 10.0, "y": 10.0 },
            "end": { "x": 54.0, "y": 54.0 },
            "color": [250, 0, 0, 255], "stroke_width": 2.0, "fill": true,
            "text": "", "font_size": 16.0,
            "layer": {"object_id": 1}
        }))
        .unwrap();
        let mut project = Project::new(source, 2000);
        assert!(project.split(0, 1000));
        assert!(project.trim(
            0,
            Clip {
                start: 100,
                end: 700
            }
        ));
        assert!(project.trim(
            1,
            Clip {
                start: 1200,
                end: 1900
            }
        ));
        let art = crate::app::frame_annotation::encode_artwork(
            Document {
                image: paused.clone(),
                annotations: vec![shape],
                geometry: Default::default(),
            },
            false,
        )
        .unwrap();
        let primary = Layer {
            start: 250,
            duration: 300,
            reference: art.reference,
            annotations: art.annotations,
        };
        let mut overlapping = primary.clone();
        overlapping.start = 400;
        overlapping.duration = 100;
        overlapping.annotations[0].start = crate::document::Point { x: 4.0, y: 4.0 };
        overlapping.annotations[0].end = crate::document::Point { x: 24.0, y: 24.0 };
        overlapping.annotations[0].color = [0, 0, 250, 255];
        assert!(project.add_layer(primary));
        assert!(project.add_layer(overlapping));
        project.save(&file).unwrap();
        let project = Project::load(&file).unwrap();
        assert_eq!(project.length(), 1300);
        let reopened = project.layers[0].clone().frame_document().unwrap();
        assert_eq!(reopened.image, paused);
        assert_eq!(reopened.annotations.len(), 1);
        let progress = AtomicU64::new(0);
        render_mp4(&project, &edited, &AtomicBool::new(false), &progress).unwrap();
        assert_eq!(progress.load(Ordering::Relaxed), 1300);
        let before = reference(&edited, 40).unwrap().get_pixel(32, 32).0;
        let during = reference(&edited, 360).unwrap().get_pixel(32, 32).0;
        let overlapped = reference(&edited, 360).unwrap().get_pixel(16, 16).0;
        let after = reference(&edited, 850).unwrap().get_pixel(32, 32).0;
        assert!(during[0] > 150 && during[0] > during[1] + 60, "{during:?}");
        assert!(
            overlapped[2] > 150 && overlapped[2] > overlapped[0] + 60,
            "{overlapped:?}"
        );
        assert!(
            (i16::from(before[0]) - i16::from(before[1])).abs() < 20,
            "{before:?}"
        );
        assert!(
            (i16::from(after[0]) - i16::from(after[1])).abs() < 20,
            "{after:?}"
        );
        let mut audio = Audio::open(&media, &edited)
            .unwrap()
            .expect("edited video retains audio");
        audio.seek(TICKS / 5).unwrap();
        let mut buffer = vec![0u8; 6400];
        audio.copy(AUDIO_RATE / 5, &mut buffer).unwrap();
        assert!(
            buffer
                .chunks_exact(2)
                .any(|byte| i16::from_le_bytes([byte[0], byte[1]]).unsigned_abs() > 500)
        );
    }
}
