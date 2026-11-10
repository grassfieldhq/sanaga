//! One web page shown by WebKit (WebKitGTK 6, GTK 4), as a normal part of
//! Sanaga's window: GTK composes the page and the rest of the window together
//! on the graphics card, in the same frame.

use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};
use webkit6::prelude::*;
use webkit6::{CookiePersistentStorage, LoadEvent, NetworkSession, UserContentInjectedFrames, UserContentManager, UserScript, UserScriptInjectionTime, WebContext, WebView};

pub type R<T> = Result<T, ()>;

/// What a page tells Sanaga.
pub enum PageSignal {
    Started(String),
    Finished(String),
    Title(String),
    NewWindow(String),
    Message(String),
    /// The page closed itself (`window.close()`, e.g. a sign-in window when done).
    Close,
    /// The page's engine process stopped (a crash, or too much memory: true).
    Crashed(bool),
    /// A page tool asked from the page's right-click menu: what, and the
    /// address it is about (a link…).
    Tool(&'static str, String),
    /// WebKit loaded the page's icon (the page's address, the icon as PNG).
    Favicon(String, Vec<u8>),
    /// A link opened elsewhere, as browsers do: Ctrl+click or middle click
    /// (0, a tab behind), Ctrl+Shift+click (1, a tab in front), Shift+click
    /// (2, a new window).
    OpenLink(String, u8),
    /// The page could not be loaded: its address, what went wrong (see
    /// `LoadError`) and the engine's own words.
    LoadFailed(String, LoadError, String),
    /// The page asks something (alert, confirm, prompt, leave the page, a
    /// login): Sanaga shows it in its own design (see `dialog`).
    Dialog(u64),
    /// The address shown changed (also without a load: pushState, #anchor).
    Uri(String),
    /// A link to a site whose videos need a DRM WebKit doesn't have
    /// (Netflix…): not loaded here; Sanaga opens it where it plays.
    Protected(String),
}

/// Why a page did not load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// The site's name does not exist (DNS).
    NotFound,
    /// No internet connection.
    Offline,
    /// The server refused the connection.
    Refused,
    /// The server or the network did not answer in time.
    Timeout,
    /// The site's certificate is not valid (the connection is not private).
    Certificate,
    /// A local file that is not there.
    NoFile,
    /// A port browsers never open (it belongs to other services: mail, FTP…).
    BlockedPort,
    Other,
}

/// What a page asks, for Sanaga's own dialog.
#[derive(Clone, Debug, PartialEq)]
pub enum DialogKind {
    Alert,
    Confirm,
    Prompt(String),
    /// "Leave this page?" (unsaved changes)
    Leave,
    /// A login asked by the site (HTTP authentication): the realm.
    Login(String),
}

#[derive(Clone, Debug)]
pub struct DialogInfo {
    pub kind: DialogKind,
    pub message: String,
    /// Where it comes from (the page's site).
    pub host: String,
}

enum Pending {
    Script(webkit6::ScriptDialog),
    Auth(webkit6::AuthenticationRequest),
}

thread_local! {
    static DIALOGS: RefCell<Vec<(u64, Pending, DialogInfo)>> = const { RefCell::new(Vec::new()) };
    static NEXT_DIALOG: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
    /// Certificates the person accepted to go on with, per host (this run only).
    static TLS_SEEN: RefCell<Vec<(String, gtk::gio::TlsCertificate)>> = const { RefCell::new(Vec::new()) };
}

fn keep_dialog(p: Pending, info: DialogInfo) -> u64 {
    let key = NEXT_DIALOG.with(|n| {
        let k = n.get();
        n.set(k + 1);
        k
    });
    DIALOGS.with(|d| d.borrow_mut().push((key, p, info)));
    key
}

/// What dialog `key` asks (None once answered).
pub fn dialog(key: u64) -> Option<DialogInfo> {
    DIALOGS.with(|d| d.borrow().iter().find(|x| x.0 == key).map(|x| x.2.clone()))
}

/// Answer dialog `key`: OK / Cancel, the text typed (prompt), or the user
/// name and password (login).
pub fn answer_dialog(key: u64, ok: bool, text: &str, password: &str) {
    let Some((_, p, _)) = DIALOGS.with(|d| {
        let mut d = d.borrow_mut();
        d.iter().position(|x| x.0 == key).map(|i| d.remove(i))
    }) else {
        return;
    };
    match p {
        Pending::Script(s) => {
            match s.dialog_type() {
                webkit6::ScriptDialogType::Confirm | webkit6::ScriptDialogType::BeforeUnloadConfirm => s.confirm_set_confirmed(ok),
                webkit6::ScriptDialogType::Prompt => {
                    if ok {
                        s.prompt_set_text(text);
                    } else {
                        s.confirm_set_confirmed(false);
                    }
                }
                _ => {}
            }
            s.close();
        }
        Pending::Auth(r) => {
            if ok {
                r.authenticate(Some(&webkit6::Credential::new(text, password, webkit6::CredentialPersistence::ForSession)));
            } else {
                r.cancel();
            }
        }
    }
}

/// Go on to `host` despite its certificate (the person said so), and reload.
pub fn accept_certificate(view: &WebView, host: &str) {
    let cert = TLS_SEEN.with(|t| t.borrow().iter().find(|(h, _)| h == host).map(|(_, c)| c.clone()));
    if let (Some(c), Some(s)) = (cert, view.network_session()) {
        s.allow_tls_certificate_for_host(&c, host);
        view.reload();
    }
}

/// What kind of failure an engine error is.
fn classify(e: &gtk::glib::Error) -> Option<LoadError> {
    use gtk::gio;
    // a load replaced by another, a download, a stop: not an error to show
    if e.matches(webkit6::NetworkError::Cancelled) || e.matches(webkit6::PolicyError::FrameLoadInterruptedByPolicyChange) || e.matches(webkit6::PolicyError::CannotShowMimeType) {
        return None;
    }
    if e.matches(webkit6::NetworkError::FileDoesNotExist) {
        return Some(LoadError::NoFile);
    }
    if e.matches(webkit6::PolicyError::CannotUseRestrictedPort) {
        return Some(LoadError::BlockedPort);
    }
    // "Offline" only when the error itself says the network can't be
    // reached (or a name can't be resolved with no network at all): the
    // system's network monitor alone is sometimes wrong while the connection
    // works, and every failure then looked like "no Internet"
    let offline = !gio::NetworkMonitor::default().is_network_available();
    let msg = e.message().to_lowercase();
    if e.matches(gio::ResolverError::NotFound) || e.matches(gio::ResolverError::TemporaryFailure) || msg.contains("resolv") || msg.contains("name or service not known") {
        return Some(if offline { LoadError::Offline } else { LoadError::NotFound });
    }
    if e.matches(gio::IOErrorEnum::ConnectionRefused) || msg.contains("refused") {
        return Some(LoadError::Refused);
    }
    if e.matches(gio::IOErrorEnum::TimedOut) || msg.contains("timed out") || msg.contains("timeout") {
        return Some(LoadError::Timeout);
    }
    if e.matches(gio::IOErrorEnum::NetworkUnreachable) || e.matches(gio::IOErrorEnum::HostUnreachable) {
        return Some(LoadError::Offline);
    }
    if msg.contains("certificate") || msg.contains("tls") {
        return Some(LoadError::Certificate);
    }
    Some(LoadError::Other)
}

