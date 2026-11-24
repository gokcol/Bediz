//! Album collection, organization, and hand-off to the existing image editor.
use super::*;
use crate::{
    album::{self, Album, Change, Layout},
    appearance::TileTone,
};
use std::{collections::HashMap, path::Path};

#[path = "album_setup.rs"]
mod setup;

#[derive(Clone)]
pub(super) struct Target {
    pub path: PathBuf,
    pub title: String,
}

#[derive(Clone)]
pub(super) struct EditTarget {
    pub path: PathBuf,
    pub id: u64,
}

struct Opened {
    path: PathBuf,
    album: Album,
    previews: HashMap<u64, Option<egui::TextureHandle>>,
}

#[derive(Default)]
pub(super) struct State {
    open: Option<Opened>,
    pub editing: Option<EditTarget>,
    setup: bool,
    name: String,
    layout: Layout,
    details: Option<(u64, String, String)>,
    remove: Option<u64>,
    preview_loading: bool,
    revision: u64,
}

pub(super) enum Event {
    Opened(Result<(PathBuf, Album), String>),
    Changed(PathBuf, Result<Album, String>),
    EntryOpened(EditTarget, Result<(Document, String), String>),
    EntrySaved(EditTarget, document::EditState, Result<Album, String>),
    Exported(PathBuf, Result<(), String>),
    Previews(u64, Vec<(u64, Option<egui::ColorImage>)>),
    Captured(Target, CaptureResult),
}

pub(super) enum CaptureResult {
    Collected(Album),
    Standalone(CaptureOutput, String),
    Failed(String),
}

#[derive(Clone)]
struct EntryDrag(u64);

impl State {
    pub(super) fn remap(&mut self, prepared: &crate::storage_move::PreparedMove) {
        if let Some(open) = &mut self.open {
            prepared.remap(&mut open.path);
        }
        if let Some(target) = &mut self.editing {
            prepared.remap(&mut target.path);
        }
    }

    pub(super) fn title(&self) -> Option<&str> {
        self.open.as_ref().map(|opened| opened.album.title.as_str())
    }

    pub(super) fn path(&self) -> Option<&Path> {
        self.open.as_ref().map(|opened| opened.path.as_path())
    }

    pub(super) fn rename(&mut self, old: &Path, new: &Path) {
        if let Some(open) = &mut self.open
            && open.path == old
        {
            open.path = new.to_owned();
        }
        if let Some(target) = &mut self.editing
            && target.path == old
        {
            target.path = new.to_owned();
        }
    }

    pub(super) fn forget(&mut self, paths: &[PathBuf]) -> bool {
        // A missing entry being edited keeps its association so Save fails
        // visibly and Save Project As can recover it to a separate file.
        let close = self
            .open
            .as_ref()
            .is_some_and(|open| paths.contains(&open.path));
        if close {
            self.open = None;
            self.details = None;
            self.remove = None;
            self.revision += 1;
        }
        close
    }

    fn apply(&mut self, path: &Path, album: Album, changed_image: Option<u64>) {
        if let Some(open) = &mut self.open
            && open.path == path
        {
            open.album = album;
            self.revision += 1;
            open.previews.retain(|id, _| {
                Some(*id) != changed_image && open.album.entries.iter().any(|entry| entry.id == *id)
            });
        }
    }
}

