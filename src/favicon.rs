//! Site icons (favicons). Sanaga never fetches them itself: WebKit loads a
//! page's icon with the page, through that page's own network session (a
//! private tab's session keeps nothing, a container has its own cookies),
//! and hands it over. Icons of normal tabs are then kept on disk so they
//! show instantly next time (in the favourites, history, new tab); icons of
//! private tabs stay in memory only. A site never visited (such as the
//! default favourites at the first start) shows the coloured letter badge
//! until the person opens it.
//!
//! What it costs. A row that shows an icon asks for it at every frame: the
//! answer is remembered per address and per window, so a frame parses
//! nothing and takes no lock. Icons are decoded by two threads at most,
//! which stop when there is nothing left to decode. Memory holds the icons
//! used lately and lets the others go (they are on disk). Each window has
//! its own pictures: a picture handed to the graphics card belongs to the
//! window it was made for, and shown in another it was a wrong picture or
//! none.

use eframe::egui;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const SIZE: u32 = 64;
/// Threads decoding icons at the same time, at most.
const WORKERS: usize = 2;
/// Decoded icons kept in memory for all the windows (16 KiB each).
const MAX_IMAGES: usize = 256;
/// Icons a window keeps as pictures — more only while more are on screen.
const MAX_SHOWN: usize = 256;
/// Addresses whose site is remembered (see `Desk::sites`).
const MAX_ADDRESSES: usize = 2048;
/// Sites remembered as having no icon on disk.
const MAX_NONE: usize = 4096;
/// Arrivals remembered for the windows that were not drawn meanwhile.
const MAX_NEWS: usize = 256;

static DIR: OnceLock<PathBuf> = OnceLock::new();
/// Icons may be written to disk (not while taking a screenshot).
static KEEP: AtomicBool = AtomicBool::new(true);
/// How many icons arrived so far: a window compares it with what it has
/// seen, without taking the lock.
static ARRIVALS: AtomicU64 = AtomicU64::new(0);

/// The most icons kept on disk: one per site visited, the folder grew for
/// ever. Past it, the ones not used for the longest go (a site visited
/// again gets its icon again).
const MAX_ON_DISK: usize = 3000;

/// Where icons are kept, and whether new ones may be written there.
pub fn setup(dir: PathBuf, keep: bool) {
    let _ = std::fs::create_dir_all(&dir);
    if keep && DIR.get().is_none() {
        // (once, at the start, away from the window: a folder to read)
        let folder = dir.clone();
        let _ = std::thread::Builder::new().name("sanaga-icons-tidy".into()).spawn(move || tidy(&folder, MAX_ON_DISK));
    }
    let _ = DIR.set(dir);
    KEEP.store(keep, Ordering::Relaxed);
}

/// Keep the `most` icons used last in `dir`; how many were removed.
fn tidy(dir: &std::path::Path, most: usize) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    // (icons only: nothing else in the folder is touched)
    let mut icons: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            m.is_file().then(|| (m.accessed().or_else(|_| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path()))
        })
        .collect();
    if icons.len() <= most {
        return 0;
    }
    icons.sort();
    let extra = icons.len() - most;
    icons.into_iter().take(extra).filter(|(_, p)| std::fs::remove_file(p).is_ok()).count()
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

fn cache_file(key: &str) -> Option<PathBuf> {
    let name: String = key.replace("://", "_").chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' }).collect();
    DIR.get().map(|d| d.join(format!("{name}.png")))
}

// ------------------------------------------------- shared by the windows

/// An icon to read or to make, for the decoding threads.
enum Work {
    /// The icon kept on disk for this site, if there is one.
    Read,
    /// The icon WebKit just handed over (PNG bytes; `private`: not kept on disk).
    Learn { png: Vec<u8>, private: bool },
}

struct Job {
    site: Arc<str>,
    work: Work,
    /// The windows to draw again when it is done.
    wake: Vec<egui::Context>,
}

