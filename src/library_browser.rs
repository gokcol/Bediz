//! Compact native library browser. The parent owns the bounded scan/preview worker.
use super::*;
use crate::library::{BrowserPreferences, GalleryView, SortBy, TileSize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Collection {
    #[default]
    All,
    #[cfg(test)]
    Projects,
    Favorites,
}

impl Collection {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All projects",
            #[cfg(test)]
            Self::Projects => "Projects",
            Self::Favorites => "Favorites",
        }
    }

    pub(super) fn includes(self, entry: &LibraryEntry, preferences: &BrowserPreferences) -> bool {
        match self {
            Self::All => true,
            #[cfg(test)]
            Self::Projects => entry.kind == EntryKind::Project,
            Self::Favorites => preferences.favorites.contains(&entry.path),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Location {
    pub collection: Collection,
    pub date: DateFilter,
}

impl LibraryPanel {
    pub fn with_preferences(preferences: BrowserPreferences) -> Self {
        let mut panel = Self::default();
        panel.preferences = preferences;
        panel
    }

    pub fn take_preferences_change(&mut self) -> Option<BrowserPreferences> {
        std::mem::take(&mut self.preferences_dirty).then(|| self.preferences.clone())
    }

    fn location(&self) -> Location {
        Location {
            collection: self.collection,
            date: self.filter,
        }
    }

    fn apply_location(&mut self, location: Location) {
        self.collection = location.collection;
        self.filter = location.date;
        self.filter_dirty = true;
        self.update_filter();
    }

    pub(super) fn navigate(&mut self, location: Location) {
        if location != self.location() {
            self.back.push(self.location());
            if self.back.len() > 64 {
                self.back.remove(0);
            }
            self.forward.clear();
            self.apply_location(location);
        }
    }

    fn go_back(&mut self) {
        if let Some(location) = self.back.pop() {
            self.forward.push(self.location());
            self.apply_location(location);
        }
    }

    fn go_forward(&mut self) {
        if let Some(location) = self.forward.pop() {
            self.back.push(self.location());
            self.apply_location(location);
        }
    }

    fn toggle_favorite(&mut self, path: &Path) {
        if !self.preferences.favorites.remove(path) {
            self.preferences.favorites.insert(path.to_owned());
        }
        self.preferences_dirty = true;
        self.filter_dirty = true;
    }

    pub(super) fn reset_location(&mut self) {
        self.filter = DateFilter::All;
        self.collection = Collection::All;
        self.back.clear();
        self.forward.clear();
        self.search.clear();
        self.selection.clear();
        self.recent_selection.clear();
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, root: &Path) -> Option<LibraryAction> {
        self.update_index(ui.ctx(), root);
        self.update_filter();
        let cached: Vec<_> = self.previews.keys().cloned().collect();
        self.check_paths(ui.ctx(), &cached);
        let mut action = None;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(10., 6.);
            ui.spacing_mut().button_padding = Vec2::new(7., 4.);
            ui.spacing_mut().interact_size.y = 28.;
            egui::Frame::new()
                .fill(crate::appearance::surface(ui.ctx()))
                .corner_radius(6)
                .inner_margin(6)
                .stroke(Stroke::new(1., crate::appearance::palette(ui.ctx()).border))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if icon_button(
                            ui,
                            "Back",
                            appearance::Icon::Back,
                            !self.back.is_empty(),
                            false,
                        )
                        .clicked()
                        {
                            self.go_back();
                        }
                        if icon_button(
                            ui,
                            "Forward",
                            appearance::Icon::Forward,
                            !self.forward.is_empty(),
                            false,
                        )
                        .clicked()
                        {
                            self.go_forward();
                        }
                        self.breadcrumbs(ui);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(
                                ui,
                                "Refresh library",
                                appearance::Icon::Refresh,
                                true,
                                false,
                            )
                            .clicked()
                            {
                                self.refresh();
                                ui.ctx().request_repaint();
                            }
                            if icon_button(
                                ui,
                                "Storage folder",
                                appearance::Icon::Open,
                                true,
                                false,
                            )
                            .on_hover_text(format!(
                                "{}\nBrowse or change capture and project storage",
                                root.display()
                            ))
                            .clicked()
                            {
                                action = Some(LibraryAction::Storage);
                            }
                            let width = (ui.available_width() - 12.).clamp(130., 230.);
                            let search = egui::TextEdit::singleline(&mut self.search)
                                .hint_text("Search your library…")
                                .desired_width(width)
                                .margin(egui::vec2(9., 6.));
                            if ui.add(search).changed() {
                                self.filter_dirty = true;
                                self.update_filter();
                            }
                            let (search_icon, _) =
                                ui.allocate_exact_size(Vec2::splat(18.), egui::Sense::hover());
                            appearance::icon(
                                ui.painter(),
                                search_icon.center(),
                                appearance::Icon::Search,
                                appearance::palette(ui.ctx()).muted,
                                14.,
                            );
                        });
                    });
                });
            ui.add_space(3.);
            let height = (ui.available_height() - 43.).max(140.);
            ui.horizontal_top(|ui| {
                egui::Frame::new()
                    .fill(crate::appearance::palette(ui.ctx()).navy)
                    .corner_radius(6)
                    .inner_margin(10)
                    .stroke(Stroke::new(1., crate::appearance::palette(ui.ctx()).navy))
                    .show(ui, |ui| {
                        ui.set_width(168.);
                        ui.visuals_mut().widgets.inactive.fg_stroke.color = Color32::from_gray(195);
                        ui.visuals_mut().widgets.hovered.fg_stroke.color =
                            appearance::palette(ui.ctx()).ginger;
                        ui.set_height(height - 20.);
                        ui.visuals_mut().override_text_color = Some(Color32::from_gray(225));
                        ui.visuals_mut().weak_text_color = Some(Color32::from_gray(165));
                        ui.vertical(|ui| self.sidebar(ui, height - 20.));
                    });
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        if let Some(next) = self.gallery_ui(ui) {
                            action = Some(next);
                        }
                    },
                );
            });
            ui.separator();
            if let Some(next) = self.footer(ui) {
                action = Some(next);
            }
        });
        let order: Vec<_> = self
            .filtered
            .iter()
            .map(|&i| self.entries[i].path.clone())
            .collect();
        if let Some(paths) = self.selection.keyboard(ui, &order) {
            action = Some(LibraryAction::Trash(paths));
        }
        action
    }

    fn breadcrumbs(&mut self, ui: &mut egui::Ui) {
        let mut date = None;
        if ui
            .add(egui::Button::new(RichText::new(self.collection.label()).strong()).frame(false))
            .clicked()
        {
            date = Some(DateFilter::All);
        }
        let (year, month, day) = match self.filter {
            DateFilter::All => (None, None, None),
            DateFilter::Year(y) => (Some(y), None, None),
            DateFilter::Month(y, m) => (Some(y), Some(m), None),
            DateFilter::Day(d) => (Some(d.year()), Some(d.month()), Some(d)),
        };
        if let Some(year) = year {
            ui.weak("›");
            if ui
                .add(egui::Button::new(year.to_string()).frame(false))
                .clicked()
            {
                date = Some(DateFilter::Year(year));
            }
            if let Some(month) = month {
                ui.weak("›");
                let name = NaiveDate::from_ymd_opt(year, month, 1)
                    .unwrap()
                    .format("%b")
                    .to_string();
                if ui.add(egui::Button::new(name).frame(false)).clicked() {
                    date = Some(DateFilter::Month(year, month));
                }
            }
            if let Some(day) = day {
                ui.weak("›");
                ui.label(day.format("%d").to_string());
            }
        }
        if let Some(date) = date {
            self.navigate(Location {
                collection: self.collection,
                date,
            });
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.spacing_mut().item_spacing.y = 2.;
        ui.spacing_mut().indent = 12.;
        ui.spacing_mut().interact_size.y = 23.;
        ui.visuals_mut().indent_has_left_vline = false;
        ui.label(
            RichText::new("Collections")
                .size(10.)
                .strong()
                .color(appearance::palette(ui.ctx()).ginger),
        );
        ui.add_space(5.);
        egui::ScrollArea::vertical()
            .id_salt("library-navigation")
            .max_height((height - 26.).max(50.))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (collection, icon) in [
                    (Collection::All, appearance::Icon::Open),
                    (Collection::Favorites, appearance::Icon::Favorite),
                ] {
                    let count = self
                        .entries
                        .iter()
                        .filter(|e| collection.includes(e, &self.preferences))
                        .count();
                    if navigation_row(
                        ui,
                        collection.label(),
                        count,
                        self.collection == collection,
                        Some(icon),
                    )
                    .clicked()
                    {
                        self.navigate(Location {
                            collection,
                            date: DateFilter::All,
                        });
                    }
                }
                ui.add_space(6.);
                let (divider, _) = ui
                    .allocate_exact_size(Vec2::new(ui.available_width(), 1.), egui::Sense::hover());
                ui.painter().line_segment(
                    [divider.left_center(), divider.right_center()],
                    Stroke::new(1., Color32::from_gray(68)),
                );
                ui.add_space(5.);
                ui.label(
                    RichText::new("Capture date")
                        .size(10.)
                        .strong()
                        .color(appearance::palette(ui.ctx()).ginger),
                );
                self.calendar_ui(ui);
            });
    }

    fn gallery_ui(&mut self, ui: &mut egui::Ui) -> Option<LibraryAction> {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 3.;
            for sort in SortBy::ALL {
                if sort_tab(ui, sort.label(), self.preferences.sort == sort).clicked() {
                    if self.preferences.sort == sort {
                        self.preferences.descending = !self.preferences.descending;
                    } else {
                        self.preferences.sort = sort;
                        self.preferences.descending = sort != SortBy::Name;
                    }
                    changed = true;
                }
            }
            let label = if self.preferences.descending {
                "Descending"
            } else {
                "Ascending"
            };
            if icon_button(
                ui,
                label,
                if self.preferences.descending {
                    appearance::Icon::Down
                } else {
                    appearance::Icon::Up
                },
                true,
                false,
            )
            .clicked()
            {
                self.preferences.descending = !self.preferences.descending;
                changed = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (view, name, icon) in [
                    (GalleryView::List, "List view", appearance::Icon::List),
                    (GalleryView::Grid, "Grid view", appearance::Icon::Library),
                ] {
                    if icon_button(ui, name, icon, true, self.preferences.view == view).clicked() {
                        self.preferences.view = view;
                        changed = true;
                    }
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} item{}",
                    self.filtered.len(),
                    if self.filtered.len() == 1 { "" } else { "s" }
                ))
                .small()
                .color(appearance::palette(ui.ctx()).muted),
            );
            let select_all = ui
                .add_enabled(
                    !self.filtered.is_empty(),
                    egui::Button::new("Select all").small(),
                )
                .on_hover_text(
                    "Select every project in the displayed date interval and current filter",
                );
            if select_all.clicked() {
                self.cancel_marquee();
                let order = self
                    .filtered
                    .iter()
                    .map(|&i| self.entries[i].path.clone())
                    .collect::<Vec<_>>();
                self.selection.select_all(&order);
                self.selection.focus = Some(select_all.id);
                select_all.request_focus();
            }
            let deselect = ui.add_enabled(
                !self.selection.paths.is_empty(),
                egui::Button::new("Deselect").small(),
            );
            if deselect.clicked() {
                self.cancel_marquee();
                self.selection.clear();
                self.selection.focus = Some(deselect.id);
                deselect.request_focus();
            }
            if !self.warnings.is_empty() {
                ui.label(
                    RichText::new(format!("{} notices", self.warnings.len()))
                        .small()
                        .color(Color32::from_rgb(147, 99, 20)),
                )
                .on_hover_text(self.warnings.join("\n"));
            }
            if self.preferences.view == GalleryView::Grid {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (size, label) in [
                        (TileSize::Large, "L"),
                        (TileSize::Medium, "M"),
                        (TileSize::Small, "S"),
                    ] {
                        if ui
                            .selectable_label(
                                self.preferences.tile_size == size,
                                RichText::new(label).size(11.),
                            )
                            .on_hover_text("Thumbnail size")
                            .clicked()
                        {
                            self.preferences.tile_size = size;
                            changed = true;
                        }
                    }
                    ui.label(
                        RichText::new("Thumbnails")
                            .size(11.)
                            .color(appearance::palette(ui.ctx()).muted),
                    );
                });
            }
        });
        if changed {
            self.preferences_dirty = true;
            self.filter_dirty = true;
            self.update_filter();
        }
        if (self.scanning || self.needs_scan) && self.entries.is_empty() {
            empty_state(
                ui,
                "Opening your library",
                "Preparing your images and projects.",
            );
            return None;
        }
        if let Some(error) = &self.error {
            empty_state(ui, "Could not open this library", error);
            return None;
        }
        if self.filtered.is_empty() {
            empty_state(
                ui,
                if self.entries.is_empty() {
                    "A home for your ideas"
                } else {
                    "No matching images"
                },
                if self.entries.is_empty() {
                    "Capture your screen or choose a folder with images and projects."
                } else {
                    "Try another collection, date or search. Star an image to keep it in Favorites."
                },
            );
            return None;
        }
        let list = self.preferences.view == GalleryView::List;
        let (minimum, card_height) = match self.preferences.tile_size {
            TileSize::Small => (148., 150.),
            TileSize::Medium => (200., 194.),
            TileSize::Large => (270., 248.),
        };
        let gap = ui.spacing().item_spacing.x;
        let available = ui.available_width() - 14.;
        let columns = if list {
            1
        } else {
            ((available + gap) / (minimum + gap)).floor().max(1.) as usize
        };
        let width = (available - gap * (columns - 1) as f32) / columns as f32;
        let row_height = if list { 72. } else { card_height };
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("library-thumbnails")
            .scroll_source(egui::scroll_area::ScrollSource {
                drag: false,
                ..Default::default()
            })
            .auto_shrink([false, false]);
        if self.reset_scroll {
            scroll = scroll.vertical_scroll_offset(0.);
            self.reset_scroll = false;
        }
        let mut action = None;
        let gallery = gallery::Gallery::new(
            &self.entries,
            &self.filtered,
            columns,
            Vec2::new(width, row_height),
            ui.spacing().item_spacing,
        );
        scroll.show_viewport(ui, |ui, visible| {
            ui.set_height(gallery.height);
            let origin = ui.max_rect().min;
            let layout = selection::GalleryLayout {
                origin,
                viewport: visible
                    .translate(origin.to_vec2())
                    .intersect(ui.clip_rect()),
                columns,
                card: Vec2::new(width, row_height),
                gap: ui.spacing().item_spacing,
                cards: &gallery.cards,
                day_checkboxes: &gallery.day_checkboxes,
            };
            let background = ui.interact(
                layout.viewport,
                ui.id().with("library-marquee"),
                egui::Sense::click_and_drag(),
            );
            background.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    "Drag to select projects",
                )
            });
            let marquee = self.marquee_selection(ui, &background, &layout);
            for (row_index, row) in gallery
                .rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.rect.intersects(visible))
            {
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .id_salt(("gallery-row", row_index))
                        .max_rect(row.rect.translate(origin.to_vec2())),
                    |ui| {
                        ui.horizontal(|ui| match &row.content {
                            gallery::RowContent::Date(date, range) => {
                                ui.style_mut().interaction.selectable_labels = false;
                                let palette = appearance::palette(ui.ctx());
                                ui.painter().rect_filled(ui.max_rect(), 4., palette.base);
                                let count = range.len();
                                let selected = range
                                    .clone()
                                    .filter(|&position| {
                                        self.selection
                                            .paths
                                            .contains(&self.entries[self.filtered[position]].path)
                                    })
                                    .count();
                                let partial = selected > 0 && selected < count;
                                let mut checked = selected == count;
                                let checkbox = ui.add_sized(
                                    Vec2::splat(28.),
                                    egui::Checkbox::without_text(&mut checked)
                                        .indeterminate(partial),
                                );
                                let label =
                                    format!("Select projects for {}", date.format("%Y-%m-%d"));
                                checkbox.widget_info(|| {
                                    if partial {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Checkbox,
                                            ui.is_enabled(),
                                            &label,
                                        )
                                    } else {
                                        egui::WidgetInfo::selected(
                                            egui::WidgetType::Checkbox,
                                            ui.is_enabled(),
                                            checked,
                                            &label,
                                        )
                                    }
                                });
                                if checkbox.changed() {
                                    self.selection.set_group(
                                        range.clone().map(|position| {
                                            self.entries[self.filtered[position]].path.clone()
                                        }),
                                        checked,
                                    );
                                    self.selection.focus = Some(checkbox.id);
                                    checkbox.request_focus();
                                    ui.ctx().request_repaint();
                                }
                                checkbox.on_hover_text(if selected == count {
                                    "Deselect this day's displayed projects"
                                } else {
                                    "Select this day's displayed projects"
                                });
                                let (icon, _) =
                                    ui.allocate_exact_size(Vec2::splat(24.), egui::Sense::hover());
                                appearance::icon(
                                    ui.painter(),
                                    icon.center(),
                                    appearance::Icon::CalendarDay,
                                    palette.accent,
                                    16.,
                                );
                                ui.strong(date.format("%Y / %B / %d · %A").to_string());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.small(format!(
                                            "{count} project{}",
                                            if count == 1 { "" } else { "s" }
                                        ));
                                    },
                                );
                            }
                            gallery::RowContent::Projects(range) => {
                                for position in range.clone() {
                                    let entry = self.entries[self.filtered[position]].clone();
                                    if let Some(next) = ui
                                        .push_id(&entry.path, |ui| {
                                            self.card(ui, &entry, width, row_height, list)
                                        })
                                        .inner
                                    {
                                        action = Some(next);
                                    }
                                }
                            }
                        });
                    },
                );
            }
            if let Some(rect) = marquee {
                let accent = appearance::palette(ui.ctx()).accent;
                ui.painter().with_clip_rect(layout.viewport).rect(
                    rect,
                    2.,
                    accent.gamma_multiply(0.13),
                    Stroke::new(1., accent),
                    egui::StrokeKind::Inside,
                );
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                self.marquee = None;
            }
        });
        action
    }

    fn card(
        &mut self,
        ui: &mut egui::Ui,
        entry: &LibraryEntry,
        width: f32,
        height: f32,
        list: bool,
    ) -> Option<LibraryAction> {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click());
        if !ui.is_rect_visible(rect) {
            return None;
        }
        self.request_preview(entry);
        let order: Vec<_> = if response.clicked() && ui.input(|i| i.modifiers.shift) {
            self.filtered
                .iter()
                .map(|&i| self.entries[i].path.clone())
                .collect()
        } else {
            Vec::new()
        };
        self.item_interaction(ui, &response, &entry.path, false, &order);
        let selected = self.selection.paths.contains(&entry.path);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                ui.is_enabled(),
                selected,
                self.display_name(entry),
            )
        });
        let border = if selected {
            Stroke::new(2., appearance::palette(ui.ctx()).accent)
        } else if response.hovered() {
            Stroke::new(1., appearance::palette(ui.ctx()).accent)
        } else {
            Stroke::new(1., crate::appearance::palette(ui.ctx()).border)
        };
        ui.painter().rect_filled(
            rect.shrink(2.).translate(Vec2::new(0., 2.)),
            6.,
            Color32::from_black_alpha(9),
        );
        ui.painter().rect(
            rect.shrink(1.),
            6.,
            if selected {
                appearance::palette(ui.ctx()).cream
            } else {
                crate::appearance::surface(ui.ctx())
            },
            border,
            egui::StrokeKind::Inside,
        );
        let preview = if list {
            Rect::from_min_size(rect.min + Vec2::splat(7.), Vec2::new(78., 58.))
        } else {
            Rect::from_min_max(rect.min + Vec2::splat(5.), rect.max - Vec2::new(5., 50.))
        };
        ui.painter()
            .rect_filled(preview, 3., appearance::palette(ui.ctx()).base);
        if let Some(cached) = self.previews.get_mut(&entry.path) {
            cached.used = self.frame;
            match &cached.preview {
                Preview::Ready { texture, .. } => {
                    let size = texture.size_vec2();
                    let scale = (preview.width() / size.x).min(preview.height() / size.y);
                    ui.painter().image(
                        texture.id(),
                        Rect::from_center_size(preview.center(), size * scale),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                        Color32::WHITE,
                    );
                }
                Preview::Failed(_) => {
                    appearance::icon(
                        ui.painter(),
                        preview.center(),
                        appearance::Icon::View,
                        appearance::palette(ui.ctx()).muted,
                        22.,
                    );
                }
            }
        } else {
            appearance::icon(
                ui.painter(),
                preview.center(),
                appearance::Icon::View,
                Color32::from_rgb(181, 194, 208),
                24.,
            );
        }
        let checkbox_rect = Rect::from_min_size(rect.min + Vec2::splat(6.), Vec2::splat(24.));
        ui.painter()
            .rect_filled(checkbox_rect, 4., crate::appearance::surface(ui.ctx()));
        let mut checked = self.selection.paths.contains(&entry.path);
        let checkbox = ui.put(checkbox_rect, egui::Checkbox::without_text(&mut checked));
        checkbox.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Checkbox,
                ui.is_enabled(),
                checked,
                format!("Select {}", self.display_name(entry)),
            )
        });
        let checkbox_clicked = checkbox.changed();
        if checkbox_clicked {
            self.selection.click(&entry.path, &[], true, false);
            self.selection.focus = Some(checkbox.id);
            checkbox.request_focus();
            ui.ctx().request_repaint();
        }
        let favorite = self.preferences.favorites.contains(&entry.path);
        let star_rect =
            Rect::from_min_size(rect.right_top() + Vec2::new(-32., 6.), Vec2::splat(24.));
        let star = ui.interact(star_rect, ui.id().with("favorite"), egui::Sense::click());
        star.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                ui.is_enabled(),
                favorite,
                if favorite {
                    "Remove favorite"
                } else {
                    "Add favorite"
                },
            )
        });
        if favorite || response.hovered() || star.hovered() || selected {
            ui.painter()
                .rect_filled(star_rect, 6., Color32::WHITE.gamma_multiply(0.94));
            appearance::icon(
                ui.painter(),
                star_rect.center(),
                appearance::Icon::Favorite,
                if favorite {
                    appearance::palette(ui.ctx()).accent
                } else {
                    appearance::palette(ui.ctx()).muted
                },
                14.,
            );
        }
        let star_clicked = star
            .on_hover_text(if favorite {
                "Remove favorite"
            } else {
                "Add favorite"
            })
            .clicked();
        if star_clicked {
            self.toggle_favorite(&entry.path);
        }
        let text_pos = if list {
            rect.min + Vec2::new(96., 12.)
        } else {
            rect.min + Vec2::new(11., height - 40.)
        };
        clipped_label(
            ui,
            text_pos,
            if list { width - 134. } else { width - 24. },
            self.display_name(entry),
            13.,
            appearance::palette(ui.ctx()).ink,
        );
        let details = if list {
            format!(
                "{}   ·   {}   ·   {}",
                match entry.kind {
                    EntryKind::Project => "Editable project",
                    EntryKind::Image => "Image",
                    EntryKind::Recording => "Video",
                    EntryKind::Album => "Capture album",
                },
                if self.preferences.sort == SortBy::Created {
                    format!("Created {}", date_text(entry.created))
                } else {
                    format!("Modified {}", date_text(entry.modified))
                },
                size_text(entry.bytes)
            )
        } else {
            self.entry_detail(entry)
        };
        clipped_label(
            ui,
            text_pos + Vec2::new(0., 20.),
            if list { width - 112. } else { width - 24. },
            &details,
            11.,
            appearance::palette(ui.ctx()).muted,
        );
        let mut action = None;
        if !ui.input(|i| i.modifiers.ctrl) {
            response.context_menu(|ui| {
                if let Some(next) = self.file_menu(ui, &entry.path, false) {
                    action = Some(next);
                }
            });
        }
        let response = response
            .on_hover_ui_at_pointer(|ui| {
                let preview = self
                    .previews
                    .get(&entry.path)
                    .filter(|_| !self.outdated_previews.contains(&entry.path))
                    .map(|cached| &cached.preview);
                file_hover(ui, entry, self.preferences.sort, preview);
            })
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if !star_clicked
            && !checkbox_clicked
            && self.marquee.is_none()
            && response.double_clicked()
            && !ui.input(|i| i.modifiers.ctrl || i.modifiers.command || i.modifiers.shift)
        {
            action = Some(LibraryAction::Open(entry.path.clone()));
        }
        action
    }

    fn entry_detail(&self, entry: &LibraryEntry) -> String {
        match self.preferences.sort {
            SortBy::Modified => format!("Modified {}", date_text(entry.modified)),
            SortBy::Created => format!("Created {}", date_text(entry.created)),
            SortBy::Name => entry.date.format("%d %b %Y").to_string(),
            SortBy::Size => size_text(entry.bytes),
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui) -> Option<LibraryAction> {
        let entry = self
            .selection
            .single()
            .and_then(|path| self.entries.iter().find(|entry| &entry.path == path));
        let mut action = None;
        ui.horizontal(|ui| {
            let count=self.selection.paths.len();
            if count > 0 {
                ui.small(format!("{count} selected"));
                if ui.small_button("Move to Trash…").clicked() { action=Some(LibraryAction::Trash(self.selection.list())); }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (label, icon) = match entry.map(|entry| entry.kind) {
                    Some(EntryKind::Recording) => ("Play video", appearance::Icon::Play),
                    Some(EntryKind::Album) => ("Open album", appearance::Icon::Library),
                    _ => ("Open in Editor", appearance::Icon::Edit),
                };
                if appearance::action_button(
                    ui,
                    label,
                    icon,
                    entry.is_some(),
                    true,
                )
                .clicked()
                {
                    action = entry.map(|entry| LibraryAction::Open(entry.path.clone()));
                }
                ui.add(
                    egui::Label::new(
                        RichText::new(entry.map_or_else(
                            || "Ctrl/⌘-click to select · Shift-click for a range · Double-click to open".into(),
                            |entry| format!("{} · {}", entry.name, size_text(entry.bytes)),
                        ))
                        .small()
                        .color(appearance::palette(ui.ctx()).muted),
                    )
                    .truncate(),
                );
            });
        });
        action
    }
}

