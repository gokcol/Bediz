//! Independent vertical tracks for a whole native window. Region capture keeps
//! its original boundary-preserving stitcher; only Window uses this compositor.
use super::{Stitcher, crop, moving_regions_with_margin, similar_frame_area};
use crate::selection::PixelRect;
use image::RgbaImage;

const MAX_PANES: usize = 12;
const ALIGNMENT_ERROR: &str = "Cannot align a scrolling pane. Scroll it back a little, then continue down slowly. Keep at least half its previous content visible.";
const SIZE_ERROR: &str =
    "This capture reached the image size limit. Finish it and start another capture.";

#[derive(Clone)]
struct Pane {
    rect: PixelRect,
    core: PixelRect,
    stitcher: Stitcher,
}

#[derive(Clone)]
pub(super) struct WindowTracks {
    // A later-discovered pane must start at the same moment as the first pane.
    // Never replace this with the most recent window sample.
    initial: RgbaImage,
    panes: Vec<Pane>,
}

impl WindowTracks {
    pub(super) fn new(initial: RgbaImage) -> Self {
        Self {
            initial,
            panes: Vec::new(),
        }
    }

    pub(super) fn len(&self) -> usize {
        self.panes.len()
    }

    pub(super) fn observe(&mut self, frame: &RgbaImage) -> Result<Option<RgbaImage>, String> {
        // Stage changed tracks separately. An ambiguous neighbor must not leave
        // the other track, composed image, or counters partially advanced.
        let mut updates = Vec::new();
        for (index, pane) in self.panes.iter().enumerate() {
            let next = crop(frame, pane.rect);
            let local_core = PixelRect {
                x: pane.core.x - pane.rect.x,
                y: 0,
                width: pane.core.width,
                height: pane.core.height,
            };
            if similar_frame_area(&pane.stitcher.last, &next, local_core) {
                continue;
            }
            let mut stitcher = pane.stitcher.clone();
            if stitcher.append(next).map_err(|_| ALIGNMENT_ERROR)? {
                updates.push((index, stitcher));
            }
        }

        let mut discovered = Vec::new();
        for strip in changed_strips(&self.initial, frame, &self.panes) {
            let before = crop(&self.initial, strip);
            let after = crop(frame, strip);
            let mut found = None;
            let mut significant = false;
            for mut core in moving_regions_with_margin(&before, &after, 0) {
                // Ignore small isolated title-bar/caret changes. A candidate is
                // a pane only if its pixels demonstrate a vertical overlap.
                significant |= core.height >= frame.height() / 4;
                core.x += strip.x;
                let left = core.x.saturating_sub(8).max(strip.x);
                let right = (core.x + core.width + 8).min(strip.x + strip.width);
                let rect = PixelRect {
                    x: left,
                    width: right - left,
                    ..core
                };
                let mut stitcher = Stitcher::new(crop(&self.initial, rect));
                if stitcher.append(crop(frame, rect)).is_ok_and(|added| added) {
                    found = Some(Pane {
                        rect,
                        core,
                        stitcher,
                    });
                    break;
                }
            }
            if let Some(pane) = found {
                discovered.push(pane);
            } else if significant {
                return Err(ALIGNMENT_ERROR.into());
            }
        }
        if updates.is_empty() && discovered.is_empty() {
            return Ok(None);
        }
        if self.panes.len() + discovered.len() > MAX_PANES {
            return Err(
                "Too many separate scrolling areas. Capture a smaller group of panes.".into(),
            );
        }

        // References let composition validate and allocate once without copying
        // the already accepted long images for idle panes.
        let mut staged: Vec<(&Pane, &Stitcher)> = self
            .panes
            .iter()
            .enumerate()
            .map(|(index, pane)| {
                let stitcher = updates
                    .iter()
                    .find_map(|(i, stitcher)| (*i == index).then_some(stitcher))
                    .unwrap_or(&pane.stitcher);
                (pane, stitcher)
            })
            .collect();
        staged.extend(discovered.iter().map(|pane| (pane, &pane.stitcher)));
        let output = compose(&self.initial, &staged)?;
        for (index, stitcher) in updates {
            self.panes[index].stitcher = stitcher;
        }
        self.panes.extend(discovered);
        Ok(Some(output))
    }
}

