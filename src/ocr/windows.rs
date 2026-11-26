//! Windows' on-device OCR service. This must run on a worker thread: recognition
//! blocks while Windows completes its asynchronous operation.

use super::Control;
use crate::privacy_scan::{Finding, Kind, sensitive_ranges};
use eframe::egui::{Rect, pos2};
use image::RgbaImage;
use std::time::Duration;
use windows::{
    Globalization::Language,
    Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap},
    Media::{
        FaceAnalysis::FaceDetector,
        Ocr::{OcrEngine, OcrResult},
    },
    Storage::Streams::DataWriter,
    Win32::{
        Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
        Storage::Packaging::Appx::GetCurrentPackageFullName,
        System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    },
    core::Error,
};

#[cfg(test)]
pub(super) fn recognize(image: &RgbaImage) -> Result<String, String> {
    recognize_cancellable(
        image,
        &Control {
            canceled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            deadline: std::time::Instant::now() + super::JOB_TIMEOUT,
        },
    )
}

pub(super) fn recognize_cancellable(
    image: &RgbaImage,
    control: &Control,
) -> Result<String, String> {
    recognize_read(image, control, |result, _| {
        let lines = result
            .Lines()
            .map_err(|error| native_error("read the text lines", error))?;
        let mut text = Vec::new();
        for line in lines {
            let line = line
                .Text()
                .map_err(|error| native_error("read a text line", error))?
                .to_string();
            if !line.is_empty() {
                text.push(line);
            }
        }
        Ok(text.join("\n"))
    })
}

pub(super) fn detect_private_text(
    image: &RgbaImage,
    control: &Control,
) -> Result<Vec<Finding>, String> {
    recognize_read(image, control, |result, [width, height]| {
        let lines = result
            .Lines()
            .map_err(|error| native_error("read the text lines", error))?;
        let mut findings = Vec::new();
        for line in lines {
            let mut text = String::new();
            let mut words = Vec::new();
            for word in line
                .Words()
                .map_err(|error| native_error("locate the text", error))?
            {
                let value = word
                    .Text()
                    .map_err(|error| native_error("read a text word", error))?
                    .to_string();
                if !text.is_empty() {
                    text.push(' ');
                }
                let start = text.len();
                text.push_str(&value);
                let bounds = word
                    .BoundingRect()
                    .map_err(|error| native_error("locate the text", error))?;
                words.push((
                    start..text.len(),
                    normalized(
                        bounds.X,
                        bounds.Y,
                        bounds.Width,
                        bounds.Height,
                        width,
                        height,
                    ),
                ));
            }
            for (kind, span) in sensitive_ranges(&text) {
                let bounds = words
                    .iter()
                    .filter(|(range, _)| range.start < span.end && span.start < range.end)
                    .fold(Rect::NOTHING, |acc, (_, bounds)| acc.union(*bounds));
                findings.push(Finding { kind, bounds });
            }
        }
        Ok(findings)
    })
}

pub(super) fn detect_faces(image: &RgbaImage, control: &Control) -> Result<Vec<Finding>, String> {
    let scan = || -> windows::core::Result<_> {
        if !FaceDetector::IsSupported()? {
            return Ok(None);
        }
        let (width, height) = image.dimensions();
        let bitmap = grayscale_bitmap(image, width, height)?;
        let result = (|| {
            let detector = FaceDetector::CreateAsync()?.get()?;
            let faces = detector.DetectFacesAsync(&bitmap)?.get()?;
            let mut findings = Vec::new();
            for face in faces {
                let b = face.FaceBox()?;
                findings.push(Finding {
                    kind: Kind::Face,
                    bounds: normalized(
                        b.X as f32,
                        b.Y as f32,
                        b.Width as f32,
                        b.Height as f32,
                        width,
                        height,
                    ),
                });
            }
            Ok(Some(findings))
        })();
        let _ = bitmap.Close();
        result
    };
    control.check()?;
    let _apartment = Apartment::new()?;
    let result = scan();
    control.check()?;
    result
        .map_err(|error| format!("Windows could not detect faces. ({error})"))?
        .ok_or_else(|| "Face detection is unavailable on this Windows device.".into())
}