/// Makes the page a site opens with `window.open` (sign-in, payment windows):
/// it gets the opener's page as its related view, so the two can talk to each
/// other as on any browser. Returns the new page.
pub type PopupMaker = Rc<dyn Fn(&WebView) -> Option<WebView>>;

/// The web engine of one profile: its processes (context) and its network
/// session (cookies, cache, site data, downloads).
#[derive(Clone, PartialEq)]
pub struct Engine {
    pub ctx: WebContext,
    pub session: NetworkSession,
}

thread_local! {
    /// One web engine per profile folder, shared by all of Sanaga's windows
    /// (two engines must never write the same cookies and site data).
    static ENGINES: RefCell<Vec<(PathBuf, Engine)>> = const { RefCell::new(Vec::new()) };
    /// The session of private tabs: nothing kept on disk.
    static PRIVATE: RefCell<Option<NetworkSession>> = const { RefCell::new(None) };
    /// Sessions whose downloads are already followed.
    static WATCHED: RefCell<Vec<NetworkSession>> = const { RefCell::new(Vec::new()) };
    /// Where finished downloads are reported: one receiver per window.
    static DOWNLOAD_SINKS: RefCell<Vec<(usize, DownloadSink)>> = const { RefCell::new(Vec::new()) };
    /// The ad and tracker blocker, and the rule that stops web fonts, once
    /// WebKit has compiled them.
    static BLOCKER: RefCell<Option<webkit6::UserContentFilter>> = const { RefCell::new(None) };
    static NO_FONTS: RefCell<Option<webkit6::UserContentFilter>> = const { RefCell::new(None) };
    /// The open pages (to give them the blocker once it is ready).
    static VIEWS: RefCell<Vec<gtk::glib::WeakRef<WebView>>> = const { RefCell::new(Vec::new()) };
    /// Download names picked whose files are not created yet.
    static RESERVED: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    /// What Sanaga follows for each open page.
    static VIEW_STATES: RefCell<Vec<(gtk::glib::WeakRef<WebView>, Rc<ViewState>)>> = const { RefCell::new(Vec::new()) };
    /// Downloads waiting for the person's answer, sites allowed to download
    /// (until Sanaga quits), and the windows that ask.
    static PENDING_DOWNLOADS: RefCell<Vec<PendingDownload>> = const { RefCell::new(Vec::new()) };
    /// (kept per network session: a profile, a container, the private
    /// tabs; in memory only)
    static DOWNLOAD_SITES: RefCell<Vec<(Option<NetworkSession>, String)>> = const { RefCell::new(Vec::new()) };
    static DOWNLOAD_ASK_SINKS: RefCell<Vec<(usize, AskSink)>> = const { RefCell::new(Vec::new()) };
}

/// What Sanaga follows for one open page.
#[derive(Default)]
struct ViewState {
    /// The address of the page shown (committed): the referrer of its downloads.
    committed: RefCell<String>,
    /// When the person last acted in the page itself (a click or key that
    /// navigates: WebKit's user gesture), and whether a download has already
    /// used that action. Forgotten when a new page is shown.
    gesture: Cell<Option<Instant>>,
    gesture_used: Cell<bool>,
    /// When the person asked Sanaga to open an address in this view (typed,
    /// a favourite…; never a restored tab): only a download that this very
    /// navigation turns into may use it (it ends when a page is shown).
    typed: Cell<Option<Instant>>,
    /// When a key was last pressed in the page (seen by GTK).
    key_at: Cell<Option<Instant>>,
}

/// The person acted in this page (a click or key that navigates): it may
/// start one download.
fn mark_gesture(v: &WebView) {
    if let Some(s) = view_state(v) {
        s.gesture.set(Some(Instant::now()));
        s.gesture_used.set(false);
    }
}

/// How long after the person's action a page may start its download.
const GESTURE_WINDOW: Duration = Duration::from_secs(10);

// (pure rules: see guard.rs, testable without GTK)
pub use crate::guard::{download_needs_ask, download_site, is_shortcut_message, message_body};

/// A download waiting for the person's answer.
struct PendingDownload {
    download: webkit6::Download,
    /// The network session it belongs to (its allowance is kept there).
    session: Option<NetworkSession>,
    suggested: String,
    uri: String,
    site: String,
    dest: Rc<RefCell<Option<PathBuf>>>,
    denied: Rc<Cell<bool>>,
}

type AskSink = Rc<dyn Fn(String)>;

/// Window `slot` shows the "Allow downloads on …?" bar.
pub fn add_download_ask_sink(slot: usize, ask: impl Fn(String) + 'static) {
    DOWNLOAD_ASK_SINKS.with(|s| s.borrow_mut().push((slot, Rc::new(ask))));
}

fn ask_downloads(site: String) {
    let active = ACTIVE_WINDOW.load(std::sync::atomic::Ordering::Relaxed);
    let sink = DOWNLOAD_ASK_SINKS.with(|s| {
        let s = s.borrow();
        s.iter().find(|(n, _)| *n == active).or_else(|| s.first()).map(|(_, f)| f.clone())
    });
    match sink {
        Some(f) => f(site),
        // no window to ask in: the download does not happen
        None => answer_downloads(&site, false),
    }
}

/// The person's answer for a site's waiting downloads: they go to the
/// Downloads folder (and the site may download until Sanaga quits, in that
/// profile, container or private session only), or they are cancelled.
pub fn answer_downloads(site: &str, allow: bool) {
    let mine: Vec<PendingDownload> = PENDING_DOWNLOADS.with(|q| {
        let mut q = q.borrow_mut();
        let (mine, rest): (Vec<PendingDownload>, Vec<PendingDownload>) = q.drain(..).partition(|p| p.site == site);
        *q = rest;
        mine
    });
    for p in mine {
        if allow {
            DOWNLOAD_SITES.with(|d| {
                let mut d = d.borrow_mut();
                if !d.iter().any(|(s, x)| *s == p.session && *x == p.site) {
                    d.push((p.session.clone(), p.site.clone()));
                }
            });
            let path = choose_destination(&p.download, &p.suggested, &p.uri);
            *p.dest.borrow_mut() = Some(path);
        } else {
            p.denied.set(true);
            p.download.cancel();
        }
    }
}

