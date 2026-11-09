//! Site icons (favicons). Sanaga never fetches them itself: WebKit loads a
//! page's icon with the page, through that page's own network session (a
//! private tab's session keeps nothing, a container has its own cookies),
//! and hands it over. Icons of normal tabs are then kept on disk so they
//! show instantly next time (in the favourites, history, new tab); icons of
//! private tabs stay in memory only. A site never visited (such as the
//! default favourites at the first start) shows the coloured letter badge
//! until the person opens it.

use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const SIZE: u32 = 64;
const MAX_AT_ONCE: usize = 6;

enum Fav {
    Waiting,
    Loading,
    Ready(egui::TextureHandle),
    Failed,
}

struct State {
    icons: HashMap<String, Fav>,
    done: Vec<(String, Option<egui::ColorImage>)>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static DIR: OnceLock<PathBuf> = OnceLock::new();
/// Icons may be written to disk (not while taking a screenshot).
static KEEP: AtomicBool = AtomicBool::new(true);
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State { icons: HashMap::new(), done: Vec::new() }))
}

/// Where icons are kept, and whether new ones may be written there.
pub fn setup(dir: PathBuf, keep: bool) {
    let _ = std::fs::create_dir_all(&dir);
    let _ = DIR.set(dir);
    KEEP.store(keep, Ordering::Relaxed);
}

/// "https://www.site.com/page" → ("https", "www.site.com").
fn origin(url: &str) -> Option<(String, String)> {
    let u = url::Url::parse(url).ok()?;
    if u.scheme() != "http" && u.scheme() != "https" {
        return None;
    }
    let host = u.host_str()?.to_string();
    let host = match u.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    };
    Some((u.scheme().to_string(), host))
}

fn key_of(url: &str) -> Option<String> {
    origin(url).map(|(scheme, host)| format!("{scheme}://{host}"))
}

/// The icon of the site of `url`, if known; looked for on disk otherwise.
pub fn get(url: &str) -> Option<egui::TextureHandle> {
    let key = key_of(url)?;
    let mut st = state().lock().ok()?;
    match st.icons.get(&key) {
        Some(Fav::Ready(t)) => Some(t.clone()),
        Some(_) => None,
        None => {
            st.icons.insert(key, Fav::Waiting);
            None
        }
    }
}

fn cache_file(key: &str) -> Option<PathBuf> {
    let name: String = key.replace("://", "_").chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' }).collect();
    DIR.get().map(|d| d.join(format!("{name}.png")))
}

/// Called every frame: reads kept icons from disk (never from the network)
/// and turns finished ones into textures.
pub fn pump(ctx: &egui::Context) {
    let Ok(mut st) = state().lock() else { return };
    let done = std::mem::take(&mut st.done);
    for (key, img) in done {
        let fav = match img {
            Some(img) => Fav::Ready(ctx.load_texture(format!("favicon {key}"), img, egui::TextureOptions::LINEAR)),
            // (an icon already shown stays when a later read fails)
            None if matches!(st.icons.get(&key), Some(Fav::Ready(_))) => continue,
            None => Fav::Failed,
        };
        st.icons.insert(key, fav);
    }
    let waiting: Vec<String> = st.icons.iter().filter(|(_, f)| matches!(f, Fav::Waiting)).map(|(k, _)| k.clone()).collect();
    for key in waiting {
        if IN_FLIGHT.load(Ordering::Relaxed) >= MAX_AT_ONCE {
            break;
        }
        st.icons.insert(key.clone(), Fav::Loading);
        IN_FLIGHT.fetch_add(1, Ordering::Relaxed);
        let ctx = ctx.clone();
        let started = std::thread::Builder::new().name("sanaga-favicon".into()).spawn(move || {
            let img = cache_file(&key).and_then(|f| std::fs::read(f).ok()).and_then(|b| to_image(&b));
            if let Ok(mut st) = state().lock() {
                st.done.push((key, img));
            }
            IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
            ctx.request_repaint();
        });
        if started.is_err() {
            IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// WebKit has the icon of the page at `page_url` (PNG bytes): shown at
/// once, and kept on disk unless it comes from a private tab.
pub fn learn(page_url: &str, png: Vec<u8>, private: bool) {
    let Some(key) = key_of(page_url) else { return };
    let _ = std::thread::Builder::new().name("sanaga-favicon".into()).spawn(move || {
        let Ok(img) = image::load_from_memory(&png) else { return };
        if img.width() < 8 {
            return;
        }
        let small = img.resize(SIZE, SIZE, image::imageops::FilterType::Lanczos3);
        if !private && KEEP.load(Ordering::Relaxed) {
            if let Some(f) = cache_file(&key) {
                let _ = small.save_with_format(f, image::ImageFormat::Png);
            }
        }
        let rgba = small.to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
        if let Ok(mut st) = state().lock() {
            st.done.push((key, Some(ci)));
        }
    });
}

fn to_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?;
    if img.width() < 8 {
        return None;
    }
    let img = img.resize(SIZE, SIZE, image::imageops::FilterType::Lanczos3).to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_are_kept_per_site() {
        assert_eq!(origin("https://www.a.com:8080/x"), Some(("https".into(), "www.a.com:8080".into())));
        assert_eq!(key_of("https://www.a.com/page?q=1"), Some("https://www.a.com".to_string()));
        assert_eq!(key_of("sanaga://newtab"), None);
        assert_eq!(key_of("file:///tmp/a.html"), None);
    }
}
