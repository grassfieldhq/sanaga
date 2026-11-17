//! Sanaga — a fast, light web browser for Linux (GTK + WebKitGTK).
//!
//! Usage:
//!   sanaga [options] [address ...]     (see `sanaga --help`)
//!   sanaga --screenshot picture.png [address ...]

#![allow(deprecated)]
// Drawing helpers take many plain values, some state holds composite types and
// a few layout loops index parallel lists: kept on purpose for readability.
#![allow(clippy::too_many_arguments, clippy::type_complexity, clippy::needless_range_loop)]

// Sanaga's window (GTK 4) and web engine (WebKitGTK) are Linux / BSD only for
// now: say so clearly instead of failing deep inside a dependency.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "ios", target_os = "android"))]
compile_error!("Sanaga currently runs on Linux (and BSD) only: it is built on GTK 4 and WebKitGTK. See docs/PORTING.md for the Windows and macOS plan.");

mod ai;
mod ai_cli;
mod app;
mod bidi;
mod blocklist;
mod codecs;
mod crash;
mod danger;
mod desktop;
mod doh;
mod extensions;
mod favicon;
mod frames;
#[cfg(test)]
mod glyphs;
mod guard;
mod history;
mod html;
mod i18n;
mod icons;
mod identity;
mod import;
mod keyring;
mod migrate;
#[cfg(test)]
mod mutate;
mod net;
mod page;
mod shell;
mod site_prefs;
mod store;
mod sync;
mod system_look;
mod theme;
mod webext;

use eframe::egui;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// A font file of the system, mapped into memory for as long as Sanaga runs.
fn system_font(path: &str) -> Option<&'static [u8]> {
    let file = std::fs::File::open(path).ok()?;
    // (the file is only read; a font package updated meanwhile replaces the
    // file, it does not change this one's content)
    let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    let kept: &'static memmap2::Mmap = Box::leak(Box::new(map));
    Some(&kept[..])
}

pub fn setup_fonts(ctx: &egui::Context) {
    match EMOJI.get() {
        Some(found) => ctx.set_fonts(fonts(*found)),
        None => match EMOJI_PLACES.iter().find_map(|p| system_font(p)) {
            Some(bytes) => ctx.set_fonts(fonts(*EMOJI.get_or_init(|| Some((bytes, 0))))),
            // Not in the usual places: the system is asked which font it has
            // for emoji, away from the window (a program to start); the
            // black-and-white ones show until it has answered.
            None => {
                ctx.set_fonts(fonts(None));
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let asked = emoji_of_the_system();
                    if let found @ Some(_) = *EMOJI.get_or_init(|| asked) {
                        ctx.set_fonts(fonts(found));
                        ctx.request_repaint();
                    }
                });
            }
        },
    }
    // Other alphabets (Arabic, Chinese, Devanagari, Khmer...): the fonts of
    // the system, looked for when a character first needs one. The system
    // itself says which font it has for a script and a language (fontconfig):
    // no list of folders and file names to keep here, and nothing is read for
    // a script that never shows.
    ctx.add_font_provider(system_fonts());
}

