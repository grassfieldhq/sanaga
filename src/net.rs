//! Background loading: pages, images and downloads run on worker threads so the
//! window never freezes. Results come back through a channel.

use crate::i18n::{tf, tr};
use eframe::egui;
use crate::html::{self, Document};
use std::io::Read;
use std::path::{Path, PathBuf};
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
    /// A fetch no action of the person's started (a restored tab) turned out
    /// to be a download: asked first (the request, to run again if allowed).
    DownloadAsk { tab: u64, nav: u64, site: String, req: Request },
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

/// Where distributions keep the certificate authorities the system trusts
/// (one PEM bundle); $SSL_CERT_FILE first, as OpenSSL does.
fn ca_bundles() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::env::var_os("SSL_CERT_FILE").map(PathBuf::from).into_iter().collect();
    v.extend(
        [
            "/etc/ssl/certs/ca-certificates.crt",                // Arch, Debian, Ubuntu, Gentoo
            "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem", // Fedora, RHEL
            "/etc/pki/tls/certs/ca-bundle.crt",                  // older Fedora, RHEL
            "/etc/ssl/ca-bundle.pem",                            // openSUSE
            "/etc/ssl/cert.pem",                                 // Alpine, OpenBSD
            "/usr/local/share/certs/ca-root-nss.crt",            // FreeBSD
        ]
        .map(PathBuf::from),
    );
    v
}

/// The system's certificate authorities, read from the first bundle found.
fn system_roots() -> rustls::RootCertStore {
    use rustls::pki_types::{pem::PemObject, CertificateDer};
    let mut roots = rustls::RootCertStore::empty();
    for path in ca_bundles() {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let (added, _) = roots.add_parsable_certificates(CertificateDer::pem_slice_iter(&bytes).filter_map(|c| c.ok()));
        if added > 0 {
            break;
        }
    }
    if roots.is_empty() {
        eprintln!("Sanaga: no system certificate store found; secure requests made by Sanaga itself will fail");
    }
    roots
}

/// TLS for the requests Sanaga makes itself (Reader, page source, AI
/// services): the system's certificate authorities, the same list the rest
/// of the system trusts (an organisation's own, the distribution's
/// removals), never a list built into Sanaga.
fn system_tls() -> Option<Arc<rustls::ClientConfig>> {
    static CONFIG: std::sync::OnceLock<Option<Arc<rustls::ClientConfig>>> = std::sync::OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .ok()?
                .with_root_certificates(system_roots())
                .with_no_client_auth();
            Some(Arc::new(config))
        })
        .clone()
}

/// A builder for Sanaga's own requests, with the system's certificates.
pub fn builder() -> ureq::AgentBuilder {
    let b = ureq::AgentBuilder::new();
    match system_tls() {
        Some(tls) => b.tls_config(tls),
        None => b,
    }
}

pub fn agent() -> ureq::Agent {
    builder()
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
    /// The page the tab showed before (the referrer of a download).
    pub referrer: String,
    /// The person asked for this (typed, clicked in Sanaga): if it is a
    /// download, it goes straight to the Downloads folder. Otherwise (a
    /// restored tab) the person is asked first, as for WebKit's downloads.
    pub allow_download: bool,
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

// (download names, the download mark: see guard.rs, testable without GTK)
pub use crate::guard::{mark_download, safe_download_name, unique_path, unique_path_with};

/// Where a download is saved: in the Downloads folder, under a safe name
/// made from `suggested` (WebKit's suggestion: the site's
/// Content-Disposition, else the address), or from the address itself.
pub fn download_destination(suggested: &str, url: &str, taken: impl Fn(&Path) -> bool) -> PathBuf {
    let from_url = url.split(['?', '#']).next().and_then(|u| u.rsplit('/').next()).unwrap_or("");
    let raw = if suggested.trim().is_empty() { from_url } else { suggested };
    unique_path_with(&download_dir(), &safe_download_name(raw), taken)
}

/// A file Sanaga saves itself in the Downloads folder (a page kept with
/// Ctrl+S): a safe, free name.
pub fn download_path_named(name: &str) -> PathBuf {
    unique_path(&download_dir(), &safe_download_name(name))
}

/// Create a new file in the Downloads folder for a download Sanaga makes
/// itself: a safe name, and the file is created only if nothing is there
/// (never replacing a file, never following a link: O_CREAT | O_EXCL |
/// O_NOFOLLOW).
pub fn create_download(suggested: &str) -> std::io::Result<(std::fs::File, PathBuf)> {
    use std::os::unix::fs::OpenOptionsExt;
    let dir = download_dir();
    let name = safe_download_name(suggested);
    for _ in 0..32 {
        let path = unique_path(&dir, &name);
        match std::fs::OpenOptions::new().write(true).create_new(true).custom_flags(libc::O_NOFOLLOW).open(&path) {
            Ok(f) => return Ok((f, path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("no free file name in the Downloads folder"))
}

pub fn fetch_page(agent: ureq::Agent, req: Request, tx: Sender<NetMsg>, ctx: egui::Context) {
    std::thread::Builder::new()
        .name("sanaga-page".into())
        .spawn(move || {
            let Request { tab, nav, url, post_body, full, source, referrer, allow_download } = req;
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
                // the source of something that is not text: nothing to show,
                // and nothing saved
                if source {
                    let _ = tx.send(NetMsg::Source { tab, nav, url: final_url, result: Err(tr("Ce n'est pas une page de texte : il n'y a pas de source à afficher.").into()) });
                    ctx.request_repaint();
                    return;
                }
                // no action of the person's behind this fetch: asked first
                if !allow_download {
                    let site = crate::guard::download_site(&referrer, &final_url);
                    let req = Request { tab, nav, url, post_body, full, source, referrer, allow_download: true };
                    let _ = tx.send(NetMsg::DownloadAsk { tab, nav, site, req });
                    ctx.request_repaint();
                    return;
                }
                // Save to the downloads folder.
                let name = resp
                    .header("Content-Disposition")
                    .and_then(|d| d.split("filename=").nth(1))
                    .map(|n| n.trim_matches(|c| c == '"' || c == '\'' || c == ';' || c == ' ').to_string())
                    .filter(|n| !n.is_empty())
                    .or_else(|| final_url.split(['?', '#']).next().and_then(|u| u.rsplit('/').next()).filter(|s| !s.is_empty()).map(|s| s.to_string()))
                    .unwrap_or_else(|| "telechargement".into());
                // (a safe name, a new file: never one that was already there)
                let (path, result) = match create_download(&name) {
                    Ok((mut f, path)) => (path, std::io::copy(&mut resp.into_reader(), &mut f).map_err(|e| e.to_string())),
                    Err(e) => (download_dir().join(safe_download_name(&name)), Err(e.to_string())),
                };
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(name);
                // where it came from, as for WebKit's downloads (the file
                // manager asks before running a downloaded program)
                if result.is_ok() {
                    if let Err(e) = mark_download(&path, &final_url, &referrer) {
                        eprintln!("download mark: {e}");
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_trust_the_systems_certificates() {
        assert!(ca_bundles().iter().any(|p| p.ends_with("ca-certificates.crt")));
        // (only where the usual bundle is installed)
        if std::path::Path::new("/etc/ssl/certs/ca-certificates.crt").is_file() && std::env::var_os("SSL_CERT_FILE").is_none() {
            assert!(system_roots().len() > 10);
            assert!(system_tls().is_some());
        }
    }
}