fn file_hover(ui: &mut egui::Ui, entry: &LibraryEntry, sort: SortBy, preview: Option<&Preview>) {
    let palette = appearance::palette(ui.ctx());
    ui.set_max_width(292.);
    ui.spacing_mut().item_spacing.y = 4.;
    ui.add(egui::Label::new(RichText::new(&entry.name).size(13.).strong()).wrap());
    ui.separator();

    let (date_label, date) = match (sort, entry.modified, entry.created) {
        (SortBy::Created, _, Some(created)) | (_, None, Some(created)) => {
            ("Created", date_text(Some(created)))
        }
        (_, Some(modified), _) => ("Modified", date_text(Some(modified))),
        (_, None, None) => ("Date", entry.date.format("%d %b %Y").to_string()),
    };
    let dimensions = match preview {
        Some(Preview::Ready {
            original_size: [width, height],
            ..
        }) => format!("{width} × {height} px"),
        Some(Preview::Failed(_)) => "Unavailable".into(),
        None => "Loading…".into(),
    };
    let format = if entry.kind == EntryKind::Album {
        ("Type", "Capture album".to_owned())
    } else {
        ("Dimensions", dimensions)
    };
    egui::Grid::new("file-hover-metadata")
        .spacing(Vec2::new(14., 3.))
        .show(ui, |ui| {
            for (label, value) in [("Size", size_text(entry.bytes)), (date_label, date), format] {
                ui.label(RichText::new(label).size(12.).color(palette.muted));
                ui.label(RichText::new(value).size(12.));
                ui.end_row();
            }
        });
    if let Some(Preview::Failed(error)) = preview {
        ui.add(egui::Label::new(RichText::new(error).small().color(palette.muted)).truncate());
    }
}

