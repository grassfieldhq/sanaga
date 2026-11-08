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
    static DOWNLOAD_SITES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static DOWNLOAD_ASK_SINKS: RefCell<Vec<(usize, AskSink)>> = const { RefCell::new(Vec::new()) };
}

/// What Sanaga follows for one open page.
#[derive(Default)]
struct ViewState {
    /// The address of the page shown (committed): the referrer of its downloads.
    committed: RefCell<String>,
    /// When the person last acted on the page (a click that navigates, an
    /// address opened from Sanaga's interface), and whether a download has
    /// already used that action.
    gesture: Cell<Option<Instant>>,
    gesture_used: Cell<bool>,
    /// When a key was last pressed in the page (seen by GTK).
    key_at: Cell<Option<Instant>>,
}

/// The person acted on this page (a click that navigates, an address
/// opened from Sanaga's interface): it may start one download.
fn mark_gesture(v: &WebView) {
    if let Some(s) = view_state(v) {
        s.gesture.set(Some(Instant::now()));
        s.gesture_used.set(false);
    }
}

/// How long after the person's action a page may start its download.
const GESTURE_WINDOW: Duration = Duration::from_secs(10);

/// Safari's rule for downloads: they go straight to the Downloads folder
/// when the person just acted on the page and no other download used that
/// action; a site that starts a download by itself, or several at once,
/// must be allowed first (unless the person already allowed that site).
pub fn download_needs_ask(gesture_fresh: bool, gesture_used: bool, site_allowed: bool) -> bool {
    !site_allowed && !(gesture_fresh && !gesture_used)
}

/// The site named in "Allow downloads on …?": the page's, else the file's.
pub fn download_site(referrer: &str, uri: &str) -> String {
    let h = host_of_uri(referrer);
    if !h.is_empty() {
        return h;
    }
    let h = host_of_uri(uri);
    if !h.is_empty() {
        return h;
    }
    uri.split(':').next().unwrap_or("").to_string()
}

