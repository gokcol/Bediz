//! Offline, task-based help in the main Bediz window.
use super::*;
use crate::appearance::TileTone;
use crate::desktop::shortcut::CaptureShortcut;
use egui::{Align, Frame, Layout, RichText, Sense, vec2};

#[path = "help_topics.rs"]
mod topics;
use topics::{Chapter, LESSONS, Lesson};

#[derive(Default)]
pub(super) struct HelpCenter {
    pub(super) open: bool,
    selected: usize,
    query: String,
}

impl MyImageApp {
    pub(super) fn open_help(&mut self, ctx: &egui::Context) {
        if !self.can_open_information() {
            return;
        }
        self.close_capture_launcher(ctx);
        self.finish_edit();
        self.about_open = false;
        self.updates.open = false;
        self.help.open = true;
        self.show(ctx);
    }
}

impl HelpCenter {
    pub(super) fn show(&mut self, ctx: &egui::Context, shortcut: &CaptureShortcut) {
        if !self.open {
            return;
        }
        let palette = appearance::palette(ctx);
        let screen = ctx.content_rect();
        let width = (screen.width() - 32.).clamp(236., 918.);
        let height = (screen.height() - 36.).clamp(210., 698.);
        let response = egui::Modal::new(egui::Id::new("bediz-help"))
            .frame(dialogs::surface(ctx).corner_radius(14).inner_margin(12))
            .backdrop_color(Color32::from_black_alpha(85))
            .show(ctx, |ui| {
                ui.set_width(width - 24.);
                ui.spacing_mut().item_spacing = vec2(6., 6.);
                let top = ui.cursor().min.y;
                let close = ui
                    .horizontal(|ui| {
                        let (badge, _) = ui.allocate_exact_size(Vec2::splat(34.), Sense::hover());
                        appearance::sculpted_icon(
                            ui.painter(),
                            badge.center(),
                            Icon::List,
                            TileTone::Blue,
                            30.,
                            None,
                        );
                        ui.label(
                            RichText::new("Bediz Help")
                                .size(21.)
                                .strong()
                                .color(palette.ink),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            appearance::icon_button(ui, "Close Help", Icon::Close).clicked()
                        })
                        .inner
                    })
                    .inner;
                ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Search tutorials…")
                        .desired_width(ui.available_width()),
                );
                let active = LESSONS[self.selected].chapter;
                ui.horizontal_wrapped(|ui| {
                    for chapter in Chapter::ALL {
                        let (label, icon, _) = chapter.presentation();
                        if appearance::action_button(
                            ui,
                            label,
                            icon,
                            true,
                            active == chapter && self.query.trim().is_empty(),
                        )
                        .clicked()
                        {
                            self.query.clear();
                            if chapter != active {
                                self.selected = LESSONS
                                    .iter()
                                    .position(|lesson| lesson.chapter == chapter)
                                    .expect("Each chapter has lessons");
                            }
                        }
                    }
                });
                ui.separator();
                let searching = !self.query.trim().is_empty();
                let candidates = lesson_indices(LESSONS[self.selected].chapter, &self.query);
                if let Some(&first) = candidates.first() {
                    if !candidates.contains(&self.selected) {
                        self.selected = first;
                    }
                    let body_height = (height - 24. - (ui.cursor().min.y - top)).max(52.);
                    self.contents(ui, &candidates, shortcut, searching, body_height);
                } else {
                    ui.label("No matching tutorials. Try a tool name or a shorter search.");
                }
                close
            });
        if response.inner || response.should_close() {
            self.open = false;
        }
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        candidates: &[usize],
        shortcut: &CaptureShortcut,
        searching: bool,
        height: f32,
    ) {
        let width = ui.available_width();
        if width >= 645. {
            ui.horizontal_top(|ui| {
                let index_width = (width * 0.30).clamp(196., 250.);
                ui.allocate_ui_with_layout(
                    vec2(index_width, height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(index_width);
                        Frame::new()
                            .fill(appearance::palette(ui.ctx()).base)
                            .corner_radius(8)
                            .inner_margin(8)
                            .show(ui, |ui| {
                                ui.set_width(index_width - 16.);
                                ui.strong(if searching {
                                    "Search results"
                                } else {
                                    "Lessons"
                                });
                                egui::ScrollArea::vertical()
                                    .id_salt("lesson-index")
                                    .max_height((height - 42.).max(32.))
                                    .show(ui, |ui| {
                                        for &index in candidates {
                                            let label = LESSONS[index].title;
                                            let row = egui::Button::selectable(
                                                index == self.selected,
                                                label,
                                            )
                                            .min_size(vec2(ui.available_width(), 30.))
                                            .wrap();
                                            if ui.add(row).clicked() {
                                                self.selected = index;
                                            }
                                        }
                                    });
                            });
                    },
                );
                let reader_width = ui.available_width();
                ui.allocate_ui_with_layout(
                    vec2(reader_width, height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(reader_width);
                        self.reader(ui, candidates, shortcut, height);
                    },
                );
            });
        } else {
            let start = ui.cursor().min.y;
            egui::ComboBox::from_id_salt("help-lesson-picker")
                .width((width - 24.).max(100.))
                .height(215.)
                .truncate()
                .selected_text(LESSONS[self.selected].title)
                .show_ui(ui, |ui| {
                    for &index in candidates {
                        ui.selectable_value(&mut self.selected, index, LESSONS[index].title);
                    }
                });
            self.reader(
                ui,
                candidates,
                shortcut,
                (height - (ui.cursor().min.y - start)).max(40.),
            );
        }
    }

    fn reader(
        &mut self,
        ui: &mut egui::Ui,
        candidates: &[usize],
        shortcut: &CaptureShortcut,
        height: f32,
    ) {
        let lesson = &LESSONS[self.selected];
        let next = candidates
            .iter()
            .skip_while(|&&index| index != self.selected)
            .nth(1)
            .copied();
        let palette = appearance::palette(ui.ctx());
        egui::ScrollArea::vertical()
            .id_salt(("help-reader", self.selected))
            .max_height(height)
            .min_scrolled_height(0.)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (chapter, icon, tone) = lesson.chapter.presentation();
                Frame::new()
                    .fill(palette.cream)
                    .corner_radius(8)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (badge, _) =
                                ui.allocate_exact_size(Vec2::splat(29.), Sense::hover());
                            appearance::sculpted_icon(
                                ui.painter(),
                                badge.center(),
                                icon,
                                tone,
                                28.,
                                None,
                            );
                            ui.label(
                                RichText::new(chapter)
                                    .strong()
                                    .color(appearance::satin_ink(ui.ctx(), tone)),
                            );
                        });
                        ui.add(
                            egui::Label::new(RichText::new(lesson.title).size(22.).strong()).wrap(),
                        );
                        ui.add(egui::Label::new(lesson.overview).wrap());
                        if lesson.show_shortcut {
                            ui.label(
                                RichText::new(format!(
                                    "Your capture shortcut   {}",
                                    shortcut.label()
                                ))
                                .strong(),
                            );
                        }
                    });
                ui.add_space(3.);
                for (index, step) in lesson.steps.iter().enumerate() {
                    ui.horizontal_top(|ui| {
                        let (badge, _) = ui.allocate_exact_size(Vec2::splat(23.), Sense::hover());
                        ui.painter().rect_filled(badge, 5, palette.cream);
                        ui.painter().text(
                            badge.center(),
                            egui::Align2::CENTER_CENTER,
                            index + 1,
                            egui::FontId::proportional(12.),
                            palette.ink,
                        );
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 3.;
                            ui.add(
                                egui::Label::new(RichText::new(step.title).size(14.).strong())
                                    .wrap(),
                            );
                            ui.add(egui::Label::new(step.detail).wrap());
                        });
                    });
                    ui.add_space(3.);
                }
                if let Some(tip) = lesson.tip {
                    Frame::new()
                        .fill(palette.base)
                        .corner_radius(7)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.strong("Tip");
                            ui.add(egui::Label::new(tip).wrap());
                        });
                }
                if let Some(next) = next {
                    ui.add_space(4.);
                    if dialogs::secondary(ui, "Next lesson", Icon::Forward)
                        .on_hover_text(LESSONS[next].title)
                        .clicked()
                    {
                        self.selected = next;
                    }
                }
            });
    }
}

