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
/// The most that is read of a picture, or of a local file shown as a page
/// (the Light engine and the Reader keep what they read in memory).
const MAX_BODY: u64 = 12 * 1024 * 1024;

/// Read a file of this computer for the Light engine: an ordinary file
/// only, and its first `MAX_BODY` bytes at most. A device such as
/// `/dev/zero` never ends and a pipe never answers: read like a file, they
/// filled Sanaga's memory or left the tab loading for ever.
fn read_local(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::os::unix::fs::OpenOptionsExt;
    // (opened without waiting — a pipe would — then looked at: the file that is read is the one that was checked)
    let f = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(path)?;
    let meta = f.metadata()?;
    if meta.is_dir() {
        return Err(std::io::Error::from_raw_os_error(libc::EISDIR));
    }
    if !meta.is_file() {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    let mut bytes = Vec::new();
    f.take(MAX_BODY).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The file name a site gives a download in its `Content-Disposition`
/// header, as the site wrote it (what it may hold — folders, `..`, encoded
/// characters — is dealt with by `safe_download_name`, which every
/// download name goes through). The header is read as browsers read it:
/// parameters separated by `;`, values in quotes or not, names in any
/// case, and `filename*=UTF-8''…` (any language, percent-encoded) rather
/// than `filename=` when both are there. (It used to be cut at the first
/// `filename=`: `filename="rapport.pdf"; filename*=UTF-8''rapport.pdf`
/// gave a file called `rapport.pdf"; filename*=UTF-8''rapport.pdf`.)
pub fn disposition_name(header: &str) -> Option<String> {
    // the parts between `;` — a `;` inside quotes belongs to its value
    let mut parts = vec![String::new()];
    let (mut quoted, mut escaped) = (false, false);
    for c in header.chars() {
        if quoted {
            quoted = escaped || c != '"';
            escaped = !escaped && c == '\\';
        } else if c == ';' {
            parts.push(String::new());
            continue;
        } else {
            quoted = c == '"';
        }
        parts.last_mut()?.push(c);
    }
    let (mut plain, mut extended) = (None::<String>, None::<String>);
    for part in &parts {
        let Some((name, value)) = part.split_once('=') else { continue };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "filename" if plain.is_none() => {
                plain = Some(match value.strip_prefix('"') {
                    // in quotes: up to the closing one, `\x` standing for x
                    Some(inside) => {
                        let mut text = String::new();
                        let mut chars = inside.chars();
                        while let Some(c) = chars.next() {
                            match c {
                                '"' => break,
                                '\\' => text.extend(chars.next()),
                                c => text.push(c),
                            }
                        }
                        text
                    }
                    None => value.trim_matches('\'').to_string(),
                });
            }
            // charset'language'percent-encoded bytes
            "filename*" if extended.is_none() => {
                let mut fields = value.splitn(3, '\'');
                let (Some(charset), Some(_language), Some(encoded)) = (fields.next(), fields.next(), fields.next()) else { continue };
                extended = match charset.trim().to_ascii_lowercase().as_str() {
                    // (left encoded: `safe_download_name` decodes it, once)
                    "utf-8" => Some(encoded.to_string()),
                    // one byte, one character: written again the UTF-8 way
                    "iso-8859-1" => {
                        let text: String = percent_encoding::percent_decode_str(encoded).map(|b| b as char).collect();
                        Some(percent_encoding::utf8_percent_encode(&text, percent_encoding::NON_ALPHANUMERIC).to_string())
                    }
                    _ => None,
                };
            }
            _ => {}
        }
    }
    let named = |n: &String| !n.trim().is_empty();
    extended.filter(named).or(plain.filter(named))
}

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

/// Where the releases of Sanaga are listed (what "Look for a new version"
/// asks, and only when the person clicks: Sanaga asks nothing by itself).
const RELEASES_API: &str = "https://api.github.com/repos/grassfieldhq/sanaga/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/grassfieldhq/sanaga/releases/latest";

/// The newest released version ("0.5.0"), as GitHub gives it.
pub fn latest_release() -> Result<String, String> {
    // (test aid, debug builds only: another address to ask)
    let address = if cfg!(debug_assertions) { std::env::var("SANAGA_UPDATE_URL").unwrap_or_else(|_| RELEASES_API.to_string()) } else { RELEASES_API.to_string() };
    let body = agent().get(&address).set("Accept", "application/vnd.github+json").call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
    let tag = crate::ai::json_string(&body, "tag_name").ok_or("no version in the answer")?;
    let version = tag.trim().trim_start_matches('v').to_string();
    // (a version is numbers and dots: nothing else is shown to the person)
    if version.is_empty() || version.len() > 32 || !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err("not a version".into());
    }
    Ok(version)
}

