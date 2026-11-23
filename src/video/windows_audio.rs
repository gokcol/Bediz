use super::windows_media::{TICKS, Writer};
use std::ptr;
use windows::{
    Win32::{
        Media::Audio::*,
        System::{
            Com::{CLSCTX_ALL, CoCreateInstance},
            Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
        },
    },
    core::Result,
};

pub(super) struct Audio {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    rate: u32,
    pub(super) channels: u32,
    frequency: i64,
    segment_start: u64,
    segment_offset: i64,
    written: i64,
    packet: Vec<u8>,
    silence: Vec<u8>,
    running: bool,
}

impl Audio {
    pub(super) fn new(loopback: bool, rate: u32) -> Result<Self> {
        let channels = if loopback { 2 } else { 1 };
        let format = WAVEFORMATEX {
            wFormatTag: 1, // WAVE_FORMAT_PCM
            nChannels: channels as u16,
            nSamplesPerSec: rate,
            nAvgBytesPerSec: rate * channels * 2,
            nBlockAlign: (channels * 2) as u16,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        let flags = AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
            | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY
            | if loopback {
                AUDCLNT_STREAMFLAGS_LOOPBACK
            } else {
                0
            };
        // SAFETY: The worker initialized COM, and the PCM format is live through Initialize.
        unsafe {
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = devices
                .GetDefaultAudioEndpoint(if loopback { eRender } else { eCapture }, eConsole)?;
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, TICKS / 2, 0, &format, None)?;
            let capture = client.GetService()?;
            let mut frequency = 0;
            QueryPerformanceFrequency(&mut frequency)?;
            Ok(Self {
                client,
                capture,
                rate,
                channels,
                frequency,
                segment_start: 0,
                segment_offset: 0,
                written: 0,
                packet: Vec::new(),
                silence: vec![0; (rate * channels / 5) as usize],
                running: false,
            })
        }
    }

    pub(super) fn start(&mut self, offset: i64) -> Result<()> {
        let mut counter = 0;
        // SAFETY: The clock destination and the initialized audio client are valid.
        unsafe {
            QueryPerformanceCounter(&mut counter)?;
            self.client.Start()?;
        }
        self.segment_start =
            ((i128::from(counter) * i128::from(TICKS)) / i128::from(self.frequency)) as u64;
        self.segment_offset = offset;
        self.running = true;
        Ok(())
    }

    pub(super) fn pause(&mut self) -> Result<()> {
        if self.running {
            // SAFETY: The client belongs to this worker and no packet is checked out.
            unsafe {
                self.client.Stop()?;
                self.running = false;
                self.client.Reset()?;
            }
        }
        Ok(())
    }

    pub(super) fn poll(&mut self, writer: &Writer<'_>, now: i64) -> Result<()> {
        // Bound the packets drained at once so control commands remain responsive.
        for _ in 0..24 {
            let (mut frames, mut flags, mut qpc) = (0, 0, 0);
            let mut data = ptr::null_mut();
            // SAFETY: WASAPI lends the buffer until ReleaseBuffer, and the
            // negotiated PCM layout fixes bytes per frame.
            unsafe {
                if self.capture.GetNextPacketSize()? == 0 {
                    break;
                }
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, Some(&mut qpc))?;
                let bytes = frames as usize * self.channels as usize * 2;
                self.packet.clear();
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    self.packet.resize(bytes, 0);
                } else {
                    self.packet
                        .extend_from_slice(std::slice::from_raw_parts(data, bytes));
                }
                self.capture.ReleaseBuffer(frames)?;
            }
            let duration = i64::from(frames) * TICKS / i64::from(self.rate);
            let timestamp = if flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32 != 0 {
                now - duration
            } else {
                self.segment_offset
                    + (i128::from(qpc) - i128::from(self.segment_start))
                        .clamp(i128::from(-TICKS), i128::from(TICKS * 86_400))
                        as i64
            }
            .clamp(self.segment_offset, now.max(self.segment_offset));
            let first = timestamp * i64::from(self.rate) / TICKS;
            if first > self.written {
                self.pad(writer, first)?;
            }
            let skipped = (self.written - first).max(0).min(i64::from(frames));
            let count = i64::from(frames) - skipped;
            if count > 0 {
                let offset = (skipped as u32 * self.channels * 2) as usize;
                self.write(writer, &self.packet[offset..], count)?;
                self.written += count;
            }
        }
        // Loopback devices can go quiet without producing packets.
        self.pad(
            writer,
            (now - TICKS / 5).max(0) * i64::from(self.rate) / TICKS,
        )
    }

    pub(super) fn finish(&mut self, writer: &Writer<'_>, duration: i64) -> Result<()> {
        self.pause()?;
        self.pad(writer, duration * i64::from(self.rate) / TICKS)
    }

    fn pad(&mut self, writer: &Writer<'_>, end: i64) -> Result<()> {
        let capacity = (self.silence.len() / (self.channels as usize * 2)) as i64;
        while self.written < end {
            let count = capacity.min(end - self.written);
            let bytes = (count * i64::from(self.channels) * 2) as usize;
            self.write(writer, &self.silence[..bytes], count)?;
            self.written += count;
        }
        Ok(())
    }

    fn write(&self, writer: &Writer<'_>, bytes: &[u8], frames: i64) -> Result<()> {
        let start = self.written * TICKS / i64::from(self.rate);
        let end = (self.written + frames) * TICKS / i64::from(self.rate);
        writer.audio(bytes, start, end - start)
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        let _ = self.pause();
    }
}