/// A download waiting for the person's answer.
struct PendingDownload {
    download: webkit6::Download,
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
/// Downloads folder (and the site may download until Sanaga quits), or
/// they are cancelled.
pub fn answer_downloads(site: &str, allow: bool) {
    if allow {
        DOWNLOAD_SITES.with(|d| {
            let mut d = d.borrow_mut();
            if !d.iter().any(|s| s == site) {
                d.push(site.to_string());
            }
        });
    }
    let mine: Vec<PendingDownload> = PENDING_DOWNLOADS.with(|q| {
        let mut q = q.borrow_mut();
        let (mine, rest): (Vec<PendingDownload>, Vec<PendingDownload>) = q.drain(..).partition(|p| p.site == site);
        *q = rest;
        mine
    });
    for p in mine {
        if allow {
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
    let _ = std::fs::create_dir_all(dir);
    let d = dir.to_string_lossy().to_string();
    let session = NetworkSession::new(Some(&d), Some(&d));
    if let Some(cm) = session.cookie_manager() {
        cm.set_persistent_storage(&dir.join("cookies").to_string_lossy(), CookiePersistentStorage::Text);
    }
    accept_cookies(&session);
    let engine = Engine { ctx: WebContext::new(), session };
    secure_context(&engine);
    ENGINES.with(|cs| cs.borrow_mut().push((dir.to_path_buf(), engine.clone())));
    engine
}

/// An engine that keeps nothing (no profile folder: tests, screenshots).
pub fn ephemeral_engine() -> Engine {
    let session = NetworkSession::new_ephemeral();
    accept_cookies(&session);
    let engine = Engine { ctx: WebContext::new(), session };
    secure_context(&engine);
    engine
}

/// Sites may keep cookies (logins…); other sites' cookies inside a page are
/// refused (third-party: trackers). Said explicitly: WebKitGTK 6's default
/// differs between versions.
fn accept_cookies(session: &NetworkSession) {
    if let Some(cm) = session.cookie_manager() {
        let policy = match std::env::var("SANAGA_COOKIES").as_deref() {
            Ok("always") => webkit6::CookieAcceptPolicy::Always,
            _ => webkit6::CookieAcceptPolicy::NoThirdParty,
        };
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
                let s = NetworkSession::new_ephemeral();
                accept_cookies(&s);
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
            // (a download Sanaga started itself has no page: never asked)
            if let Some(state) = download.web_view().as_ref().and_then(view_state) {
                let site = download_site(&site_ref, &uri);
                let fresh = state.gesture.get().is_some_and(|t| t.elapsed() <= GESTURE_WINDOW);
                let allowed = DOWNLOAD_SITES.with(|d| d.borrow().iter().any(|s| *s == site));
                if download_needs_ask(fresh, state.gesture_used.get(), allowed) {
                    // the download waits (WebKit 2.40+) until the person answers
                    PENDING_DOWNLOADS.with(|q| {
                        q.borrow_mut().push(PendingDownload { download: download.clone(), suggested: suggested.to_string(), uri, site: site.clone(), dest: d2.clone(), denied: denied2.clone() })
                    });
                    ask_downloads(site);
                    return true;
                }
                if fresh {
                    state.gesture_used.set(true);
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

fn without_fragment(u: &str) -> &str {
    u.split('#').next().unwrap_or(u)
}

/// A message of Sanaga's page scripts ("address\nbody"): its body when it
/// comes from the page the view shows now (the #fragment may differ),
/// nothing when that page has navigated since.
pub fn message_body<'a>(msg: &'a str, current: &str) -> Option<&'a str> {
    let (from, body) = msg.split_once('\n')?;
    (!current.is_empty() && without_fragment(from) == without_fragment(current)).then_some(body)
}

/// A keyboard shortcut forwarded by the page script ("cs:t", ":f5"…).
pub fn is_shortcut_message(body: &str) -> bool {
    body.split_once(':').is_some_and(|(mods, key)| !key.is_empty() && mods.chars().all(|c| matches!(c, 'c' | 's' | 'a')))
}

/// How long a key press in the page vouches for the shortcut message that
/// follows it.
const KEY_WINDOW: Duration = Duration::from_millis(1500);

pub struct PageView {
    view: WebView,
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
            st.set_enable_webaudio(true);
            // video calls (Meet, Teams, Jitsi…) and adaptive video streaming
            st.set_enable_webrtc(true);
            st.set_enable_media_stream(true);
            st.set_enable_mediasource(true);
            // test aid: let test pages open windows without a click
            if std::env::var_os("SANAGA_TEST_POPUPS").is_some() {
                st.set_javascript_can_open_windows_automatically(true);
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
        view.connect_decide_policy(move |v, decision, kind| {
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
            let Some(req) = nav.navigation_action().and_then(|a| a.request()) else { return false };
            let uri = req.uri().map(|u| u.to_string()).unwrap_or_default();
            let Some(host) = url::Url::parse(&uri).ok().and_then(|u| u.host_str().map(|h| h.to_string())) else { return false };
            let Some(st) = WebViewExt::settings(v) else { return false };
            let current = st.user_agent().map(|u| u.to_string()).unwrap_or_default();
            let ours = current == default_ua || crate::identity::Identity::ALL.iter().any(|i| i.user_agent() == Some(current.as_str()));
            if !ours {
                return false;
            }
            let want = crate::identity::for_host(&host).user_agent().unwrap_or(&default_ua).to_string();
            if want == current {
                return false;
            }
            st.set_user_agent(Some(&want));
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
        PageView { view }
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
    pub fn load_url(&self, url: &str) -> R<()> {
        // an address opened from Sanaga's interface: the person's own action
        mark_gesture(&self.view);
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
    view
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_follow_safaris_rule() {
        // a click, one download: no question
        assert!(!download_needs_ask(true, false, false));
        // no click: asked
        assert!(download_needs_ask(false, false, false));
        // a second download from the same click: asked
        assert!(download_needs_ask(true, true, false));
        // a site the person allowed: never asked
        assert!(!download_needs_ask(false, false, true));
        assert!(!download_needs_ask(true, true, true));
    }

    #[test]
    fn messages_from_a_page_that_navigated_are_dropped() {
        assert_eq!(message_body("https://a.example/x\ntool:zap-done:#ad", "https://a.example/x"), Some("tool:zap-done:#ad"));
        assert_eq!(message_body("https://a.example/x#top\nform:dirty", "https://a.example/x#end"), Some("form:dirty"));
        assert_eq!(message_body("https://a.example/x\nform:dirty", "https://b.example/"), None);
        assert_eq!(message_body("https://a.example/x\nform:dirty", ""), None);
        assert_eq!(message_body("no address", "https://a.example/x"), None);
        // the body may hold line breaks of its own
        assert_eq!(message_body("about:blank\ntr:0\n[\"a\"]", "about:blank"), Some("tr:0\n[\"a\"]"));
    }

    #[test]
    fn shortcut_messages_are_recognised() {
        assert!(is_shortcut_message("cs:t"));
        assert!(is_shortcut_message(":f5"));
        assert!(is_shortcut_message("c:q"));
        assert!(!is_shortcut_message("tool:zap-done:x"));
        assert!(!is_shortcut_message("form:dirty"));
        assert!(!is_shortcut_message("sum:x"));
        assert!(!is_shortcut_message("c:"));
    }

    #[test]
    fn download_question_names_the_site() {
        assert_eq!(download_site("https://www.example.org/page", "https://cdn.example.net/f.zip"), "www.example.org");
        assert_eq!(download_site("", "https://cdn.example.net/f.zip"), "cdn.example.net");
        assert_eq!(download_site("about:blank", "data:text/plain,hi"), "data");
    }
}
