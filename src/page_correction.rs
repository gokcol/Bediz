use crate::{
    document::validate_dimensions,
    geometry::{CanvasRect, CropRect},
};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

/// Page corners in original-image pixel-edge coordinates, clockwise from top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageCorrection {
    pub corners: [[u32; 2]; 4],
    pub clean_paper: bool,
}

impl PageCorrection {
    pub fn for_crop(crop: CropRect) -> Self {
        let (x, y) = (crop.x, crop.y);
        let (right, bottom) = (x + crop.width, y + crop.height);
        Self {
            corners: [[x, y], [right, y], [right, bottom], [x, bottom]],
            clean_paper: false,
        }
    }

    pub fn validate(self, original: [u32; 2]) -> Result<(), String> {
        if self
            .corners
            .iter()
            .any(|p| p[0] > original[0] || p[1] > original[1])
        {
            return Err("Page corners must stay within the original photo.".into());
        }
        let edges = self.edges();
        for i in 0..4 {
            let [x, y] = edges[i];
            let [next_x, next_y] = edges[(i + 1) % 4];
            if x * next_y - y * next_x < 2.0 {
                return Err("Place the four corners in order around the page.".into());
            }
        }
        let frame = self.frame(original);
        validate_dimensions(frame.width, frame.height)
    }

    fn edges(self) -> [[f64; 2]; 4] {
        std::array::from_fn(|i| {
            std::array::from_fn(|axis| {
                f64::from(self.corners[(i + 1) % 4][axis]) - f64::from(self.corners[i][axis])
            })
        })
    }

    pub fn frame(self, original: [u32; 2]) -> CropRect {
        let lengths = self.edges().map(|[x, y]| x.hypot(y));
        let width = ((lengths[0] + lengths[2]) / 2.0).round() as u32;
        let height = ((lengths[1] + lengths[3]) / 2.0).round() as u32;
        let width = width.clamp(1, original[0]);
        let height = height.clamp(1, original[1]);
        let x = self.corners.map(|p| p[0]).into_iter().min().unwrap();
        let y = self.corners.map(|p| p[1]).into_iter().min().unwrap();
        CropRect {
            x: x.min(original[0] - width),
            y: y.min(original[1] - height),
            width,
            height,
        }
    }

    /// Render directly into the requested preview or source-space canvas.
    /// Pixels outside the rectified page are transparent.
    pub fn render(
        self,
        source: &RgbaImage,
        bounds: CanvasRect,
        size: [u32; 2],
        clip: Option<CropRect>,
    ) -> RgbaImage {
        let frame = self.frame([source.width(), source.height()]);
        let transform = Homography::from_corners(self.corners);
        let paper = self.clean_paper.then(|| illumination(source, transform));
        RgbaImage::from_fn(size[0], size[1], |x, y| {
            let x = bounds.x as f64 + (x as f64 + 0.5) * bounds.width as f64 / size[0] as f64;
            let y = bounds.y as f64 + (y as f64 + 0.5) * bounds.height as f64 / size[1] as f64;
            if clip.is_some_and(|clip| !contains(clip, x, y)) || !contains(frame, x, y) {
                return Rgba([0; 4]);
            }
            let u = (x - frame.x as f64) / frame.width as f64;
            let v = (y - frame.y as f64) / frame.height as f64;
            let mut pixel = transform.sample(source, u, v);
            if let Some(paper) = &paper {
                clean_pixel(&mut pixel, paper_at(paper, u, v));
            }
            pixel
        })
    }
}

fn contains(rect: CropRect, x: f64, y: f64) -> bool {
    x >= rect.x as f64
        && y >= rect.y as f64
        && x < (rect.x + rect.width) as f64
        && y < (rect.y + rect.height) as f64
}

#[derive(Clone, Copy)]
struct Homography {
    x: [f64; 3],
    y: [f64; 3],
    perspective: [f64; 2],
}

impl Homography {
    fn from_corners(corners: [[u32; 2]; 4]) -> Self {
        let [a, b, c, d] = corners.map(|p| p.map(f64::from));
        let [dx, dy] = [a[0] - b[0] + c[0] - d[0], a[1] - b[1] + c[1] - d[1]];
        let [bx, by] = [b[0] - c[0], b[1] - c[1]];
        let [dx2, dy2] = [d[0] - c[0], d[1] - c[1]];
        let determinant = bx * dy2 - by * dx2;
        let u = (dx * dy2 - dy * dx2) / determinant;
        let v = (bx * dy - by * dx) / determinant;
        Self {
            x: [b[0] * (1.0 + u) - a[0], d[0] * (1.0 + v) - a[0], a[0]],
            y: [b[1] * (1.0 + u) - a[1], d[1] * (1.0 + v) - a[1], a[1]],
            perspective: [u, v],
        }
    }

