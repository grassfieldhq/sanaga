//! Sanaga — a fast, light web browser for Linux (GTK + WebKitGTK).
//!
//! Usage:
//!   sanaga [options] [address ...]     (see `sanaga --help`)
//!   sanaga --screenshot picture.png [address ...]

#![allow(deprecated)]
// Drawing helpers take many plain values, some state holds composite types and
// a few layout loops index parallel lists: kept on purpose for readability.
#![allow(clippy::too_many_arguments, clippy::type_complexity, clippy::needless_range_loop)]

// Sanaga's window (GTK 3) and web engine (WebKitGTK) are Linux / BSD only for
// now: say so clearly instead of failing deep inside a dependency.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "ios", target_os = "android"))]
compile_error!("Sanaga currently runs on Linux (and BSD) only: it is built on GTK 3 and WebKitGTK. See docs/PORTING.md for the Windows and macOS plan.");

mod app;
mod desktop;
mod extensions;
mod favicon;
mod keyring;
mod migrate;
mod sync;
mod html;
mod i18n;
mod import;
mod icons;
mod net;
mod frames;
mod page;
mod shell;
mod store;
mod theme;

use eframe::egui;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub fn setup_fonts(ctx: &egui::Context) {
    let mut f = egui::FontDefinitions::default();
    let fallback = f.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    for (name, bytes) in [
        ("inter", &include_bytes!("../assets/Inter-Regular.ttf")[..]),
        ("inter-bold", &include_bytes!("../assets/Inter-SemiBold.ttf")[..]),
        ("inter-italic", &include_bytes!("../assets/Inter-Italic.ttf")[..]),
        ("noto", &include_bytes!("../assets/NotoSans-Regular.ttf")[..]),
        ("serif", &include_bytes!("../assets/NotoSerif-Regular.ttf")[..]),
        ("serif-bold", &include_bytes!("../assets/NotoSerif-Bold.ttf")[..]),
        ("serif-italic", &include_bytes!("../assets/NotoSerif-Italic.ttf")[..]),
    ] {
        f.font_data.insert(name.to_string(), Arc::new(egui::FontData::from_static(bytes)));
    }
    // Other alphabets: use the system's Noto fonts when they are installed.
    let mut extra = Vec::new();
    for name in [
        "NotoSansArabic-Regular", "NotoSansHebrew-Regular", "NotoSansDevanagari-Regular", "NotoSansBengali-Regular",
        "NotoSansThai-Regular", "NotoSansArmenian-Regular", "NotoSansGeorgian-Regular", "NotoSansEthiopic-Regular",
        "NotoSansTamil-Regular", "NotoSansMath-Regular", "NotoSansSymbols-Regular", "NotoSansSymbols2-Regular",
    ] {
        for dir in ["/usr/share/fonts/noto", "/usr/share/fonts/truetype/noto", "/usr/share/fonts/google-noto"] {
            if let Ok(bytes) = std::fs::read(format!("{dir}/{name}.ttf")) {
                f.font_data.insert(name.to_string(), Arc::new(egui::FontData::from_owned(bytes)));
                extra.push(name.to_string());
                break;
            }
        }
    }
    let fallback: Vec<String> = std::iter::once("noto".to_string()).chain(fallback).chain(extra).collect();
    let with = |first: &str| {
        let mut v = vec![first.to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    f.families.insert(egui::FontFamily::Proportional, with("inter"));
    f.families.insert(egui::FontFamily::Name("bold".into()), with("inter-bold"));
    f.families.insert(egui::FontFamily::Name("italic".into()), with("inter-italic"));
    // Reader mode text
    f.families.insert(egui::FontFamily::Name("serif".into()), with("serif"));
    f.families.insert(egui::FontFamily::Name("serif-bold".into()), with("serif-bold"));
    f.families.insert(egui::FontFamily::Name("serif-italic".into()), with("serif-italic"));
    ctx.set_fonts(f);
}

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let mut screenshot: Option<app::ScreenshotJob> = None;
    let mut urls = Vec::new();
    let mut reader = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--export-icon" => {
                // packaging aid: write Sanaga's icon as a PNG (size in pixels, default 256)
                let Some(path) = args.next() else {
                    eprintln!("--export-icon needs a file name (.png)");
                    std::process::exit(2);
                };
                let size: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(256);
                let icon = icons::window_icon(size);
                if let Err(e) = image::save_buffer(&path, &icon.rgba, icon.width, icon.height, image::ColorType::Rgba8) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                return Ok(());
            }
            "--screenshot" => {
                let Some(p) = args.next() else {
                    eprintln!("--screenshot demande un nom de fichier .png");
                    std::process::exit(2);
                };
                screenshot = Some(app::ScreenshotJob { path: p.into(), frames: 0, requested: false, done: Arc::new(AtomicBool::new(false)) });
            }
            "--lecteur" | "--reader" => reader = true,
            "--apercu" => urls.push("apercu:".into()),
            "--profil" | "--profile" => {
                if let Some(p) = args.next() {
                    urls.push(format!("profil:{p}"));
                }
            }
            "--app" => {
                if let Some(u) = args.next() {
                    urls.push(format!("webapp:{u}"));
                }
            }
            "-h" | "--help" => {
                println!(
                    "Sanaga {} — a fast, light web browser\n\n\
                     Usage: sanaga [options] [address ...]\n\n\
                     Options:\n\
                     \x20 --profile NAME       use this profile (its own settings, history, cookies)\n\
                     \x20 --app URL            open one site on its own, as a web app\n\
                     \x20 --reader             open the addresses in Reader mode\n\
                     \x20 --apercu             start on the tab overview\n\
                     \x20 --screenshot FILE    draw the window once, save it as a PNG picture, quit\n\
                     \x20 -h, --help           show this help\n\n\
                     Data: ~/.local/share/sanaga (or $XDG_DATA_HOME/sanaga)",
                    env!("CARGO_PKG_VERSION")
                );
                return Ok(());
            }
            _ => urls.push(a),
        }
    }
    if reader {
        urls = urls.into_iter().map(|u| format!("lecteur:{u}")).collect();
    }
    if screenshot.is_none() {
        desktop::hide_desktop_titlebar();
        // The normal window: native GTK (Wayland or X11), pages inside it.
        match shell::run(urls.clone()) {
            Ok(()) => return Ok(()),
            Err(e) => eprintln!("Fenêtre GTK indisponible ({e}) : affichage simple sans moteur web."),
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Sanaga")
            .with_app_id("sanaga")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([560.0, 380.0])
            .with_decorations(false)
            .with_icon(Arc::new(icons::window_icon(64))),
        ..Default::default()
    };
    eframe::run_native("Sanaga", options, Box::new(move |cc| Ok(Box::new(app::Sanaga::new(&cc.egui_ctx, urls, screenshot)))))
}