fn normalized(x: f32, y: f32, w: f32, h: f32, width: u32, height: u32) -> Rect {
    Rect::from_min_max(
        pos2(x / width as f32, y / height as f32),
        pos2((x + w) / width as f32, (y + h) / height as f32),
    )
}

fn recognize_read<T>(
    image: &RgbaImage,
    control: &Control,
    read: impl FnOnce(OcrResult, [u32; 2]) -> Result<T, String>,
) -> Result<T, String> {
    control.check()?;
    // A dedicated MTA lets the completion callback run without a UI message pump.
    // Declare this first so every WinRT object is released before uninitializing.
    let _apartment = Apartment::new()?;
    let engine = engine()?;
    let maximum = OcrEngine::MaxImageDimension()
        .map_err(|error| native_error("check the image size", error))?;
    let (width, height) = fit_dimensions(image.width(), image.height(), maximum)?;
    let bitmap = grayscale_bitmap(image, width, height)
        .map_err(|error| native_error("prepare the selected area", error))?;
    let result = (|| {
        control.check()?;
        let operation = engine
            .RecognizeAsync(&bitmap)
            .map_err(|error| native_error("read the text", error))?;
        loop {
            if let Err(error) = control.check() {
                let _ = operation.Cancel();
                return Err(error);
            }
            // AsyncStatus::Started has ABI value zero. The enum comes from windows-future.
            if operation
                .Status()
                .map_err(|error| native_error("check text recognition", error))?
                .0
                != 0
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let result = operation
            .GetResults()
            .map_err(|error| native_error("read the text", error))?;
        control.check()?;
        read(result, [width, height])
    })();
    let _ = bitmap.Close();
    result
}

fn grayscale_bitmap(
    image: &RgbaImage,
    width: u32,
    height: u32,
) -> windows::core::Result<SoftwareBitmap> {
    // Gray8 is supported by the native recognizer and needs one quarter of the
    // buffer space of BGRA. The shared OCR facade already composites alpha.
    let mut grayscale = image::imageops::grayscale(image);
    if grayscale.dimensions() != (width, height) {
        grayscale = image::imageops::resize(
            &grayscale,
            width,
            height,
            image::imageops::FilterType::Lanczos3,
        );
    }
    let writer = DataWriter::new()?;
    writer.WriteBytes(grayscale.as_raw())?;
    let buffer = writer.DetachBuffer()?;
    let _ = writer.Close();
    SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buffer,
        BitmapPixelFormat::Gray8,
        width as i32,
        height as i32,
        BitmapAlphaMode::Ignore,
    )
}

fn engine() -> Result<OcrEngine, String> {
    // Windows resolves the user's preferred languages against installed OCR
    // language packs, including regional variants such as tr-TR and en-US.
    if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
        return Ok(engine);
    }
    let available = OcrEngine::AvailableRecognizerLanguages()
        .map_err(|error| native_error("find an OCR language", error))?;
    let mut languages = Vec::new();
    for language in available {
        let tag = language
            .LanguageTag()
            .map_err(|error| native_error("read an OCR language", error))?
            .to_string();
        languages.push((tag, language));
    }
    let index = fallback_language_index(languages.iter().map(|(tag, _)| tag.as_str()))
        .ok_or_else(|| {
            "Windows has no OCR language installed. In Settings → Time & language → Language & region, open Language options for a supported language and install Optical character recognition, then try again."
                .to_string()
        })?;
    create_engine(&languages[index].1)
}

fn create_engine(language: &Language) -> Result<OcrEngine, String> {
    OcrEngine::TryCreateFromLanguage(language)
        .map_err(|error| native_error("start text recognition", error))
}

