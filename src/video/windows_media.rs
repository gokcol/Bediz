//! Media Foundation MP4 and poster I/O on the recording worker.
use super::super::options::RecordingPlan;
use std::{ffi::c_void, path::Path, ptr};
use windows::{
    Win32::{
        Foundation::{E_FAIL, FreeLibrary, HMODULE},
        Media::MediaFoundation::*,
        System::{
            LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
            WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
        },
    },
    core::{Error, GUID, HRESULT, HSTRING, Interface, PCWSTR, Result, s, w},
};

pub(super) const FPS: u32 = 30;
pub(super) const TICKS: i64 = 10_000_000;

pub(super) struct Apartment;

impl Apartment {
    pub(super) fn new() -> Result<Self> {
        // SAFETY: This is called on a dedicated worker, and drop balances initialization.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
        Ok(Self)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: Drop runs on the same worker that initialized the apartment.
        unsafe { RoUninitialize() };
    }
}

struct SystemLibrary(HMODULE);

impl SystemLibrary {
    fn load(file: PCWSTR) -> Result<Self> {
        // SAFETY: The name is a constant system DLL and only System32 is searched.
        Ok(Self(unsafe {
            LoadLibraryExW(file, None, LOAD_LIBRARY_SEARCH_SYSTEM32)?
        }))
    }
}

impl Drop for SystemLibrary {
    fn drop(&mut self) {
        // SAFETY: This owns one reference returned by LoadLibraryExW.
        let _ = unsafe { FreeLibrary(self.0) };
    }
}

// Load on demand so even Windows N systems without the Media Feature Pack
// can open Bediz and use the image editor.
macro_rules! entry_point {
    ($library:expr, $name:literal, $signature:ty) => {{
        // SAFETY: The address is a named Microsoft export, cast to its documented ABI.
        let address =
            unsafe { GetProcAddress($library.0, s!($name)) }.ok_or_else(Error::from_win32)?;
        unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, $signature>(address) }
    }};
}

type Out = *mut *mut c_void;
type CreateObject = unsafe extern "system" fn(Out) -> HRESULT;
type CreateAttributes = unsafe extern "system" fn(Out, u32) -> HRESULT;
type CreateBuffer = unsafe extern "system" fn(u32, Out) -> HRESULT;
type CreateWriter = unsafe extern "system" fn(PCWSTR, *mut c_void, *mut c_void, Out) -> HRESULT;
type CreateReader = unsafe extern "system" fn(PCWSTR, *mut c_void, Out) -> HRESULT;
type DefaultStride = unsafe extern "system" fn(u32, u32, *mut i32) -> HRESULT;

pub(super) struct Media {
    _platform: SystemLibrary,
    _readwrite: SystemLibrary,
    shutdown: unsafe extern "system" fn() -> HRESULT,
    create_attributes: CreateAttributes,
    create_type: CreateObject,
    create_sample: CreateObject,
    create_buffer: CreateBuffer,
    create_writer: CreateWriter,
    create_reader: CreateReader,
    default_stride: DefaultStride,
}

impl Media {
    pub(super) fn new() -> Result<Self> {
        let platform = SystemLibrary::load(w!("mfplat.dll"))?;
        let readwrite = SystemLibrary::load(w!("mfreadwrite.dll"))?;
        let start = entry_point!(
            platform,
            "MFStartup",
            unsafe extern "system" fn(u32, u32) -> HRESULT
        );
        let shutdown = entry_point!(
            platform,
            "MFShutdown",
            unsafe extern "system" fn() -> HRESULT
        );
        let create_attributes = entry_point!(platform, "MFCreateAttributes", CreateAttributes);
        let create_type = entry_point!(platform, "MFCreateMediaType", CreateObject);
        let create_sample = entry_point!(platform, "MFCreateSample", CreateObject);
        let create_buffer = entry_point!(platform, "MFCreateMemoryBuffer", CreateBuffer);
        let create_writer = entry_point!(readwrite, "MFCreateSinkWriterFromURL", CreateWriter);
        let create_reader = entry_point!(readwrite, "MFCreateSourceReaderFromURL", CreateReader);
        let default_stride =
            entry_point!(platform, "MFGetStrideForBitmapInfoHeader", DefaultStride);
        // SAFETY: This worker initialized its COM apartment before loading Media Foundation.
        unsafe { start(MF_VERSION, MFSTARTUP_FULL).ok()? };
        Ok(Self {
            _platform: platform,
            _readwrite: readwrite,
            shutdown,
            create_attributes,
            create_type,
            create_sample,
            create_buffer,
            create_writer,
            create_reader,
            default_stride,
        })
    }

    pub(super) fn attributes(&self) -> Result<IMFAttributes> {
        object(|out| unsafe { (self.create_attributes)(out, 4) })
    }

