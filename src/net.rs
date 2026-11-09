//! Background loading: pages, images and downloads run on worker threads so the
//! window never freezes. Results come back through a channel.

use crate::i18n::{tf, tr};
use eframe::egui;
use crate::html::{self, Document};
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

pub const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) Sanaga/0.1 (navigateur leger)";
const MAX_IMAGE_SIDE: u32 = 1200;

pub enum NetMsg {
    Page { tab: u64, nav: u64, result: Result<Arc<Document>, String> },
    /// A page's source text ("view source").
    Source { tab: u64, nav: u64, url: String, result: Result<String, String> },
    Image { url: String, result: Result<egui::ColorImage, String> },
    Download { tab: u64, nav: u64, name: String, path: PathBuf, bytes: u64, result: Result<(), String> },
}

/// Ask sites for pages in the interface language first.
fn accept_language() -> String {
    let l = crate::i18n::language();
    if l == "en" {
        "en;q=1.0, fr;q=0.5".into()
    } else {
        format!("{l};q=1.0, en;q=0.6")
    }
}

pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(USER_AGENT)
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20))
        .redirects(8)
        .build()
}

pub struct Request {
    pub tab: u64,
    pub nav: u64,
    pub url: String,
    pub post_body: Option<String>,
    /// Show the whole page instead of only its main content.
    pub full: bool,
    /// Show the page's source text instead of the page.
    pub source: bool,
}

pub fn download_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    for name in ["Téléchargements", "Downloads"] {
        let p = home.join(name);
        if p.is_dir() {
            return p;
        }
    }
    let p = home.join("Downloads");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Where a download from `url` is saved (a new name if the file already exists).
pub fn download_path_for(url: &str) -> PathBuf {
    let name = url.split(['?', '#']).next().and_then(|u| u.rsplit('/').next()).filter(|s| !s.is_empty()).unwrap_or("telechargement");
    let name = percent_decode(name);
    unique_path(&download_dir(), &name)
}

fn percent_decode(s: &str) -> String {
    url::form_urlencoded::parse(format!("x={}", s.replace('+', "%2B")).as_bytes()).next().map(|(_, v)| v.into_owned()).unwrap_or_else(|| s.to_string())
}

pub fn unique_path(dir: &std::path::Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 1..1000 {
        let p = dir.join(format!("{stem} ({n}){ext}"));
        if !p.exists() {
            return p;
        }
    }
    dir.join(name)
}

