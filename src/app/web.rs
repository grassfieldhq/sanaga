//! The full web engine: WebKit (the engine behind Safari and GNOME Web), shown
//! inside Sanaga's window. Each tab with a web page owns one WebKit view.

use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use crate::page::{PageSignal, PageView};
use crate::store;
use webkit6::prelude::*;

pub(super) enum WebEvent {
    Load { tab: u64, finished: bool, url: String },
    Title { tab: u64, title: String },
    /// The page's address changed (pushState, #anchor, or a load).
    Uri { tab: u64, url: String },
    NewTab { tab: u64, url: String },
    Shortcut(String),
    /// A page closed itself (a sign-in window when done): its tab closes.
    CloseTab { tab: u64 },
    /// Texts of a page to translate (or "done").
    Translate { tab: u64, msg: String },
    /// Text typed in a form and not sent yet (true), or sent / page left.
    FormDirty { tab: u64, dirty: bool },
    /// The page stopped working (true: it used too much memory).
    Crashed { tab: u64, memory: bool },
    /// A message for the person (e.g. a screenshot was saved).
    Notice(String),
    /// A link clicked in a source page: show that file's source.
    ViewSource { tab: u64, url: String },
    /// Login seen / login form found by the page script.
    Password { tab: u64, msg: String },
    DownloadDone { url: String, path: Option<std::path::PathBuf>, ok: bool },
    /// A site started a download by itself, or several at once: ask first.
    DownloadAsk { site: String },
    /// Privacy report: trackers kept from following the person.
    Privacy(Vec<(String, usize)>),
    /// The text of a page, for its summary.
    PageText { tab: u64, text: String },
    /// A page tool (right-click menu, or an answer of a tool's script).
    Tool { tab: u64, what: String, arg: String },
    /// A link Ctrl+clicked / middle-clicked / Shift+clicked (see PageSignal::OpenLink).
    OpenLink { tab: u64, url: String, how: u8 },
    /// The page did not load.
    LoadFailed { tab: u64, url: String, kind: crate::page::LoadError, detail: String },
    /// The page asks something (alert, confirm, prompt, login…).
    Dialog { tab: u64, key: u64 },
    /// A link to a protected site (Netflix…), stopped before loading.
    Protected { tab: u64, url: String },
    /// A click inside a page (split view: that page becomes the tab in use).
    PageClicked { tab: u64 },
    /// How far a page is scrolled, as a share of its height (mobile preview side by side).
    Scrolled { tab: u64, share: f32 },
}

/// Forwards Sanaga's keyboard shortcuts while the page has the keyboard.
/// A video or sound the engine can't decode (missing codecs): Sanaga offers them.
const MEDIA_JS: &str = r#"window.addEventListener('error', function (e) {
  if (!e.isTrusted) return;
  var t = e.target;
  if (t && (t.tagName === 'VIDEO' || t.tagName === 'AUDIO' || t.tagName === 'SOURCE') && window.ipc) {
    var m = t.tagName === 'SOURCE' ? t.parentNode : t;
    if (!m || !m.error || m.error.code === 4) window.ipc.postMessage('tool:media-unsupported:');
  }
}, true);
// video calls need WebRTC; some systems' WebKitGTK is built without it
if (typeof RTCPeerConnection === 'undefined' && window.ipc && /^https?:$/.test(location.protocol)) window.ipc.postMessage('tool:no-webrtc:');"#;

const SHORTCUT_JS: &str = r#"
window.addEventListener('keydown', function (e) {
  if (!e.isTrusted) return;
  var k = (e.key || '').toLowerCase();
  var ctrl = e.ctrlKey || e.metaKey;
  var ours = (ctrl && ['t','w','l','r','d','h','j','f','n','tab','=','+','-','0','q','b','p','u','i','\\','|','o','s','a','k','e','delete','m','y','pagedown','pageup'].indexOf(k) >= 0)
    || (e.altKey && (k === 'arrowleft' || k === 'arrowright' || k === 'home'))
    || k === 'f5' || k === 'f6' || k === 'f9' || k === 'f11' || k === 'f12'
    || (k === 'escape' && window.__sanagaTool);
  if (!ours || !window.ipc) return;
  e.preventDefault();
  e.stopPropagation();
  window.ipc.postMessage((ctrl ? 'c' : '') + (e.shiftKey ? 's' : '') + (e.altKey ? 'a' : '') + ':' + k);
}, true);
"#;

/// Links of Sanaga's own source page (Ctrl+U): a real click shows the
/// source of the linked file.
const SOURCE_LINKS_JS: &str = r#"
document.addEventListener('click', function (e) {
  // (a hint only: Sanaga checks on its side that the tab shows its own source page)
  if (!e.isTrusted || !window.ipc || location.href !== 'about:blank' || !document.documentElement.hasAttribute('data-sanaga-source')) return;
  var a = e.target && e.target.closest && e.target.closest('a[data-u]');
  if (!a) return;
  e.preventDefault();
  window.ipc.postMessage('vs:' + a.getAttribute('data-u'));
}, true);
"#;