pub(super) fn navigation_row(
    ui: &mut egui::Ui,
    name: &str,
    count: usize,
    selected: bool,
    icon: Option<appearance::Icon>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(40.), 23.),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            format!("{name} ({count})"),
        )
    });
    let color = if selected {
        Color32::WHITE
    } else {
        Color32::from_gray(226)
    };
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            3.,
            if selected {
                appearance::palette(ui.ctx()).accent
            } else {
                appearance::palette(ui.ctx()).ink
            },
        );
    }
    if selected {
        appearance::satin(
            ui.painter(),
            rect,
            3.,
            true,
            response.is_pointer_button_down_on(),
        );
        ui.painter().rect_filled(
            Rect::from_min_size(
                rect.min + Vec2::new(0., 3.),
                Vec2::new(2., rect.height() - 6.),
            ),
            1.,
            appearance::palette(ui.ctx()).ginger,
        );
    }
    let offset = if let Some(icon) = icon {
        appearance::icon(
            ui.painter(),
            rect.left_center() + Vec2::new(13., 0.),
            icon,
            color,
            13.,
        );
        26.
    } else {
        7.
    };
    let count = count.to_string();
    let badge_width = count.len() as f32 * 6. + 8.;
    let badge = Rect::from_center_size(
        rect.right_center() - Vec2::new(badge_width * 0.5 + 5., 0.),
        Vec2::new(badge_width, 15.),
    );
    ui.painter().rect_filled(
        badge,
        3.,
        if selected {
            Color32::from_white_alpha(38)
        } else if appearance::palette(ui.ctx()).is_dark() {
            appearance::palette(ui.ctx()).border
        } else {
            appearance::palette(ui.ctx()).ink
        },
    );
    ui.painter().text(
        badge.center(),
        egui::Align2::CENTER_CENTER,
        count,
        FontId::proportional(10.),
        if selected {
            Color32::WHITE
        } else {
            Color32::from_gray(200)
        },
    );
    clipped_label(
        ui,
        rect.min + Vec2::new(offset, 4.),
        (badge.left() - rect.left() - offset - 4.).max(10.),
        name,
        12.,
        color,
    );
    response
        .on_hover_text(name)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn icon_button(
    ui: &mut egui::Ui,
    label: &str,
    icon: appearance::Icon,
    enabled: bool,
    selected: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
        });
        if selected || response.hovered() {
            ui.painter().rect_filled(
                rect,
                6.,
                if selected {
                    appearance::palette(ui.ctx()).cream
                } else {
                    crate::appearance::surface(ui.ctx())
                },
            );
        }
        appearance::icon(
            ui.painter(),
            rect.center(),
            icon,
            if selected {
                appearance::palette(ui.ctx()).accent
            } else {
                appearance::palette(ui.ctx()).muted
            },
            14.,
        );
        response.on_hover_text(label)
    })
    .inner
}