fn lesson_indices(chapter: Chapter, query: &str) -> Vec<usize> {
    let query = query.to_lowercase();
    LESSONS
        .iter()
        .enumerate()
        .filter_map(|(index, lesson)| {
            let selected = if query.trim().is_empty() {
                lesson.chapter == chapter
            } else {
                query
                    .split_whitespace()
                    .all(|word| lesson_matches(lesson, word))
            };
            selected.then_some(index)
        })
        .collect()
}

fn lesson_matches(lesson: &Lesson, word: &str) -> bool {
    [
        lesson.chapter.presentation().0,
        lesson.title,
        lesson.overview,
    ]
    .into_iter()
    .chain(
        lesson
            .steps
            .iter()
            .flat_map(|step| [step.title, step.detail]),
    )
    .chain(lesson.tip)
    .any(|text| text.to_lowercase().contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_finds_steps_across_chapters() {
        for chapter in Chapter::ALL {
            let lessons = lesson_indices(chapter, "   ");
            assert!(lessons.len() >= 4);
            assert!(lessons.iter().all(|&id| LESSONS[id].chapter == chapter));
        }
        let results = lesson_indices(Chapter::Customization, "Scroll PDF");
        assert!(
            results
                .iter()
                .any(|&id| LESSONS[id].title == "Capture a scrolling page")
        );
        let results = lesson_indices(Chapter::Capture, "edit annotation");
        assert!(
            results
                .iter()
                .any(|&id| LESSONS[id].title == "Open a video and annotate a frame")
        );
        assert!(lesson_indices(Chapter::Capture, "no-matching-lesson").is_empty());
    }

    #[test]
    fn help_opens_through_the_app_router_and_fits_both_layouts() {
        let ctx = egui::Context::default();
        appearance::apply(&ctx);
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(true)), true);
        app.guide_open = false;
        app.desktop_tx.send(DesktopEvent::Help).unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.handle_events(ctx));
        assert!(app.help.open);
        assert!(!app.hidden.load(Ordering::Relaxed));
        assert!(!app.workspace_unobscured());

        for size in [vec2(1200., 800.), vec2(490., 365.)] {
            app.help.query = "split clip".into();
            for _ in 0..4 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| app.help.show(ctx, &app.settings.capture_shortcut),
                );
            }
            assert_eq!(
                LESSONS[app.help.selected].chapter.presentation().0,
                "Screen Recording"
            );
            let rect = ctx
                .memory(|memory| memory.area_rect(egui::Id::new("bediz-help")))
                .unwrap();
            assert!(
                ctx.content_rect().expand(1.).contains_rect(rect),
                "{size:?}: {rect:?}"
            );
        }

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
            |ctx| app.help.show(ctx, &app.settings.capture_shortcut),
        );
        assert!(!app.help.open);
        assert!(app.workspace_unobscured());
    }
}
