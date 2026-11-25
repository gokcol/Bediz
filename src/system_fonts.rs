//! Discover installed font families on demand and load only the faces in use.
use eframe::egui::{FontData, FontDefinitions, FontFamily};
use fontdb::{Database, Family, Query, Source, Weight};
use std::{fs::File, io::Read, sync::OnceLock};

const PREFIX: &str = "bediz-system-font:";
const MAX_FONT_BYTES: u64 = 48 * 1024 * 1024;
const MAX_CACHED_BYTES: usize = 96 * 1024 * 1024;

struct Catalog {
    database: Database,
    families: Vec<(String, String)>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut database = Database::new();
        database.load_system_fonts();
        let mut families: Vec<_> = database
            .faces()
            .filter_map(|face| face.families.first())
            .map(|(name, _)| name)
            .filter(|name| !name.is_empty() && !name.starts_with('.'))
            .map(|name| (name.clone(), name.to_lowercase()))
            .collect();
        families.sort_unstable_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));
        families.dedup_by(|left, right| left.1 == right.1);
        Catalog { database, families }
    })
}

pub(super) fn families() -> &'static [(String, String)] {
    &catalog().families
}

pub(super) fn family(name: &str, bold: bool) -> FontFamily {
    let weight = if bold { "bold" } else { "regular" };
    FontFamily::Name(format!("{PREFIX}{weight}:{name}").into())
}

pub(super) fn load(fonts: &mut FontDefinitions, name: &str, bold: bool) -> bool {
    let family = family(name, bold);
    if fonts.families.contains_key(&family) {
        return false;
    }
    let database = &catalog().database;
    let query = Query {
        families: &[Family::Name(name)],
        weight: if bold { Weight::BOLD } else { Weight::NORMAL },
        ..Default::default()
    };
    let Some(face) = database.query(&query).and_then(|id| database.face(id)) else {
        return false;
    };
    let Source::File(path) = &face.source else {
        return false;
    };
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_FONT_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_FONT_BYTES
        || ab_glyph::FontRef::try_from_slice_and_index(&bytes, face.index).is_err()
    {
        return false;
    }

    let cached: usize = fonts
        .font_data
        .iter()
        .filter(|(key, _)| key.starts_with(PREFIX))
        .map(|(_, font)| font.as_ref().as_ref().len())
        .sum();
    if cached + bytes.len() > MAX_CACHED_BYTES {
        fonts.font_data.retain(|key, _| !key.starts_with(PREFIX));
        fonts.families.retain(
            |family, _| !matches!(family, FontFamily::Name(key) if key.starts_with(PREFIX)),
        );
    }
    let FontFamily::Name(key) = &family else {
        unreachable!("system fonts always use a named family");
    };
    let key = key.to_string();
    let mut data = FontData::from_owned(bytes);
    data.index = face.index;
    fonts.font_data.insert(key.clone(), data.into());
    let fallbacks = fonts.families.get(&FontFamily::Proportional);
    fonts.families.insert(
        family,
        std::iter::once(key)
            .chain(fallbacks.into_iter().flatten().cloned())
            .collect(),
    );
    true
}
