use super::*;
use crate::app::windows_project::{Clip, Layer, MAX_DURATION_MS, Project};
use std::{path::Path, sync::atomic::AtomicU64};

#[derive(Default)]
pub(super) struct Timeline {
    project: Option<Project>,
    file: Option<PathBuf>,
    changed: bool,
    clip: usize,
    layer: Option<usize>,
    frame_request: Option<u64>,
    next_reference: u64,
    preview: Option<tempfile::TempDir>,
    seek: Option<i64>,
    render: Option<Render>,
}

struct Render {
    preview: bool,
    returned_to: i64,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
}

pub(super) enum Rendered {
    Preview(tempfile::TempDir),
    File(PathBuf),
}

enum Action {
    Annotate,
    Edit(usize),
    Delete(usize),
    Seek(i64),
    Trim(Clip),
    Split,
    RemoveClip,
    Save,
    Preview,
    Source,
    Export,
}

fn time(ms: i64) -> String {
    format!("{:02}:{:02}.{:03}", ms / 60_000, ms / 1000 % 60, ms % 1000)
}

impl Timeline {
    pub(super) fn opening_frame(&self) -> bool {
        self.frame_request.is_some()
    }

    pub(super) fn player_file(&self, original: &Path) -> PathBuf {
        if let Some(preview) = &self.preview {
            return preview.path().join("preview.mp4");
        }
        self.project
            .as_ref()
            .map_or_else(|| original.to_owned(), |project| project.source.clone())
    }
}

impl MyImageApp {
    pub(super) fn load_windows_video(&mut self, path: &Path) -> Result<bool, String> {
        if !path.is_file() {
            return Err(format!("Could not find the video at {}.", path.display()));
        }
        if self.video_path.as_deref() == Some(path) {
            if self.video_player.is_none() {
                self.video_open_pending = true;
            }
            return Ok(true);
        }
        if !self.allow_windows_video_change() {
            return Ok(false);
        }
        let project_file = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bedizvideo"));
        let project = project_file.then(|| Project::load(path)).transpose()?;
        // Release Media Engine's file handle before dropping a rendered preview.
        self.video_player = None;
        self.windows_timeline = Timeline {
            seek: project.as_ref().map(|project| project.clips[0].start),
            project,
            file: project_file.then(|| path.to_owned()),
            ..Default::default()
        };
        self.video_open_pending = true;
        self.error = None;
        Ok(true)
    }

    pub(super) fn allow_windows_video_change(&mut self) -> bool {
        if self.windows_timeline.render.is_some() {
            self.show_video_status("Finish or cancel the video render first.");
            return false;
        }
        if self.editing_video_frame() {
            return false;
        }
        if !self.windows_timeline.changed {
            return true;
        }
        match rfd::MessageDialog::new()
            .set_title("Save this video project?")
            .set_description(
                "Save the timeline and editable annotations before closing this video? Choose Yes to save, No to discard, or Cancel to keep editing.",
            )
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show()
        {
            rfd::MessageDialogResult::Yes => self.save_windows_video(),
            rfd::MessageDialogResult::No => true,
            _ => false,
        }
    }

    fn show_video_status(&mut self, message: &str) {
        self.status = message.into();
    }

    pub(super) fn save_windows_video(&mut self) -> bool {
        let Some(project) = &self.windows_timeline.project else {
            return false;
        };
        let file = self.windows_timeline.file.clone().or_else(|| {
            let stem = project
                .source
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            rfd::FileDialog::new()
                .set_title("Save Bediz video project")
                .set_file_name(format!("{stem}.bedizvideo"))
                .add_filter("Bediz video project", &["bedizvideo"])
                .save_file()
        });
        let Some(file) = file else {
            return false;
        };
        if !file
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bedizvideo"))
        {
            self.error = Some("Use the .bedizvideo extension for the editable project.".into());
            return false;
        }
        match project.save(&file) {
            Ok(()) => {
                self.windows_timeline.file = Some(file);
                self.windows_timeline.changed = false;
                self.show_video_status("Video project saved");
                true
            }
            Err(error) => {
                self.error = Some(format!("Could not save the video project: {error}"));
                false
            }
        }
    }

