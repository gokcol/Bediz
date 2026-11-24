//! Render each album page with Bediz's image and text renderer.
use super::{Entry, PageLayout, Reader};
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
    let per_page = reader.album.layout.page().entries();
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
    let layout = reader.album.layout.page();
    let [page_width, page_height] = layout.size;
    let margin = PageLayout::MARGIN;
    let single_entry = layout.entries() == 1;
    let note_size = match layout.columns {
        _ if single_entry => 24.,
        3 => 15.,
        _ => 18.,
    };
    let mut pixels = RgbaImage::from_pixel(page_width, page_height, Rgba([255; 4]));
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
        [margin, 40, page_width - 2 * margin, 105],
        &reader.album.title,
        36.,
        ACCENT,
        true,
    );
    for (index, entry) in entries.iter().enumerate() {
        let slot = layout.slot(index);
        let [x, y, width, height] = slot.image;
        text(
            slot.title,
            &entry.title,
            if single_entry { 29. } else { 22. },
            INK,
            true,
        );
        imageops::replace(
            &mut pixels,
            &RgbaImage::from_pixel(width, height, Rgba([240, 244, 248, 255])),
            x.into(),
            y.into(),
        );
        if entry.size.is_some() {
            let image = render::thumbnail(&reader.document(entry.id)?, width, height)?;
            imageops::overlay(
                &mut pixels,
                &image,
                (x + (width - image.width()) / 2).into(),
                (y + (height - image.height()) / 2).into(),
            );
        } else {
            text(
                [x + 16, y + 16, width - 32, 42],
                "Waiting for capture",
                22.,
                MUTED,
                false,
            );
        }
        if !entry.note.is_empty() {
            text(slot.note, &entry.note, note_size, MUTED, false);
        }
    }
    text(
        [margin, page_height - 64, page_width - 2 * margin, 32],
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