fn fit_dimensions(width: u32, height: u32, maximum: u32) -> Result<(u32, u32), String> {
    if width == 0 || height == 0 {
        return Err("Select an area containing text first.".into());
    }
    if maximum == 0 || maximum > i32::MAX as u32 {
        return Err("Windows returned an invalid text recognition image limit.".into());
    }
    if width.max(height) <= maximum {
        return Ok((width, height));
    }
    let scale = f64::from(maximum) / f64::from(width.max(height));
    Ok((
        (f64::from(width) * scale).round().max(1.) as u32,
        (f64::from(height) * scale).round().max(1.) as u32,
    ))
}

fn fallback_language_index<'a>(languages: impl Iterator<Item = &'a str>) -> Option<usize> {
    let mut first = None;
    for (index, tag) in languages.enumerate() {
        first.get_or_insert(index);
        if tag.eq_ignore_ascii_case("en")
            || tag
                .get(..3)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("en-"))
        {
            return Some(index);
        }
    }
    first
}

fn native_error(action: &str, error: Error) -> String {
    // Microsoft documents package identity as a support requirement. Attempt the
    // API in every edition, as PowerToys does, but give an unpackaged installation
    // a useful recovery path if its Windows version rejects the native service.
    let mut length = 0;
    // SAFETY: length is valid for the call. A null buffer requests only its size.
    let status = unsafe { GetCurrentPackageFullName(&mut length, None) };
    let packaged = status == ERROR_INSUFFICIENT_BUFFER || status == ERROR_SUCCESS;
    if packaged {
        format!(
            "Windows could not {action}. Check that an OCR language is installed in Settings → Time & language → Language & region → Language options. ({error})"
        )
    } else {
        format!(
            "Windows could not {action}. Try the Microsoft Store edition of Bediz; Windows officially supports this OCR service for apps with a package identity. ({error})"
        )
    }
}

struct Apartment;

impl Apartment {
    fn new() -> Result<Self, String> {
        // SAFETY: the caller uses a dedicated worker. Every successful call,
        // including S_FALSE, is balanced by RoUninitialize on this same thread.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
            .map_err(|error| format!("Windows could not initialize text recognition. ({error})"))?;
        Ok(Self)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: this guard exists only after successful initialization, and is
        // local to the worker's stack rather than moved to another thread.
        unsafe { RoUninitialize() };
    }
}

#[cfg(test)]
mod tests {
    use super::{fallback_language_index, fit_dimensions, recognize};

    #[test]
    fn unmatched_profile_falls_back_to_installed_english_then_first_language() {
        assert_eq!(
            fallback_language_index(["tr-TR", "EN-gb"].into_iter()),
            Some(1)
        );
        assert_eq!(
            fallback_language_index(["tr-TR", "de-DE"].into_iter()),
            Some(0)
        );
        assert_eq!(fallback_language_index(std::iter::empty()), None);
    }

    #[test]
    fn native_image_limit_preserves_the_selected_area_aspect_ratio() {
        assert_eq!(fit_dimensions(4000, 3000, 2600).unwrap(), (2600, 1950));
        assert_eq!(fit_dimensions(2, 4000, 2600).unwrap(), (1, 2600));
        assert_eq!(fit_dimensions(1600, 900, 2600).unwrap(), (1600, 900));
        assert!(fit_dimensions(0, 100, 2600).is_err());
        assert!(fit_dimensions(100, 100, 0).is_err());
    }

    #[test]
    #[ignore = "Requires the Windows OCR service and an installed English OCR language pack."]
    fn windows_native_ocr_reads_both_lines() {
        let image = crate::svg_stamp::raster(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="1080" height="260">
                <rect width="1080" height="260" fill="white"/>
                <text x="40" y="95" font-family="sans-serif" font-size="64" fill="black">Selected region OCR</text>
                <text x="40" y="195" font-family="sans-serif" font-size="64" fill="black">Second line 12345</text>
            </svg>"##,
            1080,
        )
        .unwrap();
        let text =
            recognize(&image).expect("Windows native OCR should recognize the public fixture");
        let lowercase = text.to_lowercase();
        assert!(lowercase.contains("selected region ocr"), "{text:?}");
        assert!(lowercase.contains("second line 12345"), "{text:?}");
        assert!(
            text.lines().count() >= 2,
            "Line breaks must be preserved: {text:?}"
        );
    }
}