    pub(super) fn media_type(&self, major: &GUID, subtype: &GUID) -> Result<IMFMediaType> {
        let format: IMFMediaType = object(|out| unsafe { (self.create_type)(out) })?;
        // SAFETY: The returned COM media type and the GUIDs are valid.
        unsafe {
            format.SetGUID(&MF_MT_MAJOR_TYPE, major)?;
            format.SetGUID(&MF_MT_SUBTYPE, subtype)?;
        }
        Ok(format)
    }

    pub(super) fn reader(&self, path: &Path, rgb: bool) -> Result<IMFSourceReader> {
        let attributes = self.attributes()?;
        let path = HSTRING::from(path.as_os_str());
        // SAFETY: These locally owned values remain valid during initialization.
        unsafe {
            if rgb {
                attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
            }
            object(|out| (self.create_reader)(PCWSTR(path.as_ptr()), attributes.as_raw(), out))
        }
    }

    pub(super) fn rgb_stride(&self, format: &IMFMediaType, width: u32) -> Result<i32> {
        // SAFETY: Media Foundation writes to live local storage for a known RGB32 format.
        unsafe {
            format
                .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                .map(|stride| stride as i32)
                .or_else(|_| {
                    let mut stride = 0;
                    (self.default_stride)(MFVideoFormat_RGB32.data1, width, &mut stride).ok()?;
                    Ok(stride)
                })
        }
    }

    fn sample(&self, bytes: &[u8], time: i64, duration: i64) -> Result<IMFSample> {
        let length = u32::try_from(bytes.len()).map_err(|_| Error::from_hresult(E_FAIL))?;
        let buffer: IMFMediaBuffer = object(|out| unsafe { (self.create_buffer)(length, out) })?;
        let mut output = ptr::null_mut();
        // SAFETY: Lock exposes at least the allocation length, and the buffer is locally owned.
        unsafe {
            buffer.Lock(&mut output, None, None)?;
            ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(length)?;
        }
        let sample: IMFSample = object(|out| unsafe { (self.create_sample)(out) })?;
        // SAFETY: This sample takes a retained reference to the locally created buffer.
        unsafe {
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(time)?;
            sample.SetSampleDuration(duration)?;
        }
        Ok(sample)
    }
}

impl Drop for Media {
    fn drop(&mut self) {
        // SAFETY: Media Foundation startup succeeded and all its objects drop first.
        let _ = unsafe { (self.shutdown)() };
    }
}

fn object<T: Interface>(initialize: impl FnOnce(Out) -> HRESULT) -> Result<T> {
    let mut value = ptr::null_mut();
    initialize(&mut value).ok()?;
    if value.is_null() {
        return Err(Error::from_hresult(E_FAIL));
    }
    // SAFETY: The Media Foundation export returns one owning reference to this interface.
    Ok(unsafe { T::from_raw(value) })
}

pub(super) struct Writer<'a> {
    media: &'a Media,
    inner: IMFSinkWriter,
    video: u32,
    audio: Option<u32>,
}

impl<'a> Writer<'a> {
    pub(super) fn new(
        media: &'a Media,
        path: &Path,
        plan: &RecordingPlan,
        channels: Option<u32>,
    ) -> Result<Self> {
        let path = HSTRING::from(path.as_os_str());
        // SAFETY: The media types below are valid, locally owned COM interfaces.
        unsafe {
            let inner: IMFSinkWriter = object(|out| {
                (media.create_writer)(PCWSTR(path.as_ptr()), ptr::null_mut(), ptr::null_mut(), out)
            })?;
            let video_type = |subtype| -> Result<IMFMediaType> {
                let format = media.media_type(&MFMediaType_Video, subtype)?;
                format.SetUINT64(&MF_MT_FRAME_SIZE, pair(plan.width, plan.height))?;
                format.SetUINT64(&MF_MT_FRAME_RATE, pair(FPS, 1))?;
                format.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pair(1, 1))?;
                format.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
                format.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
                format.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
                format.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
                Ok(format)
            };
            let output = video_type(&MFVideoFormat_H264)?;
            output.SetUINT32(&MF_MT_AVG_BITRATE, plan.bits_per_second)?;
            output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Main.0 as u32)?;
            let input = video_type(&MFVideoFormat_NV12)?;
            input.SetUINT32(&MF_MT_DEFAULT_STRIDE, plan.width)?;
            let video = inner.AddStream(&output)?;
            inner.SetInputMediaType(video, &input, None)?;
            let audio = if let Some(channels) = channels {
                let audio_type = |subtype| -> Result<IMFMediaType> {
                    let format = media.media_type(&MFMediaType_Audio, subtype)?;
                    format.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
                    format.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, plan.audio_sample_rate)?;
                    format.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
                    Ok(format)
                };
                let output = audio_type(&MFAudioFormat_AAC)?;
                output.SetUINT32(
                    &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
                    plan.audio_bits_per_second / 8,
                )?;
                let input = audio_type(&MFAudioFormat_PCM)?;
                input.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, channels * 2)?;
                input.SetUINT32(
                    &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
                    channels * 2 * plan.audio_sample_rate,
                )?;
                let index = inner.AddStream(&output)?;
                inner.SetInputMediaType(index, &input, None)?;
                Some(index)
            } else {
                None
            };
            inner.BeginWriting()?;
            Ok(Self {
                media,
                inner,
                video,
                audio,
            })
        }
    }

    pub(super) fn video(&self, bytes: &[u8], frame: i64) -> Result<()> {
        let start = frame * TICKS / i64::from(FPS);
        let end = (frame + 1) * TICKS / i64::from(FPS);
        self.video_at(bytes, start, end - start)
    }

    pub(super) fn video_at(&self, bytes: &[u8], time: i64, duration: i64) -> Result<()> {
        self.write(self.video, bytes, time, duration)
    }

    pub(super) fn audio(&self, bytes: &[u8], time: i64, duration: i64) -> Result<()> {
        if let Some(stream) = self.audio {
            self.write(stream, bytes, time, duration)?;
        }
        Ok(())
    }

    fn write(&self, stream: u32, bytes: &[u8], time: i64, duration: i64) -> Result<()> {
        let sample = self.media.sample(bytes, time, duration)?;
        // SAFETY: The index belongs to this sink writer, which retains the sample as needed.
        unsafe { self.inner.WriteSample(stream, &sample) }
    }

    pub(super) fn finish(self) -> Result<()> {
        // SAFETY: Only the owning recording thread writes or finalizes this sink.
        unsafe { self.inner.Finalize() }
    }
}

