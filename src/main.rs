#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("Bediz currently supports macOS and Windows.");

mod app;
mod appearance;
mod area_pixels;
mod arrow_style;
mod branding;
mod calico_play;
mod callout_style;
mod canvas;
mod canvas_zoom;
mod capture;
mod capture_magnifier;
mod capture_panel;
mod capture_size;
mod capture_window;
mod connectors;
mod desktop;
mod dialogs;
mod document;
mod editor;
mod export;
mod export_panel;
mod external_open;
mod fill;
mod geometry;
mod heif;
mod icon_style;
mod image_clipboard;
mod image_effects;
mod image_loading;
mod library;
mod library_files;
mod library_panel;
mod local_adjustments;
mod magnify_ui;
mod ocr;
mod onboarding;
mod open_transition;
mod page_correction;
mod pdf_export;
mod pointer_style;
mod presets;
mod printing;
mod privacy;
mod privacy_preview;
mod project_store;
mod recovery;
mod render;
mod repair;
mod runtime;
mod screen_color;
mod scroll_capture;
mod selection;
mod shapes;
mod snagit_import;
mod stamp_designer;
mod stamps;
mod startup;
mod storage;
mod storage_move;
mod studio;
mod svg_stamp;
mod templates;
mod text_path;
mod text_style;
mod texture_cache;
mod updates;

#[cfg(test)]
mod canvas_tests;
#[cfg(test)]
mod fill_tests;
#[cfg(test)]
mod freehand_tests;
#[cfg(test)]
mod group_tests;
#[cfg(test)]
mod markup_tests;
#[cfg(test)]
mod stamp_tests;
#[cfg(test)]
mod workspace_tests;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("myimage=info,warn"),
    )
    .init();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let background = args.iter().any(|arg| arg == "--background");
    let path = args
        .windows(2)
        .find(|pair| pair[0] == "--bediz-open")
        .map(|pair| std::path::PathBuf::from(&pair[1]))
        .or_else(|| {
            // Windows Open With associations pass a single positional file.
            let [arg] = args.as_slice() else {
                return None;
            };
            (!arg.to_string_lossy().starts_with('-')).then(|| std::path::PathBuf::from(arg))
        });
    if args.iter().any(|arg| arg == "--demo") || std::env::var_os("MYIMAGE_DEMO").is_some() {
        return runtime::run(background, None, path);
    }
    match external_open::start(path, background)
        .map_err(|error| eframe::Error::AppCreation(error.into()))?
    {
        external_open::Startup::Forwarded => Ok(()),
        external_open::Startup::Primary(inbox) => runtime::run(background, Some(inbox), None),
    }
}
