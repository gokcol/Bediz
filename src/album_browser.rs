//! The Albums workspace shares the library index and thumbnail worker.
use super::*;
use crate::{
    album::{self, CapturePreferences},
    appearance::{Icon, TileTone},
};

const ROW_HEIGHT: f32 = 70.;
const ACTION_SIZE: f32 = 30.;
const ACTION_GAP: f32 = 4.;
const ACTION_COUNT: usize = 5;

pub(crate) enum AlbumAction {
    New,
    Open(PathBuf),
    SetReady(PathBuf, String, bool),
    ChooseDestination(PathBuf, String),
    Rename(PathBuf, String),
    Export(PathBuf, String),
    Reveal(PathBuf),
}

impl LibraryPanel {
    pub(crate) fn albums_ui(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        recents: &[PathBuf],
        capture: &CapturePreferences,
    ) -> Option<AlbumAction> {
        self.update_index(ui.ctx(), root);
        let entries = self.album_entries(recents.iter().chain(capture.known_paths()));
        let ready = capture.destinations().count();
        let mut action = None;
        let input_id = ui.make_persistent_id("album-list-keyboard");
        let input_mode = ui.input(|input| {
            input.events.iter().rev().find_map(|event| match event {
                egui::Event::Key { pressed: true, .. } => Some(true),
                egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. } => Some(false),
                _ => None,
            })
        });
        let keyboard = ui.ctx().data_mut(|data| {
            if let Some(keyboard) = input_mode {
                data.insert_temp(input_id, keyboard);
            }
            data.get_temp::<bool>(input_id).unwrap_or(false)
        });
        ui.add_space(12.);
        let height = ui.available_height();
        ui.vertical_centered(|ui| {
            let width = (ui.available_width() - 24.).clamp(1., 960.);
            ui.allocate_ui_with_layout(
                Vec2::new(width, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        if appearance::action_button(ui, "New album", Icon::Create, true, true)
                            .clicked()
                        {
                            action = Some(AlbumAction::New);
                        }
                        if appearance::document_icon_button(ui, "Refresh albums", Icon::Refresh)
                            .clicked()
                        {
                            self.refresh();
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.weak(format!(
                                "{ready} of {} ready for capture",
                                album::MAX_READY_ALBUMS
                            ));
                        });
                    });
                    ui.add_space(7.);
                    if let Some(error) = &self.error {
                        ui.label(error);
                    } else if entries.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(34.);
                            if self.scanning {
                                ui.spinner();
                            } else {
                                let (rect, _) =
                                    ui.allocate_exact_size(Vec2::splat(54.), egui::Sense::hover());
                                appearance::sculpted_icon(
                                    ui.painter(),
                                    rect.center(),
                                    Icon::Album,
                                    TileTone::Teal,
                                    48.,
                                    None,
                                );
                                ui.heading("No capture albums yet");
                                ui.weak("Create an album to group captures for a project.");
                            }
                        });
                    } else {
                        let mut visible_paths = Vec::new();
                        egui::ScrollArea::vertical()
                            .id_salt("capture-album-list")
                            .auto_shrink([false, false])
                            .show_rows(ui, ROW_HEIGHT, entries.len(), |ui, rows| {
                                for entry in &entries[rows] {
                                    visible_paths.push(entry.path.clone());
                                    self.request_preview(entry);
                                    if let Some(preview) = self.previews.get_mut(&entry.path) {
                                        preview.used = self.frame;
                                    }
                                    let preview =
                                        self.previews.get(&entry.path).map(|p| &p.preview);
                                    let current = capture
                                        .selected()
                                        .is_some_and(|(path, _)| path == entry.path);
                                    let accepts = capture.ready(&entry.path) == Some(true);
                                    ui.push_id(&entry.path, |ui| {
                                        if let Some(row_action) = album_row(
                                            ui,
                                            entry,
                                            preview,
                                            current,
                                            accepts,
                                            ready < album::MAX_READY_ALBUMS,
                                            keyboard,
                                        ) {
                                            action = Some(row_action);
                                        }
                                    });
                                }
                            });
                        self.check_paths(ui.ctx(), &visible_paths);
                    }
                },
            );
        });
        action
    }

    fn album_entries<'a>(&self, known: impl Iterator<Item = &'a PathBuf>) -> Vec<LibraryEntry> {
        let mut entries: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| {
                entry.kind == EntryKind::Album && !self.missing_paths.contains(&entry.path)
            })
            .cloned()
            .collect();
        entries.sort_by(|a, b| {
            b.modified
                .cmp(&a.modified)
                .then_with(|| a.path.cmp(&b.path))
        });
        let mut seen: HashSet<_> = entries.iter().map(|entry| entry.path.clone()).collect();
        for path in
            known.filter(|path| album::is_album(path) && !self.missing_paths.contains(*path))
        {
            if seen.insert(path.clone()) {
                entries.push(LibraryEntry {
                    path: path.clone(),
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    date: chrono::Local::now().date_naive(),
                    kind: EntryKind::Album,
                    modified: None,
                    created: None,
                    bytes: 0,
                });
            }
        }
        entries
    }
}

