//! A portable album contains an ordered manifest and one editable project per capture.
use crate::{
    document::{self, Document},
    render,
};
use image::{ImageEncoder, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Seek, Write},
    path::{Path, PathBuf},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

pub(crate) const EXTENSION: &str = "bedizalbum";
pub(crate) const TITLE_LIMIT: usize = 120;
pub(crate) const NOTE_LIMIT: usize = 360;
const MAX_ENTRIES: usize = 1000;
const MAX_MANIFEST: u64 = 4 * 1024 * 1024;
const MAX_PREVIEW: u64 = 1024 * 1024;
const MANIFEST: &str = "album.json";
pub(crate) const PREVIEW_SIZE: [u32; 2] = [384, 256];
const STORED: SimpleFileOptions =
    SimpleFileOptions::DEFAULT.compression_method(CompressionMethod::Stored);

pub(crate) const MAX_READY_ALBUMS: usize = 10;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CapturePreferences {
    albums: BTreeMap<PathBuf, Collection>,
    selected: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Collection {
    title: String,
    ready: bool,
}

impl CapturePreferences {
    /// None indicates an album that has never been enabled.
    pub(crate) fn ready(&self, path: &Path) -> Option<bool> {
        self.albums.get(path).map(|album| album.ready)
    }

    pub(crate) fn destinations(&self) -> impl Iterator<Item = (&Path, &str)> {
        self.albums
            .iter()
            .filter(|(_, album)| album.ready)
            .map(|(path, album)| (path.as_path(), album.title.as_str()))
    }

    pub(crate) fn known_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.albums.keys()
    }

    pub(crate) fn update_title(&mut self, path: &Path, title: &str) {
        if let Some(album) = self.albums.get_mut(path) {
            album.title = title.to_owned();
        }
    }

    pub(crate) fn selected(&self) -> Option<(&Path, &str)> {
        let path = self.selected.as_deref()?;
        let album = self.albums.get(path)?;
        album.ready.then_some((path, &album.title))
    }

    pub(crate) fn choose(&mut self, path: Option<&Path>) -> bool {
        if self.selected.as_deref() == path
            || path.is_some_and(|path| self.ready(path) != Some(true))
        {
            return false;
        }
        self.selected = path.map(Path::to_owned);
        true
    }

    pub(crate) fn set_ready(&mut self, path: &Path, title: &str, ready: bool) -> bool {
        if ready
            && self.ready(path) != Some(true)
            && self.destinations().count() >= MAX_READY_ALBUMS
        {
            return false;
        }
        self.albums.insert(
            path.to_owned(),
            Collection {
                title: title.to_owned(),
                ready,
            },
        );
        if !ready && self.selected.as_deref() == Some(path) {
            self.choose(None);
        }
        true
    }

    pub(crate) fn forget(&mut self, paths: &[PathBuf]) {
        self.albums.retain(|path, _| !paths.contains(path));
        if self
            .selected
            .as_ref()
            .is_some_and(|path| paths.contains(path))
        {
            self.choose(None);
        }
    }

    pub(crate) fn remap(&mut self, mut rename: impl FnMut(&mut PathBuf)) {
        if let Some(selected) = &mut self.selected {
            rename(selected);
        }
        self.albums = std::mem::take(&mut self.albums)
            .into_iter()
            .map(|(mut path, collection)| {
                rename(&mut path);
                (path, collection)
            })
            .collect();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Layout {
    #[default]
    Walkthrough,
    ContactSheet,
    FieldNotes,
    Comparison,
    Storyboard,
}

impl Layout {
    pub(crate) const ALL: [Self; 5] = [
        Self::Walkthrough,
        Self::ContactSheet,
        Self::FieldNotes,
        Self::Comparison,
        Self::Storyboard,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Walkthrough => "Walkthrough",
            Self::ContactSheet => "Contact sheet",
            Self::FieldNotes => "Field notes",
            Self::Comparison => "Comparison",
            Self::Storyboard => "Storyboard",
        }
    }

    pub(crate) fn page(self) -> PageLayout {
        match self {
            Self::Walkthrough => PageLayout::new([1240, 1754], [1, 1], 1040, false),
            Self::ContactSheet => PageLayout::new([1240, 1754], [2, 2], 478, false),
            Self::FieldNotes => PageLayout::new([1240, 1754], [1, 2], 500, true),
            Self::Comparison => PageLayout::new([1754, 1240], [2, 1], 650, false),
            Self::Storyboard => PageLayout::new([1754, 1240], [3, 2], 296, false),
        }
    }
}

/// Shared page geometry for the template picker and the actual PDF.
#[derive(Clone, Copy)]
pub(crate) struct PageLayout {
    pub size: [u32; 2],
    pub columns: usize,
    rows: usize,
    image_height: u32,
    notes_beside: bool,
}

pub(crate) struct PageSlot {
    pub title: [u32; 4],
    pub image: [u32; 4],
    pub note: [u32; 4],
}

impl PageLayout {
    pub(crate) const MARGIN: u32 = 42;

    fn new(size: [u32; 2], grid: [usize; 2], image_height: u32, notes_beside: bool) -> Self {
        Self {
            size,
            columns: grid[0],
            rows: grid[1],
            image_height,
            notes_beside,
        }
    }

    pub(crate) fn entries(self) -> usize {
        self.columns * self.rows
    }

    pub(crate) fn slot(self, index: usize) -> PageSlot {
        let margin = Self::MARGIN;
        let width = (self.size[0] - (self.columns as u32 + 1) * margin) / self.columns as u32;
        let row_height = (self.size[1] - 254) / self.rows as u32;
        let x = margin + (index % self.columns) as u32 * (width + margin);
        let y = 148 + (index / self.columns) as u32 * row_height;
        let image_y = y + 70;
        let image_width = if self.notes_beside {
            width * 3 / 5
        } else {
            width
        };
        let note = if self.notes_beside {
            [
                x + image_width + margin,
                image_y,
                width - image_width - margin,
                self.image_height,
            ]
        } else {
            [
                x,
                image_y + self.image_height + 12,
                width,
                row_height - self.image_height - 94,
            ]
        };
        PageSlot {
            title: [x, y, width, 66],
            image: [x, image_y, image_width, self.image_height],
            note,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) note: String,
    pub(crate) size: Option<[u32; 2]>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Album {
    version: u32,
    next_id: u64,
    pub(crate) title: String,
    pub(crate) layout: Layout,
    pub(crate) entries: Vec<Entry>,
}

pub(crate) enum Change {
    Placeholder,
    Title(String),
    Details {
        id: u64,
        title: String,
        note: String,
    },
    Move {
        id: u64,
        index: usize,
    },
    Remove(u64),
    Layout(Layout),
}

pub(crate) fn is_album(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(EXTENSION))
}

fn project_name(id: u64) -> String {
    format!("entries/{id}.myimage")
}

fn preview_name(id: u64) -> String {
    format!("entries/{id}.png")
}

fn valid_text(text: &str, limit: usize) -> bool {
    text.chars().count() <= limit
}

fn title(text: &str, fallback: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        fallback.into()
    } else {
        text.chars().take(TITLE_LIMIT).collect()
    }
}

impl Album {
    pub(crate) fn captures(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.size.is_some())
            .count()
    }

    fn add_empty(&mut self, proposed_title: &str) -> Result<usize, String> {
        if self.entries.len() >= MAX_ENTRIES {
            return Err(format!("This album already has {MAX_ENTRIES} entries."));
        }
        let index = self.entries.len();
        let id = self.next_id;
        self.next_id = id
            .checked_add(1)
            .ok_or("The album has no available entry IDs.")?;
        let noun = if self.layout == Layout::Walkthrough {
            "Step"
        } else {
            "Capture"
        };
        self.entries.push(Entry {
            id,
            title: title(proposed_title, &format!("{noun} {id}")),
            note: String::new(),
            size: None,
        });
        Ok(index)
    }

    fn index(&self, id: u64) -> Result<usize, String> {
        self.entries
            .iter()
            .position(|entry| entry.id == id)
            .ok_or_else(|| "This entry is no longer in the album.".into())
    }

    fn filled(&self, id: u64) -> Result<&Entry, String> {
        let entry = &self.entries[self.index(id)?];
        if entry.size.is_none() {
            return Err("This entry is waiting for a capture.".into());
        }
        Ok(entry)
    }
}

