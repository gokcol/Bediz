//! Capture delivery is independent of the open document and its undo/recovery state.
use super::*;
use crate::capture::CaptureDestination;

#[derive(Clone)]
pub(super) struct CaptureOutput {
    pub image: Arc<image::RgbaImage>,
    pub path: Result<PathBuf, String>,
    pub destination: CaptureDestination,
    pub clipboard: Result<(), String>,
}

impl CaptureOutput {
    fn prepare(
        image: image::RgbaImage,
        folder: &std::path::Path,
        at: DateTime<Local>,
        destination: CaptureDestination,
        size: crate::capture_size::OutputSize,
        copy: impl FnOnce(&image::RgbaImage) -> Result<(), String>,
    ) -> Self {
        let image = size.apply(image);
        // Publish the clipboard first, so slow disk storage never delays pasting.
        // Either operation may succeed independently of the other.
        let clipboard = if destination == CaptureDestination::Clipboard {
            copy(&image)
        } else {
            Ok(())
        };
        let image = Arc::new(image);
        let path = crate::project_store::capture(image.clone(), folder, at);
        Self {
            image,
            path,
            destination,
            clipboard,
        }
    }
}

impl MyImageApp {
    pub(super) fn save_captured_image(
        &mut self,
        ctx: &egui::Context,
        image: image::RgbaImage,
        at: DateTime<Local>,
    ) {
        self.queue_capture_output(ctx, at, move || Ok(image));
    }