    pub(super) fn windows_timeline_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(player) = &mut self.video_player else {
            self.video_bounds = Some(ui.available_rect_before_wrap());
            return;
        };
        let native_time = player.position();
        if let Some((_, duration)) = native_time
            && self.windows_timeline.project.is_none()
            && let Some(path) = &self.video_path
            && (1..=MAX_DURATION_MS).contains(&duration)
        {
            let path = std::path::absolute(path).unwrap_or_else(|_| path.clone());
            self.windows_timeline.project = Some(Project::new(path, duration));
        }
        if native_time.is_some()
            && let Some(seek) = self.windows_timeline.seek.take()
        {
            player.seek(seek);
        }
        let source = self.windows_timeline.project.as_ref().and_then(|project| {
            native_time.map(|(position, _)| {
                if self.windows_timeline.preview.is_some() {
                    project.source_time(position)
                } else {
                    position.clamp(0, project.duration - 1)
                }
            })
        });
        let canvas = ui.available_rect_before_wrap();
        let height = canvas
            .height()
            .mul_add(0.38, 0.)
            .clamp(48., 123.)
            .min(canvas.height());
        let mut video_frame = canvas;
        video_frame.max.y -= height;
        let deck = Rect::from_min_max(
            egui::pos2(canvas.left(), video_frame.bottom() + 4.),
            canvas.max,
        );
        let mut action = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(deck), |ui| {
            player.controls(ui);
            ui.separator();
            let timeline = &mut self.windows_timeline;
            let Some(project) = &timeline.project else {
                return;
            };
            if timeline.opening_frame() {
                ui.spinner();
                ui.weak("Opening frame…");
                return;
            }
            if let Some(render) = &timeline.render {
                ui.horizontal(|ui| {
                    let fraction =
                        render.progress.load(Ordering::Relaxed) as f32 / project.length() as f32;
                    ui.add(egui::ProgressBar::new(fraction).desired_width(220.).text(
                        if render.preview {
                            "Rendering preview…"
                        } else {
                            "Exporting MP4…"
                        },
                    ));
                    if appearance::icon_button(ui, "Cancel render", Icon::Close).clicked() {
                        render.cancel.store(true, Ordering::Relaxed);
                    }
                });
                ctx.request_repaint_after(Duration::from_millis(150));
                return;
            }
            let clip = project.clips[timeline.clip];
            let can_cut = source.is_some_and(|time| time > clip.start && clip.contains(time));
            ui.horizontal_wrapped(|ui| {
                if appearance::action_button(
                    ui,
                    "Annotate This Frame",
                    Icon::Pencil,
                    source.is_some(),
                    true,
                )
                .clicked()
                {
                    action = Some(Action::Annotate);
                }
                egui::ComboBox::from_id_salt("windows-video-clip")
                    .width(85.)
                    .selected_text(format!(
                        "Clip {} of {}",
                        timeline.clip + 1,
                        project.clips.len()
                    ))
                    .show_ui(ui, |ui| {
                        for (index, clip) in project.clips.iter().enumerate() {
                            if ui
                                .selectable_value(
                                    &mut timeline.clip,
                                    index,
                                    format!(
                                        "Clip {}  {} to {}",
                                        index + 1,
                                        time(clip.start),
                                        time(clip.end)
                                    ),
                                )
                                .changed()
                            {
                                action = Some(Action::Seek(clip.start));
                            }
                        }
                    });
                for (label, icon, trim_start) in [
                    ("Trim start to playhead", Icon::Back, true),
                    ("Trim end to playhead", Icon::Forward, false),
                ] {
                    if ui
                        .add_enabled_ui(can_cut, |ui| appearance::icon_button(ui, label, icon))
                        .inner
                        .clicked()
                        && let Some(source) = source
                    {
                        let range = if trim_start {
                            Clip {
                                start: source,
                                ..clip
                            }
                        } else {
                            Clip {
                                end: source,
                                ..clip
                            }
                        };
                        action = Some(Action::Trim(range));
                    }
                }
                if appearance::action_button(ui, "Split", Icon::Resize, can_cut, false).clicked() {
                    action = Some(Action::Split);
                }
                if ui
                    .add_enabled_ui(project.clips.len() > 1, |ui| {
                        appearance::icon_button(ui, "Remove selected clip", Icon::Trash)
                    })
                    .inner
                    .clicked()
                {
                    action = Some(Action::RemoveClip);
                }
                let selected = timeline.layer.and_then(|index| {
                    project
                        .layers
                        .get(index)
                        .map(|layer| format!("Layer {} · {}", index + 1, time(layer.start)))
                });
                ui.add_enabled_ui(!project.layers.is_empty(), |ui| {
                    egui::ComboBox::from_id_salt("windows-video-layer")
                        .width(115.)
                        .selected_text(selected.unwrap_or_else(|| "Annotations".into()))
                        .show_ui(ui, |ui| {
                            for (index, layer) in project.layers.iter().enumerate() {
                                if ui
                                    .selectable_value(
                                        &mut timeline.layer,
                                        Some(index),
                                        format!(
                                            "Layer {} · {} for {:.3} s",
                                            index + 1,
                                            time(layer.start),
                                            layer.duration as f64 / 1000.
                                        ),
                                    )
                                    .changed()
                                {
                                    action = Some(Action::Seek(layer.start));
                                }
                            }
                        });
                });
                if let Some(index) = timeline.layer {
                    if appearance::icon_button(ui, "Edit annotation", Icon::Edit).clicked() {
                        action = Some(Action::Edit(index));
                    }
                    if appearance::icon_button(ui, "Remove annotation", Icon::Remove).clicked() {
                        action = Some(Action::Delete(index));
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if appearance::action_button(ui, "Save Video Project", Icon::Save, true, false)
                    .clicked()
                {
                    action = Some(Action::Save);
                }
                if timeline.preview.is_some() {
                    if appearance::action_button(ui, "Show Source", Icon::Back, true, false)
                        .clicked()
                    {
                        action = Some(Action::Source);
                    }
                } else if appearance::action_button(ui, "Preview Edits", Icon::Play, true, false)
                    .clicked()
                {
                    action = Some(Action::Preview);
                }
                if appearance::action_button(ui, "Export MP4", Icon::SaveAs, true, false).clicked()
                {
                    action = Some(Action::Export);
                }
                ui.weak(format!("Edited length  {}", time(project.length())));
            });
        });
        self.video_bounds = Some(video_frame);
        if let Some(action) = action {
            self.windows_timeline_action(ctx, action, source.unwrap_or(0));
        }
    }