    fn sample(self, source: &RgbaImage, u: f64, v: f64) -> Rgba<u8> {
        let w = self.perspective[0] * u + self.perspective[1] * v + 1.0;
        let map = |[a, b, c]: [f64; 3]| ((a * u + b * v + c) / w - 0.5) as f32;
        crate::image_effects::sample(
            source,
            map(self.x).clamp(0.0, (source.width() - 1) as f32),
            map(self.y).clamp(0.0, (source.height() - 1) as f32),
        )
    }
}

const GRID: usize = 16;
const SAMPLES: usize = 12;
type Illumination = [[f32; GRID]; GRID];

/// Take a bright percentile from each tile. Dark glyphs and saturated ink do
/// not drive the paper estimate, and interpolation avoids visible tile edges.
fn illumination(source: &RgbaImage, transform: Homography) -> Illumination {
    let mut grid = [[255.0; GRID]; GRID];
    for (row, line) in grid.iter_mut().enumerate() {
        for (col, estimate) in line.iter_mut().enumerate() {
            let mut samples = [0_u8; SAMPLES * SAMPLES];
            let mut count = 0;
            for y in 0..SAMPLES {
                for x in 0..SAMPLES {
                    let u = (col * SAMPLES + x) as f64 + 0.5;
                    let v = (row * SAMPLES + y) as f64 + 0.5;
                    let scale = (GRID * SAMPLES) as f64;
                    let Rgba([r, g, b, a]) = transform.sample(source, u / scale, v / scale);
                    let max = r.max(g).max(b);
                    let min = r.min(g).min(b);
                    if a >= 224 && max - min <= 36 && max >= 64 {
                        samples[count] = max;
                        count += 1;
                    }
                }
            }
            if count >= 12 {
                *estimate = f32::from(*samples[..count].select_nth_unstable(count * 4 / 5).1);
            }
        }
    }
    grid
}

fn paper_at(grid: &Illumination, u: f64, v: f64) -> f32 {
    let coordinate = |p: f64| (p * GRID as f64 - 0.5).clamp(0.0, (GRID - 1) as f64);
    let (x, y) = (coordinate(u), coordinate(v));
    let (left, top) = (x.floor() as usize, y.floor() as usize);
    let (right, bottom) = ((left + 1).min(GRID - 1), (top + 1).min(GRID - 1));
    let mix = |a: f32, b: f32, t: f64| a + (b - a) * t as f32;
    mix(
        mix(grid[top][left], grid[top][right], x.fract()),
        mix(grid[bottom][left], grid[bottom][right], x.fract()),
        y.fract(),
    )
}

fn clean_pixel(pixel: &mut Rgba<u8>, paper: f32) {
    let [r, g, b, alpha] = pixel.0;
    if alpha == 0 {
        return;
    }
    let max = f32::from(r.max(g).max(b));
    let min = f32::from(r.min(g).min(b));
    let neutral = (1.0 - (max - min) / 42.0).clamp(0.0, 1.0);
    let light = ((max / paper - 0.45) / 0.45).clamp(0.0, 1.0);
    let lift = (250.0 - paper).clamp(0.0, 104.0) * neutral * light;
    for channel in &mut pixel.0[..3] {
        *channel = (f32::from(*channel) + lift).round().min(255.0) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projective_correction_matches_the_four_corners() {
        let page = PageCorrection {
            corners: [[4, 9], [80, 2], [70, 95], [9, 84]],
            clean_paper: false,
        };
        page.validate([100, 100]).unwrap();
        let image = RgbaImage::from_fn(100, 100, |x, y| Rgba([x as u8, y as u8, 0, 255]));
        let transform = Homography::from_corners(page.corners);
        for ((u, v), [x, y]) in [(0., 0.), (1., 0.), (1., 1.), (0., 1.)]
            .into_iter()
            .zip(page.corners)
        {
            let color = transform.sample(&image, u, v);
            assert!(u32::from(color[0]).abs_diff(x) <= 1);
            assert!(u32::from(color[1]).abs_diff(y) <= 1);
        }
        let mut crossed = page;
        crossed.corners.swap(1, 2);
        assert!(crossed.validate([100, 100]).is_err());
    }

    #[test]
    fn clean_paper_reduces_shadows_without_changing_saturated_ink() {
        let bounds = CanvasRect {
            x: 0,
            y: 0,
            width: 256,
            height: 256,
        };
        let mut page = PageCorrection::for_crop(CropRect {
            x: 0,
            y: 0,
            width: 256,
            height: 256,
        });
        let image = RgbaImage::from_fn(256, 256, |x, y| {
            if y == 140 && x > 32 && x < 95 {
                Rgba([20, 65, 170, 255])
            } else {
                let shade = (147 + x * 100 / 255) as u8;
                Rgba([shade, shade, shade, 255])
            }
        });
        assert_eq!(page.render(&image, bounds, [256; 2], None), image);
        page.clean_paper = true;
        let clean = page.render(&image, bounds, [256; 2], None);
        assert!(clean.get_pixel(22, 120)[0] >= 240);
        assert_eq!(clean.get_pixel(64, 140), image.get_pixel(64, 140));
        assert!(clean.get_pixel(230, 120)[0].abs_diff(clean.get_pixel(22, 120)[0]) < 10);
    }
}