/// What the windows and the decoding threads share.
#[derive(Default)]
struct Shared {
    /// Decoded icons, with when each was last asked for (`clock`).
    images: HashMap<Arc<str>, (Arc<egui::ColorImage>, u64)>,
    clock: u64,
    /// Sites with no icon on disk: not looked for again until one arrives.
    none: HashSet<Arc<str>>,
    jobs: VecDeque<Job>,
    /// Sites being read from disk (asked once, whoever asks).
    reading: HashSet<Arc<str>>,
    /// Decoding threads running.
    workers: usize,
    /// The last icons that arrived: (their number, their site).
    news: VecDeque<(u64, Arc<str>)>,
}

impl Shared {
    /// A decoded icon is kept; beyond `MAX_IMAGES`, the one asked for the
    /// longest ago goes (it is read from disk again if a row shows it).
    fn keep(&mut self, site: Arc<str>, image: Arc<egui::ColorImage>) {
        self.clock += 1;
        self.images.insert(site.clone(), (image, self.clock));
        self.none.remove(&site);
        while self.images.len() > MAX_IMAGES {
            let Some(oldest) = self.images.iter().min_by_key(|(_, (_, used))| *used).map(|(k, _)| k.clone()) else { break };
            self.images.remove(&oldest);
        }
        let n = ARRIVALS.load(Ordering::Relaxed) + 1;
        self.news.push_back((n, site));
        if self.news.len() > MAX_NEWS {
            self.news.pop_front();
        }
        ARRIVALS.store(n, Ordering::Relaxed);
    }

    /// The decoded icon of a site, marked as just used.
    fn image(&mut self, site: &str) -> Option<Arc<egui::ColorImage>> {
        self.clock += 1;
        let clock = self.clock;
        self.images.get_mut(site).map(|(image, used)| {
            *used = clock;
            image.clone()
        })
    }

    fn no_icon(&mut self, site: Arc<str>) {
        // (WebKit handed one over while the disk was being asked)
        if self.images.contains_key(&site) {
            return;
        }
        if self.none.len() >= MAX_NONE {
            self.none.clear();
        }
        self.none.insert(site);
    }
}

fn shared() -> &'static Mutex<Shared> {
    static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();
    SHARED.get_or_init(Default::default)
}

/// Hand jobs to the decoding threads, starting one if fewer than `WORKERS`
/// are at it. A thread ends when the queue is empty: none waits for nothing.
fn hand_over(sh: &mut Shared, jobs: impl IntoIterator<Item = Job>) {
    sh.jobs.extend(jobs);
    while sh.workers < WORKERS.min(sh.jobs.len()) {
        sh.workers += 1;
        if std::thread::Builder::new().name("sanaga-favicon".into()).spawn(decode_until_empty).is_err() {
            sh.workers -= 1;
            break;
        }
    }
}

fn decode_until_empty() {
    loop {
        let job = {
            let Ok(mut sh) = shared().lock() else { return };
            match sh.jobs.pop_front() {
                Some(job) => job,
                None => {
                    sh.workers -= 1;
                    return;
                }
            }
        };
        let image = match &job.work {
            Work::Read => cache_file(&job.site).and_then(|f| std::fs::read(f).ok()).and_then(|b| to_image(&b)),
            Work::Learn { png, private } => learned(&job.site, png, *private),
        };
        if let Ok(mut sh) = shared().lock() {
            match (image, &job.work) {
                (Some(image), _) => sh.keep(job.site.clone(), Arc::new(image)),
                (None, Work::Read) => sh.no_icon(job.site.clone()),
                // (an icon already shown stays when a later one can't be read)
                (None, Work::Learn { .. }) => {}
            }
            if matches!(job.work, Work::Read) {
                sh.reading.remove(&job.site);
            }
        }
        for ctx in &job.wake {
            ctx.request_repaint();
        }
    }
}