fn view_state(v: &WebView) -> Option<Rc<ViewState>> {
    VIEW_STATES.with(|s| {
        let mut s = s.borrow_mut();
        s.retain(|(w, _)| w.upgrade().is_some());
        s.iter().find(|(w, _)| w.upgrade().as_ref() == Some(v)).map(|(_, st)| st.clone())
    })
}

/// The page a download comes from: the one its view shows (committed).
fn referrer_of(view: Option<&WebView>) -> String {
    let Some(v) = view else { return String::new() };
    let committed = view_state(v).map(|s| s.committed.borrow().clone()).unwrap_or_default();
    if committed.is_empty() {
        v.uri().map(|u| u.to_string()).unwrap_or_default()
    } else {
        committed
    }
}

/// The blocker is ready: the pages already open get it (from their next request).
fn blocker_ready() {
    let views: Vec<WebView> = VIEWS.with(|v| {
        let mut v = v.borrow_mut();
        v.retain(|w| w.upgrade().is_some());
        v.iter().filter_map(|w| w.upgrade()).collect()
    });
    for view in views {
        let host = host_of_uri(&view.uri().map(|u| u.to_string()).unwrap_or_default());
        apply_site(&view, &host);
    }
}

/// The network blocker is switched on (Settings › Privacy).
pub static BLOCKER_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Compile the blocker's rules (once; WebKit keeps them compiled in `dir`).
pub fn prepare_filters(dir: &Path) {
    if BLOCKER.with(|b| b.borrow().is_some()) {
        return;
    }
    let _ = std::fs::create_dir_all(dir);
    let store = webkit6::UserContentFilterStore::new(&dir.to_string_lossy());
    let rules = gtk::glib::Bytes::from_owned(crate::site_prefs::blocker_rules().into_bytes());
    store.save("sanaga-blocker", &rules, None::<&gtk::gio::Cancellable>, |res| match res {
        Ok(f) => {
            BLOCKER.with(|b| *b.borrow_mut() = Some(f));
            blocker_ready();
        }
        Err(e) => eprintln!("bloqueur : {e}"),
    });
    let fonts = gtk::glib::Bytes::from_static(crate::site_prefs::no_fonts_rules().as_bytes());
    store.save("sanaga-nofonts", &fonts, None::<&gtk::gio::Cancellable>, |res| {
        if let Ok(f) = res {
            NO_FONTS.with(|b| *b.borrow_mut() = Some(f));
        }
    });
}

/// The website settings of `host` go into the page about to load: its
/// JavaScript, the blocker, its fonts, and its look.
fn apply_site(v: &WebView, host: &str) {
    let prefs = crate::site_prefs::for_host(host);
    if let Some(st) = WebViewExt::settings(v) {
        // (Sanaga's own scripts keep working: only the page's are stopped)
        if st.enables_javascript_markup() != prefs.js {
            st.set_enable_javascript_markup(prefs.js);
        }
        // video formats the processor would decode at a high cost (see codecs.rs)
        let hw = crate::codecs::hardware_only_types(is_video_site(host));
        if st.media_content_types_requiring_hardware_support().map(|g| g.to_string()) != hw {
            st.set_media_content_types_requiring_hardware_support(hw.as_deref());
        }
    }
    let Some(ucm) = v.user_content_manager() else { return };
    let block = prefs.blocker && BLOCKER_ON.load(std::sync::atomic::Ordering::Relaxed);
    BLOCKER.with(|b| match (block, b.borrow().as_ref()) {
        (true, Some(f)) => ucm.add_filter(f),
        _ => ucm.remove_filter_by_id("sanaga-blocker"),
    });
    NO_FONTS.with(|b| match (!prefs.fonts, b.borrow().as_ref()) {
        (true, Some(f)) => ucm.add_filter(f),
        _ => ucm.remove_filter_by_id("sanaga-nofonts"),
    });
    ucm.remove_all_style_sheets();
    let css = prefs.css();
    if !css.is_empty() {
        ucm.add_style_sheet(&webkit6::UserStyleSheet::new(&css, UserContentInjectedFrames::AllFrames, webkit6::UserStyleLevel::User, &[], &[]));
    }
}

/// Sites that offer every video in H.264 as well (see codecs.rs).
fn is_video_site(host: &str) -> bool {
    let h = host.trim_start_matches("www.").trim_start_matches("m.").trim_start_matches("music.");
    h == "youtube.com" || h == "youtube-nocookie.com"
}

/// Set the identity (user agent) chosen for `host`, before asking for a page
/// there. True when it changed. A simulated device's identity is left alone.
fn set_identity(v: &WebView, host: &str, default_ua: &str) -> bool {
    let Some(st) = WebViewExt::settings(v) else { return false };
    let current = st.user_agent().map(|u| u.to_string()).unwrap_or_default();
    let ours = current == default_ua || crate::identity::Identity::ALL.iter().any(|i| i.user_agent() == Some(current.as_str()));
    let want = crate::identity::for_host(host).user_agent().unwrap_or(default_ua).to_string();
    if !ours || want == current {
        return false;
    }
    st.set_user_agent(Some(&want));
    true
}

fn host_of_uri(uri: &str) -> String {
    url::Url::parse(uri).ok().and_then(|u| u.host_str().map(|h| h.to_string())).unwrap_or_default()
}

type DownloadSink = Rc<dyn Fn(String, Option<PathBuf>, bool)>;

/// The window used last (its number): downloads are shown there.
pub static ACTIVE_WINDOW: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Window `slot` wants to hear about finished downloads.
pub fn add_download_sink(slot: usize, done: impl Fn(String, Option<PathBuf>, bool) + 'static) {
    DOWNLOAD_SINKS.with(|s| s.borrow_mut().push((slot, Rc::new(done))));
}

/// Window `slot` closed.
pub fn remove_download_sink(slot: usize) {
    DOWNLOAD_SINKS.with(|s| s.borrow_mut().retain(|(n, _)| *n != slot));
    DOWNLOAD_ASK_SINKS.with(|s| s.borrow_mut().retain(|(n, _)| *n != slot));
}

fn report_download(url: String, path: Option<PathBuf>, ok: bool) {
    let active = ACTIVE_WINDOW.load(std::sync::atomic::Ordering::Relaxed);
    let sink = DOWNLOAD_SINKS.with(|s| {
        let s = s.borrow();
        s.iter().find(|(n, _)| *n == active).or_else(|| s.first()).map(|(_, f)| f.clone())
    });
    if let Some(f) = sink {
        f(url, path, ok);
    }
}

