//! Resize completed captures once, before all image destinations.
use image::RgbaImage;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputSize {
    #[default]
    Original,
    ThreeQuarters,
    Half,
    Quarter,
}

impl OutputSize {
    pub const ALL: [Self; 4] = [
        Self::Original,
        Self::ThreeQuarters,
        Self::Half,
        Self::Quarter,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "100% · Original",
            Self::ThreeQuarters => "75%",
            Self::Half => "50%",
            Self::Quarter => "25%",
        }
    }
    pub fn percent(self) -> u32 {
        match self {
            Self::Original => 100,
            Self::ThreeQuarters => 75,
            Self::Half => 50,
            Self::Quarter => 25,
        }
    }
    pub fn dimensions(self, width: u32, height: u32) -> (u32, u32) {
        let scale = |n: u32| ((u64::from(n) * u64::from(self.percent()) + 50) / 100).max(1) as u32;
        (scale(width), scale(height))
    }
    pub fn apply(self, image: RgbaImage) -> RgbaImage {
        let (width, height) = self.dimensions(image.width(), image.height());
        if self == Self::Original
            || image.width() == 0
            || image.height() == 0
            || image.dimensions() == (width, height)
        {
            return image;
        }
        crate::geometry::resize_premultiplied(&image, width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scales_each_dimension_and_preserves_original_pixels() {
        let original =
            RgbaImage::from_fn(2220, 1196, |x, y| image::Rgba([x as u8, y as u8, 23, 255]));
        assert_eq!(OutputSize::Original.apply(original.clone()), original);
        for (size, expected) in [
            (OutputSize::ThreeQuarters, (1665, 897)),
            (OutputSize::Half, (1110, 598)),
            (OutputSize::Quarter, (555, 299)),
        ] {
            assert_eq!(size.apply(original.clone()).dimensions(), expected);
            assert_eq!(size.dimensions(1, 1), (1, 1));
        }
        assert_eq!(OutputSize::Half.dimensions(83, 51), (42, 26));
    }
    #[test]
    fn reduced_transparent_capture_has_no_hidden_color_fringe() {
        let source = RgbaImage::from_fn(8, 8, |x, _| {
            if x < 4 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 255, 0, 0])
            }
        });
        let output = OutputSize::Half.apply(source);
        assert!(output.pixels().any(|p| p[3] > 0 && p[3] < 255));
        for pixel in output.pixels().filter(|p| p[3] > 0) {
            assert_eq!(&pixel.0[..3], &[255, 0, 0]);
        }
    }
    #[test]
    fn old_preferences_default_to_full_size_and_choices_round_trip() {
        let settings: crate::storage::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.capture_output_size, OutputSize::Original);
        for size in OutputSize::ALL {
            let settings = crate::storage::Settings {
                capture_output_size: size,
                ..settings.clone()
            };
            let reopened: crate::storage::Settings =
                serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
            assert_eq!(reopened.capture_output_size, size);
        }
    }
}