/// The icon WebKit handed over, made small; kept on disk unless private.
fn learned(site: &str, png: &[u8], private: bool) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(png).ok()?;
    if img.width() < 8 {
        return None;
    }
    let small = img.resize(SIZE, SIZE, image::imageops::FilterType::Lanczos3);
    if !private && KEEP.load(Ordering::Relaxed) {
        if let Some(f) = cache_file(site) {
            let _ = small.save_with_format(f, image::ImageFormat::Png);
        }
    }
    let rgba = small.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

fn to_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?;
    if img.width() < 8 {
        return None;
    }
    let img = img.resize(SIZE, SIZE, image::imageops::FilterType::Lanczos3).to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}

// ---------------------------------------------------------- per window

/// A site in one window: its picture there (None: no icon known, the
/// letter badge shows), and the last frame a row asked for it.
struct Shown {
    picture: Option<egui::TextureHandle>,
    used: u64,
}

/// The icons of one window. Its pictures were made for this window's
/// drawing: they mean nothing in another.
#[derive(Default)]
struct Window {
    icons: HashMap<Arc<str>, Shown>,
    /// Frames drawn.
    frame: u64,
    /// Sites met for the first time at this frame: looked up at the next `pump`.
    wanted: Vec<Arc<str>>,
    /// The number of the last arrival this window has taken in.
    seen: u64,
    /// No use looking for icons to let go before this frame.
    tidy_at: u64,
}

impl Window {
    /// Beyond `max` icons, the ones asked for the longest ago go — never
    /// one that a row asked for at this frame or the one before: what is
    /// on screen stays, however many that is.
    fn tidy(&mut self, max: usize) {
        if self.icons.len() <= max || self.frame < self.tidy_at {
            return;
        }
        let mut idle: Vec<(u64, Arc<str>)> = self.icons.iter().filter(|(_, s)| s.used + 1 < self.frame).map(|(k, s)| (s.used, k.clone())).collect();
        idle.sort();
        for (_, site) in idle.into_iter().take(self.icons.len() - max) {
            self.icons.remove(&site);
        }
        // (all the rest is on screen: look again in a while, not at every frame)
        self.tidy_at = self.frame + 120;
    }
}

/// A window's icons while another window is being drawn: kept with that
/// window's own data, and so let go with it when it closes.
type Home = Arc<Mutex<Option<Window>>>;

/// The side of the window's thread: the window being drawn, and what was
/// worked out for the addresses shown.
#[derive(Default)]
struct Desk {
    /// The window being drawn: its icons, and where they are kept otherwise.
    now: Option<(Home, Window)>,
    /// Address → site ("https://www.site.com"; None: an address with no
    /// site, such as a file). A row gives its page's address at every
    /// frame: parsed once, then found here.
    sites: HashMap<Box<str>, Option<Arc<str>>>,
    /// Icons WebKit handed over since the last frame.
    learned: Vec<Job>,
}

thread_local! {
    static DESK: RefCell<Desk> = RefCell::new(Desk::default());
}

/// The icon of the site of `url` in the window being drawn, if known;
/// looked for on disk otherwise (it shows at one of the next frames).
pub fn get(url: &str) -> Option<egui::TextureHandle> {
    DESK.with(|desk| {
        let Desk { now, sites, .. } = &mut *desk.borrow_mut();
        let (_, window) = now.as_mut()?;
        if !sites.contains_key(url) {
            if sites.len() >= MAX_ADDRESSES {
                sites.clear();
            }
            sites.insert(url.into(), key_of(url).map(Arc::from));
        }
        let site = sites.get(url)?.as_ref()?;
        match window.icons.get_mut(site) {
            Some(shown) => {
                shown.used = window.frame;
                shown.picture.clone()
            }
            None => {
                window.icons.insert(site.clone(), Shown { picture: None, used: window.frame });
                window.wanted.push(site.clone());
                None
            }
        }
    })
}