/// Is version `latest` newer than `current`? ("0.10.0" is newer than
/// "0.9.2": numbers are compared, not letters.)
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.trim().parse().unwrap_or(0)).collect() };
    let (a, b) = (parts(latest), parts(current));
    for k in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(k).copied().unwrap_or(0), b.get(k).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
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
                match local_page(&path, &url, !full) {
                    Ok(doc) => send(Ok(Arc::new(doc))),
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
                    .and_then(disposition_name)
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

/// A file of this computer as a page of the Light engine: a web page, a
/// picture, or plain text (see `read_local` for what is read of it).
fn local_page(path: &Path, url: &str, main_only: bool) -> std::io::Result<Document> {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    if [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|e| lower.ends_with(e)) {
        // (only its size is wanted here: the picture is read when it is shown)
        let meta = std::fs::metadata(path)?;
        if !meta.is_file() {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
        }
        return Ok(html::image_doc(url, meta.len() as usize));
    }
    let bytes = read_local(path)?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(if lower.ends_with(".html") || lower.ends_with(".htm") { html::parse_page(&text, url, main_only) } else { html::plain(&text, url) })
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
                    // (a local page's own picture — a page from the network
                    // never names one: see html.rs — read as far as one from
                    // the network is)
                    let p = url::Url::parse(&url).ok().and_then(|u| u.to_file_path().ok()).ok_or("chemin invalide")?;
                    read_local(&p).map_err(|e| e.to_string())?
                } else {
                    let resp = agent.get(&url).set("Accept", "image/png,image/jpeg,image/gif,image/webp,*/*;q=0.5").call().map_err(|e| e.to_string())?;
                    let mut buf = Vec::new();
                    resp.into_reader().take(MAX_BODY).read_to_end(&mut buf).map_err(|e| e.to_string())?;
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

/// One of Sanaga's processes, for the Activity page.
#[derive(Clone, Debug)]
pub struct Process {
    /// 0 the interface, 1 the network, 2 a page, 3 a helper.
    pub kind: u8,
    pub bytes: u64,
    /// Share of one processor core used over the last moments, in percent.
    pub cpu: f32,
    /// When it started (clock ticks since the computer did) and its number
    /// as the process itself sees it (inside the engine's sandbox, not the
    /// real one): what a page's process tells Sanaga about itself, to find
    /// which tab it serves (see page-helper/).
    pub start: u64,
    pub inner_pid: u32,
}

/// (start time, number as seen from inside its sandbox) of a process.
fn process_identity(pid: u32) -> (u64, u32) {
    let start = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| s.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(19).and_then(|v| v.parse::<u64>().ok())))
        .unwrap_or(0);
    // "NSpid: 15700 2": its number in each nested set of processes, innermost last
    let inner = std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|s| s.lines().find_map(|l| l.strip_prefix("NSpid:").and_then(|v| v.split_whitespace().last().and_then(|n| n.parse::<u32>().ok()))))
        .unwrap_or(pid);
    (start, inner)
}