pub(crate) struct Reader {
    pub(crate) album: Album,
    archive: ZipArchive<File>,
}

impl Reader {
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        let file =
            File::open(path).map_err(|e| format!("Could not open {}: {e}", path.display()))?;
        let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid album: {e}"))?;
        if archive.len() > MAX_ENTRIES * 2 + 1 {
            return Err("This album contains too many files.".into());
        }
        let album: Album = {
            let mut manifest = archive
                .by_name(MANIFEST)
                .map_err(|e| format!("Missing album manifest: {e}"))?;
            if manifest.size() > MAX_MANIFEST {
                return Err("The album manifest exceeds its size limit.".into());
            }
            let mut bytes = Vec::new();
            (&mut manifest)
                .take(MAX_MANIFEST + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| format!("Could not read the album manifest: {e}"))?;
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid album manifest: {e}"))?
        };
        if album.version != 1 {
            return Err(format!("Album version {} is not supported.", album.version));
        }
        let mut ids = HashSet::with_capacity(album.entries.len());
        if album.entries.len() > MAX_ENTRIES
            || album.next_id == 0
            || !valid_text(&album.title, TITLE_LIMIT)
            || album.entries.iter().any(|entry| {
                entry.id == 0
                    || entry.id >= album.next_id
                    || !ids.insert(entry.id)
                    || !valid_text(&entry.title, TITLE_LIMIT)
                    || !valid_text(&entry.note, NOTE_LIMIT)
            })
        {
            return Err("The album contains invalid entry details.".into());
        }
        if archive.len() != album.captures() * 2 + 1 {
            return Err("The album is missing entry data or contains unexpected files.".into());
        }
        Ok(Self { album, archive })
    }

    pub(crate) fn document(&mut self, id: u64) -> Result<Document, String> {
        self.album.filled(id)?;
        let name = project_name(id);
        let bytes = self
            .archive
            .by_name(&name)
            .map_err(|e| format!("Cannot read album entry: {e}"))?
            .size();
        if bytes > document::MAX_PROJECT_BYTES {
            return Err("This album entry exceeds the 512 MiB project limit.".into());
        }
        let file = self
            .archive
            .by_name_seek(&name)
            .map_err(|e| format!("Cannot open album entry: {e}"))?;
        document::read_project_from(BufReader::new(file), bytes, document::MAX_PROJECT_BYTES)
    }

    pub(crate) fn preview(&mut self, id: u64) -> Result<RgbaImage, String> {
        self.album.filled(id)?;
        let file = self
            .archive
            .by_name(&preview_name(id))
            .map_err(|e| format!("Cannot read album preview: {e}"))?;
        if file.size() > MAX_PREVIEW {
            return Err("This album preview exceeds its size limit.".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_PREVIEW + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let mut reader =
            image::ImageReader::with_format(io::Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(PREVIEW_SIZE[0]);
        limits.max_image_height = Some(PREVIEW_SIZE[1]);
        limits.max_alloc = Some(4 * MAX_PREVIEW);
        reader.limits(limits);
        reader
            .decode()
            .map(|image| image.into_rgba8())
            .map_err(|e| e.to_string())
    }
}

fn temporary(
    parent: &Path,
    album: &Album,
    mut source: Option<ZipArchive<File>>,
    replacement: Option<(u64, &Document)>,
) -> Result<tempfile::NamedTempFile, String> {
    let json = serde_json::to_vec(album).map_err(|e| e.to_string())?;
    if json.len() as u64 > MAX_MANIFEST {
        return Err("The album details exceed the supported size.".into());
    }
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut writer = ZipWriter::new(file.as_file_mut());
    writer
        .start_file(MANIFEST, STORED)
        .map_err(|e| e.to_string())?;
    writer.write_all(&json).map_err(|e| e.to_string())?;
    for entry in album.entries.iter().filter(|entry| entry.size.is_some()) {
        if let Some((id, document)) = replacement.filter(|(id, _)| *id == entry.id) {
            let mut project = tempfile::tempfile().map_err(|e| e.to_string())?;
            let resolved = crate::connectors::resolved(document);
            document::write_project_to(resolved.as_ref(), &mut BufWriter::new(&mut project))?;
            project.rewind().map_err(|e| e.to_string())?;
            writer
                .start_file(project_name(id), STORED)
                .map_err(|e| e.to_string())?;
            io::copy(&mut project, &mut writer).map_err(|e| e.to_string())?;
            let preview = render::thumbnail(document, PREVIEW_SIZE[0], PREVIEW_SIZE[1])?;
            writer
                .start_file(preview_name(id), STORED)
                .map_err(|e| e.to_string())?;
            image::codecs::png::PngEncoder::new(&mut writer)
                .write_image(
                    preview.as_raw(),
                    preview.width(),
                    preview.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| format!("Cannot write album preview: {e}"))?;
        } else {
            let source = source
                .as_mut()
                .ok_or("The album entry has no source data.")?;
            for name in [project_name(entry.id), preview_name(entry.id)] {
                let entry = source
                    .by_name(&name)
                    .map_err(|e| format!("Missing album entry {name}: {e}"))?;
                writer
                    .raw_copy_file(entry)
                    .map_err(|e| format!("Cannot copy album entry {name}: {e}"))?;
            }
        }
    }
    writer
        .finish()
        .map_err(|e| format!("Cannot finish album: {e}"))?;
    file.as_file()
        .sync_all()
        .map_err(|e| format!("Cannot flush album: {e}"))?;
    Ok(file)
}

pub(crate) fn create(
    folder: &Path,
    name: &str,
    layout: Layout,
) -> Result<(PathBuf, Album), String> {
    fs::create_dir_all(folder).map_err(|e| format!("Cannot create the album folder: {e}"))?;
    let album = Album {
        version: 1,
        next_id: 1,
        title: title(name, "Untitled album"),
        layout,
        entries: Vec::new(),
    };
    let mut stem = String::new();
    for character in album
        .title
        .chars()
        .filter(|c| !c.is_control() && !"/\\:*?\"<>|".contains(*c))
    {
        if stem.len() + character.len_utf8() > 160 {
            break;
        }
        stem.push(character);
    }
    let stem = stem.trim().trim_matches('.');
    let stem = if stem.is_empty() {
        "Untitled album"
    } else {
        stem
    };
    let mut file = temporary(folder, &album, None, None)?;
    for index in 0..10000 {
        let label = if index == 0 {
            stem.to_owned()
        } else {
            format!("{stem} ({index})")
        };
        let path = crate::library_files::rename_target(&folder.join("capture.bedizalbum"), &label)
            .unwrap_or_else(|_| folder.join(format!("Album-{index}.bedizalbum")));
        match file.persist_noclobber(&path) {
            Ok(_) => return Ok((path, album)),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => file = error.file,
            Err(error) => return Err(format!("Cannot create album: {}", error.error)),
        }
    }
    Err("Could not choose a unique album filename.".into())
}

fn publish(
    path: &Path,
    reader: Reader,
    replacement: Option<(u64, &Document)>,
) -> Result<Album, String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    temporary(parent, &reader.album, Some(reader.archive), replacement)?
        .persist(path)
        .map_err(|e| format!("Cannot save album {}: {}", path.display(), e.error))?;
    Ok(reader.album)
}

pub(crate) fn append(
    path: &Path,
    document: &Document,
    proposed_title: &str,
) -> Result<Album, String> {
    let mut reader = Reader::open(path)?;
    let index = match reader
        .album
        .entries
        .iter()
        .position(|entry| entry.size.is_none())
    {
        Some(index) => index,
        None => reader.album.add_empty(proposed_title)?,
    };
    let entry = &mut reader.album.entries[index];
    entry.size = Some(document.output_size());
    let id = entry.id;
    publish(path, reader, Some((id, document)))
}

pub(crate) fn save_entry(path: &Path, id: u64, document: &Document) -> Result<Album, String> {
    let mut reader = Reader::open(path)?;
    reader.album.filled(id)?;
    let index = reader.album.index(id)?;
    reader.album.entries[index].size = Some(document.output_size());
    publish(path, reader, Some((id, document)))
}

pub(crate) fn change(path: &Path, change: Change) -> Result<Album, String> {
    let mut reader = Reader::open(path)?;
    let album = &mut reader.album;
    match change {
        Change::Placeholder => {
            album.add_empty("")?;
        }
        Change::Title(name) => album.title = title(&name, &album.title),
        Change::Details {
            id,
            title: name,
            note,
        } => {
            let index = album.index(id)?;
            let entry = &mut album.entries[index];
            entry.title = title(&name, &entry.title);
            entry.note = note.trim().chars().take(NOTE_LIMIT).collect();
        }
        Change::Move { id, index } => {
            let from = album.index(id)?;
            let index = index.min(album.entries.len() - 1);
            if from != index {
                let entry = album.entries.remove(from);
                album.entries.insert(index, entry);
            }
        }
        Change::Remove(id) => {
            let index = album.index(id)?;
            album.entries.remove(index);
        }
        Change::Layout(layout) => album.layout = layout,
    }
    publish(path, reader, None)
}

#[path = "album_pdf.rs"]
mod pdf;
pub(crate) use pdf::export_pdf;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document::AnnotationKind, templates};
    use std::sync::Arc;

    fn capture(color: [u8; 4]) -> Document {
        Document {
            image: Arc::new(RgbaImage::from_pixel(48, 32, image::Rgba(color))),
            annotations: vec![],
            geometry: Default::default(),
        }
    }

    #[test]
    fn capture_destinations_keep_selection_and_independent_pause_state_across_restart() {
        let mut preferences = CapturePreferences::default();
        let paths = (0..=MAX_READY_ALBUMS)
            .map(|index| PathBuf::from(format!("Album {index}.bedizalbum")))
            .collect::<Vec<_>>();
        let first = &paths[0];
        let second = &paths[1];
        let extra = &paths[MAX_READY_ALBUMS];
        for path in &paths[..MAX_READY_ALBUMS] {
            assert!(preferences.set_ready(path, "Ready album", true));
        }
        assert!(!preferences.set_ready(extra, "Extra", true));
        assert!(!preferences.choose(Some(extra)));
        assert!(preferences.choose(Some(first)));
        assert!(preferences.set_ready(second, "Paused album", false));
        assert_eq!(preferences.selected().unwrap().0, first);
        assert!(preferences.set_ready(extra, "Extra", true));

        let bytes = serde_json::to_vec(&preferences).unwrap();
        let mut restored: CapturePreferences = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.ready(second), Some(false));
        assert_eq!(restored.selected(), Some((first.as_path(), "Ready album")));
        assert!(!restored.set_ready(second, "Paused album", true));
        assert!(restored.set_ready(first, "Ready album", false));
        assert!(restored.selected().is_none());
        assert!(restored.set_ready(second, "Resumed album", true));
        assert!(restored.choose(Some(second)));
        let moved = PathBuf::from("Moved album.bedizalbum");
        restored.remap(|path| {
            if path == second {
                *path = moved.clone();
            }
        });
        assert_eq!(
            restored.selected(),
            Some((moved.as_path(), "Resumed album"))
        );
        restored.forget(std::slice::from_ref(&moved));
        assert!(restored.selected().is_none());
        assert_eq!(restored.ready(extra), Some(true));
    }

    #[test]
    fn collection_fills_placeholders_and_keeps_entries_editable_in_one_file() {
        let temp = tempfile::tempdir().unwrap();
        let (path, _) = create(temp.path(), "Başlangıç", Layout::Walkthrough).unwrap();
        let placeholder = change(&path, Change::Placeholder).unwrap().entries[0].id;
        change(
            &path,
            Change::Details {
                id: placeholder,
                title: "İlk adım".into(),
                note: "Ekranı açın".into(),
            },
        )
        .unwrap();
        let red = capture([240, 31, 30, 255]);
        let blue = capture([30, 61, 244, 255]);
        let first = append(&path, &red, "").unwrap();
        assert_eq!(first.entries[0].title, "İlk adım");
        assert_eq!(first.entries[0].note, "Ekranı açın");
        let second = append(&path, &blue, "").unwrap().entries[1].id;
        change(
            &path,
            Change::Move {
                id: second,
                index: 0,
            },
        )
        .unwrap();

        let mut annotated = red.clone();
        let mut label = templates::annotation(AnnotationKind::Text, [5., 5., 38., 22.]);
        label.text = "Done".into();
        label.font_size = 14.;
        annotated.annotations.push(label);
        save_entry(&path, placeholder, &annotated).unwrap();

        let mut reopened = Reader::open(&path).unwrap();
        assert_eq!(reopened.album.captures(), 2);
        assert_eq!(reopened.album.entries[0].id, second);
        assert_eq!(reopened.document(second).unwrap().image, blue.image);
        assert_eq!(
            reopened.document(placeholder).unwrap().edit_state(),
            annotated.edit_state()
        );
        assert_eq!(
            reopened.preview(second).unwrap().get_pixel(0, 0).0,
            [30, 61, 244, 255]
        );
        let library = crate::library::scan(temp.path()).unwrap();
        assert_eq!(library.entries.len(), 1);
        assert_eq!(library.entries[0].kind, crate::library::EntryKind::Album);
    }

    #[test]
    fn failed_entry_write_preserves_the_published_album() {
        let temp = tempfile::tempdir().unwrap();
        let (path, _) = create(temp.path(), "Failure", Layout::Walkthrough).unwrap();
        append(&path, &capture([255; 4]), "").unwrap();
        let before = fs::read(&path).unwrap();
        let mut invalid = capture([0; 4]);
        invalid.image = Arc::new(RgbaImage::new(0, 0));
        assert!(append(&path, &invalid, "").is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn contact_sheet_export_creates_another_page_when_the_first_is_full() {
        let temp = tempfile::tempdir().unwrap();
        let (path, _) = create(temp.path(), "Field notes", Layout::ContactSheet).unwrap();
        for _ in 0..5 {
            append(&path, &capture([86, 150, 174, 255]), "").unwrap();
        }
        let pdf = temp.path().join("Field notes.pdf");
        export_pdf(&path, &pdf).unwrap();
        let bytes = fs::read(pdf).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(
            bytes
                .windows(b"/Count 2".len())
                .any(|chunk| chunk == b"/Count 2")
        );
        assert_eq!(Reader::open(&path).unwrap().album.captures(), 5);
    }

    #[test]
    fn new_layouts_paginate_and_keep_existing_album_entries() {
        let temp = tempfile::tempdir().unwrap();
        let (path, _) = create(temp.path(), "Release notes", Layout::Walkthrough).unwrap();
        let image = capture([86, 150, 174, 255]);
        let id = append(&path, &image, "Original capture").unwrap().entries[0].id;
        for _ in 1..7 {
            change(&path, Change::Placeholder).unwrap();
        }
        for (layout, pages, landscape) in [
            (Layout::FieldNotes, 4, false),
            (Layout::Comparison, 4, true),
            (Layout::Storyboard, 2, true),
        ] {
            change(&path, Change::Layout(layout)).unwrap();
            let mut reader = Reader::open(&path).unwrap();
            assert_eq!(reader.album.layout, layout);
            assert_eq!(reader.album.entries.len(), 7);
            assert_eq!(reader.album.entries[0].title, "Original capture");
            assert_eq!(reader.document(id).unwrap().image, image.image);

            let pdf = temp.path().join("Album.pdf");
            export_pdf(&path, &pdf).unwrap();
            let bytes = fs::read(pdf).unwrap();
            let page_count = format!("/Count {pages}");
            assert!(
                bytes
                    .windows(page_count.len())
                    .any(|chunk| chunk == page_count.as_bytes())
            );
            let media_box = String::from_utf8_lossy(&bytes)
                .lines()
                .find(|line| line.contains("/MediaBox"))
                .unwrap()
                .split_once('[')
                .unwrap()
                .1
                .trim_end_matches(']')
                .split_whitespace()
                .map(|coordinate| coordinate.parse::<f32>().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(media_box[2] > media_box[3], landscape);
        }
    }
}