/// Called at the start of every frame of a window, with that window's
/// context: the icons asked for until the next call are this window's.
/// Icons that arrived become pictures; those met for the first time are
/// looked for (in memory, else on disk — never on the network).
pub fn pump(ctx: &egui::Context) {
    DESK.with(|desk| {
        let Desk { now, learned, .. } = &mut *desk.borrow_mut();
        // (this window's icons are kept in its own data)
        let home: Home = ctx.data_mut(|d| d.get_temp_mut_or_insert_with(egui::Id::new("sanaga-favicons"), || Arc::new(Mutex::new(Some(Window::default())))).clone());
        if now.as_ref().is_none_or(|(at, _)| !Arc::ptr_eq(at, &home)) {
            // another window than at the last frame: its icons come out,
            // the other's go back to it
            if let Some((at, window)) = now.take() {
                if let Ok(mut slot) = at.lock() {
                    *slot = Some(window);
                }
            }
            let window = home.lock().ok().and_then(|mut slot| slot.take()).unwrap_or_default();
            *now = Some((home, window));
        }
        let Some((_, window)) = now.as_mut() else { return };
        window.frame += 1;
        let arrivals = ARRIVALS.load(Ordering::Relaxed);
        // (the usual frame: nothing new, nothing asked — and no lock taken)
        if window.wanted.is_empty() && learned.is_empty() && window.seen == arrivals {
            window.tidy(MAX_SHOWN);
            return;
        }
        let Ok(mut sh) = shared().lock() else { return };
        let picture = |site: &str, image: Arc<egui::ColorImage>| ctx.load_texture(format!("favicon {site}"), image, egui::TextureOptions::LINEAR);
        // icons that arrived since this window last looked
        if window.seen != arrivals {
            let first_kept = sh.news.front().map(|(n, _)| *n).unwrap_or(arrivals + 1);
            if first_kept > window.seen + 1 {
                // (it missed more arrivals than are remembered: it starts
                // again, from what is in memory)
                window.icons.clear();
            } else {
                let arrived: Vec<Arc<str>> = sh.news.iter().filter(|(n, _)| *n > window.seen).map(|(_, site)| site.clone()).collect();
                for site in arrived {
                    if window.icons.contains_key(&site) {
                        if let Some(image) = sh.image(&site) {
                            let used = window.icons.get(&site).map(|s| s.used).unwrap_or(window.frame);
                            window.icons.insert(site.clone(), Shown { picture: Some(picture(&site, image)), used });
                        }
                    }
                }
            }
            window.seen = arrivals;
        }
        // icons asked for the first time: in memory, known to be missing, or read from disk
        let mut jobs: Vec<Job> = std::mem::take(learned);
        for job in &mut jobs {
            job.wake.push(ctx.clone());
        }
        for site in std::mem::take(&mut window.wanted) {
            if let Some(image) = sh.image(&site) {
                if let Some(shown) = window.icons.get_mut(&site) {
                    shown.picture = Some(picture(&site, image));
                }
            } else if !sh.none.contains(&site) {
                if sh.reading.insert(site.clone()) {
                    jobs.push(Job { site, work: Work::Read, wake: vec![ctx.clone()] });
                } else if let Some(job) = sh.jobs.iter_mut().find(|j| j.site == site && matches!(j.work, Work::Read)) {
                    // (another window asked first: this one is told too)
                    job.wake.push(ctx.clone());
                }
            }
        }
        hand_over(&mut sh, jobs);
        drop(sh);
        window.tidy(MAX_SHOWN);
    });
}

