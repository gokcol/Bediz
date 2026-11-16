//! Persistent capture shortcut editor, shared by Home and the resident panel.
use super::*;
use crate::desktop::shortcut::CaptureShortcut;

pub(super) struct State {
    draft: CaptureShortcut,
    error: Option<String>,
}

impl MyImageApp {
    pub(super) fn open_capture_hotkey(&mut self, ctx: &egui::Context) {
        if !self.can_open_information() || self.about_open || self.updates.open {
            return;
        }
        self.close_capture_launcher(ctx);
        self.finish_edit();
        self.capture_hotkey_editor = Some(State {
            draft: self.settings.capture_shortcut.clone(),
            error: None,
        });
        self.show(ctx);
    }

    pub(super) fn capture_hotkey_ui(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.capture_hotkey_editor.take() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        Dialog::new(
            "Capture hotkey",
            "Your shortcut, wherever you’re working.",
            DialogIcon::Capture,
            440.,
        )
        .modal_closable(ctx, egui::Id::new("capture-hotkey"), &mut open, |ui| {
            let palette = appearance::palette(ctx);
            egui::Frame::new()
                .fill(palette.cream)
                .corner_radius(8)
                .inner_margin(18)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(state.draft.label())
                                .size(26.)
                                .strong()
                                .color(palette.accent),
                        );
                        ui.small("Capture from any app");
                    });
                });
            ui.add_space(8.);
            ui.label("Choose modifiers and a key");
            ui.horizontal_wrapped(|ui| {
                ui.toggle_value(&mut state.draft.control, "Control");
                ui.toggle_value(
                    &mut state.draft.alt,
                    if cfg!(target_os = "macos") {
                        "Option"
                    } else {
                        "Alt"
                    },
                );
                ui.toggle_value(&mut state.draft.shift, "Shift");
                ui.toggle_value(
                    &mut state.draft.command,
                    if cfg!(target_os = "macos") {
                        "Command"
                    } else {
                        "Windows"
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Key");
                egui::ComboBox::from_id_salt("capture-hotkey-key")
                    .selected_text(CaptureShortcut::key_label(&state.draft.key))
                    .width(140.)
                    .show_ui(ui, |ui| {
                        for key in CaptureShortcut::keys() {
                            let label = CaptureShortcut::key_label(&key).to_owned();
                            ui.selectable_value(&mut state.draft.key, key, label);
                        }
                    });
            });
            ui.small(if cfg!(target_os = "macos") {
                "⌃⇧. and ⌘⇧. also remain available."
            } else {
                "Ctrl+Shift+. also remains available."
            });
            if let Err(error) = state.draft.validate() {
                ui.colored_label(dialogs::DANGER, error);
            }
            if let Some(error) = &state.error {
                ui.colored_label(dialogs::DANGER, error);
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Restore default").clicked() {
                    state.draft = CaptureShortcut::default();
                    state.error = None;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    apply = ui
                        .add_enabled(
                            state.draft.validate().is_ok(),
                            appearance::primary(ctx, "Save shortcut"),
                        )
                        .clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        });
        if !open || cancel {
            return;
        }
        if apply {
            let mut settings = self.settings.clone();
            settings.capture_shortcut = state.draft.clone();
            let result = if self.demo {
                Ok(())
            } else if let Some(desktop) = &mut self.desktop {
                desktop.change_capture_shortcut(&state.draft, || storage::save_settings(&settings))
            } else {
                Err(
                    "Desktop shortcuts are unavailable. Retry desktop integration, then try again."
                        .into(),
                )
            };
            match result {
                Ok(()) => {
                    self.settings.capture_shortcut = state.draft;
                    self.status = format!(
                        "Capture shortcut saved: {}",
                        self.settings.capture_shortcut.label()
                    );
                    return;
                }
                Err(error) => state.error = Some(error),
            }
        }
        self.capture_hotkey_editor = Some(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcut_editor_blocks_capture_and_escape_discards_changes() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.guide_open = false;
        app.open_capture_hotkey(&ctx);
        assert!(app.capture_hotkey_editor.is_some());
        app.capture_hotkey_editor.as_mut().unwrap().draft.key = "KeyK".into();
        let generation = app.generation;
        app.desktop_tx.send(DesktopEvent::Capture).unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.handle_events(ctx));
        assert_eq!(app.generation, generation);
        assert!(app.pending_capture.is_none());
        assert!(!app.workspace_unobscured());
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| app.capture_hotkey_ui(ctx),
        );
        assert!(app.capture_hotkey_editor.is_none());
        assert_eq!(app.settings.capture_shortcut, CaptureShortcut::default());
    }
}