impl MyImageApp {
    fn album_work(
        &mut self,
        ctx: &egui::Context,
        status: &str,
        work: impl FnOnce() -> Event + Send + 'static,
    ) {
        self.busy = true;
        self.error = None;
        self.status = status.into();
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(WorkEvent::Album(work()));
            crate::runtime::wake_root(&ctx);
        });
    }

    pub(super) fn begin_open_album(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.album_work(ctx, "Opening album…", move || {
            Event::Opened(album::Reader::open(&path).map(|reader| (path, reader.album)))
        });
    }

    pub(super) fn open_album_entry(&mut self, ctx: &egui::Context, path: PathBuf, id: u64) {
        if !self.can_replace_image(ctx, PendingAction::AlbumEntry(path.clone(), id)) {
            return;
        }
        self.album_work(ctx, "Opening album entry…", move || {
            let result = (|| {
                let mut reader = album::Reader::open(&path)?;
                let document = reader.document(id)?;
                let title = reader
                    .album
                    .entries
                    .iter()
                    .find(|entry| entry.id == id)
                    .ok_or("This entry is no longer in the album.")?
                    .title
                    .clone();
                Ok((document, title))
            })();
            Event::EntryOpened(EditTarget { path, id }, result)
        });
    }

    pub(super) fn save_album_entry(&mut self, ctx: &egui::Context, target: EditTarget) {
        let Some(document) = self.document.clone() else {
            return;
        };
        self.project_session.saving = true;
        self.album_work(ctx, "Saving album entry…", move || {
            let result = album::save_entry(&target.path, target.id, &document);
            Event::EntrySaved(target, document.edit_state(), result)
        });
    }

    fn change_album(&mut self, ctx: &egui::Context, path: PathBuf, change: Change) {
        if let Change::Remove(id) = &change
            && self
                .albums
                .editing
                .as_ref()
                .is_some_and(|target| target.path == path && target.id == *id)
            && self.has_unsaved_edits()
        {
            self.error = Some("Save the open entry before removing it from the album.".into());
            return;
        }
        self.album_work(ctx, "Saving album…", move || {
            Event::Changed(path.clone(), album::change(&path, change))
        });
    }

    fn import_album_image(&mut self, ctx: &egui::Context, path: PathBuf) {
        let Some(source) = rfd::FileDialog::new()
            .set_title("Add image or capture to album")
            .set_directory(&self.settings.output_dir)
            .add_filter(
                "Images and editable projects",
                &[
                    "myimage", "png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif", "heic",
                    "heif",
                ],
            )
            .pick_file()
        else {
            return;
        };
        self.album_work(ctx, "Adding image to album…", move || {
            let result = (|| {
                let prepared = crate::image_loading::LoadCache::default().open(&source, 384)?;
                let title = source
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default();
                album::append(&path, &prepared.document, title)
            })();
            Event::Changed(path, result)
        });
    }

    fn export_album_pdf(&mut self, ctx: &egui::Context, path: PathBuf, title: &str) {
        let mut dialog = rfd::FileDialog::new()
            .set_title("Export album PDF")
            .set_file_name(format!("{title}.pdf"))
            .add_filter("PDF", &["pdf"]);
        if let Some(parent) = path.parent() {
            dialog = dialog.set_directory(parent);
        }
        let Some(mut destination) = dialog.save_file() else {
            return;
        };
        if destination.extension().is_none() {
            destination.set_extension("pdf");
        }
        if !destination
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            self.error = Some("Choose a filename ending in .pdf.".into());
            return;
        }
        self.album_work(ctx, "Exporting album PDF…", move || {
            let result = album::export_pdf(&path, &destination);
            Event::Exported(destination, result)
        });
    }

    pub(super) fn queue_album_capture(
        &mut self,
        ctx: &egui::Context,
        target: Target,
        at: DateTime<Local>,
        prepare: impl FnOnce() -> Result<image::RgbaImage, String> + Send + 'static,
    ) {
        self.restore_window(ctx, self.desktop.is_some());
        let size = self.settings.capture_output_size;
        let folder = self.settings.output_dir.clone();
        self.album_work(ctx, &format!("Adding to {}…", target.title), move || {
            let result = match prepare() {
                Ok(image) => {
                    let image = Arc::new(size.apply(image));
                    let document = Document {
                        image: image.clone(),
                        annotations: Vec::new(),
                        geometry: Default::default(),
                    };
                    match album::append(&target.path, &document, "") {
                        Ok(album) => CaptureResult::Collected(album),
                        Err(error) => {
                            let path = crate::project_store::capture(image.clone(), &folder, at);
                            CaptureResult::Standalone(
                                CaptureOutput {
                                    image,
                                    path,
                                    destination: capture::CaptureDestination::Editor,
                                    clipboard: Ok(()),
                                },
                                error,
                            )
                        }
                    }
                }
                Err(error) => CaptureResult::Failed(error),
            };
            Event::Captured(target, result)
        });
    }

    fn album_changed(&mut self, path: &Path, album: Album, changed_image: Option<u64>) {
        self.albums.apply(path, album, changed_image);
        self.library.file_saved(path);
        self.remember_recent(path.to_owned());
    }

    pub(super) fn album_event(&mut self, ctx: &egui::Context, event: Event) {
        if let Event::Previews(revision, previews) = event {
            self.albums.preview_loading = false;
            if revision == self.albums.revision
                && let Some(open) = &mut self.albums.open
            {
                for (id, pixels) in previews {
                    open.previews.insert(
                        id,
                        pixels.map(|image| {
                            ctx.load_texture(
                                format!("album-preview-{id}"),
                                image,
                                egui::TextureOptions::LINEAR,
                            )
                        }),
                    );
                }
            }
            return;
        }
        self.busy = false;
        match event {
            Event::Opened(Ok((path, album))) => {
                self.albums.open = Some(Opened {
                    path: path.clone(),
                    album,
                    previews: HashMap::new(),
                });
                self.albums.revision += 1;
                self.albums.setup = false;
                self.albums.details = None;
                self.albums.remove = None;
                self.view = View::Album;
                self.library.file_saved(&path);
                self.remember_opened_recent(path);
                self.status = "Album ready".into();
            }
            Event::Changed(path, Ok(album)) => {
                if self.albums.editing.as_ref().is_some_and(|target| {
                    target.path == path && !album.entries.iter().any(|entry| entry.id == target.id)
                }) {
                    self.clear_current_image();
                }
                self.album_changed(&path, album, None);
                self.status = "Album saved".into();
            }
            Event::EntryOpened(target, Ok((document, title))) => {
                self.install_document(document, Some(target.path.clone()));
                self.last_path = None;
                self.draft_title = title;
                self.albums.editing = Some(target);
                self.view = View::Editor;
                self.status = "Album entry · edits save to the album".into();
            }
            Event::EntrySaved(target, saved, Ok(album)) => {
                self.album_changed(&target.path, album, Some(target.id));
                self.history.mark_saved(&saved);
                self.project_session = Default::default();
                if !self.has_unsaved_edits() {
                    self.clear_recovery();
                }
                self.status = "Album entry saved".into();
                if let Some(action) = self.after_save.take() {
                    self.perform_action(ctx, action, false);
                }
            }
            Event::EntrySaved(_, _, Err(error)) => {
                self.error = Some(error);
                self.status = "Album entry not saved · edits are still open".into();
                self.project_save_failed();
                self.pending_action = self.after_save.take();
            }
            Event::Exported(path, Ok(())) => {
                self.status = format!("Album PDF saved to {}", path.display())
            }
            Event::Captured(target, CaptureResult::Collected(album)) => {
                self.status = format!("Added to {} · {} captures", target.title, album.captures());
                self.album_changed(&target.path, album, None);
                if self.hidden.load(Ordering::Relaxed)
                    && let Some(desktop) = &self.desktop
                {
                    desktop.capture_feedback(&self.status);
                }
            }
            Event::Captured(target, CaptureResult::Standalone(mut output, error)) => {
                self.settings
                    .album_capture
                    .set_ready(&target.path, &target.title, false);
                self.persist_settings();
                let error = format!("Could not add to {}: {error}", target.title);
                match &mut output.path {
                    Ok(path) => {
                        self.library.file_saved(path);
                        self.remember_recent(path.clone());
                        self.status = format!("{} paused · capture saved separately", target.title);
                        self.error = Some(error);
                        if self.hidden.load(Ordering::Relaxed)
                            && let Some(desktop) = &self.desktop
                        {
                            desktop.capture_feedback(&self.status);
                        }
                    }
                    Err(save) => {
                        *save = format!("{error}. The separate save also failed: {save}");
                        self.open_captured_image(ctx, output);
                    }
                }
            }
            Event::Captured(_, CaptureResult::Failed(error)) => {
                self.error = Some(error);
                self.restore_window(ctx, false);
            }
            Event::Opened(Err(error))
            | Event::Changed(_, Err(error))
            | Event::EntryOpened(_, Err(error))
            | Event::Exported(_, Err(error)) => {
                self.error = Some(error);
                self.status = "Album operation could not finish".into();
            }
            Event::Previews(_, _) => unreachable!(),
        }
        if !self.busy && std::mem::take(&mut self.quit_after_work) {
            self.request_quit(ctx);
        }
    }

    pub(super) fn album_workspace_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(open) = &mut self.albums.open else {
            self.view = View::Create;
            return;
        };
        let path = open.path.clone();
        let title = open.album.title.clone();
        let ready = self.settings.album_capture.ready(&path);
        let ready_count = self.settings.album_capture.destinations().count();
        let can_enable = ready == Some(true) || ready_count < album::MAX_READY_ALBUMS;
        let (collection_action, collection_icon) = match ready {
            Some(true) => ("Pause collecting", Icon::Pause),
            Some(false) => ("Resume collecting", Icon::Play),
            None => ("Enable collecting", Icon::Capture),
        };
        let mut toggle_collection = false;
        let mut requested = None;
        let mut open_entry = None;
        let mut export = false;
        let mut import = false;
        let mut previews = Vec::new();
        ui.horizontal_wrapped(|ui| {
            if appearance::action_button(ui, "Create", Icon::Back, true, false).clicked() {
                self.albums.setup = true;
                self.view = View::Create;
            }
            ui.separator();
            if appearance::action_button(
                ui,
                collection_action,
                collection_icon,
                can_enable,
                ready != Some(true),
            )
            .on_hover_text(if can_enable {
                "Manage whether this album appears in the capture destination menu"
            } else {
                "Pause another album before enabling this one. Up to 10 can be ready."
            })
            .clicked()
            {
                toggle_collection = true;
            }
            if appearance::action_button(ui, "Add placeholder", Icon::Create, true, false).clicked()
            {
                requested = Some(Change::Placeholder);
            }
            import = appearance::action_button(ui, "Add image…", Icon::Open, true, false).clicked();
            export = appearance::action_button(
                ui,
                "Export PDF…",
                Icon::SaveAs,
                !open.album.entries.is_empty(),
                false,
            )
            .clicked();
            egui::ComboBox::from_id_salt("album-layout")
                .selected_text(open.album.layout.label())
                .show_ui(ui, |ui| {
                    for layout in Layout::ALL {
                        if ui
                            .selectable_label(open.album.layout == layout, layout.label())
                            .clicked()
                            && open.album.layout != layout
                        {
                            requested = Some(Change::Layout(layout));
                        }
                    }
                });
        });
        ui.add_space(6.);
        let collecting = self
            .settings
            .album_capture
            .selected()
            .is_some_and(|(selected, _)| selected == path);
        if let Some(ready) = ready {
            ui.label(
                egui::RichText::new(if collecting {
                    "Current capture destination"
                } else if ready {
                    "Ready in the capture destination menu"
                } else {
                    "Collection paused"
                })
                .color(if ready {
                    appearance::satin_ink(ctx, TileTone::Teal)
                } else {
                    appearance::palette(ctx).muted
                }),
            );
        }
        let captures = open.album.captures();
        let placeholders = open.album.entries.len() - captures;
        if captures + placeholders > 0 {
            ui.weak(if placeholders == 0 {
                format!("{captures} captures")
            } else {
                format!("{captures} captures · {placeholders} waiting")
            });
        }
        ui.separator();
        if let Some((id, title, note)) = &mut self.albums.details {
            let mut close = false;
            ui.horizontal_wrapped(|ui| {
                ui.label("Title");
                ui.add(
                    egui::TextEdit::singleline(title)
                        .char_limit(album::TITLE_LIMIT)
                        .desired_width(240.),
                );
                ui.label("Note");
                ui.add(
                    egui::TextEdit::singleline(note)
                        .char_limit(album::NOTE_LIMIT)
                        .desired_width(300.),
                );
                if appearance::action_button(ui, "Save details", Icon::Save, true, true).clicked() {
                    requested = Some(Change::Details {
                        id: *id,
                        title: title.clone(),
                        note: note.clone(),
                    });
                    close = true;
                }
                close |=
                    appearance::action_button(ui, "Cancel", Icon::Close, true, false).clicked();
            });
            if close {
                self.albums.details = None;
            }
            ui.separator();
        }
        if let Some(id) = self.albums.remove {
            ui.horizontal_wrapped(|ui| {
                ui.label("Remove this entry and its artwork from the album?");
                if appearance::action_button(ui, "Remove entry", Icon::Trash, true, true).clicked()
                {
                    requested = Some(Change::Remove(id));
                    self.albums.remove = None;
                }
                if appearance::action_button(ui, "Cancel", Icon::Close, true, false).clicked() {
                    self.albums.remove = None;
                }
            });
            ui.separator();
        }
        if open.album.entries.is_empty() {
            ui.add_space(28.);
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(52.), egui::Sense::hover());
                appearance::sculpted_icon(
                    ui.painter(),
                    rect.center(),
                    Icon::Capture,
                    TileTone::Teal,
                    46.,
                    None,
                );
                ui.add_space(8.);
                ui.heading("Ready for your first capture");
                ui.label(if collecting {
                    "Use your capture shortcut to add a screenshot."
                } else if ready == Some(true) {
                    "Choose this album in the capture destination menu, or add an existing image."
                } else {
                    "Enable collecting, add a placeholder, or bring in an existing image."
                });
            });
        } else {
            let layout = open.album.layout;
            let minimum_width = 250.;
            let gap = ui.spacing().item_spacing.x;
            let fits = ((ui.available_width() + gap) / (minimum_width + gap)) as usize;
            let columns = layout.page().columns.min(fits.max(1));
            let rows = open.album.entries.len().div_ceil(columns);
            let row_height = if layout == Layout::Comparison {
                270.
            } else {
                230.
            };
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .id_salt(("album-rows", &path))
                .show_rows(ui, row_height, rows, |ui, visible_rows| {
                    let gap = ui.spacing().item_spacing.x;
                    let width = ((ui.available_width() - gap * (columns - 1) as f32)
                        / columns as f32)
                        .max(100.);
                    let mut visible_ids = Vec::new();
                    for row in visible_rows {
                        ui.horizontal(|ui| {
                            for index in
                                row * columns..((row + 1) * columns).min(open.album.entries.len())
                            {
                                let entry = &open.album.entries[index];
                                visible_ids.push(entry.id);
                                if entry.size.is_some() && !open.previews.contains_key(&entry.id) {
                                    previews.push(entry.id);
                                }
                                ui.push_id(entry.id, |ui| {
                                    let card = album_card(
                                        ui,
                                        entry,
                                        open.previews.get(&entry.id).and_then(Option::as_ref),
                                        index,
                                        open.album.entries.len(),
                                        width,
                                        row_height,
                                    );
                                    if card.edit_image {
                                        open_entry = Some(entry.id);
                                    }
                                    if card.edit_details {
                                        self.albums.details = Some((
                                            entry.id,
                                            entry.title.clone(),
                                            entry.note.clone(),
                                        ));
                                    }
                                    if card.remove {
                                        self.albums.remove = Some(entry.id);
                                    }
                                    if let Some((id, index)) = card.destination {
                                        requested = Some(Change::Move { id, index });
                                    }
                                });
                            }
                        });
                    }
                    // Preview textures are bounded by the viewport. Rescrolling
                    // only decodes the small PNGs already stored in the album.
                    if open.previews.len() > 48 {
                        open.previews.retain(|id, _| visible_ids.contains(id));
                    }
                });
        }
        if !previews.is_empty() && !self.albums.preview_loading {
            self.albums.preview_loading = true;
            let source = path.clone();
            let revision = self.albums.revision;
            let tx = self.work_tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let mut reader = album::Reader::open(&source).ok();
                let previews = previews
                    .into_iter()
                    .map(|id| {
                        let image = reader
                            .as_mut()
                            .and_then(|reader| reader.preview(id).ok())
                            .map(|image| crate::image_loading::color_image(&image));
                        (id, image)
                    })
                    .collect();
                let _ = tx.send(WorkEvent::Album(Event::Previews(revision, previews)));
                ctx.request_repaint();
            });
        }
        if toggle_collection
            && self
                .settings
                .album_capture
                .set_ready(&path, &title, ready != Some(true))
        {
            self.status = if ready == Some(true) {
                format!("Collection paused for {title}")
            } else {
                format!("{title} is ready in the capture destination menu")
            };
            self.persist_settings();
        }
        if let Some(id) = open_entry {
            self.open_album_entry(ctx, path, id);
        } else if let Some(change) = requested {
            self.change_album(ctx, path, change);
        } else if import {
            self.import_album_image(ctx, path);
        } else if export {
            self.export_album_pdf(ctx, path, &title);
        }
    }
}