fn album_row(
    ui: &mut egui::Ui,
    entry: &LibraryEntry,
    preview: Option<&Preview>,
    current: bool,
    accepts: bool,
    room: bool,
    keyboard: bool,
) -> Option<AlbumAction> {
    let (texture, summary) = match preview {
        Some(Preview::Ready { texture, album, .. }) => (Some(texture), album.as_ref()),
        _ => (None, None),
    };
    let title = summary.map_or_else(
        || entry.path.file_stem().unwrap_or_default().to_string_lossy(),
        |summary| std::borrow::Cow::Borrowed(summary.title.as_str()),
    );
    let details = match (summary, preview) {
        (Some(album), _) => match album.captures {
            1 => "1 capture".into(),
            count => format!("{count} captures"),
        },
        (_, Some(Preview::Failed(_))) => "Album unavailable".into(),
        _ => "Loading album…".into(),
    };
    let palette = appearance::palette(ui.ctx());
    let accent = appearance::satin_ink(ui.ctx(), TileTone::Teal);
    let (row, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), ROW_HEIGHT),
        egui::Sense::hover(),
    );
    let open_id = ui.id().with("album-open");
    let action_ids =
        std::array::from_fn::<_, ACTION_COUNT, _>(|index| ui.id().with(("album-action", index)));
    // Pointer users see actions on one row. Tab focus provides the equivalent
    // reveal for keyboard users and keeps the buttons present as focus advances.
    let reveal = if keyboard {
        ui.memory(|memory| {
            memory
                .focused()
                .is_some_and(|id| id == open_id || action_ids.contains(&id))
        })
    } else {
        ui.rect_contains_pointer(row)
    };
    ui.painter().rect(
        row,
        10,
        palette.surface,
        Stroke::new(1., palette.border),
        egui::StrokeKind::Inside,
    );

    let body = row.shrink(8.);
    let actions_width = ACTION_COUNT as f32 * ACTION_SIZE + (ACTION_COUNT - 1) as f32 * ACTION_GAP;
    let actions_x = body.right() - actions_width;
    let hit_rect = Rect::from_min_max(
        body.min,
        Pos2::new((actions_x - 10.).max(body.left()), body.bottom()),
    );
    let open = ui.interact(hit_rect, open_id, egui::Sense::click());
    open.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!("Open {title}"),
        )
    });
    if open.has_focus() {
        ui.painter().rect_stroke(
            hit_rect,
            6,
            Stroke::new(1., accent),
            egui::StrokeKind::Inside,
        );
    }
    let cover = Rect::from_min_size(body.min, Vec2::new(78., 54.));
    ui.painter().rect_filled(cover, 6, palette.base);
    if let Some(texture) = texture {
        let size = texture.size_vec2();
        let scale = (cover.width() / size.x).min(cover.height() / size.y);
        ui.painter().image(
            texture.id(),
            Rect::from_center_size(cover.center(), size * scale),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
            Color32::WHITE,
        );
    } else {
        appearance::icon(ui.painter(), cover.center(), Icon::Album, accent, 25.);
    }
    let title_x = cover.right() + 12.;
    let title_width = (hit_rect.right() - title_x).max(0.);
    clipped_label(
        ui,
        Pos2::new(title_x, body.top() + 6.),
        title_width,
        &title,
        15.,
        palette.ink,
    );
    let subtitle = if current {
        format!("{details}  ·  Current destination")
    } else {
        details
    };
    let subtitle_pos = Pos2::new(title_x, body.top() + 30.);
    let mut subtitle_width = title_width;
    if !accepts {
        let painter = ui.painter();
        let badge = painter.layout_no_wrap(
            "Captures off".into(),
            FontId::proportional(11.),
            palette.ink,
        );
        let badge_size = badge.size() + Vec2::new(12., 4.);
        if title_width >= badge_size.x + 60. {
            let details_width = painter
                .layout_no_wrap(subtitle.clone(), FontId::proportional(12.), palette.muted)
                .size()
                .x;
            subtitle_width = details_width.min(title_width - badge_size.x - 8.);
            let badge_rect = Rect::from_min_size(
                subtitle_pos + Vec2::new(subtitle_width + 8., -1.),
                badge_size,
            );
            painter.rect_filled(badge_rect, 5, palette.base);
            painter.galley(badge_rect.center() - badge.size() * 0.5, badge, palette.ink);
        }
    }
    clipped_label(
        ui,
        subtitle_pos,
        subtitle_width,
        &subtitle,
        12.,
        palette.muted,
    );
    if open
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("{title}\n{}", entry.path.display()))
        .clicked()
    {
        return Some(AlbumAction::Open(entry.path.clone()));
    }
    if !reveal {
        return None;
    }

    let accept_label = if accepts {
        "Pause accepting captures"
    } else {
        "Accept captures"
    };
    let accept_enabled = accepts || (summary.is_some() && room);
    let actions = [
        (
            accept_label,
            Icon::Capture,
            TileTone::Teal,
            accept_enabled,
            AlbumAction::SetReady(entry.path.clone(), title.to_string(), !accepts),
        ),
        (
            "Use as capture destination",
            Icon::Pin,
            TileTone::Blue,
            summary.is_some() && (room || accepts) && !current,
            AlbumAction::ChooseDestination(entry.path.clone(), title.to_string()),
        ),
        (
            "Rename album…",
            Icon::Pencil,
            TileTone::Violet,
            summary.is_some(),
            AlbumAction::Rename(entry.path.clone(), title.to_string()),
        ),
        (
            "Export PDF…",
            Icon::SaveAs,
            TileTone::Clay,
            summary.is_some_and(|album| album.entries > 0),
            AlbumAction::Export(entry.path.clone(), title.to_string()),
        ),
        (
            "Reveal file",
            Icon::Open,
            TileTone::Slate,
            true,
            AlbumAction::Reveal(entry.path.clone()),
        ),
    ];
    let mut action = None;
    for (index, (label, icon, tone, enabled, command)) in actions.into_iter().enumerate() {
        let button = Rect::from_min_size(
            Pos2::new(
                actions_x + index as f32 * (ACTION_SIZE + ACTION_GAP),
                row.center().y - ACTION_SIZE / 2.,
            ),
            Vec2::splat(ACTION_SIZE),
        );
        let mut button_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("album-action-ui", index))
                .max_rect(button),
        );
        if !enabled {
            button_ui.disable();
        }
        let ui = &button_ui;
        let response = ui.interact(button, action_ids[index], egui::Sense::click());
        let toggle = index == 0;
        let active = (toggle && accepts) || (index == 1 && current);
        response.widget_info(|| {
            if toggle {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Checkbox,
                    ui.is_enabled(),
                    accepts,
                    label,
                )
            } else {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
            }
        });
        let tint = appearance::satin_ink(ui.ctx(), tone);
        let ink = appearance::satin_button(ui, button, &response, active, tint);
        appearance::icon(ui.painter(), button.center(), icon, ink, 18.);
        let disabled_hint = if index == 0 && !room {
            "Pause another album before enabling this one"
        } else if index == 1 && current {
            "Current capture destination"
        } else {
            label
        };
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(label)
            .on_disabled_hover_text(disabled_hint)
            .clicked()
        {
            action = Some(command);
        }
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_lists_older_library_albums_and_known_external_albums_once() {
        let library_root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let (old, _) = album::create(
            library_root.path(),
            "Older project",
            album::Layout::Walkthrough,
        )
        .unwrap();
        let (outside, _) = album::create(
            external.path(),
            "External project",
            album::Layout::Storyboard,
        )
        .unwrap();
        let image = library_root.path().join("Image.png");
        std::fs::write(&image, b"image").unwrap();
        let absent = library_root.path().join("Removed.bedizalbum");
        let mut panel = LibraryPanel::default();
        panel.entries = library::scan(library_root.path()).unwrap().entries;
        panel.missing_paths.insert(absent.clone());
        let known = [&outside, &outside, &absent, &image];
        let albums = panel.album_entries(known.into_iter());
        let paths: Vec<_> = albums.iter().map(|entry| entry.path.as_path()).collect();
        assert_eq!(paths, [old.as_path(), outside.as_path()]);
        let summary = library::thumbnail(&albums[0]).unwrap().album.unwrap();
        assert_eq!(summary.title, "Older project");
        assert_eq!((summary.captures, summary.entries), (0, 0));
    }

    #[test]
    fn manager_uses_the_workspace_height_to_show_multiple_albums() {
        let root = Path::new("/album-test");
        let paths = [
            "Design.bedizalbum",
            "Review.bedizalbum",
            "Release.bedizalbum",
        ]
        .map(|name| root.join(name));
        let mut panel = LibraryPanel::default();
        panel.root = Some(root.to_owned());
        panel.needs_scan = false;
        let mut capture = CapturePreferences::default();
        capture.set_ready(&paths[1], "Review", true);
        capture.choose(Some(&paths[1]));
        let ctx = egui::Context::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(920., 500.))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    panel.albums_ui(ui, root, &paths, &capture);
                });
            },
        );
        for title in ["Design", "Review", "Release"] {
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.job.text == title
                    && shape.clip_rect.contains_rect(Rect::from_min_size(text.pos, text.galley.size()))
            )), "Album {title} should be visible");
        }
        let off_badges = output
            .shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.job.text == "Captures off"
                )
            })
            .count();
        assert_eq!(off_badges, 2);
        let row_fills: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.rect.height() == ROW_HEIGHT => Some(rect.fill),
                _ => None,
            })
            .collect();
        assert_eq!(row_fills, vec![appearance::surface(&ctx); 3]);
    }

    #[test]
    fn only_the_hovered_album_exposes_its_actions() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let paths = ["Design.bedizalbum", "Review.bedizalbum"].map(PathBuf::from);
        let mut controls = [egui::Id::NULL; 2];
        let mut rows = [Rect::NOTHING; 2];
        let mut chosen = None;
        let mut frame = |events| {
            let output = ctx.run(
                egui::RawInput {
                    events,
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640., 360.))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        for (index, path) in paths.iter().enumerate() {
                            ui.push_id(path, |ui| {
                                rows[index] = Rect::from_min_size(
                                    ui.next_widget_position(),
                                    Vec2::new(ui.available_width(), ROW_HEIGHT),
                                );
                                controls[index] = ui.id().with(("album-action", 0));
                                let entry = LibraryEntry {
                                    path: path.clone(),
                                    name: path.to_string_lossy().into_owned(),
                                    date: chrono::Local::now().date_naive(),
                                    kind: EntryKind::Album,
                                    modified: None,
                                    created: None,
                                    bytes: 0,
                                };
                                if let Some(action) =
                                    album_row(ui, &entry, None, false, true, true, false)
                                {
                                    chosen = Some(action);
                                }
                            });
                        }
                    });
                },
            );
            (output, controls, rows)
        };
        let (idle, _, rows) = frame(vec![]);
        let action_ids = |output: egui::FullOutput| {
            output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .into_iter()
                .filter_map(|(id, node)| {
                    (node.label() == Some("Pause accepting captures")).then_some(id)
                })
                .collect::<Vec<_>>()
        };
        assert!(action_ids(idle).is_empty());
        let (hover, ids, _) = frame(vec![egui::Event::PointerMoved(rows[1].center())]);
        assert_eq!(action_ids(hover).len(), 1);
        assert!(ctx.read_response(ids[0]).is_none());
        let pos = ctx.read_response(ids[1]).unwrap().rect.center();
        let button = |pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(pos), button(true)]);
        frame(vec![button(false)]);
        let (_, _, revealed_rows) = frame(vec![egui::Event::PointerMoved(rows[0].center())]);
        assert_eq!(revealed_rows, rows);
        assert!(matches!(chosen, Some(AlbumAction::SetReady(path, _, false)) if path == paths[1]));
    }
}