/// Find horizontal motion bands before looking for vertical overlaps. The old
/// single bounding rectangle could combine panes scrolling by different amounts.
fn changed_strips(initial: &RgbaImage, frame: &RgbaImage, panes: &[Pane]) -> Vec<PixelRect> {
    let (width, height) = initial.dimensions();
    let owned = |x| {
        panes
            .iter()
            .any(|pane| x >= pane.core.x && x < pane.core.x + pane.core.width)
    };
    // Most samples are idle. Test a small spread in the unclaimed columns
    // before scanning all of their pixels; do not repeatedly rescan static UI.
    let changed_outside = (0..width).step_by(8).filter(|&x| !owned(x)).any(|x| {
        (0..64).any(|row| {
            let y = row * height.saturating_sub(1) / 63;
            initial.get_pixel(x, y).0[..3]
                .iter()
                .zip(&frame.get_pixel(x, y).0[..3])
                .any(|(a, b)| a.abs_diff(*b) > 12)
        })
    });
    if !changed_outside {
        return Vec::new();
    }
    let mut columns = Vec::new();
    for x in 0..width {
        // A track owns its column; its moving document may initially have blank
        // top/bottom margins. Do not rediscover those margins as another pane.
        if owned(x) {
            continue;
        }
        let changed = (0..height)
            .filter(|&y| {
                initial.get_pixel(x, y).0[..3]
                    .iter()
                    .zip(&frame.get_pixel(x, y).0[..3])
                    .any(|(a, b)| a.abs_diff(*b) > 12)
            })
            .take((6.max(height / 200) + 1) as usize)
            .count();
        if changed > 6.max(height / 200) as usize {
            columns.push(x);
        }
    }
    let Some(&first) = columns.first() else {
        return Vec::new();
    };
    let mut bands = Vec::new();
    let mut left = first;
    let mut right = first + 1;
    for x in columns.into_iter().skip(1) {
        if x - right > 16 {
            bands.push((left, right));
            left = x;
        }
        right = x + 1;
    }
    bands.push((left, right));
    bands
        .into_iter()
        .filter(|(left, right)| right - left >= 32)
        .map(|(left, right)| {
            let previous = panes
                .iter()
                .map(|p| p.core.x + p.core.width)
                .filter(|&edge| edge <= left)
                .max()
                .unwrap_or(0);
            let next = panes
                .iter()
                .map(|p| p.core.x)
                .filter(|&edge| edge >= right)
                .min()
                .unwrap_or(width);
            let left = left.saturating_sub(8).max(previous);
            let right = (right + 8).min(next);
            PixelRect {
                x: left,
                y: 0,
                width: right - left,
                height,
            }
        })
        .collect()
}

fn composed_height(initial: &RgbaImage, panes: &[(&Pane, &Stitcher)]) -> Result<u32, String> {
    let growth = panes
        .iter()
        .map(|(pane, stitcher)| stitcher.image.height().saturating_sub(pane.rect.height))
        .max()
        .unwrap_or(0);
    checked_height(initial.width(), initial.height(), growth)
}

fn checked_height(width: u32, original_height: u32, growth: u32) -> Result<u32, String> {
    let height = original_height.checked_add(growth).ok_or(SIZE_ERROR)?;
    if height > 32_768 || u64::from(width) * u64::from(height) > 100_000_000 {
        return Err(SIZE_ERROR.into());
    }
    Ok(height)
}