fn sort_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let text = ui.painter().layout_no_wrap(
        label.into(),
        FontId::proportional(12.),
        appearance::palette(ui.ctx()).ink,
    );
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(text.size().x + 20., 30.), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            6.,
            if selected {
                appearance::palette(ui.ctx()).navy
            } else {
                crate::appearance::surface(ui.ctx())
            },
        );
    }
    if selected {
        appearance::calico_tabs(ui.painter(), rect.min + Vec2::new(8., 0.));
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.),
        if selected {
            Color32::WHITE
        } else {
            appearance::palette(ui.ctx()).muted
        },
    );
    response.on_hover_text("Sort by this field · Click again to reverse")
}

pub(super) fn date_text(value: Option<std::time::SystemTime>) -> String {
    value
        .map(|time| {
            chrono::DateTime::<chrono::Local>::from(time)
                .format("%d %b %Y, %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "Unavailable".into())
}

pub(super) fn size_text(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024. * 1024.))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str, kind: EntryKind, day: u32) -> LibraryEntry {
        LibraryEntry {
            name: name.into(),
            path: name.into(),
            kind,
            date: NaiveDate::from_ymd_opt(2026, 10, day).unwrap(),
            modified: None,
            created: None,
            bytes: 1,
        }
    }

    #[test]
    fn grid_and_list_hover_show_file_metadata_and_wait_for_fresh_dimensions() {
        for list in [false, true] {
            let ctx = egui::Context::default();
            let mut panel = LibraryPanel::default();
            let mut entry = fixture("Quarterly report.myimage", EntryKind::Project, 18);
            entry.bytes = 4096;
            entry.path = Path::new("/library").join(&entry.name);
            let texture = ctx.load_texture(
                "preview",
                egui::ColorImage::filled([2, 2], Color32::WHITE),
                egui::TextureOptions::default(),
            );
            panel.previews.insert(
                entry.path.clone(),
                CachedPreview {
                    preview: Preview::Ready {
                        texture,
                        original_size: [1200, 600],
                        album: None,
                    },
                    bytes: 16,
                    used: 0,
                },
            );
            let frame = |panel: &mut LibraryPanel, time, events| {
                ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            assert!(
                                panel
                                    .card(ui, &entry, 240., if list { 72. } else { 160. }, list)
                                    .is_none()
                            );
                        });
                    },
                )
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
            };
            assert!(
                !frame(&mut panel, 0., vec![])
                    .iter()
                    .any(|text| text == "Dimensions")
            );
            frame(
                &mut panel,
                0.1,
                vec![egui::Event::PointerMoved(Pos2::new(92., 48.))],
            );
            frame(&mut panel, 1., vec![]);
            let tooltip = frame(&mut panel, 1.1, vec![]);
            for text in [
                "Quarterly report.myimage",
                "Size",
                "4.0 KB",
                "18 Oct 2026",
                "Dimensions",
                "1200 × 600 px",
            ] {
                assert!(
                    tooltip.iter().any(|value| value == text),
                    "missing {text}: {tooltip:?}"
                );
            }
            assert!(!tooltip.iter().any(|value| value.contains("/library")));
            panel.outdated_previews.insert(entry.path.clone());
            let refreshing = frame(&mut panel, 1.2, vec![]);
            assert!(refreshing.iter().any(|value| value == "Loading…"));
            assert!(!refreshing.iter().any(|value| value == "1200 × 600 px"));
        }
    }

    #[test]
    fn collections_dates_search_and_history_combine_and_favorites_persist() {
        let mut panel = LibraryPanel::default();
        panel.entries = vec![
            fixture("Alpha.png", EntryKind::Image, 17),
            fixture("Beta.myimage", EntryKind::Project, 18),
            fixture("Alpha.jpg", EntryKind::Image, 18),
        ];
        panel.toggle_favorite(Path::new("Alpha.jpg"));
        panel.toggle_favorite(Path::new("Beta.myimage"));
        panel.navigate(Location {
            collection: Collection::Favorites,
            date: DateFilter::Day(panel.entries[2].date),
        });
        assert_eq!(panel.filtered.len(), 2);
        panel.search = "alpha".into();
        panel.filter_dirty = true;
        panel.update_filter();
        assert_eq!(panel.filtered, [2]);
        panel.search.clear();
        panel.go_back();
        assert_eq!(panel.location(), Location::default());
        assert_eq!(panel.filtered.len(), 3);
        panel.go_forward();
        assert_eq!(panel.filtered.len(), 2);
        panel.navigate(Location {
            collection: Collection::Projects,
            date: DateFilter::All,
        });
        assert_eq!(panel.filtered, [1]);
        assert!(panel.forward.is_empty());
        let prefs = panel.take_preferences_change().unwrap();
        assert!(panel.take_preferences_change().is_none());
        let stored = serde_json::to_string(&prefs).unwrap();
        assert_eq!(
            serde_json::from_str::<BrowserPreferences>(&stored).unwrap(),
            prefs
        );
        panel.navigate(Location {
            collection: Collection::Favorites,
            date: DateFilter::All,
        });
        panel.toggle_favorite(Path::new("Beta.myimage"));
        panel.update_filter();
        assert_eq!(panel.filtered, [2]);
        assert_eq!(panel.calendar[&2026][&10].len(), 1);
    }

    #[test]
    fn every_sort_key_and_direction_uses_real_metadata_with_missing_dates_last() {
        use std::time::{Duration, UNIX_EPOCH};
        let mut entries = vec![
            fixture("z.png", EntryKind::Image, 1),
            fixture("Alpha.png", EntryKind::Image, 2),
            fixture("beta.png", EntryKind::Image, 3),
        ];
        entries[0].modified = Some(UNIX_EPOCH + Duration::from_secs(10));
        entries[0].created = Some(UNIX_EPOCH + Duration::from_secs(30));
        entries[0].bytes = 9;
        entries[1].modified = Some(UNIX_EPOCH + Duration::from_secs(30));
        entries[1].created = Some(UNIX_EPOCH + Duration::from_secs(10));
        entries[1].bytes = 3;
        entries[2].bytes = 6;
        for (sort, asc, desc) in [
            (SortBy::Modified, [0, 1, 2], [1, 0, 2]),
            (SortBy::Created, [1, 0, 2], [0, 1, 2]),
            (SortBy::Name, [1, 2, 0], [0, 2, 1]),
            (SortBy::Size, [1, 2, 0], [0, 2, 1]),
        ] {
            let mut indices = vec![2, 0, 1];
            library::sort_entries(&entries, &mut indices, sort, false);
            assert_eq!(indices, asc);
            library::sort_entries(&entries, &mut indices, sort, true);
            assert_eq!(indices, desc);
        }
        assert_eq!(date_text(None), "Unavailable");
    }
}