/// The buttons of Sanaga's own error page (back to safety, go on despite the
/// certificate): a real click sends the action; Sanaga checks on its side
/// that the tab shows that error page (see `page_tool`).
/// (Nothing a page can set counts: the secret is given by Sanaga to its own
/// world in that error page only, and checked in Rust with the failed
/// address.)
const ERROR_ACTIONS_JS: &str = r#"
document.addEventListener('click', function (e) {
  if (!e.isTrusted || !window.ipc || !window.__sanagaErrorToken) return;
  var b = e.target && e.target.closest && e.target.closest('[data-sanaga-action]');
  if (!b) return;
  e.preventDefault();
  window.ipc.postMessage('tool:' + b.getAttribute('data-sanaga-action') + ':' + window.__sanagaErrorToken + '|' + (b.getAttribute('data-sanaga-arg') || ''));
}, true);
"#;

/// The page tools whose answer may come from Sanaga's page scripts (the
/// others are only asked from Sanaga's menus). Each one is still checked
/// against what the person started (see `page_tool`).
fn page_may_answer(what: &str) -> bool {
    matches!(what, "pip-info" | "zap-done" | "media-unsupported" | "safe" | "tls-allow") || (cfg!(debug_assertions) && what == "probe")
}

fn shortcut_name(msg: &str) -> Option<&'static str> {
    let (mods, key) = msg.split_once(':')?;
    let c = mods.contains('c');
    let s = mods.contains('s');
    let a = mods.contains('a');
    Some(match (c, s, a, key) {
        (true, true, _, "n") => "private",
        (true, true, _, "t") => "reopen",
        (true, true, _, "b") => "bmbar",
        (true, true, _, "r") => "reader",
        (true, true, _, "\\") | (true, true, _, "|") => "overview",
        (true, true, _, "l") => "sidebar",
        (true, true, _, "o") => "overview",
        (true, true, _, "f") => "contacts",
        (true, true, _, "a") => "tabsearch",
        (true, false, false, "k") => "tabsearch",
        (true, true, _, "e") => "split",
        (true, true, _, "delete") => "cleardata",
        (true, false, false, "s") => "savepage",
        (true, false, true, "i") => "inspector",
        (true, true, _, "i") => "inspector",
        (_, _, _, "f12") => "inspector",
        (true, true, _, "m") => "devices",
        (_, _, _, "f9") => "focus",
        (true, true, _, "y") => "summary",
        (_, _, _, "escape") => "esc",
        (true, false, false, "p") => "print",
        (true, false, false, "u") => "source",
        (_, _, _, "f11") => "fullscreen",
        (true, true, _, "d") => "readinglist",
        (true, true, _, "tab") => "prev",
        (true, _, _, "pagedown") => "tabright",
        (true, _, _, "pageup") => "tableft",
        (true, false, _, "t") => "new",
        (true, false, _, "n") => "window",
        (true, _, _, "w") => "close",
        (true, false, _, "tab") => "next",
        (true, _, _, "l") | (_, _, _, "f6") => "addr",
        (true, _, _, "r") | (_, _, _, "f5") => "reload",
        (_, _, true, "arrowleft") => "back",
        (_, _, true, "arrowright") => "fwd",
        (_, _, true, "home") => "home",
        (true, _, _, "d") => "bookmark",
        (true, _, _, "h") => "history",
        (true, _, _, "j") => "downloads",
        (true, _, _, "f") => "find",
        (true, _, _, "=") | (true, _, _, "+") => "zin",
        (true, _, _, "-") => "zout",
        (true, _, _, "0") => "zreset",
        (true, _, _, "q") => "quit",
        _ => return None,
    })
}