/// WebKit has the icon of the page at `page_url` (PNG bytes): shown at
/// one of the next frames, and kept on disk unless it comes from a private
/// tab. (Called in the window's thread.)
pub fn learn(page_url: &str, png: Vec<u8>, private: bool) {
    let Some(site) = key_of(page_url) else { return };
    // (handed to the decoding threads at the next frame, with the window
    // to draw again once the icon is ready)
    DESK.with(|desk| desk.borrow_mut().learned.push(Job { site: site.into(), work: Work::Learn { png, private }, wake: Vec::new() }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icons_folder_keeps_the_ones_used_last() {
        let dir = std::env::temp_dir().join(format!("sanaga-icons-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for k in 0..8u64 {
            let p = dir.join(format!("site{k}.png"));
            std::fs::write(&p, b"x").unwrap();
            let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + k * 60);
            std::fs::File::options().write(true).open(&p).unwrap().set_times(std::fs::FileTimes::new().set_accessed(when).set_modified(when)).unwrap();
        }
        std::fs::write(dir.join("notes.txt"), b"not an icon").unwrap();
        assert_eq!(tidy(&dir, 5), 3);
        let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        left.sort();
        assert_eq!(left, ["notes.txt", "site3.png", "site4.png", "site5.png", "site6.png", "site7.png"]);
        assert_eq!(tidy(&dir, 5), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn icons_are_kept_per_site() {
        assert_eq!(origin("https://www.a.com:8080/x"), Some(("https".into(), "www.a.com:8080".into())));
        assert_eq!(key_of("https://www.a.com/page?q=1"), Some("https://www.a.com".to_string()));
        assert_eq!(key_of("sanaga://newtab"), None);
        assert_eq!(key_of("file:///tmp/a.html"), None);
    }

    fn site(n: usize) -> Arc<str> {
        format!("https://site{n}.example").into()
    }

    #[test]
    fn memory_keeps_the_icons_used_lately() {
        let mut sh = Shared::default();
        let image = Arc::new(egui::ColorImage::filled([2, 2], egui::Color32::WHITE));
        for n in 0..MAX_IMAGES {
            sh.keep(site(n), image.clone());
        }
        assert_eq!(sh.images.len(), MAX_IMAGES);
        // the first one is asked for again: it is no longer the oldest
        assert!(sh.image(&site(0)).is_some());
        for n in MAX_IMAGES..MAX_IMAGES + 10 {
            sh.keep(site(n), image.clone());
        }
        assert_eq!(sh.images.len(), MAX_IMAGES, "never more than the limit");
        assert!(sh.image(&site(0)).is_some(), "used lately: kept");
        assert!(sh.image(&site(1)).is_none(), "the oldest went");
        assert!(sh.image(&site(MAX_IMAGES + 9)).is_some());
        assert!(sh.news.len() <= MAX_NEWS);
        // a site said to have no icon gets one: no longer "none"
        sh.no_icon(site(5000));
        sh.keep(site(5000), image);
        assert!(!sh.none.contains(&site(5000)));
    }

    #[test]
    fn a_window_lets_go_of_icons_no_longer_shown_only() {
        let mut w = Window { frame: 10, ..Default::default() };
        // 300 icons asked for at frame 10 (all on screen), 100 older ones
        for n in 0..300 {
            w.icons.insert(site(n), Shown { picture: None, used: 10 });
        }
        for n in 300..400 {
            w.icons.insert(site(n), Shown { picture: None, used: (n - 300) as u64 % 5 });
        }
        w.tidy(256);
        assert_eq!(w.icons.len(), 300, "what is on screen stays, even beyond the limit");
        assert!(w.icons.keys().all(|k| w.icons[k].used == 10));
        // nothing more to let go: not looked at again at once
        assert!(w.tidy_at > w.frame);
        // later, fewer on screen: back under the limit, the oldest first
        let mut w = Window { frame: 50, ..Default::default() };
        for n in 0..300 {
            w.icons.insert(site(n), Shown { picture: None, used: n as u64 % 40 });
        }
        w.icons.insert(site(1000), Shown { picture: None, used: 50 });
        w.tidy(256);
        assert_eq!(w.icons.len(), 256);
        assert!(w.icons.contains_key(&site(1000)));
        assert!(!w.icons.contains_key(&site(0)), "asked for at frame 0: gone");
        assert!(w.icons.contains_key(&site(39)), "asked for at frame 39: kept");
    }
}
