use super::Stitcher;
use image::{Rgba, RgbaImage, imageops};

const WINDOW_WIDTH: u32 = 640;
const WINDOW_HEIGHT: u32 = 500;
const PANE_WIDTH: u32 = 240;
const PANE_HEIGHT: u32 = 400;
const PANE_TOP: u32 = 60;
const PANE_X: [u32; 2] = [40, 340];

struct Fixture {
    pages: [RgbaImage; 2],
}

impl Fixture {
    fn new() -> Self {
        let page = |seed: u32| {
            RgbaImage::from_fn(PANE_WIDTH, 1600, |x, y| {
                let n = x
                    .wrapping_mul(4397)
                    .wrapping_add(y.wrapping_mul(19997))
                    .wrapping_add(seed.wrapping_mul(1_000_003))
                    .wrapping_mul(31337);
                Rgba([
                    (n % 251) as u8,
                    ((n >> 8) % 251) as u8,
                    ((n >> 16) % 251) as u8,
                    255,
                ])
            })
        };
        Self {
            pages: [page(7), page(113)],
        }
    }

    fn frame(&self, offsets: [u32; 2]) -> RgbaImage {
        let mut frame = RgbaImage::from_pixel(WINDOW_WIDTH, WINDOW_HEIGHT, Rgba([30, 40, 50, 255]));
        for y in 0..40 {
            for x in 0..WINDOW_WIDTH {
                frame.put_pixel(x, y, Rgba([70 + (x % 37) as u8, 80, y as u8, 255]));
            }
        }
        for y in 40..470 {
            for x in 292..324 {
                frame.put_pixel(x, y, Rgba([55, 65, 75, 255]));
            }
        }
        for y in 50..450 {
            for x in 8..24 {
                frame.put_pixel(x, y, Rgba([90, (y % 97) as u8, x as u8, 255]));
            }
        }
        for y in 470..WINDOW_HEIGHT {
            for x in 0..WINDOW_WIDTH {
                frame.put_pixel(x, y, Rgba([160 + (x % 41) as u8, (y - 470) as u8, 60, 255]));
            }
        }
        for pane in 0..2 {
            imageops::replace(
                &mut frame,
                &imageops::crop_imm(&self.pages[pane], 0, offsets[pane], PANE_WIDTH, PANE_HEIGHT)
                    .to_image(),
                i64::from(PANE_X[pane]),
                i64::from(PANE_TOP),
            );
        }
        frame
    }

    fn assert_composite(&self, image: &RgbaImage, offsets: [u32; 2]) {
        let added_height = offsets[0].max(offsets[1]);
        assert_eq!(
            image.dimensions(),
            (WINDOW_WIDTH, WINDOW_HEIGHT + added_height)
        );
        for pane in 0..2 {
            let height = PANE_HEIGHT + offsets[pane];
            assert_eq!(
                imageops::crop_imm(image, PANE_X[pane], PANE_TOP, PANE_WIDTH, height).to_image(),
                imageops::crop_imm(&self.pages[pane], 0, 0, PANE_WIDTH, height).to_image(),
                "pane {pane} must remain at its original position with no omitted or repeated rows"
            );
            if offsets[pane] < added_height {
                let top = PANE_TOP + height;
                let bottom = PANE_TOP + PANE_HEIGHT + added_height;
                let neutral = *image.get_pixel(PANE_X[pane], top);
                assert!(
                    (top..bottom).all(|y| (PANE_X[pane]..PANE_X[pane] + PANE_WIDTH)
                        .all(|x| *image.get_pixel(x, y) == neutral)),
                    "space below shorter pane {pane} must be neutral, not a repeated tail"
                );
            }
        }
        let original = self.frame([0, 0]);
        assert_eq!(
            imageops::crop_imm(image, 0, 0, WINDOW_WIDTH, 40).to_image(),
            imageops::crop_imm(&original, 0, 0, WINDOW_WIDTH, 40).to_image(),
            "fixed header"
        );
        assert_eq!(
            imageops::crop_imm(image, 8, 50, 16, 400).to_image(),
            imageops::crop_imm(&original, 8, 50, 16, 400).to_image(),
            "fixed sidebar"
        );
        assert_eq!(
            imageops::crop_imm(image, 292, 40, 32, 420).to_image(),
            imageops::crop_imm(&original, 292, 40, 32, 420).to_image(),
            "fixed gutter"
        );
        assert_eq!(
            imageops::crop_imm(image, 0, 470 + added_height, WINDOW_WIDTH, 30).to_image(),
            imageops::crop_imm(&original, 0, 470, WINDOW_WIDTH, 30).to_image(),
            "footer moves to the bottom"
        );
        let footer_row = imageops::crop_imm(&original, 0, 480, WINDOW_WIDTH, 1).to_image();
        let footer_locations: Vec<_> = (0..image.height())
            .filter(|&y| imageops::crop_imm(image, 0, y, WINDOW_WIDTH, 1).to_image() == footer_row)
            .collect();
        assert_eq!(
            footer_locations,
            vec![480 + added_height],
            "footer appears only once"
        );
    }
}