static PAGE_INPUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Note mouse, wheel and keyboard use inside a page: Sanaga then ticks fast
/// for a moment, and slowly the rest of the time (saves CPU when idle).
pub(super) fn watch_page_input(w: &PageView, tab: u64, tx: std::sync::mpsc::Sender<WebEvent>, ctx: egui::Context) {
    use gtk::prelude::*;
    let watch = gtk::EventControllerLegacy::new();
    watch.set_propagation_phase(gtk::PropagationPhase::Capture);
    watch.connect_event(move |_, ev| {
        use gtk::gdk::EventType as E;
        if ev.event_type() == E::ButtonPress {
            let _ = tx.send(WebEvent::PageClicked { tab });
            ctx.request_repaint();
        }
        if matches!(
            ev.event_type(),
            E::MotionNotify | E::ButtonPress | E::ButtonRelease | E::Scroll | E::KeyPress | E::KeyRelease | E::TouchBegin | E::TouchUpdate | E::TouchEnd | E::EnterNotify
        ) {
            PAGE_INPUT.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        gtk::glib::Propagation::Proceed
    });
    w.webview().add_controller(watch);
}

/// Load an address (asked for now by the person, or not: a restored or
/// woken tab), or HTML made by Sanaga (e.g. a page's source).
fn load_into(w: &PageView, url: &str, by_person: bool) {
    match url.strip_prefix(super::source::HTML_MARK) {
        Some(html) => w.webview().load_html(html, None),
        None => {
            let _ = w.load_url(url, by_person);
        }
    }
}

/// Makes Sanaga's pages, and the pages they open themselves (window.open):
/// those are linked to their opener and become new tabs (see adopt_popups).
#[derive(Clone)]
pub(super) struct PageMaker {
    ctx: Option<crate::page::Engine>,
    /// The profile's web data folder (containers keep theirs inside it).
    base: Option<std::path::PathBuf>,
    /// Sanaga's own scripts: they run in Sanaga's world, out of the page's reach.
    scripts: Rc<Vec<String>>,
    tx: std::sync::mpsc::Sender<WebEvent>,
    repaint: egui::Context,
    popups: Rc<RefCell<Vec<(u64, u64, PageView)>>>,
    ids: Rc<Cell<u64>>,
    /// The person's extensions: they act on the page itself (its world).
    page_scripts: Rc<Vec<String>>,
}

impl PageMaker {
    /// The engine of a container: its own cookies, logins and site data.
    pub(super) fn container_engine(&self, name: &str) -> Option<crate::page::Engine> {
        let base = self.base.as_ref()?;
        let safe: String = name.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
        let e = crate::page::profile_context(&base.join("containers").join(safe));
        crate::page::follow_downloads(&e);
        Some(e)
    }

    pub(super) fn make(&self, id: u64, private: bool, opener: Option<&webkit6::WebView>, container: &str) -> PageView {
        let me = self.clone();
        let cont = container.to_string();
        let popup: crate::page::PopupMaker = Rc::new(move |op: &webkit6::WebView| {
            let nid = me.ids.get();
            me.ids.set(nid + 1);
            let pv = me.make(nid, private, Some(op), &cont);
            let w = pv.webview();
            me.popups.borrow_mut().push((id, nid, pv));
            me.repaint.request_repaint();
            Some(w)
        });
        let (tx, repaint) = (self.tx.clone(), self.repaint.clone());
        let scripts: Vec<&str> = self.scripts.iter().map(|s| s.as_str()).collect();
        let page_scripts: Vec<&str> = self.page_scripts.iter().map(|s| s.as_str()).collect();
        let engine = if container.is_empty() || private { self.ctx.clone() } else { self.container_engine(container).or_else(|| self.ctx.clone()) };
        PageView::new(engine.as_ref(), private, &scripts, &page_scripts, opener, Some(popup), move |sig| {
            let ev = match sig {
                PageSignal::Started(url) => WebEvent::Load { tab: id, finished: false, url },
                PageSignal::Finished(url) => WebEvent::Load { tab: id, finished: true, url },
                PageSignal::Title(title) => WebEvent::Title { tab: id, title },
                PageSignal::Uri(url) => WebEvent::Uri { tab: id, url },
                PageSignal::NewWindow(url) => WebEvent::NewTab { tab: id, url },
                PageSignal::Close => WebEvent::CloseTab { tab: id },
                PageSignal::Crashed(memory) => WebEvent::Crashed { tab: id, memory },
                PageSignal::Tool(what, arg) => WebEvent::Tool { tab: id, what: what.to_string(), arg },
                PageSignal::Favicon(url, png) => {
                    // kept on disk for normal tabs only
                    crate::favicon::learn(&url, png, private);
                    repaint.request_repaint();
                    return;
                }
                PageSignal::OpenLink(url, how) => WebEvent::OpenLink { tab: id, url, how },
                PageSignal::LoadFailed(url, kind, detail) => WebEvent::LoadFailed { tab: id, url, kind, detail },
                PageSignal::Dialog(key) => WebEvent::Dialog { tab: id, key },
                PageSignal::Protected(url) => WebEvent::Protected { tab: id, url },
                PageSignal::Message(body) => {
                    if let Some((what, arg)) = body.strip_prefix("tool:").and_then(|r| r.split_once(':')) {
                        if !page_may_answer(what) {
                            return;
                        }
                        WebEvent::Tool { tab: id, what: what.to_string(), arg: arg.to_string() }
                    } else if let Some(rest) = body.strip_prefix("tr:") {
                        WebEvent::Translate { tab: id, msg: rest.to_string() }
                    } else if body == "form:dirty" || body == "form:clean" {
                        WebEvent::FormDirty { tab: id, dirty: body == "form:dirty" }
                    } else if let Some(text) = body.strip_prefix("sum:") {
                        WebEvent::PageText { tab: id, text: text.to_string() }
                    } else if let Some(share) = body.strip_prefix("scr:").and_then(|s| s.parse::<f32>().ok()) {
                        WebEvent::Scrolled { tab: id, share }
                    } else if body.starts_with("pw:") {
                        WebEvent::Password { tab: id, msg: body }
                    } else if let Some(u) = body.strip_prefix("vs:") {
                        WebEvent::ViewSource { tab: id, url: u.to_string() }
                    } else {
                        WebEvent::Shortcut(body)
                    }
                }
            };
            let _ = tx.send(ev);
            repaint.request_repaint();
        })
    }
}

/// Can WebKit be shown? Only inside Sanaga's GTK window (see shell.rs).
pub fn web_engine_available() -> bool {
    crate::shell::active()
}

impl Sanaga {
    /// Let WebKit do its work (it runs inside the same loop as the window).
    pub(super) fn pump_web(&mut self, ctx: &egui::Context) {
        if !self.web_ok {
            return;
        }
        if self.frame >= 2 {
            self.ensure_engine();
        }
        let n = 0;
        // pages opened by sites (sign-in windows…) become tabs
        if !self.popups.borrow().is_empty() {
            self.adopt_popups();
        }
        let mut got_event = false;
        while let Ok(ev) = self.web_rx.try_recv() {
            self.on_web_event(ev);
            got_event = true;
        }
        if self.tabs.iter().any(|t| t.web.is_some()) {
            // WebKit runs inside our loop: tick often while something happens
            // (page loading, mouse or keys in the page), rarely when idle.
            let now = ctx.input(|i| i.time);
            let activity = PAGE_INPUT.swap(false, std::sync::atomic::Ordering::Relaxed);
            // (a page playing sound needs no fast ticks: its icon is refreshed
            // every second)
            if activity || got_event || self.tabs.iter().any(|t| t.loading) {
                self.web_busy_until = now + 1.5;
            }
            // (GTK runs WebKit on its own: these ticks only refresh Sanaga's
            // own state, e.g. the loading bar and the sound icon)
            let idle = if self.low_power_active() { 3000 } else { 1000 };
            let wait = if self.tabs.iter().any(|t| t.loading) { 100 } else if now < self.web_busy_until { 250 } else { idle };
            if std::env::var_os("SANAGA_DEBUG_TICK").is_some() {
                eprintln!("tick t={now:.2} gtk_events={n} x={activity} web_event={got_event} loading={}", self.tabs.iter().any(|t| t.loading));
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(wait));
        }
    }

    fn on_web_event(&mut self, ev: WebEvent) {
        let addr_focused = self.ctx.memory(|m| m.has_focus(egui::Id::new("address-bar")));
        match ev {
            WebEvent::Load { tab, finished, url } => {
                let active_id = self.tabs.get(self.active).map(|t| t.id);
                let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab) else { return };
                if !matches!(t.view, View::Web) {
                    return;
                }
                t.loading = !finished;
                if !finished {
                    // a new page: nothing typed in it yet
                    t.form_dirty = false;
                    // its saved accounts may be offered again
                    if self.pw_fill_dismissed.as_ref().is_some_and(|(id, _)| *id == tab) {
                        self.pw_fill_dismissed = None;
                    }
                    // a translation belongs to the page it started on
                    if self.translating == Some(tab) {
                        self.translating = None;
                        self.translate_batches = 0;
                        self.translate_gen += 1;
                    }
                    // an error page's buttons belong to that page only: it is
                    // armed when it is committed itself (its secret goes to
                    // Sanaga's world there), and forgotten at the next commit
                    match self.error_pages.get_mut(&tab) {
                        Some(e) if !e.shown && e.url == url => {
                            e.shown = true;
                            e.token = super::errors::new_token();
                            if let Some(w) = &t.web {
                                let _ = w.evaluate_own(&format!("window.__sanagaErrorToken = '{}';", e.token));
                            }
                        }
                        Some(_) => {
                            self.error_pages.remove(&tab);
                        }
                        None => {}
                    }
                }
                if t.entries[t.idx].url.starts_with("view-source:") && url == "about:blank" {
                    return; // Sanaga's own source page: the address stays
                }
                if !(addr_focused && active_id == Some(tab)) {
                    t.address = url.clone();
                }
                let new_host = host_of(&url) != host_of(&t.entries[t.idx].url);
                t.entries[t.idx].url = url.clone();
                if new_host && !finished {
                    let ti = self.tabs.iter().position(|x| x.id == tab).unwrap_or(0);
                    self.apply_site_zoom(ti);
                    return;
                }
                if finished {
                    // mobile preview side by side: the phone goes where the
                    // page went, and hears about its scrolling again
                    if let Some(c) = self.companion_of(tab) {
                        self.sync_companion_scroll(tab);
                        if super::chrome::same_page_key(self.tabs[c].url()) != super::chrome::same_page_key(&url) && url.starts_with("http") {
                            self.navigate(c, url.clone(), None, true);
                        }
                    }
                    let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab) else { return };
                    self.pages_loaded += 1;
                    let private = t.private;
                    let title = t.entries[t.idx].title.clone();
                    let error_page = self.error_tabs.remove(&tab);
                    if !private && !url.starts_with("about:") && !error_page {
                        self.store.add_visit(&url, &title);
                        self.visited.insert(url);
                    }
                    self.save_session();
                }
            }
            WebEvent::Uri { tab, url } => {
                let active_id = self.tabs.get(self.active).map(|t| t.id);
                let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab) else { return };
                if !matches!(t.view, View::Web) || url.is_empty() || url == "about:blank" || t.entries.is_empty() {
                    return;
                }
                let e = &mut t.entries[t.idx];
                if e.url == url || e.url.starts_with("view-source:") {
                    return;
                }
                let new_host = host_of(&url) != host_of(&e.url);
                e.url = url.clone();
                let title = e.title.clone();
                if !(addr_focused && active_id == Some(tab)) {
                    t.address = url.clone();
                }
                // a move inside a single-page site is a visit too (as in
                // Chrome); a real load records its own when it finishes
                let loading = t.loading || t.web.as_ref().is_some_and(|w| w.webview().is_loading());
                let private = t.private;
                if !loading && !private && !url.starts_with("about:") {
                    self.store.add_visit(&url, &title);
                }
                // another site: its own zoom
                if new_host {
                    if let Some(ti) = self.tabs.iter().position(|x| x.id == tab) {
                        self.apply_site_zoom(ti);
                    }
                }
                self.save_session();
            }
            WebEvent::Title { tab, title } => {
                let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab) else { return };
                if title.trim().is_empty() {
                    return;
                }
                t.entries[t.idx].title = title.clone();
                let url = t.entries[t.idx].url.clone();
                if !t.private && !url.is_empty() {
                    // updates the title of the visit just recorded
                    self.store.add_visit(&url, &title);
                }
            }
            WebEvent::NewTab { tab, url } => {
                let (private, container) = self.tabs.iter().find(|t| t.id == tab).map(|t| (t.private, t.container.clone())).unwrap_or_default();
                let i = self.open_tab(&url, private);
                // a tab opened from a page goes under it (tree tabs), in its container
                self.tabs[i].parent = Some(tab);
                self.tabs[i].container = container;
                self.tabs[i].group = self.tabs.iter().find(|t| t.id == tab).map(|t| t.group).unwrap_or(0);
            }
            WebEvent::Tool { tab, what, arg } => self.page_tool(tab, &what, &arg),
            WebEvent::OpenLink { tab, url, how } => self.open_link_from(tab, &url, how),
            WebEvent::LoadFailed { tab, url, kind, detail } => self.show_load_error(tab, &url, kind, &detail),
            WebEvent::Dialog { tab, key } => self.page_dialog(tab, key),
            WebEvent::Protected { tab, url } => {
                if let Some(i) = self.tabs.iter().position(|t| t.id == tab) {
                    self.protected_site(i, &url);
                }
            }
            WebEvent::Password { tab, msg } => self.on_password_msg(tab, &msg),
            WebEvent::Crashed { tab, memory } => {
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab) {
                    t.loading = false;
                    let why = if memory {
                        tr("Elle utilisait trop de mémoire. Les autres onglets ne sont pas touchés.")
                    } else {
                        tr("Le moteur de la page s'est arrêté. Les autres onglets ne sont pas touchés.")
                    };
                    t.view = View::Error(format!("{CRASH_MARK}{why}"));
                }
            }
            WebEvent::Scrolled { tab, share } => {
                // mobile preview side by side: the phone follows the page
                if let Some(c) = self.companion_of(tab) {
                    if let Some(w) = &self.tabs[c].web {
                        let _ = w.evaluate_own(&format!("(function(){{var d=document.documentElement;window.scrollTo(0,{:.4}*Math.max(0,d.scrollHeight-window.innerHeight));}})();", share.clamp(0.0, 1.0)));
                    }
                }
            }
            WebEvent::PageClicked { tab } => {
                // split view: the page clicked becomes the tab in use
                if self.split.is_some_and(|(a, b)| tab == a || tab == b) {
                    if let Some(i) = self.tabs.iter().position(|t| t.id == tab) {
                        if i != self.active {
                            self.current_group = self.tabs[i].group;
                            self.active = i;
                        }
                    }
                }
            }
            WebEvent::FormDirty { tab, dirty } => {
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab) {
                    t.form_dirty = dirty;
                }
            }
            WebEvent::Translate { tab, msg } => self.translation_batch(tab, &msg),
            WebEvent::CloseTab { tab } => {
                if let Some(i) = self.tabs.iter().position(|t| t.id == tab) {
                    self.close_tab(i);
                }
            }
            WebEvent::Notice(msg) => self.notice = Some((msg, self.ctx.input(|i| i.time))),
            WebEvent::PageText { tab, text } => self.summary_text_arrived(tab, text),
            WebEvent::ViewSource { tab, url } => {
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return;
                }
                // only from Sanaga's own source page (Ctrl+U), never from web
                // content: the tab must show a view-source address, and its
                // view the page Sanaga made for it (about:blank, no site)
                let own_source_page = self
                    .tabs
                    .iter()
                    .find(|t| t.id == tab)
                    .is_some_and(|t| t.url().starts_with("view-source:") && t.web.as_ref().and_then(|w| w.uri()).as_deref() == Some("about:blank"));
                if !own_source_page {
                    return;
                }
                if let Some(i) = self.tabs.iter().position(|t| t.id == tab) {
                    self.navigate(i, format!("view-source:{url}"), None, true);
                }
            }
            WebEvent::Shortcut(msg) => {
                if std::env::var_os("SANAGA_DEBUG_MENU").is_some() {
                    eprintln!("page shortcut {msg:?} -> {:?}", shortcut_name(&msg));
                }
                if let Some(name) = shortcut_name(&msg) {
                    if name == "quit" {
                        self.quit_app();
                    } else {
                        self.run_shortcut(name);
                    }
                }
            }
            WebEvent::Privacy(list) => self.privacy = list,
            WebEvent::DownloadAsk { site } => {
                if !self.download_asks.contains(&site) {
                    self.download_asks.push(site);
                }
            }
            WebEvent::DownloadDone { url, path, ok } => {
                // (a download that never got a place: only its address is known)
                let path = path.unwrap_or_else(|| net::download_dir().join(net::safe_download_name(url.split(['?', '#']).next().and_then(|u| u.rsplit('/').next()).unwrap_or(""))));
                let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| tr("téléchargement").into());
                let error = if ok { String::new() } else { tr("le téléchargement n'a pas abouti").into() };
                self.store.downloads.push(store::Download { name, path, bytes, ok, time: store::now(), error });
                self.downloads_bar = true;
            }
        }
    }

    /// Create, place, show or hide the WebKit views to match the tabs.
    pub(super) fn sync_web(&mut self, content: Rect) {
        if !self.web_ok {
            return;
        }
        // the window's first picture comes before the web engine starts
        if self.web_context.is_none() {
            if self.frame < 2 {
                self.ctx.request_repaint();
                return;
            }
            self.ensure_engine();
        }
        let mut overlay = self.menu_open || self.suggest_open || self.share_open || self.overview_open || self.tab_search_open || self.bm_popover || self.bm_undo.is_some() || self.translate_ask || self.identity_ask || self.site_panel || self.container_ask.is_some() || self.overflow_open || self.switcher.is_some() || self.ai_menu_open || self.profile_open || self.group_editor.is_some() || self.shown_dialog().is_some() || egui::Popup::is_any_open(&self.ctx);
        // A menu or a panel over a page: the interface is drawn again in a
        // see-through layer above the page, which stays alive under it
        // (shell.rs). One frame after the menu opens: by then the interface
        // leaves the page area clear. The keyboard goes to the interface
        // meanwhile, and back to the page afterwards.
        let tab_live = self.tabs.get(self.active).is_some_and(|t| matches!(t.view, View::Web) && t.web.is_some() && t.web_shown);
        let live = overlay && !self.overview_open && self.device.is_none() && tab_live && crate::shell::live_overlay_available();
        let ready = live && self.live_overlay;
        if live != self.live_overlay {
            self.live_overlay = live;
            self.ctx.request_repaint();
        }
        crate::shell::set_live_overlay(ready);
        if ready != self.live_shown {
            self.live_shown = ready;
            if ready {
                crate::shell::focus_interface();
            } else if !self.ctx.memory(|m| m.focused().is_some()) {
                if let Some(w) = self.tabs.get(self.active).and_then(|t| t.web.as_ref()).filter(|_| tab_live) {
                    use gtk::prelude::WidgetExt;
                    w.webview().grab_focus();
                }
            }
        }
        if live {
            overlay = false;
        }
        // Where that layer can't be used (the mobile simulator, a system
        // without it): the page sits above Sanaga's drawing, so take a
        // picture of it first and show that picture while the menu is open
        // (instead of a blank page).
        let active_id = self.tabs.get(self.active).map(|t| t.id).unwrap_or(0);
        while let Ok((id, img)) = self.freeze_rx.try_recv() {
            if overlay && id == active_id && self.freeze_asked.is_some() {
                self.freeze = Some((id, self.ctx.load_texture("page-freeze", img, egui::TextureOptions::LINEAR)));
            }
        }
        if overlay && !self.overview_open {
            let tab = &self.tabs[self.active];
            let has_freeze = self.freeze.as_ref().is_some_and(|f| f.0 == active_id);
            if !has_freeze && matches!(tab.view, View::Web) && tab.web_shown {
                let now = self.ctx.input(|i| i.time);
                match self.freeze_asked {
                    None => {
                        // first the picture GTK already has (instant)
                        let quick = tab.web.as_ref().and_then(|w| crate::shell::widget_texture(&w.webview())).and_then(super::tabs::surface_to_full_image);
                        if let Some(img) = quick {
                            self.freeze = Some((active_id, self.ctx.load_texture("page-freeze", img, egui::TextureOptions::LINEAR)));
                            self.freeze_asked = Some(now);
                        } else if let Some(w) = &tab.web {
                            let (tx, ctx) = (self.freeze_tx.clone(), self.ctx.clone());
                            w.webview().snapshot(webkit6::SnapshotRegion::Visible, webkit6::SnapshotOptions::NONE, None::<&webkit6::gio::Cancellable>, move |res| {
                                if let Some(img) = res.ok().and_then(super::tabs::surface_to_full_image) {
                                    let _ = tx.send((active_id, img));
                                }
                                ctx.request_repaint();
                            });
                            self.freeze_asked = Some(now);
                            overlay = false;
                        }
                    }
                    // keep the page for a moment while its picture is being taken
                    Some(t) if now - t < 0.25 => {
                        overlay = false;
                        self.ctx.request_repaint_after(std::time::Duration::from_millis(16));
                    }
                    _ => {}
                }
            }
        } else if !overlay {
            self.freeze = None;
            self.freeze_asked = None;
        }
        // The page area, as distances from the window's edges: GTK keeps the
        // page there by itself while the window is resized (same frame).
        // mobile simulator: the page takes the device's screen size (scaled to
        // fit), with a mobile identity for phones and tablets
        if self.device_identity_stale() {
            self.apply_device_identity();
        }
        let dev_scale = self.device_scale(content);
        // (typing an address in the device: its frame steps aside for the field)
        self.device_overlay(content, overlay || self.device_url_edit.is_some());
        let content = match self.device_viewport(content) {
            Some(r) => r.translate(vec2(0.0, -1.0)),
            None => content,
        };
        let zf = self.ctx.zoom_factor();
        let win = self.ctx.content_rect();
        let insets = [
            (content.left() - win.left()) * zf,
            (content.top() + 1.0 - win.top()) * zf,
            (win.right() - content.right()) * zf,
            (win.bottom() - content.bottom()) * zf,
        ]
        .map(|v| v.round().max(0.0) as i32);
        crate::shell::set_page_area(insets);
        let active = self.active;
        // Split view: the tab in use and its companion side by side, each in
        // its half of the page area (the first of the pair on the left),
        // under a thin bar that says which one is in use.
        let partner = self.split_partner();
        self.split_shown = None;
        let mut halves: Option<([i32; 4], [i32; 4], usize)> = None;
        if let (Some(p), Some((left_id, _))) = (partner, self.split) {
            let (bar, gap) = (3.0, 2.0);
            // (a mobile preview: a phone's width on the right, the page keeps the rest)
            let phone = self.tabs[p].companion || self.tabs[active].companion;
            let mid = if phone { (content.right() - 393.0 - gap).max(content.left() + 320.0).round() } else { content.center().x.round() };
            let left = Rect::from_min_max(pos2(content.left(), content.top() + bar), pos2(mid - gap / 2.0, content.bottom()));
            let right = Rect::from_min_max(pos2(mid + gap / 2.0, content.top() + bar), content.right_bottom());
            let place = |r: Rect| [(r.left() - win.left()) * zf, (r.top() + 1.0 - win.top()) * zf, (win.right() - r.right()) * zf, (win.bottom() - r.bottom()) * zf].map(|v| v.round().max(0.0) as i32);
            let left_index = if self.tabs[active].id == left_id { active } else { p };
            halves = Some((place(left), place(right), left_index));
            self.split_shown = Some((left, right, left_index == active));
            // the companion's page starts if it was asleep
            let t = &mut self.tabs[p];
            if t.web.is_none() && t.web_pending.is_none() {
                t.sleeping = false;
                t.loading = true;
                t.web_pending = Some(t.url().to_string());
            }
        }
        for i in 0..self.tabs.len() {
            let show = (i == active || Some(i) == partner) && matches!(self.tabs[i].view, View::Web) && !overlay;
            if self.tabs[i].web.is_none() {
                // A tab's page only starts when the tab is shown (100 open tabs
                // don't mean 100 running pages).
                if i != active && Some(i) != partner {
                    if self.tabs[i].web_pending.is_some() {
                        self.tabs[i].loading = false;
                    }
                    continue;
                }
                let Some(url) = self.tabs[i].web_pending.take() else { continue };
                let by_person = std::mem::take(&mut self.tabs[i].web_by_person);
                let w = self.build_webview(i);
                // a mobile preview's page introduces itself as a phone, with touch
                if self.tabs[i].companion {
                    use webkit6::prelude::*;
                    let wk = w.webview();
                    if let Some(st) = WebViewExt::settings(&wk) {
                        st.set_user_agent(Some(super::devices::IOS_PHONE_UA));
                    }
                    if let Some(ucm) = wk.user_content_manager() {
                        ucm.add_script(&webkit6::UserScript::new(super::devices::TOUCH_JS, webkit6::UserContentInjectedFrames::AllFrames, webkit6::UserScriptInjectionTime::Start, &[], &[]));
                    }
                }
                self.prepare_view(i, &w, show);
                load_into(&w, &url, by_person);
                self.tabs[i].web = Some(w);
                self.tabs[i].web_shown = show;
                self.tabs[i].web_zoom = 1.0;
                continue;
            }
            let tab = &mut self.tabs[i];
            let Some(w) = &tab.web else { continue };
            if let Some(url) = tab.web_pending.take() {
                load_into(w, &url, std::mem::take(&mut tab.web_by_person));
            }
            if show != tab.web_shown {
                let _ = w.set_visible(show);
                tab.web_shown = show;
            }
            if show {
                let place = match halves {
                    Some((left, right, left_index)) => if i == left_index { left } else { right },
                    None => insets,
                };
                crate::shell::place_view(&w.webview(), place);
            }
            // in the simulator the page is scaled with the drawn device
            let want = tab.zoom * if i == active { dev_scale } else { 1.0 };
            if (want - tab.web_zoom).abs() > 0.001 {
                let _ = w.zoom(want as f64);
                tab.web_zoom = want;
            }
        }
    }

    /// A new page joins the window: placed, set up (privacy, sound, cache,
    /// permissions, input) and shown if its tab is.
    fn prepare_view(&mut self, i: usize, w: &PageView, show: bool) {
        let wk = w.webview();
        crate::shell::attach(&wk);
        let _ = w.set_visible(show);
        // Safari's "Prevent cross-site tracking"
        if let Some(s) = wk.network_session() {
            s.set_itp_enabled(self.store.settings.itp);
        }
        wk.set_is_muted(self.tabs[i].muted);
        // test aid (debug builds only): SANAGA_HWACCEL=never|always
        #[cfg(debug_assertions)]
        {
            if let Some(st) = WebViewExt::settings(&wk) {
                use webkit6::HardwareAccelerationPolicy as H;
                match std::env::var("SANAGA_HWACCEL").as_deref() {
                    Ok("never") => st.set_hardware_acceleration_policy(H::Never),
                    Ok("always") => st.set_hardware_acceleration_policy(H::Always),
                    _ => {}
                }
            }
        }
        // smaller caches: less memory per page
        if let Some(c) = wk.web_context() {
            c.set_cache_model(webkit6::CacheModel::DocumentBrowser);
        }
        super::plus::watch_permissions(w, self.tabs[i].id, self.perm_queue.clone(), self.ctx.clone());
        watch_page_input(w, self.tabs[i].id, self.web_tx.clone(), self.ctx.clone());
        self.run_pending_clear(w);
        // (the keyboard goes to the tab in use, not to its companion in a split view)
        if show && i == self.active {
            use gtk::prelude::WidgetExt;
            wk.grab_focus();
        }
    }

    /// Pages that sites opened with window.open (sign-in, payment…): each
    /// becomes a new tab next to its opener, already loading.
    pub(super) fn adopt_popups(&mut self) {
        let list: Vec<(u64, u64, PageView)> = self.popups.borrow_mut().drain(..).collect();
        for (opener, id, pv) in list {
            let (private, group, container) = self.tabs.iter().find(|t| t.id == opener).map(|t| (t.private, t.group, t.container.clone())).unwrap_or((false, 0, String::new()));
            let i = self.open_tab("sanaga://newtab", private);
            self.focus_address = false;
            let t = &mut self.tabs[i];
            t.id = id;
            t.group = group;
            t.parent = Some(opener);
            t.container = container;
            t.view = View::Web;
            t.loading = true;
            t.web_pending = None;
            t.address.clear();
            if let Some(e) = t.entries.get_mut(t.idx) {
                e.url = "about:blank".into();
            }
            self.prepare_view(i, &pv, true);
            let t = &mut self.tabs[i];
            t.web = Some(pv);
            t.web_shown = true;
            t.web_zoom = 1.0;
            // right after its opener in the tab row
            if let Some(o) = self.tabs.iter().position(|t| t.id == opener) {
                let to = (o + 1).min(self.tabs.len() - 1);
                self.move_tab(i, to);
            }
        }
    }

    /// Extensions switched on or off, or removed: every open page gets the
    /// new set at once (it reloads), not only the tabs opened afterwards.
    pub(super) fn refresh_extensions(&mut self) {
        let maker = self.page_maker();
        let own: Vec<&str> = maker.scripts.iter().map(|s| s.as_str()).collect();
        let page: Vec<&str> = maker.page_scripts.iter().map(|s| s.as_str()).collect();
        for t in &self.tabs {
            if let Some(w) = &t.web {
                w.set_scripts(&own, &page);
            }
        }
    }

    /// Switch an extension on or off.
    pub(super) fn toggle_extension(&mut self, id: &str) {
        if let Some(p) = self.store.extensions_on.iter().position(|x| x == id) {
            self.store.extensions_on.remove(p);
        } else {
            self.store.extensions_on.push(id.to_string());
        }
        self.store.save_extensions();
        self.refresh_extensions();
    }

    /// Remove one of the person's extensions (its file goes too).
    pub(super) fn delete_extension(&mut self, id: &str) {
        if let Some(name) = id.strip_prefix("fichier:") {
            let path = crate::extensions::folder().join(name);
            if path.parent() == Some(crate::extensions::folder().as_path()) {
                let _ = std::fs::remove_file(path);
            }
        }
        self.store.extensions_on.retain(|x| x != id);
        self.store.save_extensions();
        self.refresh_extensions();
        self.notice = Some((tr("Extension supprimée.").into(), self.ctx.input(|i| i.time)));
    }

    pub(super) fn page_maker(&self) -> PageMaker {
        let extensions = crate::extensions::init_script(&self.store.extensions_on);
        PageMaker {
            ctx: self.web_context.clone(),
            base: self.web_dir.clone(),
            scripts: Rc::new(vec![SHORTCUT_JS.to_string(), MEDIA_JS.to_string(), SOURCE_LINKS_JS.to_string(), ERROR_ACTIONS_JS.to_string(), super::accounts::PASSWORD_JS.to_string()]),
            tx: self.web_tx.clone(),
            repaint: self.ctx.clone(),
            popups: self.popups.clone(),
            ids: self.popup_ids.clone(),
            page_scripts: Rc::new(vec![extensions]),
        }
    }

    fn build_webview(&self, i: usize) -> PageView {
        let tab = &self.tabs[i];
        self.page_maker().make(tab.id, tab.private, None, &tab.container)
    }

    /// Find in a WebKit page (uses the page's own search).
    pub(super) fn web_find(&self, backwards: bool) {
        if let Some(w) = self.tabs.get(self.active).and_then(|t| t.web.as_ref()) {
            let q = self.find_query.replace('\\', "\\\\").replace('\'', "\\'");
            let _ = w.evaluate_own(&format!("window.find('{q}', false, {backwards}, true, false, false, false);"));
        }
    }
}