/// The system's colour emoji font (its bytes and its number within its file),
/// once it has been looked for; `Some(None)`: there is none.
static EMOJI: std::sync::OnceLock<Option<(&'static [u8], u32)>> = std::sync::OnceLock::new();

/// Where the usual colour emoji font is on most systems: found there, no
/// program has to be started to ask.
const EMOJI_PLACES: [&str; 4] = [
    "/usr/share/fonts/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/google-noto-color-emoji-fonts/NotoColorEmoji.ttf",
    "/usr/share/fonts/google-noto-emoji/NotoColorEmoji.ttf",
];

/// The system's own answer for "emoji", whatever the font is called and
/// wherever it is.
fn emoji_of_the_system() -> Option<(&'static [u8], u32)> {
    let out = std::process::Command::new("fc-match").args(["-f", "%{file}\n%{index}\n%{color}\n", "emoji"]).stderr(std::process::Stdio::null()).output().ok()?;
    let (file, index) = emoji_answer(&String::from_utf8_lossy(&out.stdout))?;
    Some((system_font(&file)?, index))
}

/// What `fc-match` answered for "emoji": the file and the font's number in it,
/// when it is a colour font the interface can read. (Asked for a font that is
/// not installed, the system answers with its closest one: a plain text font
/// placed before the interface's own would redraw its digits and signs.)
fn emoji_answer(out: &str) -> Option<(String, u32)> {
    let mut lines = out.lines();
    let file = lines.next()?.trim();
    let index: u32 = lines.next()?.trim().parse().ok()?;
    let colour = lines.next()?.trim().eq_ignore_ascii_case("true");
    let readable = [".ttf", ".otf", ".ttc"].iter().any(|e| file.to_ascii_lowercase().ends_with(e));
    (colour && readable && file.starts_with('/')).then(|| (file.to_string(), index))
}

/// The system's fonts, listed once (on a thread of its own: the list can take
/// a moment on a machine with many fonts) and shared by every window.
fn system_fonts() -> Arc<egui_system_font_provider::SystemFontProvider> {
    static FONTS: std::sync::OnceLock<Arc<egui_system_font_provider::SystemFontProvider>> = std::sync::OnceLock::new();
    FONTS.get_or_init(|| Arc::new(egui_system_font_provider::SystemFontProvider::new())).clone()
}

/// The interface's fonts: its own (Inter, Noto), then the colour emoji font
/// of the system when there is one, then the built-in signs and emoji.
fn fonts(emoji: Option<(&'static [u8], u32)>) -> egui::FontDefinitions {
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
    // Emoji in colour, before the built-in black-and-white ones. (The file is
    // mapped, not read: a font takes memory only for the parts really used.)
    let mut fallback = fallback;
    if let Some((bytes, index)) = emoji {
        let mut data = egui::FontData::from_static(bytes);
        data.index = index;
        f.font_data.insert("emoji".to_string(), Arc::new(data));
        fallback.insert(0, "emoji".to_string());
    }
    let fallback: Vec<String> = std::iter::once("noto".to_string()).chain(fallback).collect();
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
    f
}

/// If Sanaga ever crashes, note why in `crash.log` (in its data folder), so the
/// problem can be reported and fixed; the usual message still goes to the terminal.
fn log_crashes() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let dir = store::Store::root_dir();
        store::ensure_private_dir(&dir);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(dir.join("crash.log")) {
            let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
            let what = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
            let _ = writeln!(f, "[{}] Sanaga {} crashed at {place}: {what}", store::now(), env!("CARGO_PKG_VERSION"));
        }
        // the report the person can read and send (sanaga://crashes): where
        // it happened and the message, private details taken out
        let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let what = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("?").to_string();
        crash::write(&dir, crash::Kind::Interface, &page::engine_version(), &format!("Where: {place}\nThread: {thread}\nMessage: {what}"));
        // (what was being saved reaches the disk before Sanaga goes)
        store::flush_saves();
        default(info);
    }));
}

/// Test aid (`SANAGA_DEBUG_START=1`): how long each step of the start took.
pub fn stage(name: &str) {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let t = START.get_or_init(std::time::Instant::now).elapsed();
    if std::env::var_os("SANAGA_DEBUG_START").is_some() {
        eprintln!("start {:>7.1} ms  {name}", t.as_secs_f64() * 1000.0);
    }
}

