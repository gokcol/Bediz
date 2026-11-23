use eframe::egui::{self, Color32, FontDefinitions, FontFamily, FontId, Stroke};
use serde::{Deserialize, Serialize};

const BOLD_SANS: &str = "annotation-bold-sans";
const BOLD_MONO: &str = "annotation-bold-mono";

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
    fonts
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<crate::text_path::TextPath>,
    pub monospace: bool,
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
        let family = match (self.bold, self.monospace) {
            (false, false) => FontFamily::Proportional,
            (false, true) => FontFamily::Monospace,
            (true, false) => FontFamily::Name(BOLD_SANS.into()),
            (true, true) => FontFamily::Name(BOLD_MONO.into()),
        };
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