/// Sanaga's processes with their memory and processor use, measured on a
/// background thread every 2 s while someone looks (the Activity page).
pub fn processes_cached() -> Vec<Process> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    static LIST: Mutex<Vec<Process>> = Mutex::new(Vec::new());
    static WANTED: AtomicBool = AtomicBool::new(false);
    static STARTED: std::sync::Once = std::sync::Once::new();
    WANTED.store(true, Ordering::Relaxed);
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new().name("sanaga-activity".into()).spawn(|| {
            // processor time used so far, per process (in clock ticks: 100 a second on Linux)
            let ticks = |pid: u32| -> u64 {
                std::fs::read_to_string(format!("/proc/{pid}/stat"))
                    .ok()
                    .and_then(|s| s.rsplit_once(')').map(|(_, r)| r.split_whitespace().skip(11).take(2).filter_map(|v| v.parse::<u64>().ok()).sum()))
                    .unwrap_or(0)
            };
            let mut before: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
            let mut at = std::time::Instant::now();
            loop {
                if WANTED.swap(false, Ordering::Relaxed) {
                    let me = std::process::id();
                    let secs = at.elapsed().as_secs_f32().max(0.1);
                    at = std::time::Instant::now();
                    let mut now: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
                    let mut list = Vec::new();
                    for pid in family_of(me) {
                        let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
                        let kind = match name.trim() {
                            _ if pid == me => 0,
                            n if n.starts_with("WebKitNetwork") => 1,
                            n if n.starts_with("WebKitWebProc") => 2,
                            _ => 3,
                        };
                        let t = ticks(pid);
                        let cpu = before.get(&pid).map(|b| t.saturating_sub(*b) as f32 / secs).unwrap_or(0.0);
                        now.insert(pid, t);
                        let (start, inner_pid) = if kind == 2 { process_identity(pid) } else { (0, pid) };
                        list.push(Process { kind, bytes: process_memory(pid), cpu, start, inner_pid });
                    }
                    before = now;
                    list.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.bytes.cmp(&a.bytes)));
                    if let Ok(mut l) = LIST.lock() {
                        *l = list;
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        });
    });
    LIST.lock().map(|l| l.clone()).unwrap_or_default()
}

/// A process's fair share of memory, in bytes ("Pss": shared libraries are
/// split between the processes using them).
fn process_memory(pid: u32) -> u64 {
    if let Ok(t) = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")) {
        if let Some(kb) = t.lines().find(|l| l.starts_with("Pss:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<u64>().ok()) {
            return kb * 1024;
        }
    }
    0
}

