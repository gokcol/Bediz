use eframe::egui::{self, Color32, FontDefinitions, FontFamily, FontId, Stroke};
use serde::{Deserialize, Serialize};

const BOLD_SANS: &str = "annotation-bold-sans";
const BOLD_MONO: &str = "annotation-bold-mono";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontFace {
    SansSerif,
    Monospace,
    Lato,
    CrimsonText,
    BarlowCondensed,
    AtkinsonHyperlegible,
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

    fn label(self) -> &'static str {
        match self {
            Self::SansSerif => "Sans serif",
            Self::Monospace => "Monospace",
            Self::Lato => "Lato",
            Self::CrimsonText => "Crimson Text",
            Self::BarlowCondensed => "Barlow Condensed",
            Self::AtkinsonHyperlegible => "Atkinson Hyperlegible",
        }
    }

    fn family(self, bold: bool) -> FontFamily {
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
        };
        FontFamily::Name(names[usize::from(bold)].into())
    }

    fn preview(self, size: f32) -> egui::RichText {
        egui::RichText::new(self.label()).font(FontId::new(size, self.family(false)))
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

    fn face(&self) -> FontFace {
        self.face.unwrap_or(if self.monospace {
            FontFace::Monospace
        } else {
            FontFace::SansSerif
        })
    }

    fn set_face(&mut self, face: FontFace) {
        self.monospace = face == FontFace::Monospace;
        self.face = match face {
            FontFace::SansSerif | FontFace::Monospace => None,
            _ => Some(face),
        };
    }

    pub(crate) fn font_face_picker(&mut self, ui: &mut egui::Ui) {
        let size = egui::TextStyle::Button.resolve(ui.style()).size;
        let selected = self.face();
        egui::CollapsingHeader::new(selected.preview(size))
            .id_salt("annotation-font-face")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.;
                for face in FontFace::ALL {
                    if ui
                        .selectable_label(selected == face, face.preview(size))
                        .clicked()
                    {
                        self.set_face(face);
                    }
                }
            });
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

    pub(crate) fn format(&self, size: f32, color: Color32) -> egui::TextFormat {
        egui::TextFormat {
            font_id: FontId::new(size, self.face().family(self.bold)),
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