fn pair(first: u32, second: u32) -> u64 {
    (u64::from(first) << 32) | u64::from(second)
}

pub(super) fn thumbnail(path: &Path, width: u32, height: u32) -> Result<crate::library::Thumbnail> {
    let _apartment = Apartment::new()?;
    let media = Media::new()?;
    let attributes = media.attributes()?;
    let path = HSTRING::from(path.as_os_str());
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    // SAFETY: The path, types and all output storage remain live through each synchronous call.
    unsafe {
        attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
        let reader: IMFSourceReader =
            object(|out| (media.create_reader)(PCWSTR(path.as_ptr()), attributes.as_raw(), out))?;
        reader.SetCurrentMediaType(
            stream,
            None,
            &media.media_type(&MFMediaType_Video, &MFVideoFormat_RGB32)?,
        )?;
        let format = reader.GetCurrentMediaType(stream)?;
        let packed = format.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let original_size = [(packed >> 32) as u32, packed as u32];
        let [w, h] = original_size;
        if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 100_000_000 {
            return Err(Error::new(E_FAIL, "Unsupported video frame dimensions."));
        }
        for _ in 0..24 {
            let mut sample = None;
            let mut flags = 0;
            reader.ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample))?;
            if let Some(sample) = sample {
                let buffer = sample.ConvertToContiguousBuffer()?;
                let stride = format
                    .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                    .map(|stride| stride as i32)
                    .or_else(|_| {
                        let mut stride = 0;
                        (media.default_stride)(MFVideoFormat_RGB32.data1, w, &mut stride).ok()?;
                        Ok::<_, Error>(stride)
                    })?;
                let (mut data, mut length) = (ptr::null_mut(), 0);
                buffer.Lock(&mut data, None, Some(&mut length))?;
                let stride_width = stride.unsigned_abs() as usize;
                let image = if !data.is_null()
                    && stride_width >= w as usize * 4
                    && stride_width.saturating_mul(h as usize) <= length as usize
                {
                    let scale = (f64::from(width) / f64::from(w))
                        .min(f64::from(height) / f64::from(h))
                        .min(1.);
                    let fit = |n| (f64::from(n) * scale).round().max(1.) as u32;
                    let pixels = std::slice::from_raw_parts(data, length as usize);
                    Some(image::RgbaImage::from_fn(fit(w), fit(h), |x, y| {
                        let x = (u64::from(x) * u64::from(w) / u64::from(fit(w))) as usize;
                        let y = (u64::from(y) * u64::from(h) / u64::from(fit(h))) as usize;
                        let row = if stride < 0 { h as usize - 1 - y } else { y };
                        let at = row * stride_width + x * 4;
                        image::Rgba([pixels[at + 2], pixels[at + 1], pixels[at], 255])
                    }))
                } else {
                    None
                };
                buffer.Unlock()?;
                let image =
                    image.ok_or_else(|| Error::new(E_FAIL, "Unsupported video pixel layout."))?;
                return Ok(crate::library::Thumbnail {
                    image,
                    original_size,
                });
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
        }
    }
    Err(Error::new(E_FAIL, "This video has no readable frame."))
}