fn main() -> eframe::Result {
    stage("main");
    log_crashes();
    // "Is there a network?": from the system's routes (netlink), not from
    // NetworkManager's Internet check. On Ubuntu that check (a request to
    // Ubuntu's server) often fails while the connection works, and then
    // WebKit tells every page it is offline (navigator.onLine false: Google
    // and many sites behave as if there were no Internet). Set before GTK,
    // GIO and WebKit's processes start; a choice made by the person stays.
    if std::env::var_os("GIO_USE_NETWORK_MONITOR").is_none() {
        unsafe {
            std::env::set_var("GIO_USE_NETWORK_MONITOR", "netlink");
        }
    }
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
            // a new window in the running Sanaga (or a normal start)
            "--new-window" | "--nouvelle-fenetre" => urls.push("nouvelle-fenetre:".into()),
            // a link previewed in a small window (in the running Sanaga)
            "--preview" => {
                if let Some(u) = args.next() {
                    urls.push(format!("preview:{u}"));
                }
            }
            "--profil" | "--profile" => {
                if let Some(p) = args.next() {
                    urls.push(format!("profil:{p}"));
                }
            }
            // a window that keeps nothing (Chrome's guest)
            "--guest" | "--invite" => urls.push("invite:".into()),
            // driven by test software (WebDriver, see docs/AUTOMATION.md): a
            // window of its own that keeps nothing and never touches the
            // person's profile
            "--automation" => {
                urls.push("invite:".into());
                urls.push("automation:".into());
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
                     \x20 --guest              a guest window (nothing is kept)\n\
                     \x20 --app URL            open one site on its own, as a web app\n\
                     \x20 --new-window         open a new window (in the running Sanaga)\n\
                     \x20 --preview URL        preview a link in a small window\n\
                     \x20 --reader             open the addresses in Reader mode\n\
                     \x20 --apercu             start on the tab overview\n\
                     \x20 --screenshot FILE    draw the window once, save it as a PNG picture, quit\n\
                     \x20 --automation         let test software drive Sanaga (WebDriver); keeps nothing\n\
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
    match screenshot.take() {
        None => {
            desktop::hide_desktop_titlebar();
            // The normal window: native GTK (Wayland or X11), pages inside it.
            match shell::run(urls.clone()) {
                Ok(()) => return Ok(()),
                Err(e) => eprintln!("Fenêtre GTK indisponible ({e}) : affichage simple sans moteur web."),
            }
        }
        Some(job) => {
            // the same GTK window, which draws even when it is not on screen
            let path = job.path.clone();
            match shell::run_screenshot(urls.clone(), job) {
                Ok(()) if path.exists() => return Ok(()),
                Ok(()) => eprintln!("Capture : nouvel essai sans GTK"),
                Err(e) => eprintln!("Fenêtre GTK indisponible ({e}) : capture sans moteur web."),
            }
            screenshot = Some(app::ScreenshotJob { path, frames: 0, requested: false, done: Arc::new(AtomicBool::new(false)) });
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
    let done = eframe::run_native("Sanaga", options, Box::new(move |cc| Ok(Box::new(app::Sanaga::new(&cc.egui_ctx, urls, screenshot)))));
    store::flush_saves();
    crash::mark_stopped();
    done
}

#[cfg(test)]
mod font_tests {
    use super::emoji_answer;

    #[test]
    fn only_a_colour_font_file_is_taken_for_emoji() {
        assert_eq!(emoji_answer("/usr/share/fonts/x/Twemoji.ttf\n0\nTrue\n"), Some(("/usr/share/fonts/x/Twemoji.ttf".to_string(), 0)));
        assert_eq!(emoji_answer("/usr/share/fonts/x/Emoji.ttc\n2\nTrue\n"), Some(("/usr/share/fonts/x/Emoji.ttc".to_string(), 2)));
        // no emoji font installed: the system answers with a text font
        assert_eq!(emoji_answer("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf\n0\nFalse\n"), None);
        // a bitmap font of another kind, an empty answer, a missing line
        assert_eq!(emoji_answer("/usr/share/fonts/x/emoji.pcf.gz\n0\nTrue\n"), None);
        assert_eq!(emoji_answer(""), None);
        assert_eq!(emoji_answer("/usr/share/fonts/x/Twemoji.ttf\n"), None);
    }
}
