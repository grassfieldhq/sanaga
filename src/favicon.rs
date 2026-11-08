//! Site icons (favicons): looked up once per site, in the background, then
//! kept on disk so they show instantly next time. Until an icon is ready (or
//! when a site has none) Sanaga shows the coloured letter badge.

use eframe::egui;
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const SIZE: u32 = 64;
const MAX_AT_ONCE: usize = 6;
const KEEP_DAYS: u64 = 30;

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
static NETWORK: AtomicBool = AtomicBool::new(true);
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State { icons: HashMap::new(), done: Vec::new() }))
}

/// Where icons are kept, and whether they may be fetched from the network.
pub fn setup(dir: PathBuf, network: bool) {
    let _ = std::fs::create_dir_all(&dir);
    let _ = DIR.set(dir);
    NETWORK.store(network, Ordering::Relaxed);
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

/// The icon of the site of `url`, if known; asks for it otherwise.
pub fn get(url: &str) -> Option<egui::TextureHandle> {
    let (scheme, host) = origin(url)?;
    let key = format!("{scheme}://{host}");
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

fn fresh(p: &PathBuf) -> bool {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|age| age.as_secs() < KEEP_DAYS * 86_400)
        .unwrap_or(false)
}

/// Called every frame: starts lookups and turns finished ones into textures.
pub fn pump(ctx: &egui::Context, agent: &ureq::Agent) {
    let Ok(mut st) = state().lock() else { return };
    let done = std::mem::take(&mut st.done);
    for (key, img) in done {
        let fav = match img {
            Some(img) => Fav::Ready(ctx.load_texture(format!("favicon {key}"), img, egui::TextureOptions::LINEAR)),
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
        let (agent, ctx) = (agent.clone(), ctx.clone());
        let started = std::thread::Builder::new().name("sanaga-favicon".into()).spawn(move || {
            let img = find_icon(&agent, &key);
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

fn to_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?;
    if img.width() < 8 {
        return None;
    }
    let img = img.resize(SIZE, SIZE, image::imageops::FilterType::Lanczos3).to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}

fn download(agent: &ureq::Agent, url: &str, limit: u64) -> Option<(String, Vec<u8>)> {
    let resp = agent.get(url).timeout(Duration::from_secs(8)).call().ok()?;
    let ctype = resp.content_type().to_ascii_lowercase();
    let mut buf = Vec::new();
    resp.into_reader().take(limit).read_to_end(&mut buf).ok()?;
    Some((ctype, buf))
}

/// Icon addresses announced by a page: <link rel="icon" href="...">.
pub fn icon_links(html: &str, base: &str) -> Vec<String> {
    let base = url::Url::parse(base).ok();
    let lower = html.to_ascii_lowercase();
    let mut found: Vec<(i32, String)> = Vec::new();
    let mut pos = 0;
    while let Some(i) = lower[pos..].find("<link") {
        let start = pos + i;
        let end = lower[start..].find('>').map(|e| start + e).unwrap_or(lower.len());
        let tag_l = &lower[start..end];
        let tag = &html[start..end];
        pos = end;
        let Some(rel) = attr(tag_l, tag, "rel") else { continue };
        let rel = rel.to_ascii_lowercase();
        if !rel.split_whitespace().any(|r| r == "icon" || r == "apple-touch-icon" || r == "apple-touch-icon-precomposed") {
            continue;
        }
        let Some(href) = attr(tag_l, tag, "href") else { continue };
        let href = crate::html::decode_entities(&href);
        if href.to_ascii_lowercase().ends_with(".svg") || attr(tag_l, tag, "type").map(|t| t.contains("svg")).unwrap_or(false) {
            continue; // vector icons are not drawn
        }
        let abs = match &base {
            Some(b) => b.join(&href).map(|u| u.to_string()).unwrap_or(href),
            None => href,
        };
        // prefer big, square, PNG icons
        let mut score = if rel.contains("apple-touch-icon") { 30 } else { 20 };
        if let Some(sizes) = attr(tag_l, tag, "sizes") {
            if let Some(n) = sizes.split(['x', 'X']).next().and_then(|n| n.trim().parse::<i32>().ok()) {
                score += (n.min(192)) / 8;
            }
        }
        if abs.to_ascii_lowercase().contains(".png") {
            score += 5;
        }
        found.push((score, abs));
    }
    found.sort_by_key(|b| std::cmp::Reverse(b.0));
    found.into_iter().map(|(_, u)| u).collect()
}

fn attr(tag_l: &str, tag: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = tag_l[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let before_ok = at == 0 || tag_l.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = tag_l[at + name.len()..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let eq = at + name.len() + (tag_l[at + name.len()..].len() - rest.len()); // index of '='
        let after = &tag_l[eq + 1..];
        let vstart = eq + 1 + (after.len() - after.trim_start().len());
        let v = &tag[vstart..];
        let q = v.chars().next()?;
        return if q == '"' || q == '\'' {
            v[1..].find(q).map(|e| v[1..1 + e].to_string())
        } else {
            Some(v.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or("").to_string())
        };
    }
    None
}

/// Disk cache, then the site's announced icons, then /favicon.ico.
fn find_icon(agent: &ureq::Agent, key: &str) -> Option<egui::ColorImage> {
    let file = cache_file(key);
    if let Some(f) = &file {
        if fresh(f) {
            if let Some(img) = std::fs::read(f).ok().and_then(|b| to_image(&b)) {
                return Some(img);
            }
        }
    }
    if !NETWORK.load(Ordering::Relaxed) {
        return file.and_then(|f| std::fs::read(f).ok()).and_then(|b| to_image(&b));
    }
    let mut candidates = Vec::new();
    if let Some((_, page)) = download(agent, &format!("{key}/"), 600_000) {
        candidates = icon_links(&String::from_utf8_lossy(&page), &format!("{key}/"));
    }
    candidates.push(format!("{key}/favicon.ico"));
    for c in candidates.iter().take(4) {
        let Some((ctype, bytes)) = download(agent, c, 1_000_000) else { continue };
        if ctype.contains("svg") || ctype.starts_with("text/") {
            continue;
        }
        let Ok(img) = image::load_from_memory(&bytes) else { continue };
        if img.width() < 8 {
            continue;
        }
        let small = img.resize(SIZE, SIZE, image::imageops::FilterType::Lanczos3);
        if let Some(f) = &file {
            let _ = small.save_with_format(f, image::ImageFormat::Png);
        }
        let rgba = small.to_rgba8();
        return Some(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_icon_links() {
        let html = r#"<head><link rel="stylesheet" href="a.css"><link rel="icon" type="image/png" sizes="32x32" href="/i32.png"><link rel='apple-touch-icon' sizes="180x180" href="https://cdn.x/apple.png"><link rel="icon" href="/logo.svg"></head>"#;
        let v = icon_links(html, "https://site.org/page");
        assert_eq!(v, vec!["https://cdn.x/apple.png".to_string(), "https://site.org/i32.png".to_string()]);
        assert_eq!(origin("https://www.a.com:8080/x"), Some(("https".into(), "www.a.com:8080".into())));
    }
}