/// Follow the downloads of this engine (once, whatever the number of windows),
/// and of private tabs.
pub fn follow_downloads(engine: &Engine) {
    for session in [engine.session.clone(), private_session()] {
        let known = WATCHED.with(|w| w.borrow().contains(&session));
        if !known {
            WATCHED.with(|w| w.borrow_mut().push(session.clone()));
            watch_downloads(&session, report_download);
        }
    }
}

/// A download name picked but whose file WebKit has not created yet: no
/// other download may take it meanwhile.
fn is_reserved(p: &Path) -> bool {
    RESERVED.with(|r| r.borrow().iter().any(|x| x == p))
}

fn reserve(p: &Path) {
    RESERVED.with(|r| r.borrow_mut().push(p.to_path_buf()));
}

fn release(p: &Path) {
    RESERVED.with(|r| r.borrow_mut().retain(|x| x != p));
}

/// Where a download goes: the Downloads folder, under a safe, free name
/// made from WebKit's suggestion. WebKit creates the file only if nothing
/// is there (it never replaces a file).
fn choose_destination(download: &webkit6::Download, suggested: &str, uri: &str) -> PathBuf {
    let path = crate::net::download_destination(suggested, uri, is_reserved);
    reserve(&path);
    download.set_allow_overwrite(false);
    download.set_destination(&path.to_string_lossy());
    path
}

/// The engine of a profile folder (made once, then shared by the windows).
pub fn profile_context(dir: &Path) -> Engine {
    if let Some(e) = ENGINES.with(|c| c.borrow().iter().find(|(d, _)| d == dir).map(|(_, e)| e.clone())) {
        return e;
    }
    // cookies and site data: closed to other users (folder 0700, files 0600)
    crate::store::ensure_private_dir(dir);
    crate::store::close_files(dir);
    let d = dir.to_string_lossy().to_string();
    limit_network();
    let session = NetworkSession::new(Some(&d), Some(&d));
    if let Some(cm) = session.cookie_manager() {
        cm.set_persistent_storage(&dir.join("cookies").to_string_lossy(), CookiePersistentStorage::Text);
    }
    accept_cookies(&session);
    enable_favicons(&session);
    let engine = Engine { ctx: new_context(), session };
    secure_context(&engine);
    ENGINES.with(|cs| cs.borrow_mut().push((dir.to_path_buf(), engine.clone())));
    engine
}

/// Site icons are loaded by WebKit itself, with the page, through that
/// page's own session (Sanaga makes no request of its own for them).
fn enable_favicons(session: &NetworkSession) {
    if let Some(dm) = session.website_data_manager() {
        dm.set_favicons_enabled(true);
    }
}