/// A process and the processes it started, and theirs (Sanaga and its engine).
fn family_of(root: u32) -> Vec<u32> {
    let mut parents: Vec<(u32, u32)> = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for e in dir.flatten() {
            let name = e.file_name();
            let Some(pid) = name.to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
            if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                if let Some(ppid) = stat.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(1)).and_then(|p| p.parse().ok()) {
                    parents.push((pid, ppid));
                }
            }
        }
    }
    let mut family = vec![root];
    let mut k = 0;
    while k < family.len() {
        let p = family[k];
        family.extend(parents.iter().filter(|(_, pp)| *pp == p).map(|(c, _)| *c));
        k += 1;
    }
    family
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
    fn versions_are_compared_as_numbers() {
        assert!(super::is_newer("0.5.0", "0.4.0"));
        assert!(super::is_newer("0.10.0", "0.9.2"));
        assert!(super::is_newer("1.0", "0.99.99"));
        assert!(super::is_newer("0.4.1", "0.4"));
        assert!(!super::is_newer("0.4.0", "0.4.0"));
        assert!(!super::is_newer("0.3.9", "0.4.0"));
        assert!(!super::is_newer("", "0.4.0"));
    }

    #[test]
    fn a_downloads_name_is_read_from_its_header() {
        let name = |header: &str| disposition_name(header).map(|n| safe_download_name(&n));
        // both forms, as servers send them: one name, not the rest of the header
        assert_eq!(name("attachment; filename=\"rapport.pdf\"; filename*=UTF-8''rapport.pdf").as_deref(), Some("rapport.pdf"));
        assert_eq!(disposition_name("attachment; filename=\"rapport.pdf\"; size=3").as_deref(), Some("rapport.pdf"));
        // the encoded form first, wherever it is, names in any case
        assert_eq!(name("attachment; filename=\"Recu n1.pdf\"; filename*=UTF-8''Re%C3%A7u%20n%C2%B01.pdf").as_deref(), Some("Reçu n°1.pdf"));
        assert_eq!(name("Attachment; FileName*=utf-8'fr'%C3%A9t%C3%A9.txt; FILENAME=ete.txt").as_deref(), Some("été.txt"));
        assert_eq!(name("attachment; filename*=ISO-8859-1''Re%E7u%20100%25.pdf").as_deref(), Some("Reçu 100%.pdf"));
        // a charset Sanaga does not read: the plain name
        assert_eq!(name("attachment; filename*=koi8-r''%D2.txt; filename=r.txt").as_deref(), Some("r.txt"));
        assert_eq!(name("attachment; filename*=UTF-8''; filename=r.txt").as_deref(), Some("r.txt"));
        // quotes: a `;` and an escaped quote inside, spaces around
        assert_eq!(disposition_name("attachment; filename = \"a;b \\\"c\\\".pdf\" ; x=y").as_deref(), Some("a;b \"c\".pdf"));
        assert_eq!(disposition_name("attachment;filename=plain.txt").as_deref(), Some("plain.txt"));
        assert_eq!(disposition_name("attachment; filename='quoted.txt'").as_deref(), Some("quoted.txt"));
        assert_eq!(disposition_name("filename=\"sans-type.txt\"").as_deref(), Some("sans-type.txt"));
        // no name
        for none in ["attachment", "inline", "", "attachment; filename=", "attachment; filename=\"\"", "attachment; filename*=UTF-8''", "attachment; name=x"] {
            assert_eq!(disposition_name(none), None, "{none}");
        }
        // whatever the name says, it stays a name in the Downloads folder
        assert_eq!(name("attachment; filename=\"..%2F..%2F.config%2Fautostart%2Fevil.desktop\"").as_deref(), Some("evil.desktop.download"));
        assert_eq!(name("attachment; filename*=UTF-8''..%2F..%2Fevil.desktop").as_deref(), Some("evil.desktop.download"));
        assert_eq!(name("attachment; filename=\"/etc/passwd\"").as_deref(), Some("passwd"));
        // odd headers never crash the reader
        crate::mutate::run(&[b"attachment; filename=\"a;b.pdf\"; filename*=UTF-8''a%20b.pdf".as_slice()], 3000, |data| {
            let _ = disposition_name(&String::from_utf8_lossy(data));
        });
    }

    #[test]
    fn local_files_are_read_as_far_as_the_networks() {
        let dir = std::env::temp_dir().join(format!("sanaga-net-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // what never ends, what never answers, what is not a file: refused
        assert!(read_local(Path::new("/dev/zero")).is_err());
        let zero = dir.join("zero.png");
        std::os::unix::fs::symlink("/dev/zero", &zero).unwrap();
        assert!(read_local(&zero).is_err());
        assert!(local_page(&zero, "file:///x/zero.png", true).is_err());
        assert!(local_page(Path::new("/dev/zero"), "file:///dev/zero", true).is_err());
        assert!(read_local(&dir).is_err());
        if std::process::Command::new("mkfifo").arg(dir.join("pipe.txt")).status().is_ok_and(|s| s.success()) {
            assert!(read_local(&dir.join("pipe.txt")).is_err());
        }
        // a file bigger than a page may be: its start
        let big = dir.join("big.txt");
        std::fs::File::create(&big).unwrap().set_len(MAX_BODY + 4096).unwrap();
        assert_eq!(read_local(&big).unwrap().len() as u64, MAX_BODY);
        // ordinary files, as before
        std::fs::write(dir.join("a.html"), "<title>Local</title><p>Bonjour<img src=photo.png>").unwrap();
        let doc = local_page(&dir.join("a.html"), "file:///x/a.html", false).unwrap();
        assert_eq!(doc.title, "Local");
        std::fs::write(dir.join("photo.png"), [0u8; 10]).unwrap();
        assert_eq!(local_page(&dir.join("photo.png"), "file:///x/photo.png", false).unwrap().bytes, 10);
        assert!(local_page(&dir.join("missing.txt"), "file:///x/missing.txt", false).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

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