pub fn fetch_page(agent: ureq::Agent, req: Request, tx: Sender<NetMsg>, ctx: egui::Context) {
    std::thread::Builder::new()
        .name("sanaga-page".into())
        .spawn(move || {
            let Request { tab, nav, url, post_body, full, source } = req;
            let send = |result| {
                let _ = tx.send(NetMsg::Page { tab, nav, result });
                ctx.request_repaint();
            };
            if let Some(path) = url.strip_prefix("file://") {
                let path = url::Url::parse(&url).ok().and_then(|u| u.to_file_path().ok()).unwrap_or_else(|| PathBuf::from(path));
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        let lower = path.to_string_lossy().to_ascii_lowercase();
                        let doc = if lower.ends_with(".html") || lower.ends_with(".htm") {
                            html::parse_page(&text, &url, !full)
                        } else if [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|e| lower.ends_with(e)) {
                            html::image_doc(&url, bytes.len())
                        } else {
                            html::plain(&text, &url)
                        };
                        send(Ok(Arc::new(doc)));
                    }
                    Err(e) => send(Err(tf("Fichier introuvable : {}", &[&e]))),
                }
                return;
            }
            let resp = match &post_body {
                Some(body) => agent
                    .post(&url)
                    .set("Content-Type", "application/x-www-form-urlencoded")
                    .set("Accept", "text/html,text/plain;q=0.9,*/*;q=0.5")
                    .set("Accept-Language", &accept_language())
                    .send_string(body),
                None => agent.get(&url).set("Accept", "text/html,text/plain;q=0.9,*/*;q=0.5").set("Accept-Language", &accept_language()).call(),
            };
            let resp = match resp {
                Ok(r) => r,
                Err(ureq::Error::Status(_, r)) => r, // still show the server's error page
                Err(e) => {
                    send(Err(describe_error(&e)));
                    return;
                }
            };
            let final_url = resp.get_url().to_string();
            let ctype = resp.content_type().to_ascii_lowercase();
            let is_text = ctype.starts_with("text/") || ctype.contains("xml") || ctype.contains("json") || ctype.is_empty();
            let attachment = resp.header("Content-Disposition").map(|d| d.to_ascii_lowercase().starts_with("attachment")).unwrap_or(false);
            if ctype.starts_with("image/") && !attachment {
                let len = resp.header("Content-Length").and_then(|l| l.parse().ok()).unwrap_or(0);
                send(Ok(Arc::new(html::image_doc(&final_url, len))));
                return;
            }
            if !is_text || attachment {
                // Save to the downloads folder.
                let name = resp
                    .header("Content-Disposition")
                    .and_then(|d| d.split("filename=").nth(1))
                    .map(|n| n.trim_matches(|c| c == '"' || c == '\'' || c == ';' || c == ' ').to_string())
                    .filter(|n| !n.is_empty())
                    .or_else(|| final_url.split(['?', '#']).next().and_then(|u| u.rsplit('/').next()).filter(|s| !s.is_empty()).map(|s| s.to_string()))
                    .unwrap_or_else(|| "telechargement".into());
                let name: String = name.chars().map(|c| if c == '/' || c == '\\' { '_' } else { c }).collect();
                let path = unique_path(&download_dir(), &name);
                let result = std::fs::File::create(&path)
                    .and_then(|mut f| std::io::copy(&mut resp.into_reader(), &mut f))
                    .map_err(|e| e.to_string());
                let bytes = *result.as_ref().unwrap_or(&0);
                let _ = tx.send(NetMsg::Download { tab, nav, name, path, bytes, result: result.map(|_| ()) });
                ctx.request_repaint();
                return;
            }
            match resp.into_string() {
                Ok(body) if source => {
                    let _ = tx.send(NetMsg::Source { tab, nav, url: final_url, result: Ok(body) });
                    ctx.request_repaint();
                }
                Ok(body) => {
                    let doc = if ctype.starts_with("text/html") || ctype.contains("xhtml") || ctype.is_empty() {
                        html::parse_page(&body, &final_url, !full)
                    } else {
                        html::plain(&body, &final_url)
                    };
                    send(Ok(Arc::new(doc)));
                }
                Err(e) => send(Err(tf("La page n'a pas pu être lue : {}", &[&e]))),
            }
        })
        .ok();
}

fn describe_error(e: &ureq::Error) -> String {
    let s = e.to_string();
    let low = s.to_ascii_lowercase();
    if low.contains("dns") || low.contains("resolve") || low.contains("lookup") {
        tr("Adresse introuvable (DNS). Vérifiez l'orthographe ou votre connexion.").into()
    } else if low.contains("timed out") || low.contains("timeout") {
        tr("Le site met trop de temps à répondre.").into()
    } else if low.contains("refused") {
        tr("Le site a refusé la connexion.").into()
    } else if low.contains("certificate") || low.contains("tls") {
        tf("Connexion sécurisée impossible : {}", &[&s])
    } else {
        s
    }
}