    pub(super) fn queue_capture_output(
        &mut self,
        ctx: &egui::Context,
        at: DateTime<Local>,
        prepare: impl FnOnce() -> Result<image::RgbaImage, String> + Send + 'static,
    ) {
        if let Some(target) = self.albums.collecting.clone() {
            self.queue_album_capture(ctx, target, at, prepare);
            return;
        }
        self.busy = true;
        self.error = None;
        self.status = match self.settings.capture_destination {
            CaptureDestination::Editor => "Saving capture…",
            CaptureDestination::Clipboard => "Copying capture…",
        }
        .into();
        // Never briefly display the Editor between the selection overlay and
        // clipboard completion. The tray remains available to reopen MyImage.
        self.restore_window(ctx, self.desktop.is_some());
        let folder = self.settings.output_dir.clone();
        let destination = self.settings.capture_destination;
        let output_size = self.settings.capture_output_size;
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = prepare().map(|image| {
                CaptureOutput::prepare(
                    image,
                    &folder,
                    at,
                    destination,
                    output_size,
                    crate::image_clipboard::write_image,
                )
            });
            let _ = tx.send(WorkEvent::CaptureFinished(result));
            crate::runtime::wake_root(&ctx);
        });
    }

    pub(super) fn finish_capture_output(
        &mut self,
        ctx: &egui::Context,
        result: Result<CaptureOutput, String>,
    ) {
        self.busy = false;
        match result {
            Err(error) => {
                self.error = Some(error);
                self.view = View::Capture;
                self.restore_window(ctx, false);
            }
            Ok(output) => {
                if let Ok(path) = &output.path {
                    self.library.file_saved(path);
                    self.remember_recent(path.clone());
                }
                if output.destination == CaptureDestination::Editor {
                    self.open_captured_image(ctx, output);
                } else if let Err(error) = &output.clipboard {
                    self.error = Some(format!(
                        "{error}. Your capture is retained. Retry copy or open it in Editor below."
                    ));
                    self.capture_copy_retry = Some(output);
                    self.view = View::Capture;
                    self.status = "Capture ready · Clipboard copy failed".into();
                    self.show(ctx);
                } else {
                    self.capture_copy_retry = None;
                    self.status = format!(
                        "Copied to clipboard · {} × {} px · Ready to paste",
                        output.image.width(),
                        output.image.height(),
                    );
                    self.error = output.path.err().map(|error| {
                        format!("Capture copied, but the Library copy could not be saved: {error}")
                    });
                    if let Some(desktop) = &mut self.desktop {
                        desktop.capture_feedback(&self.status);
                    }
                    // Keep document, paths, view, undo and recovery completely intact.
                    if self.desktop.is_none() {
                        self.show(ctx);
                    }
                }
            }
        }
        if std::mem::take(&mut self.quit_after_work) {
            self.request_quit(ctx);
        }
    }

    pub(super) fn open_captured_image(&mut self, ctx: &egui::Context, output: CaptureOutput) {
        if !self.can_replace_image(ctx, PendingAction::OpenCapture(output.clone())) {
            return;
        }
        self.capture_copy_retry = None;
        self.install_document(
            Document {
                image: output.image,
                annotations: Vec::new(),
                geometry: Default::default(),
            },
            output.path.as_ref().ok().cloned(),
        );
        self.last_texture = None;
        self.fit = true;
        match output.path {
            Ok(path) => {
                self.last_path = Some(path);
                self.status = "Capture saved".into();
                self.error = None;
            }
            Err(error) => {
                self.last_path = None;
                self.status = "Capture is in memory — save failed".into();
                self.error = Some(error);
            }
        }
        self.view = View::Editor;
        self.show(ctx);
    }

    pub(super) fn retry_capture_copy(&mut self, ctx: &egui::Context) {
        let Some(mut output) = self.capture_copy_retry.take() else {
            return;
        };
        self.busy = true;
        self.error = None;
        self.status = "Copying capture…".into();
        if self.desktop.is_some() {
            self.hide_during_capture(ctx);
        }
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            output.clipboard = crate::image_clipboard::write_image(&output.image);
            let _ = tx.send(WorkEvent::CaptureFinished(Ok(output)));
            crate::runtime::wake_root(&ctx);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels() -> image::RgbaImage {
        image::RgbaImage::from_fn(83, 51, |x, y| {
            image::Rgba([x as u8, y as u8, 177, if x < 3 { 0 } else { 255 }])
        })
    }

    #[test]
    fn capture_completion_wakes_hidden_editor() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(true)), true);
        let folder = tempfile::tempdir().unwrap();
        app.settings.output_dir = folder.path().to_owned();
        app.settings.capture_destination = CaptureDestination::Editor;
        let (woke, wake) = std::sync::mpsc::channel();
        crate::runtime::install_root_waker(&ctx, Arc::new(AtomicBool::new(false)), move |_| {
            let _ = woke.send(());
        });

        app.queue_capture_output(&ctx, Local::now(), || Ok(pixels()));

        wake.recv_timeout(Duration::from_secs(5)).unwrap();
        let WorkEvent::CaptureFinished(Ok(result)) = app.work_rx.try_recv().unwrap() else {
            panic!("The capture completion must be queued before waking the editor");
        };
        assert!(result.path.unwrap().is_file());
    }

    fn dirty_editor(ctx: &egui::Context) -> MyImageApp {
        let mut app = MyImageApp::with_context(ctx, false, Arc::new(AtomicBool::new(false)), true);
        let doc = Document {
            image: Arc::new(image::RgbaImage::new(30, 20)),
            annotations: Vec::new(),
            geometry: Default::default(),
        };
        app.install_document_state(doc, Some(PathBuf::from("Working.myimage")));
        app.last_path = Some(PathBuf::from("Working.myimage"));
        app.view = View::Editor;
        let doc = app.document.as_mut().unwrap();
        let before = doc.edit_state();
        doc.geometry.flip_horizontal = true;
        app.history.commit(before, doc);
        assert!(app.has_unsaved_edits());
        app
    }

    fn recent_textures(app: &mut MyImageApp, ctx: &egui::Context) -> Vec<egui::TextureId> {
        fn collect(shape: &egui::Shape, textures: &mut Vec<egui::TextureId>) {
            match shape {
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, textures);
                    }
                }
                egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::Managed(0) => {
                    textures.push(mesh.texture_id);
                }
                _ => {}
            }
        }
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 150.))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    app.library.recents_ui(
                        ui,
                        &app.settings.output_dir,
                        &app.settings.recent_files,
                        app.project_path.as_deref(),
                        false,
                    );
                });
            },
        );
        let mut textures = Vec::new();
        for shape in output.shapes {
            collect(&shape.shape, &mut textures);
        }
        textures
    }

    #[test]
    fn capture_keeps_existing_recents_visible_while_only_the_new_preview_loads() {
        for destination in CaptureDestination::ALL {
            let ctx = egui::Context::default();
            let mut app =
                MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
            let dir = tempfile::tempdir().unwrap();
            app.settings.output_dir = dir.path().to_owned();
            let old_paths: Vec<_> = ["first.myimage", "second.myimage"]
                .into_iter()
                .map(|name| dir.path().join(name))
                .collect();
            for path in &old_paths {
                document::save_project(
                    &Document {
                        image: Arc::new(pixels()),
                        annotations: vec![],
                        geometry: Default::default(),
                    },
                    path,
                )
                .unwrap();
            }
            app.settings.recent_files = old_paths.clone();
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let old_textures = loop {
                let textures = recent_textures(&mut app, &ctx);
                if textures.len() == old_paths.len() {
                    break textures;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "Initial previews did not load"
                );
                std::thread::sleep(Duration::from_millis(5));
            };

            // Exercise the capture-only hidden branch without creating a native
            // tray/hotkey integration. This also covers returning after Cancel.
            let hidden = ctx.run(Default::default(), |ctx| app.restore_window(ctx, true));
            assert!(app.hidden.load(Ordering::Relaxed));
            assert!(
                hidden.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .any(|command| matches!(command, ViewportCommand::Visible(false)))
            );
            assert_eq!(recent_textures(&mut app, &ctx), old_textures);
            app.restore_window(&ctx, false);
            assert!(!app.hidden.load(Ordering::Relaxed));
            assert_eq!(recent_textures(&mut app, &ctx), old_textures);

            // Clipboard retry uses the same temporary concealment, without
            // restoring old geometry or unmaximizing the user's window.
            let retry_hide = ctx.run(Default::default(), |ctx| app.hide_during_capture(ctx));
            assert!(app.hidden.load(Ordering::Relaxed));
            assert!(
                !retry_hide.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .any(|command| matches!(
                        command,
                        ViewportCommand::InnerSize(_)
                            | ViewportCommand::OuterPosition(_)
                            | ViewportCommand::Maximized(_)
                    ))
            );
            assert_eq!(recent_textures(&mut app, &ctx), old_textures);

            let output = CaptureOutput::prepare(
                pixels(),
                dir.path(),
                Local::now(),
                destination,
                crate::capture_size::OutputSize::Original,
                |_| Ok(()),
            );
            let path = output.path.clone().unwrap();
            app.work_tx
                .send(WorkEvent::CaptureFinished(Ok(output)))
                .unwrap();
            app.handle_events(&ctx);
            assert_eq!(app.settings.recent_files[0], path);
            assert_eq!(&app.settings.recent_files[1..], old_paths);

            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                let textures = recent_textures(&mut app, &ctx);
                // Check every loading frame, not just the final ready state:
                // old thumbnails must never disappear or acquire new textures.
                assert!(old_textures.iter().all(|id| textures.contains(id)));
                if textures.len() == old_textures.len() + 1 {
                    assert_eq!(&textures[1..], old_textures);
                    assert!(!old_textures.contains(&textures[0]));
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "New capture preview did not load"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    #[test]
    fn destinations_round_trip_and_legacy_settings_keep_editor_default() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.capture_destination, CaptureDestination::Editor);
        let settings = Settings {
            capture_destination: CaptureDestination::Clipboard,
            ..settings
        };
        let reopened: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(reopened.capture_destination, CaptureDestination::Clipboard);
    }

    #[test]
    fn output_size_is_applied_once_and_identically_to_clipboard_library_and_editor() {
        for size in crate::capture_size::OutputSize::ALL {
            for destination in CaptureDestination::ALL {
                let dir = tempfile::tempdir().unwrap();
                let original = pixels();
                let expected = size.apply(original.clone());
                let output = CaptureOutput::prepare(
                    original,
                    dir.path(),
                    Local::now(),
                    destination,
                    size,
                    |image| {
                        assert_eq!(destination, CaptureDestination::Clipboard);
                        assert_eq!(*image, expected);
                        Ok(())
                    },
                );
                assert_eq!(*output.image, expected);
                assert_eq!(
                    *document::load_project(output.path.as_ref().unwrap())
                        .unwrap()
                        .image,
                    expected
                );
                // Clipboard retries and Open in Editor reuse these exact pixels.
                let retained = output.clone();
                assert!(Arc::ptr_eq(&retained.image, &output.image));
            }
        }
    }

    #[test]
    fn clipboard_gets_exact_pixels_before_library_save_and_editor_does_not_copy() {
        let dir = tempfile::tempdir().unwrap();
        let expected = pixels();
        let output = CaptureOutput::prepare(
            expected.clone(),
            dir.path(),
            Local::now(),
            CaptureDestination::Clipboard,
            crate::capture_size::OutputSize::Original,
            |image| {
                assert_eq!(*image, expected);
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
                Ok(())
            },
        );
        assert!(output.clipboard.is_ok());
        assert_eq!(
            *document::load_project(&output.path.unwrap()).unwrap().image,
            expected
        );
        let output = CaptureOutput::prepare(
            pixels(),
            dir.path(),
            Local::now(),
            CaptureDestination::Editor,
            crate::capture_size::OutputSize::Original,
            |_| panic!("Editor capture must not overwrite the user's clipboard"),
        );
        assert!(output.path.is_ok());
    }

    #[test]
    fn clipboard_completion_preserves_dirty_document_paths_and_undo_even_if_disk_fails() {
        let ctx = egui::Context::default();
        let mut app = dirty_editor(&ctx);
        let old_doc = app.document.clone().unwrap();
        let revision = app.history.revision();
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("not-a-folder");
        std::fs::write(&blocked, b"occupied").unwrap();
        let output = CaptureOutput::prepare(
            pixels(),
            &blocked,
            Local::now(),
            CaptureDestination::Clipboard,
            crate::capture_size::OutputSize::Original,
            |_| Ok(()),
        );
        assert!(output.path.is_err());
        app.work_tx
            .send(WorkEvent::CaptureFinished(Ok(output)))
            .unwrap();
        app.handle_events(&ctx);
        assert!(Arc::ptr_eq(
            &app.document.as_ref().unwrap().image,
            &old_doc.image
        ));
        assert_eq!(
            app.document.as_ref().unwrap().edit_state(),
            old_doc.edit_state()
        );
        assert_eq!(app.history.revision(), revision);
        assert_eq!(app.project_path, Some(PathBuf::from("Working.myimage")));
        assert_eq!(app.last_path, app.project_path);
        assert!(app.view == View::Editor);
        assert!(app.pending_action.is_none());
        assert!(app.status.starts_with("Copied to clipboard · 83 × 51"));
        assert!(app.error.as_deref().unwrap().contains("Library copy"));
        assert!(app.history.undo(app.document.as_mut().unwrap()));
        assert!(!app.document.as_ref().unwrap().geometry.flip_horizontal);
    }

    #[test]
    fn failed_clipboard_retains_capture_and_successful_retry_preserves_original() {
        let ctx = egui::Context::default();
        let mut app = dirty_editor(&ctx);
        let original = app.document.clone().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let output = CaptureOutput::prepare(
            pixels(),
            dir.path(),
            Local::now(),
            CaptureDestination::Clipboard,
            crate::capture_size::OutputSize::Original,
            |_| Err("Clipboard busy".into()),
        );
        let path = output.path.clone().unwrap();
        app.finish_capture_output(&ctx, Ok(output));
        assert!(app.view == View::Capture);
        assert!(app.error.as_deref().unwrap().contains("Clipboard busy"));
        assert_eq!(*app.capture_copy_retry.as_ref().unwrap().image, pixels());
        assert!(app.settings.recent_files.contains(&path));
        assert_eq!(
            app.document.as_ref().unwrap().edit_state(),
            original.edit_state()
        );
        let mut retry = app.capture_copy_retry.take().unwrap();
        retry.clipboard = Ok(());
        app.finish_capture_output(&ctx, Ok(retry));
        assert!(app.capture_copy_retry.is_none());
        assert!(app.error.is_none());
        assert!(app.status.starts_with("Copied to clipboard"));
        assert_eq!(
            app.document.as_ref().unwrap().edit_state(),
            original.edit_state()
        );
    }

    #[test]
    fn editor_capture_flushes_current_project_before_replacement() {
        let ctx = egui::Context::default();
        let mut app = dirty_editor(&ctx);
        let original = app.document.clone().unwrap();
        let dir = tempfile::tempdir().unwrap();
        app.project_path = Some(dir.path().join("Working.myimage"));
        app.last_path = app.project_path.clone();
        let output = CaptureOutput::prepare(
            pixels(),
            dir.path(),
            Local::now(),
            CaptureDestination::Editor,
            crate::capture_size::OutputSize::Original,
            |_| unreachable!(),
        );
        app.finish_capture_output(&ctx, Ok(output));
        assert_eq!(
            app.document.as_ref().unwrap().edit_state(),
            original.edit_state()
        );
        assert!(matches!(
            app.after_save,
            Some(PendingAction::OpenCapture(_))
        ));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.busy && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
            app.handle_events(&ctx);
        }
        assert!(!app.busy);
        assert_eq!(
            document::load_project(&dir.path().join("Working.myimage"))
                .unwrap()
                .edit_state(),
            original.edit_state()
        );
        assert_eq!(*app.document.as_ref().unwrap().image, pixels());
        assert_eq!(app.project_path, app.last_path);
        assert!(app.last_path.as_ref().unwrap().is_file());
        assert!(!app.has_unsaved_edits());
    }
}
