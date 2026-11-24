//! Render each album page with Bediz's image and text renderer.
use super::{Entry, Layout, Reader};
use crate::{
    document::{AnnotationKind, Document},
    pdf_export, render, templates,
};
use image::{Rgba, RgbaImage, imageops};
use std::{
    io::{BufWriter, Write},
    path::Path,
    sync::Arc,
};

const PAGE_WIDTH: u32 = 1240;
const PAGE_HEIGHT: u32 = 1754;
const GUTTER: u32 = 42;
const INK: [u8; 4] = [26, 43, 63, 255];
const MUTED: [u8; 4] = [68, 87, 107, 255];
const ACCENT: [u8; 4] = [0, 117, 122, 255];

pub(crate) fn export_pdf(path: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists()
        && std::fs::canonicalize(path).ok() == std::fs::canonicalize(destination).ok()
    {
        return Err("Choose a PDF destination other than the album file.".into());
    }
    let mut reader = Reader::open(path)?;
    let entries = reader.album.entries.clone();
    let per_page = if reader.album.layout == Layout::Walkthrough {
        1
    } else {
        4
    };
    let pages = entries
        .chunks(per_page)
        .enumerate()
        .map(|(index, entries)| render_page(&mut reader, entries, index + 1));
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut output = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    {
        let mut writer = BufWriter::new(output.as_file_mut());
        pdf_export::write_pages(pages, &mut writer)?;
        writer.flush().map_err(|e| e.to_string())?;
    }
    output.as_file().sync_all().map_err(|e| e.to_string())?;
    output
        .persist(destination)
        .map_err(|e| format!("Cannot save PDF: {}", e.error))?;
    Ok(())
}

fn render_page(reader: &mut Reader, entries: &[Entry], page: usize) -> Result<RgbaImage, String> {
    let layout = reader.album.layout;
    let columns = layout.columns() as u32;
    let width = (PAGE_WIDTH - (columns + 1) * GUTTER) / columns;
    let sheet = layout == Layout::ContactSheet;
    let row_height = if sheet { 750 } else { 1500 };
    let image_height = if sheet { 478 } else { 1040 };
    let mut pixels = RgbaImage::from_pixel(PAGE_WIDTH, PAGE_HEIGHT, Rgba([255; 4]));
    let mut captions = Vec::new();
    let mut text = |at: [u32; 4], label: &str, font: f32, color, bold| {
        let mut annotation = templates::annotation(AnnotationKind::Text, at.map(|x| x as f32));
        annotation.text = label.to_owned();
        annotation.font_size = font;
        annotation.text_style.bold = bold;
        annotation.color = color;
        captions.push(annotation);
    };
    text(
        [GUTTER, 40, PAGE_WIDTH - 2 * GUTTER, 105],
        &reader.album.title,
        36.,
        ACCENT,
        true,
    );
    for (index, entry) in entries.iter().enumerate() {
        let x = GUTTER + (index as u32 % columns) * (width + GUTTER);
        let y = 148 + (index as u32 / columns) * row_height;
        let image_top = y + 70;
        text(
            [x, y, width, 66],
            &entry.title,
            if sheet { 22. } else { 29. },
            INK,
            true,
        );
        imageops::replace(
            &mut pixels,
            &RgbaImage::from_pixel(width, image_height, Rgba([240, 244, 248, 255])),
            x.into(),
            image_top.into(),
        );
        if entry.size.is_some() {
            let image = render::thumbnail(&reader.document(entry.id)?, width, image_height)?;
            imageops::overlay(
                &mut pixels,
                &image,
                (x + (width - image.width()) / 2).into(),
                (image_top + (image_height - image.height()) / 2).into(),
            );
        } else {
            text(
                [x + 16, image_top + 16, width - 32, 42],
                "Waiting for capture",
                22.,
                MUTED,
                false,
            );
        }
        if !entry.note.is_empty() {
            text(
                [
                    x,
                    image_top + image_height + 12,
                    width,
                    row_height - image_height - 94,
                ],
                &entry.note,
                if sheet { 18. } else { 24. },
                MUTED,
                false,
            );
        }
    }
    text(
        [GUTTER, 1690, PAGE_WIDTH - 2 * GUTTER, 32],
        &format!("Page {page}"),
        18.,
        MUTED,
        false,
    );
    render::flatten(&Document {
        image: Arc::new(pixels),
        annotations: captions,
        geometry: Default::default(),
    })
}