#[test]
fn independent_panes_sequential_scrolls_preserve_both_in_either_order() {
    let fixture = Fixture::new();
    for first in 0..2 {
        let mut stitch = Stitcher::window(fixture.frame([0, 0]));
        let mut offsets = [0, 0];
        for offset in [150, 310] {
            offsets[first] = offset;
            assert!(stitch.observe_window(fixture.frame(offsets)).unwrap());
        }
        for offset in [120, 250] {
            offsets[1 - first] = offset;
            assert!(stitch.observe_window(fixture.frame(offsets)).unwrap());
            fixture.assert_composite(&stitch.image, offsets);
        }
    }
}

#[test]
fn independent_panes_alternating_unequal_offsets_keep_separate_histories() {
    let fixture = Fixture::new();
    let mut stitch = Stitcher::window(fixture.frame([0, 0]));
    for offsets in [
        [150, 0],
        [150, 100],
        [300, 100],
        [300, 240],
        [430, 240],
        [430, 420],
    ] {
        assert!(stitch.observe_window(fixture.frame(offsets)).unwrap());
        if offsets[1] > 0 {
            fixture.assert_composite(&stitch.image, offsets);
        }
    }
}

#[test]
fn independent_panes_simultaneous_different_offsets_do_not_use_one_union_offset() {
    let fixture = Fixture::new();
    let mut stitch = Stitcher::window(fixture.frame([0, 0]));
    for (step, offsets) in [[150, 100], [300, 240], [430, 390]].into_iter().enumerate() {
        assert!(stitch.observe_window(fixture.frame(offsets)).unwrap());
        assert_eq!(stitch.sections, step + 2, "one section per accepted frame");
        fixture.assert_composite(&stitch.image, offsets);
    }
}

#[test]
fn independent_panes_unequal_lengths_leave_neutral_space_and_fixed_chrome_once() {
    let fixture = Fixture::new();
    let mut stitch = Stitcher::window(fixture.frame([0, 0]));
    for offsets in [[150, 0], [300, 0], [450, 0], [450, 120]] {
        assert!(stitch.observe_window(fixture.frame(offsets)).unwrap());
    }
    fixture.assert_composite(&stitch.image, [450, 120]);
}

