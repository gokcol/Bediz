use eframe::egui::{self, Color32, FontDefinitions, FontFamily, FontId, Stroke};
use serde::{Deserialize, Serialize};

#[path = "system_fonts.rs"]
mod system_fonts;

const BOLD_SANS: &str = "annotation-bold-sans";
const BOLD_MONO: &str = "annotation-bold-mono";
pub(crate) const FONT_LIST_HEIGHT: f32 = 126.;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontFace {
    SansSerif,
    Monospace,
    Lato,
    CrimsonText,
    BarlowCondensed,
    AtkinsonHyperlegible,
    System(String),
}

impl FontFace {
    const ALL: [Self; 6] = [
        Self::SansSerif,
        Self::Monospace,
        Self::Lato,
        Self::CrimsonText,
        Self::BarlowCondensed,
        Self::AtkinsonHyperlegible,
    ];

    fn label(&self) -> &str {
        match self {
            Self::SansSerif => "Sans serif",
            Self::Monospace => "Monospace",
            Self::Lato => "Lato",
            Self::CrimsonText => "Crimson Text",
            Self::BarlowCondensed => "Barlow Condensed",
            Self::AtkinsonHyperlegible => "Atkinson Hyperlegible",
            Self::System(name) => name,
        }
    }

    fn family(&self, bold: bool) -> FontFamily {
        let names = match self {
            Self::SansSerif if bold => return FontFamily::Name(BOLD_SANS.into()),
            Self::Monospace if bold => return FontFamily::Name(BOLD_MONO.into()),
            Self::SansSerif => return FontFamily::Proportional,
            Self::Monospace => return FontFamily::Monospace,
            Self::Lato => ["Lato-Regular", "Lato-Bold"],
            Self::CrimsonText => ["CrimsonText-Regular", "CrimsonText-Bold"],
            Self::BarlowCondensed => ["BarlowCondensed-Regular", "BarlowCondensed-Bold"],
            Self::AtkinsonHyperlegible => {
                ["AtkinsonHyperlegible-Regular", "AtkinsonHyperlegible-Bold"]
            }
            Self::System(name) => return system_fonts::family(name, bold),
        };
        FontFamily::Name(names[usize::from(bold)].into())
    }

    fn preview(&self, size: f32) -> egui::RichText {
        let family = match self {
            Self::System(_) => FontFamily::Proportional,
            _ => self.family(false),
        };
        egui::RichText::new(self.label()).font(FontId::new(size, family))
    }
}

pub(crate) fn font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (name, regular, bytes) in [
        (
            BOLD_SANS,
            FontFamily::Proportional,
            include_bytes!("../assets/fonts/Ubuntu-Bold.ttf").as_slice(),
        ),
        (
            BOLD_MONO,
            FontFamily::Monospace,
            include_bytes!("../assets/fonts/Hack-Bold.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), egui::FontData::from_static(bytes).into());
        let family = std::iter::once(name.to_owned())
            .chain(fonts.families[&regular].iter().cloned())
            .collect();
        fonts.families.insert(FontFamily::Name(name.into()), family);
    }
    let proportional = fonts.families[&FontFamily::Proportional].clone();
    for (face, regular, bold) in [
        (
            FontFace::Lato,
            include_bytes!("../assets/fonts/Lato-Regular.ttf").as_slice(),
            include_bytes!("../assets/fonts/Lato-Bold.ttf").as_slice(),
        ),
        (
            FontFace::CrimsonText,
            include_bytes!("../assets/fonts/CrimsonText-Regular.ttf").as_slice(),
            include_bytes!("../assets/fonts/CrimsonText-Bold.ttf").as_slice(),
        ),
        (
            FontFace::BarlowCondensed,
            include_bytes!("../assets/fonts/BarlowCondensed-Regular.ttf").as_slice(),
            include_bytes!("../assets/fonts/BarlowCondensed-Bold.ttf").as_slice(),
        ),
        (
            FontFace::AtkinsonHyperlegible,
            include_bytes!("../assets/fonts/AtkinsonHyperlegible-Regular.ttf").as_slice(),
            include_bytes!("../assets/fonts/AtkinsonHyperlegible-Bold.ttf").as_slice(),
        ),
    ] {
        for (bold, bytes) in [(false, regular), (true, bold)] {
            let family = face.family(bold);
            let FontFamily::Name(name) = &family else {
                unreachable!("bundled fonts have named families");
            };
            let name = name.to_string();
            fonts
                .font_data
                .insert(name.clone(), egui::FontData::from_static(bytes).into());
            fonts.families.insert(
                family,
                std::iter::once(name)
                    .chain(proportional.iter().cloned())
                    .collect(),
            );
        }
    }
    fonts
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<crate::text_path::TextPath>,
    pub monospace: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) face: Option<FontFace>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub alignment: Alignment,
}
impl TextStyle {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn face(&self) -> &FontFace {
        self.face.as_ref().unwrap_or(if self.monospace {
            &FontFace::Monospace
        } else {
            &FontFace::SansSerif
        })
    }

