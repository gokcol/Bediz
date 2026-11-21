//! Tone adjustments in the existing ordered pixel-edit stack.
use crate::{area_pixels::masked_spans, document::Annotation};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

pub const MAX_FEATHER: u8 = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub shadows: u8,
    pub highlights: u8,
    pub feather: u8,
}

impl Settings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(self) -> Result<(), String> {
        if self.shadows > 100 || self.highlights > 100 || self.feather > MAX_FEATHER {
            return Err("Local adjustment values are out of range.".into());
        }
        Ok(())
    }
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}

fn tone(pixel: &mut Rgba<u8>, shadows: f32, highlights: f32, coverage: f32) {
    if pixel[3] == 0 {
        return;
    }
    let brightness =
        (0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32) / 255.;
    let lift = shadows * smoothstep(1. - brightness / 0.65) * coverage;
    let darken = highlights * smoothstep((brightness - 0.35) / 0.65) * coverage;
    for channel in &mut pixel.0[..3] {
        let channel_float = f32::from(*channel);
        *channel =
            (channel_float + (255. - channel_float) * lift - channel_float * darken).round() as u8;
    }
}

/// Inward feather for lassos and saved pixel masks. A padded, byte-sized
/// distance field limits memory to the affected region and preserves holes.
/// Horizontal/vertical neighbors cost three and diagonal neighbors cost four.
fn mask_distances(
    annotation: &Annotation,
    rect: [i64; 4],
    clip: [u32; 4],
    radius: f32,
) -> (Vec<u8>, usize, [usize; 2]) {
    let margin = (radius + 0.5).ceil() as i64;
    let [left, top, right, bottom] = clip.map(i64::from);
    let x = rect[0].max(left - margin) - 1;
    let y = rect[1].max(top - margin) - 1;
    let width = (rect[2].min(right + margin) + 1 - x) as usize;
    let height = (rect[3].min(bottom + margin) + 1 - y) as usize;
    let local = [rect[0] - x, rect[1] - y, rect[2] - x, rect[3] - y];
    let limit = (3. * (radius + 0.5)).ceil() as u8;
    let mut distances = vec![0; width * height];
    for row in 1..height - 1 {
        for (left, right) in masked_spans(annotation, local, row as u32, 1, (width - 1) as u32) {
            distances[row * width + left as usize..row * width + right as usize].fill(limit);
        }
    }
    for row in 1..height - 1 {
        for column in 1..width - 1 {
            let i = row * width + column;
            if distances[i] != 0 {
                distances[i] = distances[i]
                    .min(distances[i - 1].saturating_add(3))
                    .min(distances[i - width].saturating_add(3))
                    .min(distances[i - width - 1].saturating_add(4))
                    .min(distances[i - width + 1].saturating_add(4));
            }
        }
    }
    for row in (1..height - 1).rev() {
        for column in (1..width - 1).rev() {
            let i = row * width + column;
            if distances[i] != 0 {
                distances[i] = distances[i]
                    .min(distances[i + 1].saturating_add(3))
                    .min(distances[i + width].saturating_add(3))
                    .min(distances[i + width - 1].saturating_add(4))
                    .min(distances[i + width + 1].saturating_add(4));
            }
        }
    }
    (distances, width, [(left - x) as usize, (top - y) as usize])
}

pub fn apply(image: &mut RgbaImage, annotation: &Annotation, scale: [f32; 2]) {
    let settings = annotation.local_adjustment;
    if settings.shadows == 0 && settings.highlights == 0 {
        return;
    }
    let rect = [
        (annotation.start.x * scale[0]).floor() as i64,
        (annotation.start.y * scale[1]).floor() as i64,
        (annotation.end.x * scale[0]).ceil() as i64,
        (annotation.end.y * scale[1]).ceil() as i64,
    ];
    let [left, top, right, bottom] = [
        rect[0].clamp(0, i64::from(image.width())) as u32,
        rect[1].clamp(0, i64::from(image.height())) as u32,
        rect[2].clamp(0, i64::from(image.width())) as u32,
        rect[3].clamp(0, i64::from(image.height())) as u32,
    ];
    if right <= left || bottom <= top {
        return;
    }
    let radius = f32::from(settings.feather) * scale[0].min(scale[1]);
    let shadows = f32::from(settings.shadows) * 0.0035;
    let highlights = f32::from(settings.highlights) * 0.0035;
    let mut apply = |x, y, coverage| tone(image.get_pixel_mut(x, y), shadows, highlights, coverage);
    if radius < 0.5 {
        for y in top..bottom {
            for (start, end) in masked_spans(annotation, rect, y, left, right) {
                for x in start..end {
                    apply(x, y, 1.);
                }
            }
        }
    } else if annotation.area_mask.is_empty() && annotation.pixel_mask.is_none() {
        for y in top..bottom {
            let dy = (i64::from(y) - rect[1]).min(rect[3] - 1 - i64::from(y));
            for x in left..right {
                let dx = (i64::from(x) - rect[0]).min(rect[2] - 1 - i64::from(x));
                apply(x, y, smoothstep((dx.min(dy) as f32 + 0.5) / radius));
            }
        }
    } else {
        let (distances, width, [dx, dy]) =
            mask_distances(annotation, rect, [left, top, right, bottom], radius);
        let coverage: [f32; 256] =
            std::array::from_fn(|i| smoothstep((i as f32 / 3. - 0.5) / radius));
        for y in top..bottom {
            let offset = (dy + (y - top) as usize) * width + dx;
            for x in left..right {
                let distance = distances[offset + (x - left) as usize];
                if distance != 0 {
                    apply(x, y, coverage[distance as usize]);
                }
            }
        }
    }
}
