//! One web page shown by WebKit, as a normal part of Sanaga's GTK window
//! (no separate window glued on top any more: GTK draws the page and the rest
//! of the window together, in the same frame).

use gtk::prelude::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use webkit2gtk::{
    CookieManagerExt, CookiePersistentStorage, DownloadExt, LoadEvent, SettingsExt, URIRequestExt, UserContentInjectedFrames, UserContentManager,
    UserContentManagerExt, UserScript, UserScriptInjectionTime, WebContext, WebContextExt, WebInspectorExt, WebView, WebViewExt, WebsiteDataManager, WebsiteDataManagerExt,
};

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
}

/// Makes the page a site opens with `window.open` (sign-in, payment windows):
/// it gets the opener's page as its related view, so the two can talk to each
/// other as on any browser. Returns the new page's widget.
pub type PopupMaker = Rc<dyn Fn(&WebView) -> Option<WebView>>;

/// The engine settings shared by the pages of one profile (cookies, cache,
/// site data are kept in `dir`).
pub fn profile_context(dir: &Path) -> WebContext {
    let _ = std::fs::create_dir_all(dir);
    let dm = WebsiteDataManager::builder().base_cache_directory(dir.to_string_lossy()).base_data_directory(dir.to_string_lossy()).build();
    if let Some(cm) = dm.cookie_manager() {
        cm.set_persistent_storage(&dir.join("cookies").to_string_lossy(), CookiePersistentStorage::Text);
    }
    let ctx = WebContext::builder().website_data_manager(&dm).build();
    secure_context(&ctx);
    ctx
}

/// Spell checking, and pages walled off from the rest of the computer
/// (bubblewrap sandbox, as in GNOME Web) when the system provides it.
pub fn secure_context(ctx: &WebContext) {
    enable_spell_checking(ctx);
    let has = |tool: &str| std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(tool).is_file()));
    if has("bwrap") && has("xdg-dbus-proxy") && std::env::var_os("SANAGA_NO_SANDBOX").is_none() {
        ctx.set_sandbox_enabled(true);
    }
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
pub fn watch_downloads(ctx: &WebContext, pick: impl Fn(&str) -> PathBuf + 'static, done: impl Fn(String, Option<PathBuf>, bool) + 'static) {
    let pick = Rc::new(pick);
    let done = Rc::new(done);
    ctx.connect_download_started(move |_, download| {
        let failed = Rc::new(RefCell::new(false));
        let dest: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
        let (pick, d2) = (pick.clone(), dest.clone());
        download.connect_decide_destination(move |download, _suggested| {
            let uri = download.request().and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
            let path = pick(&uri);
            download.set_destination(&glib_file_uri(&path));
            *d2.borrow_mut() = Some(path);
            true
        });
        let f2 = failed.clone();
        download.connect_failed(move |_, _| *f2.borrow_mut() = true);
        let done = done.clone();
        download.connect_finished(move |download| {
            let uri = download.request().and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
            done(uri, dest.borrow().clone(), !*failed.borrow());
        });
    });
}

fn glib_file_uri(p: &Path) -> String {
    gtk::glib::filename_to_uri(p, None).map(|s| s.to_string()).unwrap_or_else(|_| format!("file://{}", p.display()))
}

/// Script that gives pages `window.ipc.postMessage(...)` (Sanaga's page scripts use it).
const IPC_JS: &str = "Object.defineProperty(window, 'ipc', { value: Object.freeze({ postMessage: function(x) { window.webkit.messageHandlers['ipc'].postMessage(x) } }) })";

pub struct PageView {
    view: WebView,
}

impl PageView {
    /// A new page. `scripts` run at the start of every page (top frame).
    /// `opener`: the page that opened this one (its windows share the engine).
    /// `popups`: makes the pages this one opens; without it they open as new tabs.
    pub fn new(ctx: Option<&WebContext>, private: bool, scripts: &[&str], opener: Option<&WebView>, popups: Option<PopupMaker>, on: impl Fn(PageSignal) + 'static) -> PageView {
        let ucm = UserContentManager::new();
        for js in std::iter::once(IPC_JS).chain(scripts.iter().copied()) {
            ucm.add_script(&UserScript::new(js, UserContentInjectedFrames::TopFrame, UserScriptInjectionTime::Start, &[], &[]));
        }
        let on = Rc::new(on);
        let o = on.clone();
        ucm.connect_script_message_received(Some("ipc"), move |_, msg| {
            if let Some(v) = msg.js_value() {
                o(PageSignal::Message(v.to_string()));
            }
        });
        ucm.register_script_message_handler("ipc");
        let mut b = WebView::builder().user_content_manager(&ucm);
        match opener {
            // same engine process and data as the opener
            Some(o) => b = b.related_view(o),
            None => {
                b = b.is_ephemeral(private);
                if let Some(c) = ctx {
                    b = b.web_context(c);
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
        if let Some(c) = view.context() {
            c.set_use_system_appearance_for_scrollbars(false);
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
        // a page that closes itself (window.close(), allowed by WebKit only
        // for windows a script opened): its tab closes
        let o = on.clone();
        view.connect_close(move |_| o(PageSignal::Close));
        PageView { view }
    }

    pub fn webview(&self) -> WebView {
        self.view.clone()
    }
    pub fn load_url(&self, url: &str) -> R<()> {
        self.view.load_uri(url);
        Ok(())
    }
    #[allow(deprecated)]
    pub fn evaluate_script(&self, js: &str) -> R<()> {
        self.view.run_javascript(js, None::<&webkit2gtk::gio::Cancellable>, |_| {});
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
        let op = webkit2gtk::PrintOperation::new(&self.view);
        let parent = self.view.toplevel().and_then(|t| t.downcast::<gtk::Window>().ok());
        webkit2gtk::PrintOperationExt::run_dialog(&op, parent.as_ref());
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
        if let Some(p) = self.view.parent().and_then(|p| p.downcast::<gtk::Container>().ok()) {
            p.remove(&self.view);
        }
        unsafe { self.view.destroy() };
    }
}