#[test]
fn independent_panes_unequal_viewport_heights_keep_the_short_pane_footer_once() {
    let fixture = Fixture::new();
    let short_height = 280;
    let source_bottom = PANE_TOP + short_height;
    let common_bottom = PANE_TOP + PANE_HEIGHT;
    let footer_height = common_bottom - source_bottom;
    let footer = RgbaImage::from_fn(PANE_WIDTH, footer_height, |x, y| {
        Rgba([230, (x % 173) as u8, (y + 17) as u8, 255])
    });
    let frame = |offsets: [u32; 2]| {
        let mut frame = fixture.frame(offsets);
        // A shorter left document has its own fixed status area beneath it,
        // while the right document continues to the common window footer.
        imageops::replace(
            &mut frame,
            &footer,
            i64::from(PANE_X[0]),
            i64::from(source_bottom),
        );
        frame
    };
    let mut stitch = Stitcher::window(frame([0, 0]));
    for offsets in [[100, 0], [100, 150], [180, 150], [180, 310]] {
        assert!(stitch.observe_window(frame(offsets)).unwrap());
    }
    let offsets = [180, 310];
    assert_eq!(
        stitch.image.dimensions(),
        (WINDOW_WIDTH, WINDOW_HEIGHT + 310)
    );
    for (pane, height) in [(0, short_height), (1, PANE_HEIGHT)] {
        assert_eq!(
            imageops::crop_imm(
                &stitch.image,
                PANE_X[pane],
                PANE_TOP,
                PANE_WIDTH,
                height + offsets[pane],
            )
            .to_image(),
            imageops::crop_imm(
                &fixture.pages[pane],
                0,
                0,
                PANE_WIDTH,
                height + offsets[pane]
            )
            .to_image(),
            "pane {pane} keeps every collected row at its original top and column"
        );
    }
    assert_eq!(
        imageops::crop_imm(
            &stitch.image,
            PANE_X[0],
            source_bottom + offsets[0],
            PANE_WIDTH,
            footer_height,
        )
        .to_image(),
        footer,
        "the shorter pane's fixed footer follows its own expanded content"
    );
    let footer_row = imageops::crop_imm(&footer, 0, 31, PANE_WIDTH, 1).to_image();
    let locations: Vec<_> = (0..stitch.image.height())
        .filter(|&y| {
            imageops::crop_imm(&stitch.image, PANE_X[0], y, PANE_WIDTH, 1).to_image() == footer_row
        })
        .collect();
    assert_eq!(locations, [source_bottom + offsets[0] + 31]);
    let neutral = Rgba([30, 40, 50, 255]);
    for y in common_bottom + offsets[0]..common_bottom + offsets[1] {
        assert!(
            (PANE_X[0]..PANE_X[0] + PANE_WIDTH).all(|x| *stitch.image.get_pixel(x, y) == neutral),
            "unused space follows the short pane's footer; y={y}"
        );
    }
    let original = frame([0, 0]);
    assert_eq!(
        imageops::crop_imm(&stitch.image, 0, 470 + offsets[1], WINDOW_WIDTH, 30).to_image(),
        imageops::crop_imm(&original, 0, 470, WINDOW_WIDTH, 30).to_image(),
        "the shared footer still moves below the longest pane"
    );
}

#[test]
fn independent_panes_idle_and_failed_frames_leave_all_accepted_tracks_unchanged() {
    let fixture = Fixture::new();
    let mut stitch = Stitcher::window(fixture.frame([0, 0]));
    assert!(stitch.observe_window(fixture.frame([150, 100])).unwrap());
    fixture.assert_composite(&stitch.image, [150, 100]);
    let accepted = stitch.image.clone();
    let sections = stitch.sections;
    assert!(!stitch.observe_window(fixture.frame([150, 100])).unwrap());
    assert_eq!(stitch.image, accepted);
    assert_eq!(stitch.sections, sections);

    // The left pane has a valid next section while the right has unrelated
    // content. Neither track may advance unless the entire frame is accepted.
    let mut invalid = fixture.frame([280, 100]);
    imageops::replace(
        &mut invalid,
        &RgbaImage::from_pixel(PANE_WIDTH, PANE_HEIGHT, Rgba([180, 20, 150, 255])),
        i64::from(PANE_X[1]),
        i64::from(PANE_TOP),
    );
    assert!(!stitch.observe_window(invalid.clone()).unwrap());
    assert_eq!(stitch.image, accepted);
    assert_eq!(stitch.sections, sections);
    assert!(stitch.observe_window(invalid).is_err());
    assert_eq!(stitch.image, accepted);
    assert_eq!(stitch.sections, sections);

    assert!(stitch.observe_window(fixture.frame([280, 220])).unwrap());
    fixture.assert_composite(&stitch.image, [280, 220]);
}

#[test]
fn independent_panes_do_not_change_region_capture_bounds_or_alignment_rules() {
    let fixture = Fixture::new();
    let region_frame = |offset| {
        imageops::crop_imm(&fixture.pages[0], 0, offset, PANE_WIDTH, PANE_HEIGHT).to_image()
    };
    let mut region = Stitcher::region(region_frame(0));
    assert!(region.observe_window(region_frame(100)).unwrap());
    assert!(region.observe_window(region_frame(250)).unwrap());
    assert_eq!(
        region.image,
        imageops::crop_imm(&fixture.pages[0], 0, 0, PANE_WIDTH, 650).to_image()
    );

    // A Region remains one selected rectangle. Two incompatible scroll
    // offsets must not silently turn it into an automatically cropped Window.
    let original = fixture.frame([0, 0]);
    let mut region = Stitcher::region(original.clone());
    assert!(!region.observe_window(fixture.frame([150, 100])).unwrap());
    assert!(region.observe_window(fixture.frame([150, 100])).is_err());
    assert_eq!(region.image, original);
    assert_eq!(region.sections, 1);
}