    fn windows_timeline_action(&mut self, ctx: &egui::Context, action: Action, time: i64) {
        if let Some(player) = &self.video_player {
            player.pause();
        }
        match action {
            Action::Annotate => self.request_windows_frame(ctx, time, None),
            Action::Edit(layer) => self.request_windows_frame(ctx, time, Some(layer)),
            Action::Seek(source) => {
                if let Some(player) = &self.video_player {
                    let time = self
                        .windows_timeline
                        .project
                        .as_ref()
                        .filter(|_| self.windows_timeline.preview.is_some())
                        .map_or(source, |p| p.output_time(source));
                    player.seek(time);
                }
            }
            Action::Save => {
                self.save_windows_video();
            }
            Action::Preview => self.start_windows_video_render(ctx, time, true),
            Action::Export => self.start_windows_video_render(ctx, time, false),
            Action::Source => self.return_to_windows_source(time),
            action => {
                let timeline = &mut self.windows_timeline;
                let Some(project) = &mut timeline.project else {
                    return;
                };
                let changed = match action {
                    Action::Trim(range) => project.trim(timeline.clip, range),
                    Action::Split => {
                        let changed = project.split(timeline.clip, time);
                        if changed {
                            timeline.clip += 1;
                        }
                        changed
                    }
                    Action::RemoveClip => project.remove(timeline.clip),
                    Action::Delete(index) if index < project.layers.len() => {
                        project.layers.remove(index);
                        timeline.layer = None;
                        true
                    }
                    _ => false,
                };
                if changed {
                    timeline.clip = timeline.clip.min(project.clips.len() - 1);
                    timeline.changed = true;
                    self.return_to_windows_source(time);
                }
            }
        }
    }

    fn return_to_windows_source(&mut self, time: i64) {
        if self.windows_timeline.preview.is_some() {
            self.video_player = None;
            self.windows_timeline.preview = None;
            self.windows_timeline.seek = Some(time);
            self.video_open_pending = true;
        } else if let Some(player) = &self.video_player {
            player.seek(time);
        }
    }