    fn set_face(&mut self, face: FontFace) {
        self.monospace = face == FontFace::Monospace;
        self.face = match face {
            FontFace::SansSerif | FontFace::Monospace => None,
            _ => Some(face),
        };
    }

    pub(crate) fn font_face_picker(&mut self, ui: &mut egui::Ui) -> bool {
        let size = egui::TextStyle::Button.resolve(ui.style()).size;
        let selected = self.face().clone();
        let picker_id = ui.make_persistent_id("annotation-font-face");
        let mut expanded = ui.data(|data| data.get_temp::<bool>(picker_id).unwrap_or(false));
        ui.horizontal(|ui| {
            let (_, arrow) = ui.allocate_exact_size(egui::vec2(14., 24.), egui::Sense::click());
            arrow.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    if expanded { "Hide fonts" } else { "Show fonts" },
                )
            });
            let label = ui.add(egui::Button::new(selected.label()).frame(false).truncate());
            if arrow.clicked() || label.clicked() {
                expanded = !expanded;
            }
            let icon = egui::Rect::from_center_size(arrow.rect.center(), egui::Vec2::splat(12.));
            egui::collapsing_header::paint_default_icon(
                ui,
                if expanded { 1. } else { 0. },
                &arrow.with_new_rect(icon),
            );
        });
        // Expanding upward changes the popup's origin. Show the full list in
        // one step so its rows do not slide away while the user scrolls or clicks.
        let accepted = expanded
            && ui
                .indent(picker_id, |ui| {
                    let search_id = ui.id().with("font-search");
                    let mut search = ui
                        .data_mut(|data| data.get_temp::<String>(search_id))
                        .unwrap_or_default();
                    let search_changed = ui
                        .add(
                            egui::TextEdit::singleline(&mut search)
                                .hint_text("Search fonts")
                                .desired_width(ui.available_width()),
                        )
                        .changed();
                    let query = search.trim().to_lowercase();
                    ui.data_mut(|data| data.insert_temp(search_id, search));
                    let bundled = FontFace::ALL
                        .into_iter()
                        .filter(|face| face.label().to_lowercase().contains(&query))
                        .collect::<Vec<_>>();
                    let installed = system_fonts::families()
                        .iter()
                        .filter(|(name, key)| {
                            key.contains(&query)
                                && !FontFace::ALL
                                    .iter()
                                    .any(|face| name.eq_ignore_ascii_case(face.label()))
                        })
                        .map(|(name, _)| name)
                        .collect::<Vec<_>>();
                    let count = bundled.len() + installed.len();
                    if count == 0 {
                        ui.weak("No matching fonts");
                        return false;
                    }
                    ui.spacing_mut().item_spacing.y = 2.;
                    // Keep the entire picker width scrollable as different names
                    // enter the visible rows. A filter starts at its first result.
                    let mut list = egui::ScrollArea::vertical()
                        .id_salt("annotation-font-list")
                        .max_height(FONT_LIST_HEIGHT)
                        .min_scrolled_height(FONT_LIST_HEIGHT)
                        // A click on a row must not start dragging the list.
                        .scroll_source(
                            egui::scroll_area::ScrollSource::SCROLL_BAR
                                | egui::scroll_area::ScrollSource::MOUSE_WHEEL,
                        )
                        .auto_shrink([false, true]);
                    if search_changed {
                        list = list.vertical_scroll_offset(0.);
                    }
                    let row_height = (size + 4.).max(ui.spacing().interact_size.y);
                    list.show_rows(ui, row_height, count, |ui, rows| {
                        let mut accepted = false;
                        let row_size = egui::vec2(ui.available_width(), row_height);
                        for row in rows {
                            let face = bundled.get(row).cloned().unwrap_or_else(|| {
                                FontFace::System(installed[row - bundled.len()].clone())
                            });
                            let response = ui.add(
                                egui::Button::selectable(selected == face, face.preview(size))
                                    .min_size(row_size)
                                    .truncate(),
                            );
                            if response.clicked() {
                                self.set_face(face);
                                accepted = response.double_clicked();
                            }
                        }
                        accepted
                    })
                    .inner
                })
                .inner;
        ui.data_mut(|data| data.insert_temp(picker_id, expanded && !accepted));
        if accepted
            && ui
                .stack()
                .iter()
                .any(|stack| stack.kind() == Some(egui::UiKind::Menu))
        {
            ui.close_kind(egui::UiKind::Menu);
        }
        accepted
    }

    pub(crate) fn emphasis_buttons(&mut self, ui: &mut egui::Ui) {
        let size = egui::TextStyle::Button.resolve(ui.style()).size;
        let bold =
            egui::RichText::new("B").font(FontId::new(size, FontFamily::Name(BOLD_SANS.into())));
        for (value, label, name) in [
            (&mut self.bold, bold, "Bold"),
            (
                &mut self.italic,
                egui::RichText::new("I").monospace().italics(),
                "Italic",
            ),
            (
                &mut self.underline,
                egui::RichText::new("U").underline(),
                "Underline",
            ),
        ] {
            let mut response = ui.add_sized(
                [26., 24.],
                egui::Button::selectable(*value, label).corner_radius(5),
            );
            if response.clicked() {
                *value = !*value;
                response.mark_changed();
            }
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    ui.is_enabled(),
                    *value,
                    name,
                )
            });
            response.on_hover_text(name);
        }
    }

    pub(crate) fn with_system_font(&self, fonts: &FontDefinitions) -> Option<FontDefinitions> {
        if let FontFace::System(name) = self.face() {
            if fonts
                .families
                .contains_key(&system_fonts::family(name, self.bold))
            {
                return None;
            }
            let mut definitions = fonts.clone();
            return system_fonts::load(&mut definitions, name, self.bold).then_some(definitions);
        }
        None
    }

    pub(crate) fn format(
        &self,
        size: f32,
        color: Color32,
        fonts: &FontDefinitions,
    ) -> egui::TextFormat {
        let mut family = self.face().family(self.bold);
        if !fonts.families.contains_key(&family) {
            family = FontFace::SansSerif.family(self.bold);
        }
        egui::TextFormat {
            font_id: FontId::new(size, family),
            color,
            italics: self.italic,
            underline: if self.underline {
                Stroke::new((size / 18.).max(1.), color)
            } else {
                Stroke::NONE
            },
            ..Default::default()
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_over_the_right_side_of_the_font_list_reaches_its_last_family() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut style = TextStyle::default();
        let last_face = system_fonts::families()
            .iter()
            .rev()
            .find(|(name, _)| {
                !FontFace::ALL
                    .iter()
                    .any(|face| name.eq_ignore_ascii_case(face.label()))
            })
            .map(|(name, _)| name.as_str())
            .expect("the test system has installed fonts");
        let mut last_face_visible = false;
        let mut header = egui::Pos2::ZERO;
        for frame in 0..10 {
            let events = if matches!(frame, 1 | 2) {
                vec![
                    egui::Event::PointerMoved(header),
                    egui::Event::PointerButton {
                        pos: header,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 1,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else if frame >= 4 {
                vec![
                    egui::Event::PointerMoved(egui::pos2(352., 104.)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., -30_000.),
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else {
                vec![]
            };
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 300.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        style.font_face_picker(ui);
                    });
                },
            );
            if frame == 0 {
                header = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "Sans serif" => {
                            Some(text.pos + text.galley.size() * 0.5)
                        }
                        _ => None,
                    })
                    .unwrap();
            }
            last_face_visible |= output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == last_face)
            });
        }
        assert!(last_face_visible, "could not reach {last_face}");
    }
}