#[test]
fn independent_panes_late_third_pane_retains_every_existing_history() {
    let fixture = Fixture::new();
    let third = imageops::flip_horizontal(&fixture.pages[0]);
    let frame = |offsets: [u32; 3]| {
        let mut frame = RgbaImage::from_pixel(960, WINDOW_HEIGHT, Rgba([30, 40, 50, 255]));
        imageops::replace(&mut frame, &fixture.frame([offsets[0], offsets[1]]), 0, 0);
        imageops::replace(
            &mut frame,
            &imageops::crop_imm(&third, 0, offsets[2], PANE_WIDTH, PANE_HEIGHT).to_image(),
            660,
            i64::from(PANE_TOP),
        );
        frame
    };
    let mut stitch = Stitcher::window(frame([0, 0, 0]));
    let steps = [
        ([150, 0, 0], 1),
        ([150, 100, 0], 2),
        ([300, 100, 0], 2),
        ([300, 100, 150], 3),
        ([420, 100, 150], 3),
        ([420, 240, 290], 3),
    ];
    for (step, (offsets, pane_count)) in steps.into_iter().enumerate() {
        assert!(stitch.observe_window(frame(offsets)).unwrap());
        assert_eq!(stitch.pane_count(), pane_count);
        assert_eq!(stitch.sections, step + 2);
        if pane_count < 2 {
            continue;
        }
        let height = WINDOW_HEIGHT + offsets.into_iter().max().unwrap();
        assert_eq!(stitch.image.dimensions(), (960, height));
        for (pane, (x, page)) in [
            (PANE_X[0], &fixture.pages[0]),
            (PANE_X[1], &fixture.pages[1]),
            (660, &third),
        ]
        .into_iter()
        .enumerate()
        {
            let height = PANE_HEIGHT + offsets[pane];
            assert_eq!(
                imageops::crop_imm(&stitch.image, x, PANE_TOP, PANE_WIDTH, height).to_image(),
                imageops::crop_imm(page, 0, 0, PANE_WIDTH, height).to_image(),
                "step {step}, pane {pane}"
            );
        }
    }
}

#[test]
#[ignore = "requires fresh native Firefox screenshots in MYIMAGE_SCROLL_REVIEW_DIR"]
fn review_native_independent_pane_frames() {
    let root = std::path::PathBuf::from(std::env::var("MYIMAGE_SCROLL_REVIEW_DIR").unwrap());
    let read = |index| {
        image::ImageReader::open(root.join(format!("frame-{index}.jpg")))
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap()
            .into_rgba8()
    };
    let initial = read(0);
    let original_width = initial.width();
    let mut stitch = Stitcher::window(initial);
    for index in 1..=4 {
        let frame = read(index);
        let mut added = stitch
            .observe_window(frame.clone())
            .unwrap_or_else(|error| panic!("native frame {index}: {error}"));
        if !added {
            added = stitch
                .observe_window(frame)
                .unwrap_or_else(|error| panic!("settled native frame {index}: {error}"));
        }
        assert!(added, "native frame {index} must add its scrolling section");
        if index >= 2 {
            assert!(stitch.pane_count() >= 2, "native frame {index}");
        }
        println!(
            "native frame {index}: {} panes, {} sections, {} × {} px",
            stitch.pane_count(),
            stitch.sections,
            stitch.image.width(),
            stitch.image.height()
        );
    }
    assert_eq!(stitch.image.width(), original_width);
    assert_eq!(stitch.sections, 5);
    stitch
        .image
        .save(root.join("multi-pane-native-result.png"))
        .unwrap();
}