    fn request_windows_frame(&mut self, ctx: &egui::Context, time: i64, index: Option<usize>) {
        let state = &mut self.windows_timeline;
        let Some(project) = &state.project else {
            return;
        };
        if state.frame_request.is_some() || state.render.is_some() {
            return;
        }
        let layer = index.and_then(|index| project.layers.get(index)).cloned();
        let time = layer.as_ref().map_or(time, |layer| layer.start);
        state.next_reference += 1;
        let request = frame_annotation::Request {
            id: state.next_reference,
            time,
            available: layer
                .as_ref()
                .map_or(project.duration - time, |layer| layer.duration),
            editing: layer.is_some(),
        };
        if request.available <= 0 {
            return;
        }
        state.frame_request = Some(request.id);
        let source = project.source.clone();
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let prepare = || -> Result<Document, String> {
                if let Some(layer) = layer {
                    return layer.frame_document();
                }
                let document = Document {
                    image: video_session::video_frame(&source, time)?,
                    annotations: Vec::new(),
                    geometry: Default::default(),
                };
                document::validate_document(&document)?;
                Ok(document)
            };
            let _ = tx.send(WorkEvent::VideoFrameOpened(request, prepare()));
            crate::runtime::wake_root(&ctx);
        });
    }

    pub(super) fn complete_windows_frame(
        &mut self,
        request: frame_annotation::Request,
        duration: i64,
        art: Option<frame_annotation::Artwork>,
    ) -> bool {
        let timeline = &mut self.windows_timeline;
        if timeline.frame_request != Some(request.id) {
            return false;
        }
        if let Some(art) = art {
            let Some(project) = &mut timeline.project else {
                return false;
            };
            if !(1..=request.available).contains(&duration) {
                return false;
            }
            let layer = Layer {
                start: request.time,
                duration,
                reference: art.reference,
                annotations: art.annotations,
            };
            if request.editing {
                let Some(previous) = timeline
                    .layer
                    .and_then(|index| project.layers.get_mut(index))
                else {
                    return false;
                };
                *previous = layer;
            } else {
                if !project.add_layer(layer) {
                    return false;
                }
                timeline.layer = Some(project.layers.len() - 1);
            }
            timeline.changed = true;
            self.return_to_windows_source(request.time);
            self.show_video_status(
                "Annotation added to the timeline. Preview Edits plays the result.",
            );
        }
        self.windows_timeline.frame_request = None;
        true
    }

    fn start_windows_video_render(&mut self, ctx: &egui::Context, position: i64, preview: bool) {
        let Some(project) = self.windows_timeline.project.clone() else {
            return;
        };
        let destination = if preview {
            None
        } else {
            let name = project
                .source
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            let Some(path) = rfd::FileDialog::new()
                .set_title("Export edited video")
                .add_filter("H.264 video", &["mp4"])
                .set_file_name(format!("{name} - edited.mp4"))
                .save_file()
            else {
                return;
            };
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"))
                || path.exists()
            {
                self.error = Some("Choose a new filename ending in .mp4 for the export.".into());
                return;
            }
            Some(path)
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicU64::new(0));
        self.windows_timeline.render = Some(Render {
            preview,
            returned_to: project.output_time(position),
            cancel: cancel.clone(),
            progress: progress.clone(),
        });
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let render = || -> Result<Rendered, String> {
                let scratch = match destination.as_ref().and_then(|path| path.parent()) {
                    Some(folder) => tempfile::tempdir_in(folder),
                    None => tempfile::tempdir(),
                }
                .map_err(|e| format!("Could not prepare a video output: {e}"))?;
                let movie = scratch.path().join("preview.mp4");
                video_session::render_mp4(&project, &movie, &cancel, &progress)?;
                if cancel.load(Ordering::Relaxed) {
                    return Err("Video rendering canceled.".into());
                }
                if let Some(destination) = destination {
                    video_session::publish_video(&movie, &destination)
                        .map_err(|e| format!("Could not save the MP4: {e}"))?;
                    Ok(Rendered::File(destination))
                } else {
                    Ok(Rendered::Preview(scratch))
                }
            };
            let _ = tx.send(WorkEvent::WindowsVideoRendered(render()));
            crate::runtime::wake_root(&ctx);
        });
    }

    pub(super) fn windows_video_rendered(&mut self, result: Result<Rendered, String>) {
        let Some(render) = self.windows_timeline.render.take() else {
            return;
        };
        match result {
            Ok(Rendered::Preview(preview)) => {
                self.video_player = None;
                self.windows_timeline.preview = Some(preview);
                self.windows_timeline.seek = Some(render.returned_to);
                self.video_open_pending = true;
                self.show_video_status("Edited preview is ready");
            }
            Ok(Rendered::File(path)) => {
                self.status = format!("Exported {}", path.display());
                self.remember_recent(path);
            }
            Err(_) if render.cancel.load(Ordering::Relaxed) => {
                self.show_video_status("Video rendering canceled")
            }
            Err(error) => self.error = Some(error),
        }
    }
}