#[derive(Default)]
struct CardAction {
    edit_image: bool,
    edit_details: bool,
    remove: bool,
    destination: Option<(u64, usize)>,
}

fn album_card(
    ui: &mut egui::Ui,
    entry: &album::Entry,
    preview: Option<&egui::TextureHandle>,
    index: usize,
    count: usize,
    width: f32,
    height: f32,
) -> CardAction {
    let mut action = CardAction::default();
    let palette = appearance::palette(ui.ctx());
    let dragging = egui::DragAndDrop::has_payload_of_type::<EntryDrag>(ui.ctx());
    let card = egui::Frame::new()
        .fill(appearance::surface(ui.ctx()))
        .stroke(Stroke::new(1., palette.border))
        .corner_radius(10)
        .inner_margin(10)
        .show(ui, |ui| {
            let content_width = (width - 22.).max(78.);
            ui.set_width(content_width);
            ui.set_min_height(height - 22.);
            ui.horizontal(|ui| {
                let grip = ui.add_sized(
                    [24., 24.],
                    egui::Button::new("")
                        .frame(false)
                        .sense(egui::Sense::drag()),
                );
                grip.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        ui.is_enabled(),
                        "Drag to reorder",
                    )
                });
                appearance::icon(
                    ui.painter(),
                    grip.rect.center(),
                    Icon::List,
                    palette.muted,
                    16.,
                );
                grip.dnd_set_drag_payload(EntryDrag(entry.id));
                grip.on_hover_text("Drag to reorder")
                    .on_hover_cursor(egui::CursorIcon::Grab);
                ui.add(egui::Label::new(egui::RichText::new(&entry.title).strong()).truncate())
                    .on_hover_text(&entry.title);
            });
            let (rect, response) = ui.allocate_exact_size(
                Vec2::new(content_width, height - 108.),
                egui::Sense::click(),
            );
            ui.painter().rect_filled(rect, 6, palette.paper);
            if let Some(texture) = preview {
                let size = texture.size_vec2();
                let fit = (rect.width() / size.x).min(rect.height() / size.y);
                ui.painter().image(
                    texture.id(),
                    Rect::from_center_size(rect.center(), size * fit),
                    Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                    Color32::WHITE,
                );
            } else {
                appearance::icon(
                    ui.painter(),
                    rect.center() - Vec2::new(0., 10.),
                    Icon::Capture,
                    palette.muted,
                    25.,
                );
                ui.painter().text(
                    rect.center() + Vec2::new(0., 17.),
                    egui::Align2::CENTER_CENTER,
                    if entry.size.is_some() {
                        "Capture preview"
                    } else {
                        "Waiting for capture"
                    },
                    egui::FontId::proportional(12.),
                    palette.muted,
                );
            }
            action.edit_image = response.double_clicked() && entry.size.is_some();
            ui.add(
                egui::Label::new(if entry.note.is_empty() {
                    " "
                } else {
                    &entry.note
                })
                .truncate(),
            )
            .on_hover_text(&entry.note);
            ui.horizontal(|ui| {
                action.edit_image |= ui
                    .add_enabled_ui(entry.size.is_some(), |ui| {
                        appearance::icon_button(ui, "Edit capture", Icon::Edit).clicked()
                    })
                    .inner;
                action.edit_details =
                    appearance::icon_button(ui, "Edit title and note", Icon::Pencil).clicked();
                if ui
                    .add_enabled_ui(index > 0, |ui| {
                        appearance::icon_button(ui, "Move earlier", Icon::Up).clicked()
                    })
                    .inner
                {
                    action.destination = Some((entry.id, index - 1));
                }
                if ui
                    .add_enabled_ui(index + 1 < count, |ui| {
                        appearance::icon_button(ui, "Move later", Icon::Down).clicked()
                    })
                    .inner
                {
                    action.destination = Some((entry.id, index + 1));
                }
                action.remove = appearance::icon_button(ui, "Remove entry", Icon::Trash).clicked();
            });
        });
    if dragging && card.response.contains_pointer() {
        ui.painter().rect_stroke(
            card.response.rect,
            10,
            Stroke::new(2., palette.accent),
            egui::StrokeKind::Inside,
        );
    }
    if let Some(source) = card.response.dnd_release_payload::<EntryDrag>()
        && source.0 != entry.id
    {
        action.destination = Some((source.0, index));
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finish(app: &mut MyImageApp, ctx: &egui::Context) {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while app.busy && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            app.handle_events(ctx);
        }
        assert!(!app.busy);
    }

    #[test]
    fn captured_images_route_into_the_album_and_entry_edits_save_back_to_it() {
        let storage = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.settings.output_dir = storage.path().to_owned();
        app.settings.capture_destination = capture::CaptureDestination::Clipboard;
        app.settings.capture_output_size = crate::capture_size::OutputSize::Half;
        let (path, _) = album::create(storage.path(), "Demo", Layout::Walkthrough).unwrap();
        let (other, _) = album::create(storage.path(), "Other", Layout::Walkthrough).unwrap();
        app.settings.album_capture.set_ready(&path, "Demo", true);
        app.settings.album_capture.set_ready(&other, "Other", true);
        assert!(app.settings.album_capture.selected().is_none());
        app.settings.album_capture.choose(Some(&path));
        app.view = View::Home;
        app.save_captured_image(&ctx, image::RgbaImage::new(64, 48), Local::now());
        finish(&mut app, &ctx);
        assert!(app.view == View::Home);
        assert!(app.document.is_none());
        assert_eq!(app.settings.recent_files, std::slice::from_ref(&path));
        assert_eq!(
            crate::library::scan(storage.path()).unwrap().entries.len(),
            2
        );
        assert!(
            album::Reader::open(&other)
                .unwrap()
                .album
                .entries
                .is_empty()
        );

        let reader = album::Reader::open(&path).unwrap();
        let entry = reader.album.entries[0].clone();
        assert_eq!(entry.size, Some([32, 24]));
        app.open_path(&ctx, path.clone(), true);
        finish(&mut app, &ctx);
        assert!(app.view == View::Album);
        app.open_album_entry(&ctx, path.clone(), entry.id);
        finish(&mut app, &ctx);
        let doc = app.document.as_mut().unwrap();
        let before = doc.edit_state();
        let mut label =
            crate::templates::annotation(document::AnnotationKind::Text, [1., 1., 30., 22.]);
        label.text = "Local edit".into();
        label.font_size = 10.;
        doc.annotations.push(label);
        app.history.commit(before, doc);
        assert!(app.has_unsaved_edits());
        app.open_path(&ctx, path.clone(), true);
        finish(&mut app, &ctx);
        assert!(app.view == View::Album);
        assert!(!app.has_unsaved_edits());
        let saved = album::Reader::open(&path)
            .unwrap()
            .document(entry.id)
            .unwrap();
        assert_eq!(saved.annotations[0].text, "Local edit");
        assert_eq!(
            crate::library::scan(storage.path()).unwrap().entries.len(),
            2
        );
        app.settings.album_capture.choose(Some(&other));
        app.save_captured_image(&ctx, image::RgbaImage::new(64, 48), Local::now());
        finish(&mut app, &ctx);
        assert_eq!(album::Reader::open(&path).unwrap().album.captures(), 1);
        assert_eq!(album::Reader::open(&other).unwrap().album.captures(), 1);
    }

    #[test]
    fn a_missing_album_routes_the_capture_to_a_recoverable_standalone_project() {
        let storage = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.settings.output_dir = storage.path().to_owned();
        let missing = storage.path().join("Missing.bedizalbum");
        let other = storage.path().join("Other.bedizalbum");
        app.settings
            .album_capture
            .set_ready(&missing, "Missing album", true);
        app.settings.album_capture.set_ready(&other, "Other", true);
        app.settings.album_capture.choose(Some(&missing));
        app.save_captured_image(&ctx, image::RgbaImage::new(48, 32), Local::now());
        finish(&mut app, &ctx);
        assert!(app.settings.album_capture.selected().is_none());
        assert_eq!(app.settings.album_capture.ready(&missing), Some(false));
        assert_eq!(app.settings.album_capture.ready(&other), Some(true));
        let paths = &app.settings.recent_files;
        assert_eq!(paths.len(), 1);
        assert_eq!(
            document::load_project(&paths[0]).unwrap().output_size(),
            [48, 32]
        );
    }

    #[test]
    fn dropping_on_an_album_card_targets_that_entry_position() {
        let ctx = egui::Context::default();
        let entry = album::Entry {
            id: 2,
            title: "Destination".into(),
            note: String::new(),
            size: None,
        };
        let pointer = egui::pos2(100., 110.);
        let button = |pressed| egui::Event::PointerButton {
            pos: pointer,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut action = CardAction::default();
        for events in [
            vec![],
            vec![egui::Event::PointerMoved(pointer), button(true)],
            vec![button(false)],
        ] {
            if events.len() == 1 {
                egui::DragAndDrop::set_payload(&ctx, EntryDrag(1));
            }
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400., 320.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        action = album_card(ui, &entry, None, 1, 2, 320., 230.);
                    });
                },
            );
        }
        assert_eq!(action.destination, Some((1, 1)));
    }
}