#[test]
fn independent_panes_fixed_side_padding_extends_without_repeating_corner_marks() {
    let fixture = Fixture::new();
    let border = Rgba([90, 120, 150, 255]);
    let top_mark = Rgba([251, 0, 251, 255]);
    let bottom_mark = Rgba([0, 251, 251, 255]);
    let frame = |offsets| {
        let mut frame = fixture.frame(offsets);
        for x in PANE_X {
            for side in [x - 6, x + PANE_WIDTH] {
                for y in PANE_TOP..PANE_TOP + PANE_HEIGHT {
                    for x in side..side + 6 {
                        frame.put_pixel(x, y, border);
                    }
                }
                for y in PANE_TOP + 2..PANE_TOP + 6 {
                    for x in side + 1..side + 4 {
                        frame.put_pixel(x, y, top_mark);
                    }
                }
                for y in PANE_TOP + PANE_HEIGHT - 6..PANE_TOP + PANE_HEIGHT - 2 {
                    for x in side + 1..side + 4 {
                        frame.put_pixel(x, y, bottom_mark);
                    }
                }
            }
        }
        frame
    };
    let mut stitch = Stitcher::window(frame([0, 0]));
    for offsets in [[150, 0], [150, 100], [310, 100], [310, 250]] {
        assert!(stitch.observe_window(frame(offsets)).unwrap());
    }
    let offsets = [310, 250];
    fixture.assert_composite(&stitch.image, offsets);
    for pane in 0..2 {
        let bottom = PANE_TOP + PANE_HEIGHT + offsets[pane];
        for side in [PANE_X[pane] - 6, PANE_X[pane] + PANE_WIDTH] {
            // The side's middle extends as a straight border. Fixed corner
            // pixels belong only at its top and its new final bottom, never
            // at each accepted section's former viewport edge.
            for y in PANE_TOP..bottom {
                for x in side..side + 6 {
                    let in_mark_column = (side + 1..side + 4).contains(&x);
                    let expected = if in_mark_column && (PANE_TOP + 2..PANE_TOP + 6).contains(&y) {
                        top_mark
                    } else if in_mark_column && (bottom - 6..bottom - 2).contains(&y) {
                        bottom_mark
                    } else {
                        border
                    };
                    assert_eq!(
                        *stitch.image.get_pixel(x, y),
                        expected,
                        "pane {pane}, border pixel ({x}, {y})"
                    );
                }
            }
        }
    }
}

#[test]
fn independent_panes_adjacent_eight_pixel_gap_preserves_both_discovery_orders() {
    let fixture = Fixture::new();
    let pane_x = [40, 288];
    let background = Rgba([30, 40, 50, 255]);
    let frame = |offsets: [u32; 2]| {
        let mut frame = fixture.frame([0, 0]);
        for y in PANE_TOP..PANE_TOP + PANE_HEIGHT {
            for x in PANE_X[0]..PANE_X[1] + PANE_WIDTH {
                frame.put_pixel(x, y, background);
            }
        }
        for pane in 0..2 {
            imageops::replace(
                &mut frame,
                &imageops::crop_imm(
                    &fixture.pages[pane],
                    0,
                    offsets[pane],
                    PANE_WIDTH,
                    PANE_HEIGHT,
                )
                .to_image(),
                i64::from(pane_x[pane]),
                i64::from(PANE_TOP),
            );
        }
        frame
    };
    for first in 0..2 {
        let mut stitch = Stitcher::window(frame([0, 0]));
        let mut offsets = [0, 0];
        for (pane, offset) in [
            (first, 150),
            (1 - first, 100),
            (first, 310),
            (1 - first, 250),
        ] {
            offsets[pane] = offset;
            assert!(stitch.observe_window(frame(offsets)).unwrap());
        }
        assert_eq!(stitch.pane_count(), 2);
        assert_eq!(stitch.sections, 5);
        assert_eq!(
            stitch.image.dimensions(),
            (WINDOW_WIDTH, WINDOW_HEIGHT + 310)
        );
        for pane in 0..2 {
            let height = PANE_HEIGHT + offsets[pane];
            assert_eq!(
                imageops::crop_imm(&stitch.image, pane_x[pane], PANE_TOP, PANE_WIDTH, height)
                    .to_image(),
                imageops::crop_imm(&fixture.pages[pane], 0, 0, PANE_WIDTH, height).to_image(),
                "first pane {first}, content pane {pane}"
            );
        }
        assert!(
            (PANE_TOP..PANE_TOP + PANE_HEIGHT + 310)
                .all(|y| (280..288).all(|x| *stitch.image.get_pixel(x, y) == background)),
            "the narrow fixed gap stays between the independent documents"
        );
    }
}