/// The memory a page's process may use before WebKit starts giving memory
/// back (caches, decoded pictures, unused JavaScript memory), in MB. WebKit's
/// own default is the whole memory up to 3 GB, so it only tidies up once a
/// page uses 1 GB or more; Sanaga starts at a third of this (about 340 MB on
/// 8 GB, 680 MB from 16 GB) and is firmer from a half.
fn page_memory_limit() -> u32 {
    let total_mb = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|m| m.lines().find(|l| l.starts_with("MemTotal:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|k| k.parse::<u64>().ok()))
        .map(|kb| kb / 1024)
        .unwrap_or(8192);
    (total_mb / 8).clamp(768, 2048) as u32
}

fn pressure_settings() -> webkit6::MemoryPressureSettings {
    let mut s = webkit6::MemoryPressureSettings::new();
    s.set_memory_limit(page_memory_limit());
    s.set_conservative_threshold(0.33);
    s.set_strict_threshold(0.5);
    // checked every 10 s (WebKit: 30 s)
    s.set_poll_interval(10.0);
    s
}

/// The processes of a profile's pages, with Sanaga's memory limits.
fn new_context() -> WebContext {
    WebContext::builder().memory_pressure_settings(&pressure_settings()).build()
}

/// The network process's memory limits (once, before the first session).
fn limit_network() {
    static NETWORK: std::sync::Once = std::sync::Once::new();
    NETWORK.call_once(|| NetworkSession::set_memory_pressure_settings(&mut pressure_settings()));
}

/// An engine that keeps nothing (no profile folder: tests, screenshots).
pub fn ephemeral_engine() -> Engine {
    limit_network();
    let session = NetworkSession::new_ephemeral();
    accept_cookies(&session);
    enable_favicons(&session);
    let engine = Engine { ctx: new_context(), session };
    secure_context(&engine);
    engine
}

/// Sites may keep cookies (logins…); other sites' cookies inside a page are
/// refused (third-party: trackers). Said explicitly: WebKitGTK 6's default
/// differs between versions.
fn accept_cookies(session: &NetworkSession) {
    if let Some(cm) = session.cookie_manager() {
        #[allow(unused_mut)]
        let mut policy = webkit6::CookieAcceptPolicy::NoThirdParty;
        // test aid (debug builds only): SANAGA_COOKIES=always
        #[cfg(debug_assertions)]
        {
            if std::env::var("SANAGA_COOKIES").as_deref() == Ok("always") {
                policy = webkit6::CookieAcceptPolicy::Always;
            }
        }
        cm.set_accept_policy(policy);
    }
}

/// The engine of private tabs (the profile's processes, a session that keeps nothing).
pub fn private_engine(base: Option<&Engine>) -> Engine {
    Engine { ctx: base.map(|e| e.ctx.clone()).unwrap_or_default(), session: private_session() }
}

/// The session shared by private tabs.
fn private_session() -> NetworkSession {
    PRIVATE.with(|p| {
        p.borrow_mut()
            .get_or_insert_with(|| {
                limit_network();
                let s = NetworkSession::new_ephemeral();
                accept_cookies(&s);
                enable_favicons(&s);
                s
            })
            .clone()
    })
}

/// Spell checking. (With WebKitGTK 6 every page always runs in its sandbox,
/// walled off from the rest of the computer.)
pub fn secure_context(engine: &Engine) {
    enable_spell_checking(&engine.ctx);
}

/// Spell checking in text fields: the interface language and English.
pub fn enable_spell_checking(ctx: &WebContext) {
    let lang = match crate::i18n::language() {
        "fr" => "fr_FR",
        "es" => "es_ES",
        "pt" => "pt_PT",
        "de" => "de_DE",
        _ => "en_US",
    };
    ctx.set_spell_checking_enabled(true);
    let langs: Vec<&str> = if lang == "en_US" { vec![lang] } else { vec![lang, "en_US"] };
    ctx.set_spell_checking_languages(&langs);
}

/// Where downloads go, and who is told when one ends.
pub fn watch_downloads(session: &NetworkSession, done: impl Fn(String, Option<PathBuf>, bool) + 'static) {
    let done = Rc::new(done);
    session.connect_download_started(move |_, download| {
        if std::env::var_os("SANAGA_DEBUG_DOWNLOADS").is_some() {
            eprintln!("download started {:?}", download.request().and_then(|r| r.uri()));
        }
        let failed = Rc::new(RefCell::new(false));
        let dest: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
        let view = download.web_view();
        let referrer = referrer_of(view.as_ref());
        let denied = Rc::new(Cell::new(false));
        let (d2, denied2, site_ref) = (dest.clone(), denied.clone(), referrer.clone());
        download.connect_decide_destination(move |download, suggested| {
            let uri = download.request().and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
            // (a download Sanaga started itself has no page: never asked; a
            // page Sanaga doesn't follow is always asked)
            let view = download.web_view();
            let session = view.as_ref().and_then(|v| v.network_session());
            if view.is_some() && view.as_ref().and_then(view_state).is_none() {
                let site = download_site(&site_ref, &uri);
                PENDING_DOWNLOADS.with(|q| {
                    q.borrow_mut().push(PendingDownload { download: download.clone(), session, suggested: suggested.to_string(), uri, site: site.clone(), dest: d2.clone(), denied: denied2.clone() })
                });
                ask_downloads(site);
                return true;
            }
            if let Some(state) = view.as_ref().and_then(view_state) {
                let site = download_site(&site_ref, &uri);
                // a real action of the person's in that page, not used yet;
                // or the address the person opened turning into a download
                let recent = |t: Option<Instant>| t.is_some_and(|t| t.elapsed() <= GESTURE_WINDOW);
                let in_page = recent(state.gesture.get()) && !state.gesture_used.get();
                let fresh = in_page || recent(state.typed.get());
                let allowed = DOWNLOAD_SITES.with(|d| d.borrow().iter().any(|(s, x)| *s == session && *x == site));
                if download_needs_ask(fresh, false, allowed) {
                    // the download waits (WebKit 2.40+) until the person answers
                    PENDING_DOWNLOADS.with(|q| {
                        q.borrow_mut().push(PendingDownload { download: download.clone(), session, suggested: suggested.to_string(), uri, site: site.clone(), dest: d2.clone(), denied: denied2.clone() })
                    });
                    ask_downloads(site);
                    return true;
                }
                // (one download per action)
                if fresh {
                    state.gesture_used.set(true);
                    state.typed.set(None);
                }
            }
            let path = choose_destination(download, suggested, &uri);
            *d2.borrow_mut() = Some(path);
            true
        });
        let f2 = failed.clone();
        download.connect_failed(move |_, e| {
            if std::env::var_os("SANAGA_DEBUG_DOWNLOADS").is_some() {
                eprintln!("download failed: {e}");
            }
            *f2.borrow_mut() = true;
        });
        let done = done.clone();
        download.connect_finished(move |download| {
            let uri = download.request().and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
            let path = dest.borrow().clone();
            if let Some(p) = &path {
                release(p);
            }
            if denied.get() {
                return; // the person said no: nothing to show
            }
            let ok = !*failed.borrow();
            if let (true, Some(p)) = (ok, &path) {
                // where it came from (after redirections), for the file manager
                let from = download.response().and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_else(|| uri.clone());
                if let Err(e) = crate::net::mark_download(p, &from, &referrer) {
                    eprintln!("download mark: {e}");
                }
            }
            done(uri, path, ok);
        });
    });
}

/// The JavaScript world of Sanaga's own scripts. The page's scripts (and
/// any frame inside it) can't see it, can't replace its functions and can't
/// reach its message channel: only Sanaga's scripts, run in the top frame,
/// talk to Sanaga.
pub const WORLD: &str = "sanaga";

/// Gives Sanaga's scripts `window.ipc.postMessage(...)` in their world (top
/// frame only). Each message carries the address of the page that sent it,
/// so a message from a page that has navigated since is dropped
/// (`message_body`).
const IPC_JS: &str = "if (window.top === window) Object.defineProperty(window, 'ipc', { value: Object.freeze({ postMessage: function (x) { window.webkit.messageHandlers.ipc.postMessage(location.href + '\\n' + String(x)); } }) });";

/// Sanaga's scripts go into its own world; the person's extensions act on
/// the page itself (its world), without Sanaga's channel. All run in the top
/// frame only.
fn add_scripts(ucm: &UserContentManager, own: &[&str], page: &[&str]) {
    for js in std::iter::once(IPC_JS).chain(own.iter().copied()) {
        ucm.add_script(&UserScript::for_world(js, UserContentInjectedFrames::TopFrame, UserScriptInjectionTime::Start, WORLD, &[], &[]));
    }
    for js in page {
        ucm.add_script(&UserScript::new(js, UserContentInjectedFrames::TopFrame, UserScriptInjectionTime::Start, &[], &[]));
    }
}

/// How long a key press in the page vouches for the shortcut message that
/// follows it.
const KEY_WINDOW: Duration = Duration::from_millis(1500);

pub struct PageView {
    view: WebView,
    /// WebKit's own identity (user agent) for this page.
    default_ua: String,
}

impl PageView {
    /// A new page. `scripts` run at the start of every page (top frame).
    /// `opener`: the page that opened this one (its windows share the engine).
    /// `popups`: makes the pages this one opens; without it they open as new tabs.
    pub fn new(engine: Option<&Engine>, private: bool, own: &[&str], page: &[&str], opener: Option<&WebView>, popups: Option<PopupMaker>, on: impl Fn(PageSignal) + 'static) -> PageView {
        let ucm = UserContentManager::new();
        add_scripts(&ucm, own, page);
        let on = Rc::new(on);
        let o = on.clone();
        // (the view, once made: to check where each message comes from)
        let me: Rc<RefCell<Option<gtk::glib::WeakRef<WebView>>>> = Rc::new(RefCell::new(None));
        let me2 = me.clone();
        ucm.connect_script_message_received(Some("ipc"), move |_, value| {
            let text = value.to_str().to_string();
            let Some(view) = me2.borrow().as_ref().and_then(|w| w.upgrade()) else { return };
            let current = view.uri().map(|u| u.to_string()).unwrap_or_default();
            let Some(body) = message_body(&text, &current) else { return };
            // a shortcut only follows a real key press in the page (seen by
            // GTK, not claimed by a script)
            if is_shortcut_message(body) && !view_state(&view).is_some_and(|s| s.key_at.get().is_some_and(|t| t.elapsed() <= KEY_WINDOW)) {
                return;
            }
            o(PageSignal::Message(body.to_string()));
        });
        // Sanaga's channel exists only in Sanaga's world
        ucm.register_script_message_handler("ipc", Some(WORLD));
        // Like Safari: videos may start by themselves only without sound
        // (no page shouting at you when it opens)
        let policies = webkit6::WebsitePolicies::builder().autoplay(webkit6::AutoplayPolicy::AllowWithoutSound).build();
        let mut b = WebView::builder().user_content_manager(&ucm).website_policies(&policies);
        match opener {
            // same engine process and data as the opener
            Some(o) => b = b.related_view(o),
            None => {
                if let Some(e) = engine {
                    b = b.web_context(&e.ctx);
                    b = b.network_session(&if private { private_session() } else { e.session.clone() });
                } else if private {
                    b = b.network_session(&private_session());
                }
            }
        }
        let view = b.build();
        VIEWS.with(|v| v.borrow_mut().push(view.downgrade()));
        let state = Rc::new(ViewState::default());
        VIEW_STATES.with(|s| s.borrow_mut().push((view.downgrade(), state.clone())));
        *me.borrow_mut() = Some(view.downgrade());
        // real key presses in the page (they vouch for forwarded shortcuts)
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let st = state.clone();
        keys.connect_key_pressed(move |_, _, _, _| {
            st.key_at.set(Some(Instant::now()));
            gtk::glib::Propagation::Proceed
        });
        view.add_controller(keys);
        if let Some(st) = WebViewExt::settings(&view) {
            st.set_enable_webgl(true);
            // drawing apps and games (Photopea, Canva, canvas games) draw
            // with the graphics card
            st.set_enable_2d_canvas_acceleration(true);
            st.set_enable_webaudio(true);
            // video calls (Meet, Teams, Jitsi…) and adaptive video streaming
            st.set_enable_webrtc(true);
            st.set_enable_media_stream(true);
            st.set_enable_mediasource(true);
            // test aid (debug builds only): let test pages open windows without a click
            #[cfg(debug_assertions)]
            {
                if std::env::var_os("SANAGA_TEST_POPUPS").is_some() {
                    st.set_javascript_can_open_windows_automatically(true);
                }
            }
            // (clipboard: WebKit's default, a page reads it only on a user gesture)
            st.set_enable_page_cache(true);
            st.set_enable_developer_extras(true);
            // Sanaga handles the two-finger swipe itself (shell.rs)
            st.set_enable_back_forward_navigation_gestures(false);
        }
        let o = on.clone();
        let st = state.clone();
        view.connect_load_changed(move |v, ev| {
            let uri = v.uri().map(|u| u.to_string()).unwrap_or_default();
            match ev {
                // website settings: before the new page is made
                LoadEvent::Started | LoadEvent::Redirected => apply_site(v, &host_of_uri(&uri)),
                LoadEvent::Committed => {
                    *st.committed.borrow_mut() = uri.clone();
                    // a new page: its downloads need its own actions
                    st.gesture.set(None);
                    st.typed.set(None);
                    let js = crate::site_prefs::for_host(&host_of_uri(&uri)).script();
                    if !js.is_empty() {
                        v.evaluate_javascript(&js, None, None, None::<&gtk::gio::Cancellable>, |_| {});
                    }
                    o(PageSignal::Started(uri))
                }
                LoadEvent::Finished => o(PageSignal::Finished(uri)),
                _ => {}
            }
        });
        let o = on.clone();
        view.connect_title_notify(move |v| o(PageSignal::Title(v.title().map(|t| t.to_string()).unwrap_or_default())));
        // the page's icon, as WebKit loaded it
        let o = on.clone();
        view.connect_favicon_notify(move |v| {
            if let (Some(tex), Some(uri)) = (v.favicon(), v.uri()) {
                o(PageSignal::Favicon(uri.to_string(), tex.save_to_png_bytes().to_vec()));
            }
        });
        // the page's address changed without a new page being loaded:
        // single-page sites (YouTube, Google, Facebook, X…) move with
        // history.pushState, and #anchors — the address bar follows
        let o = on.clone();
        view.connect_uri_notify(move |v| o(PageSignal::Uri(v.uri().map(|u| u.to_string()).unwrap_or_default())));
        let o = on.clone();
        view.connect_create(move |v, action| {
            // window.open(): a real page linked to this one (sign-in windows
            // need it to send their answer back), shown in a new tab
            if let Some(make) = &popups {
                if let Some(w) = make(v) {
                    // the click that opened it may start a download there
                    if action.is_user_gesture() {
                        mark_gesture(&w);
                    }
                    return Some(w.upcast::<gtk::Widget>());
                }
            }
            let action = action.clone();
            if let Some(u) = action.request().and_then(|r| r.uri()) {
                o(PageSignal::NewWindow(u.to_string()));
            }
            None
        });
        // How Sanaga introduces itself, per site (Chrome for the few web apps
        // that refuse WebKit, or the person's choice): set before the page is
        // asked for. A simulated device's identity is left alone.
        let default_ua = WebViewExt::settings(&view).and_then(|s| s.user_agent()).map(|u| u.to_string()).unwrap_or_default();
        let default_ua_kept = default_ua.clone();
        let o = on.clone();
        view.connect_decide_policy(move |v, decision, kind| {
            // Ctrl+click, middle click, Shift+click on a link: a new tab or
            // window (WebKit leaves this to the browser)
            if kind == webkit6::PolicyDecisionType::NavigationAction || kind == webkit6::PolicyDecisionType::NewWindowAction {
                if let Some(a) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>().and_then(|n| n.navigation_action()) {
                    // a link to Netflix, Disney+, Spotify… (clicked, or opened
                    // in a new window): nothing is loaded here, where it can't
                    // play; Sanaga opens it where it does
                    if a.navigation_type() == webkit6::NavigationType::LinkClicked || kind == webkit6::PolicyDecisionType::NewWindowAction {
                        if let Some(u) = a.request().and_then(|r| r.uri()).map(|u| u.to_string()).filter(|u| crate::identity::protected_url(u).is_some()) {
                            decision.ignore();
                            o(PageSignal::Protected(u));
                            return true;
                        }
                    }
                    let m = a.modifiers();
                    let ctrl = m & gtk::gdk::ModifierType::CONTROL_MASK.bits() != 0;
                    let shift = m & gtk::gdk::ModifierType::SHIFT_MASK.bits() != 0;
                    let middle = a.mouse_button() == 2;
                    if std::env::var_os("SANAGA_DEBUG_MENU").is_some() {
                        eprintln!("navigation {:?} kind={kind:?} modifiers={m:#x} button={} gesture={}", a.navigation_type(), a.mouse_button(), a.is_user_gesture());
                    }
                    // (a real click only, never a script's; a web address,
                    // or a local file from a local page: never Sanaga's own
                    // pages or other schemes)
                    let page = v.uri().map(|u| u.to_string()).unwrap_or_default();
                    if a.navigation_type() == webkit6::NavigationType::LinkClicked && (ctrl || middle || shift) && a.is_user_gesture() {
                        if let Some(u) = a.request().and_then(|r| r.uri()).map(|u| u.to_string()).filter(|u| crate::guard::link_may_open_elsewhere(u, &page)) {
                            let how = if (ctrl || middle) && shift { 1 } else if ctrl || middle { 0 } else { 2 };
                            decision.ignore();
                            o(PageSignal::OpenLink(u, how));
                            return true;
                        }
                    }
                }
            }
            // A file the browser cannot show (archive, installer, unknown
            // type): downloaded, as in Chrome (WebKit would just ignore it).
            if kind == webkit6::PolicyDecisionType::Response {
                if let Some(r) = decision.downcast_ref::<webkit6::ResponsePolicyDecision>() {
                    if std::env::var_os("SANAGA_DEBUG_DOWNLOADS").is_some() {
                        eprintln!("response {:?} {:?} main={} shown={}", r.response().and_then(|x| x.mime_type()), r.response().and_then(|x| x.uri()), r.is_main_frame_main_resource(), r.is_mime_type_supported());
                    }
                    if r.is_main_frame_main_resource() && !r.is_mime_type_supported() {
                        let status = r.response().map(|x| x.status_code()).unwrap_or(200);
                        if (200..300).contains(&status) {
                            decision.download();
                            return true;
                        }
                    }
                }
                return false;
            }
            // a click on the page (for its downloads: see download_needs_ask)
            if kind == webkit6::PolicyDecisionType::NavigationAction || kind == webkit6::PolicyDecisionType::NewWindowAction {
                if let Some(nav) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>() {
                    if nav.navigation_action().is_some_and(|a| a.is_user_gesture()) {
                        mark_gesture(v);
                    }
                }
            }
            if kind != webkit6::PolicyDecisionType::NavigationAction {
                return false;
            }
            let Some(nav) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>() else { return false };
            let Some(action) = nav.navigation_action() else { return false };
            // Only a link clicked by the person: WebKit asks here for the
            // frames inside a page too (a "Sign in with Google" frame…), and
            // loading such a frame's address would replace the whole tab.
            // (Addresses typed or opened by Sanaga get their identity in load_url.)
            if action.navigation_type() != webkit6::NavigationType::LinkClicked || !action.is_user_gesture() {
                return false;
            }
            let Some(req) = action.request() else { return false };
            let uri = req.uri().map(|u| u.to_string()).unwrap_or_default();
            let Some(host) = url::Url::parse(&uri).ok().and_then(|u| u.host_str().map(|h| h.to_string())) else { return false };
            if !set_identity(v, &host, &default_ua) {
                return false;
            }
            // ask again with the new identity (a form being sent goes on as it is)
            if req.http_method().is_none_or(|m| m == "GET") {
                decision.ignore();
                v.load_uri(&uri);
                return true;
            }
            false
        });
        // the page's right-click menu: Sanaga's page tools at the end
        let o = on.clone();
        view.connect_context_menu(move |_, menu, hit| {
            let add = |label: &str, what: &'static str, arg: String| {
                let action = gtk::gio::SimpleAction::new(&format!("sanaga-{what}"), None);
                let o = o.clone();
                action.connect_activate(move |_, _| o(PageSignal::Tool(what, arg.clone())));
                menu.append(&webkit6::ContextMenuItem::from_gaction(&action, label, None));
            };
            menu.append(&webkit6::ContextMenuItem::new_separator());
            if hit.context_is_link() {
                if let Some(link) = hit.link_uri() {
                    add(crate::i18n::tr("Aperçu du lien"), "preview", link.to_string());
                    add(crate::i18n::tr("Ouvrir le lien dans un onglet privé"), "private-link", link.to_string());
                    add(crate::i18n::tr("Ouvrir le lien dans un conteneur…"), "container-link", link.to_string());
                }
            }
            if hit.context_is_media() {
                add(crate::i18n::tr("Vidéo flottante"), "pip", String::new());
            }
            add(crate::i18n::tr("Supprimer un élément de la page…"), "zap", String::new());
            add(crate::i18n::tr("Modifier le texte de la page"), "edit-text", String::new());
            add(crate::i18n::tr("Réglages de ce site…"), "site-settings", String::new());
            if std::env::var_os("SANAGA_DEBUG_MENU").is_some() {
                eprintln!("page menu: {} items", menu.n_items());
            }
            false
        });
        // the page did not load: Sanaga's own error page (not WebKit's)
        let o = on.clone();
        view.connect_load_failed(move |_, _, uri, e| {
            if std::env::var_os("SANAGA_DEBUG_MENU").is_some() {
                eprintln!("load failed {uri}: domain={} message={:?}", e.domain().as_str(), e.message());
            }
            match classify(e) {
                Some(kind) => {
                    o(PageSignal::LoadFailed(uri.to_string(), kind, e.message().to_string()));
                    true
                }
                None => false,
            }
        });
        let o = on.clone();
        view.connect_load_failed_with_tls_errors(move |_, uri, cert, flags| {
            let host = host_of_uri(uri);
            TLS_SEEN.with(|t| {
                let mut t = t.borrow_mut();
                t.retain(|(h, _)| *h != host);
                t.push((host.clone(), cert.clone()));
            });
            o(PageSignal::LoadFailed(uri.to_string(), LoadError::Certificate, format!("{flags:?}")));
            true
        });
        // alert(), confirm(), prompt(), "leave this page?": Sanaga's dialogs
        let o = on.clone();
        view.connect_script_dialog(move |v, d| {
            let kind = match d.dialog_type() {
                webkit6::ScriptDialogType::Alert => DialogKind::Alert,
                webkit6::ScriptDialogType::Confirm => DialogKind::Confirm,
                webkit6::ScriptDialogType::Prompt => DialogKind::Prompt(d.prompt_get_default_text().map(|t| t.to_string()).unwrap_or_default()),
                _ => DialogKind::Leave,
            };
            let info = DialogInfo { kind, message: d.message().map(|m| m.to_string()).unwrap_or_default(), host: host_of_uri(&v.uri().map(|u| u.to_string()).unwrap_or_default()) };
            let key = keep_dialog(Pending::Script(d.clone()), info);
            o(PageSignal::Dialog(key));
            true
        });
        // a site asking for a user name and password (HTTP authentication)
        let o = on.clone();
        view.connect_authenticate(move |_, r| {
            if r.is_for_proxy() {
                return false;
            }
            let host = r.host().map(|h| h.to_string()).unwrap_or_default();
            let info = DialogInfo { kind: DialogKind::Login(r.realm().map(|x| x.to_string()).unwrap_or_default()), message: String::new(), host };
            let key = keep_dialog(Pending::Auth(r.clone()), info);
            o(PageSignal::Dialog(key));
            true
        });
        // a page that closes itself (window.close(), allowed by WebKit only
        // for windows a script opened): its tab closes
        let o = on.clone();
        view.connect_close(move |_| o(PageSignal::Close));
        // the page's process stopped: say so, instead of a blank page (the
        // other tabs are not touched); ending it on purpose is not a crash
        let o = on.clone();
        view.connect_web_process_terminated(move |_, reason| match reason {
            webkit6::WebProcessTerminationReason::ExceededMemoryLimit => o(PageSignal::Crashed(true)),
            webkit6::WebProcessTerminationReason::Crashed => o(PageSignal::Crashed(false)),
            _ => {}
        });
        PageView { view, default_ua: default_ua_kept }
    }

    pub fn webview(&self) -> WebView {
        self.view.clone()
    }
    /// The address the page really shows (not what a page script claims).
    pub fn uri(&self) -> Option<String> {
        self.view.uri().map(|u| u.to_string())
    }
    /// Replace the scripts run in this page (extensions switched on or off)
    /// and load it again so the change shows at once.
    pub fn set_scripts(&self, own: &[&str], page: &[&str]) {
        if let Some(ucm) = self.view.user_content_manager() {
            ucm.remove_all_scripts();
            ucm.remove_all_style_sheets();
            add_scripts(&ucm, own, page);
            self.view.reload();
        }
    }
    /// Open an address. `by_person`: the person asked for it now (typed, a
    /// favourite, a link in Sanaga), not a restored or woken tab; if this
    /// address itself turns into a download, it goes straight to Downloads.
    pub fn load_url(&self, url: &str, by_person: bool) -> R<()> {
        if let Some(s) = view_state(&self.view) {
            s.typed.set(by_person.then(Instant::now));
        }
        set_identity(&self.view, &host_of_uri(url), &self.default_ua);
        self.view.load_uri(url);
        Ok(())
    }
    /// Run a script in the page's own world (the person's buttons, site
    /// settings): it has no access to Sanaga's channel.
    pub fn evaluate_script(&self, js: &str) -> R<()> {
        self.view.evaluate_javascript(js, None, None, None::<&gtk::gio::Cancellable>, |_| {});
        Ok(())
    }

    /// Run one of Sanaga's scripts in Sanaga's world (top frame), where the
    /// page can't see or change it.
    pub fn evaluate_own(&self, js: &str) -> R<()> {
        self.view.evaluate_javascript(js, Some(WORLD), None, None::<&gtk::gio::Cancellable>, |_| {});
        Ok(())
    }
    pub fn zoom(&self, z: f64) -> R<()> {
        self.view.set_zoom_level(z);
        Ok(())
    }
    pub fn reload(&self) -> R<()> {
        self.view.reload();
        Ok(())
    }
    pub fn go_back(&self) -> R<()> {
        self.view.go_back();
        Ok(())
    }
    pub fn go_forward(&self) -> R<()> {
        self.view.go_forward();
        Ok(())
    }
    pub fn can_go_back(&self) -> R<bool> {
        Ok(self.view.can_go_back())
    }
    pub fn can_go_forward(&self) -> R<bool> {
        Ok(self.view.can_go_forward())
    }
    pub fn print(&self) -> R<()> {
        let op = webkit6::PrintOperation::new(&self.view);
        let parent = self.view.root().and_then(|r| r.downcast::<gtk::Window>().ok());
        op.run_dialog(parent.as_ref());
        Ok(())
    }
    pub fn open_devtools(&self) {
        if let Some(i) = self.view.inspector() {
            i.show();
        }
    }
    pub fn set_visible(&self, v: bool) -> R<()> {
        self.view.set_visible(v);
        Ok(())
    }
}

impl Drop for PageView {
    fn drop(&mut self) {
        // the page leaves the window (and its engine process ends)
        if let Some(o) = self.view.parent().and_then(|p| p.downcast::<gtk::Overlay>().ok()) {
            o.remove_overlay(&self.view);
        } else if self.view.parent().is_some() {
            self.view.unparent();
        }
        self.view.terminate_web_process();
    }
}

/// A plain page for a small window of its own (link preview, floating
/// video), with the profile's cookies.
pub fn plain_view(engine: Option<&Engine>) -> WebView {
    let mut b = WebView::builder();
    if let Some(e) = engine {
        b = b.web_context(&e.ctx).network_session(&e.session);
    }
    let view = b.build();
    if let Some(st) = WebViewExt::settings(&view) {
        st.set_enable_mediasource(true);
        st.set_enable_webgl(true);
        st.set_media_playback_requires_user_gesture(false);
    }
    // its downloads follow the same rule as a tab's (see download_needs_ask):
    // the page shown, and the person's actions in it
    let state = Rc::new(ViewState::default());
    VIEW_STATES.with(|s| s.borrow_mut().push((view.downgrade(), state.clone())));
    view.connect_load_changed(move |v, ev| {
        if matches!(ev, LoadEvent::Committed) {
            *state.committed.borrow_mut() = v.uri().map(|u| u.to_string()).unwrap_or_default();
            state.gesture.set(None);
            state.typed.set(None);
        }
    });
    view.connect_decide_policy(|v, decision, kind| {
        if kind == webkit6::PolicyDecisionType::NavigationAction || kind == webkit6::PolicyDecisionType::NewWindowAction {
            if let Some(nav) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>() {
                if nav.navigation_action().is_some_and(|a| a.is_user_gesture()) {
                    mark_gesture(v);
                }
            }
        }
        false
    });
    view
}

/// Open an address the person asked for in a small window (a link preview):
/// if it turns into a download, it goes straight to Downloads.
pub fn load_by_person(view: &WebView, url: &str) {
    if let Some(s) = view_state(view) {
        s.typed.set(Some(Instant::now()));
    }
    view.load_uri(url);
}

/// A WebKit snapshot (GDK texture) as a cairo picture (to draw a device
/// frame around it, or make thumbnails).
pub fn texture_to_surface(tex: &gtk::gdk::Texture) -> Option<gtk::cairo::ImageSurface> {
    let (w, h) = (tex.width(), tex.height());
    if w <= 0 || h <= 0 {
        return None;
    }
    let mut surf = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w, h).ok()?;
    let stride = surf.stride() as usize;
    {
        let mut data = surf.data().ok()?;
        // cairo's ARGB32 is premultiplied B, G, R, A in memory on little-endian machines
        tex.download(&mut data, stride);
    }
    surf.mark_dirty();
    Some(surf)
}