pub fn fetch_image(agent: ureq::Agent, url: String, tx: Sender<NetMsg>, ctx: egui::Context) {
    std::thread::Builder::new()
        .name("sanaga-img".into())
        .spawn(move || {
            let result = (|| -> Result<egui::ColorImage, String> {
                let bytes = if url.starts_with("file://") {
                    let p = url::Url::parse(&url).ok().and_then(|u| u.to_file_path().ok()).ok_or("chemin invalide")?;
                    std::fs::read(p).map_err(|e| e.to_string())?
                } else {
                    let resp = agent.get(&url).set("Accept", "image/png,image/jpeg,image/gif,image/webp,*/*;q=0.5").call().map_err(|e| e.to_string())?;
                    let mut buf = Vec::new();
                    resp.into_reader().take(12 * 1024 * 1024).read_to_end(&mut buf).map_err(|e| e.to_string())?;
                    buf
                };
                decode_image(&bytes)
            })();
            let _ = tx.send(NetMsg::Image { url, result });
            ctx.request_repaint();
        })
        .ok();
}

pub fn decode_image(bytes: &[u8]) -> Result<egui::ColorImage, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    // Downscale big pictures right away: keeps memory use low.
    let img = if img.width() > MAX_IMAGE_SIDE || img.height() > MAX_IMAGE_SIDE {
        img.thumbnail(MAX_IMAGE_SIDE, MAX_IMAGE_SIDE)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

/// Resident memory of this process, in bytes (Linux).
/// Includes the helper processes started by Sanaga (WebKit's page and network processes).
pub fn memory_used() -> Option<u64> {
    // "Pss": each process's fair share of memory (shared libraries are split
    // between the processes using them instead of being counted several times).
    let rss = |pid: &str| -> u64 {
        if let Ok(t) = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")) {
            if let Some(kb) = t.lines().find(|l| l.starts_with("Pss:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<u64>().ok()) {
                return kb * 1024;
            }
        }
        std::fs::read_to_string(format!("/proc/{pid}/statm"))
            .ok()
            .and_then(|s| s.split_whitespace().nth(1).and_then(|p| p.parse::<u64>().ok()))
            .unwrap_or(0)
            * 4096
    };
    let me = std::process::id();
    let mut parents: Vec<(u32, u32)> = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for e in dir.flatten() {
            let name = e.file_name();
            let Some(pid) = name.to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
            if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                // the parent id is the 2nd field after the ")" that ends the name
                if let Some(ppid) = stat.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(1)).and_then(|p| p.parse().ok()) {
                    parents.push((pid, ppid));
                }
            }
        }
    }
    let mut family = vec![me];
    let mut k = 0;
    while k < family.len() {
        let p = family[k];
        family.extend(parents.iter().filter(|(_, pp)| *pp == p).map(|(c, _)| *c));
        k += 1;
    }
    let total: u64 = family.iter().map(|p| rss(&p.to_string())).sum();
    if total == 0 {
        None
    } else {
        Some(total)
    }
}

/// The memory used by Sanaga and its engine, measured in a background
/// thread every 10 s while someone looks at it (the toolbar's counter):
/// measuring walks every process of the computer and costs some 30 ms, too
/// much for the window's own thread (the window would hitch).
pub fn memory_used_cached() -> u64 {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    static VALUE: AtomicU64 = AtomicU64::new(0);
    static WANTED: AtomicBool = AtomicBool::new(false);
    static STARTED: std::sync::Once = std::sync::Once::new();
    WANTED.store(true, Ordering::Relaxed);
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new().name("sanaga-memory".into()).spawn(|| loop {
            // (not shown since the last measure: nothing to do)
            if WANTED.swap(false, Ordering::Relaxed) {
                VALUE.store(memory_used().unwrap_or(0), Ordering::Relaxed);
            }
            std::thread::sleep(std::time::Duration::from_secs(10));
        });
    });
    VALUE.load(Ordering::Relaxed)
}

pub fn human_bytes(b: u64) -> String {
    if b >= 1024 * 1024 * 1024 {
        tf("{} Go", &[&format!("{:.1}", b as f64 / 1073741824.0)])
    } else if b >= 1024 * 1024 {
        tf("{} Mo", &[&format!("{:.1}", b as f64 / 1048576.0)])
    } else if b >= 1024 {
        tf("{} Ko", &[&(b / 1024)])
    } else {
        tf("{} o", &[&b])
    }
}