fn compose(initial: &RgbaImage, panes: &[(&Pane, &Stitcher)]) -> Result<RgbaImage, String> {
    // Keep the familiar cropped result for a single moving document. As soon as
    // another pane moves, compose both tracks back into their original window.
    if let [(_, stitcher)] = panes {
        return Ok(stitcher.image.clone());
    }
    for (index, (a, _)) in panes.iter().enumerate() {
        if panes
            .iter()
            .skip(index + 1)
            .any(|(b, _)| a.core.x < b.core.x + b.core.width && b.core.x < a.core.x + a.core.width)
        {
            return Err(
                "The scrolling areas overlap. Use Region capture for each area separately.".into(),
            );
        }
    }
    let height = composed_height(initial, panes)?;
    let growth = height - initial.height();
    let bottom = panes
        .iter()
        .map(|(pane, _)| pane.rect.y + pane.rect.height)
        .max()
        .unwrap_or(0);
    let background = *initial.get_pixel(0, bottom.saturating_sub(1));
    let mut output = RgbaImage::from_pixel(initial.width(), height, background);
    // Fixed content stays in place once. Only the bottom chrome is displaced by
    // the longest pane, leaving neutral space below shorter captures.
    image::imageops::replace(
        &mut output,
        &image::imageops::crop_imm(initial, 0, 0, initial.width(), bottom).to_image(),
        0,
        0,
    );
    image::imageops::replace(
        &mut output,
        &image::imageops::crop_imm(
            initial,
            0,
            bottom,
            initial.width(),
            initial.height() - bottom,
        )
        .to_image(),
        0,
        i64::from(bottom + growth),
    );
    for (pane, stitcher) in panes {
        let rect = pane.rect;
        let core = pane.core;
        let own_growth = stitcher.image.height() - rect.height;
        let source_bottom = rect.y + rect.height;
        // Padding may overlap a neighbor's padding, but must never paint over
        // its captured content. Only the moving core is taken from the stitch.
        let left = panes
            .iter()
            .filter(|(other, _)| other.core.x + other.core.width <= core.x)
            .map(|(other, _)| other.core.x + other.core.width)
            .max()
            .unwrap_or(0)
            .max(rect.x);
        let right = panes
            .iter()
            .filter(|(other, _)| other.core.x >= core.x + core.width)
            .map(|(other, _)| other.core.x)
            .min()
            .unwrap_or(initial.width())
            .min(rect.x + rect.width);
        image::imageops::replace(
            &mut output,
            &image::imageops::crop_imm(
                &stitcher.image,
                core.x - rect.x,
                0,
                core.width,
                stitcher.image.height(),
            )
            .to_image(),
            i64::from(core.x),
            i64::from(core.y),
        );
        // Fixed side edges are stretched in the middle, not stitched as if
        // they were page pixels. Preserve their original top and bottom ends.
        let middle = rect.y + rect.height / 2;
        for x in (left..core.x).chain(core.x + core.width..right) {
            for y in rect.y..source_bottom + own_growth {
                let source_y = if y < middle {
                    y
                } else if y < middle + own_growth {
                    middle
                } else {
                    y - own_growth
                };
                output.put_pixel(x, y, *initial.get_pixel(x, source_y));
            }
        }
        // A shorter source pane can have its own fixed footer below it. Move
        // that column's remaining fixed content once, by this pane's growth.
        if source_bottom < bottom {
            let footer = image::imageops::crop_imm(
                initial,
                left,
                source_bottom,
                right - left,
                bottom - source_bottom,
            )
            .to_image();
            image::imageops::replace(
                &mut output,
                &footer,
                i64::from(left),
                i64::from(source_bottom + own_growth),
            );
        }
        for y in bottom + own_growth..bottom + growth {
            for x in left..right {
                output.put_pixel(x, y, background);
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::checked_height;

    #[test]
    fn composed_canvas_limits_apply_to_all_panes_before_allocation() {
        assert_eq!(checked_height(5_000, 1_000, 19_000).unwrap(), 20_000);
        assert!(checked_height(5_000, 1_000, 19_001).is_err());
        assert_eq!(checked_height(64, 100, 32_668).unwrap(), 32_768);
        assert!(checked_height(64, 100, 32_669).is_err());
        assert!(checked_height(64, 100, u32::MAX).is_err());
    }
}
