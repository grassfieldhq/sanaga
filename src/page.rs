//! One web page shown by WebKit (WebKitGTK 6, GTK 4), as a normal part of
//! Sanaga's window: GTK composes the page and the rest of the window together
//! on the graphics card, in the same frame.

use gtk::prelude::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
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
pub fn follow_downloads(engine: &Engine, pick: impl Fn(&str) -> PathBuf + 'static) {
    let pick = Rc::new(pick);
    for session in [engine.session.clone(), private_session()] {
        let known = WATCHED.with(|w| w.borrow().contains(&session));
        if !known {
            WATCHED.with(|w| w.borrow_mut().push(session.clone()));
            let p = pick.clone();
            watch_downloads(&session, move |u| p(u), report_download);
        }
    }
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
    let engine = Engine { ctx: WebContext::new(), session };
    secure_context(&engine);
    ENGINES.with(|cs| cs.borrow_mut().push((dir.to_path_buf(), engine.clone())));
    engine
}

/// An engine that keeps nothing (no profile folder: tests, screenshots).
pub fn ephemeral_engine() -> Engine {
    let engine = Engine { ctx: WebContext::new(), session: NetworkSession::new_ephemeral() };
    secure_context(&engine);
    engine
}

/// The session shared by private tabs.
fn private_session() -> NetworkSession {
    PRIVATE.with(|p| p.borrow_mut().get_or_insert_with(NetworkSession::new_ephemeral).clone())
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
pub fn watch_downloads(session: &NetworkSession, pick: impl Fn(&str) -> PathBuf + 'static, done: impl Fn(String, Option<PathBuf>, bool) + 'static) {
    let pick = Rc::new(pick);
    let done = Rc::new(done);
    session.connect_download_started(move |_, download| {
        if std::env::var_os("SANAGA_DEBUG_DOWNLOADS").is_some() {
            eprintln!("download started {:?}", download.request().and_then(|r| r.uri()));
        }
        let failed = Rc::new(RefCell::new(false));
        let dest: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
        let (pick, d2) = (pick.clone(), dest.clone());
        download.connect_decide_destination(move |download, _suggested| {
            let uri = download.request().and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
            let path = pick(&uri);
            download.set_destination(&path.to_string_lossy());
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
            done(uri, dest.borrow().clone(), !*failed.borrow());
        });
    });
}

/// Script that gives pages `window.ipc.postMessage(...)` (Sanaga's page scripts use it).
const IPC_JS: &str = "Object.defineProperty(window, 'ipc', { value: Object.freeze({ postMessage: function(x) { window.webkit.messageHandlers['ipc'].postMessage(x) } }) })";

fn add_scripts(ucm: &UserContentManager, scripts: &[&str]) {
    for js in std::iter::once(IPC_JS).chain(scripts.iter().copied()) {
        ucm.add_script(&UserScript::new(js, UserContentInjectedFrames::TopFrame, UserScriptInjectionTime::Start, &[], &[]));
    }
}

pub struct PageView {
    view: WebView,
}

impl PageView {
    /// A new page. `scripts` run at the start of every page (top frame).
    /// `opener`: the page that opened this one (its windows share the engine).
    /// `popups`: makes the pages this one opens; without it they open as new tabs.
    pub fn new(engine: Option<&Engine>, private: bool, scripts: &[&str], opener: Option<&WebView>, popups: Option<PopupMaker>, on: impl Fn(PageSignal) + 'static) -> PageView {
        let ucm = UserContentManager::new();
        add_scripts(&ucm, scripts);
        let on = Rc::new(on);
        let o = on.clone();
        ucm.connect_script_message_received(Some("ipc"), move |_, value| {
            o(PageSignal::Message(value.to_str().to_string()));
        });
        ucm.register_script_message_handler("ipc", None);
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
            st.set_javascript_can_access_clipboard(true);
            st.set_enable_page_cache(true);
            st.set_enable_developer_extras(true);
            // Sanaga handles the two-finger swipe itself (shell.rs)
            st.set_enable_back_forward_navigation_gestures(false);
        }
        let o = on.clone();
        view.connect_load_changed(move |v, ev| {
            let uri = v.uri().map(|u| u.to_string()).unwrap_or_default();
            match ev {
                LoadEvent::Committed => o(PageSignal::Started(uri)),
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
    pub fn set_scripts(&self, scripts: &[&str]) {
        if let Some(ucm) = self.view.user_content_manager() {
            ucm.remove_all_scripts();
            ucm.remove_all_style_sheets();
            add_scripts(&ucm, scripts);
            self.view.reload();
        }
    }
    pub fn load_url(&self, url: &str) -> R<()> {
        self.view.load_uri(url);
        Ok(())
    }
    pub fn evaluate_script(&self, js: &str) -> R<()> {
        self.view.evaluate_javascript(js, None, None, None::<&gtk::gio::Cancellable>, |_| {});
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
