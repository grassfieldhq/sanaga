//! Page tools and window comforts: focus mode, the window border, low power
//! mode, floating video, removing a part of a page, editing a page's text,
//! copy unlocked, the archived version of a page, link previews and keeping
//! the window above the others.

use super::*;
use crate::site_prefs::SitePrefs;
use eframe::egui::Color32;
use gtk::prelude::WidgetExt;

/// The video of the page (the one playing, else the largest) and what is
/// needed to keep it playing elsewhere.
const PIP_JS: &str = r#"(function(){var vs=[].slice.call(document.querySelectorAll('video'));
if(!vs.length){window.ipc.postMessage('tool:pip-info:');return;}
function score(v){var r=v.getBoundingClientRect();return (v.paused?0:1e9)+r.width*r.height;}
vs.sort(function(a,b){return score(b)-score(a)});var v=vs[0];window.__sanagaPip=v;
window.ipc.postMessage('tool:pip-info:'+[v.currentSrc||v.src||'',v.currentTime||0,v.videoWidth||16,v.videoHeight||9].join('\u001f'));})();"#;

/// The video floats in a corner of the page (for streams that can't leave it).
const PIP_IN_PAGE_JS: &str = r#"(function(){var v=window.__sanagaPip;if(!v)return;
v.style.setProperty('position','fixed','important');v.style.setProperty('right','16px','important');v.style.setProperty('bottom','16px','important');
v.style.setProperty('left','auto','important');v.style.setProperty('top','auto','important');v.style.setProperty('width','380px','important');
v.style.setProperty('height','auto','important');v.style.setProperty('z-index','2147483647','important');
v.style.setProperty('box-shadow','0 10px 34px rgba(0,0,0,.45)','important');v.style.setProperty('border-radius','12px','important');
if(v.paused)v.play();})();"#;

/// Point at a part of the page, click: it is removed (and stays removed on
/// this site). Escape stops.
const ZAP_JS: &str = r#"(function(TIP){if(window.__sanagaTool)return;window.__sanagaTool='zap';
var box=document.createElement('div');box.style.cssText='position:fixed;pointer-events:none;z-index:2147483647;border:2px solid #e5484d;background:rgba(229,72,77,.15);border-radius:4px';
var tip=document.createElement('div');tip.textContent=TIP;tip.style.cssText='position:fixed;top:12px;left:50%;transform:translateX(-50%);z-index:2147483647;background:#222;color:#fff;font:13px system-ui,sans-serif;padding:8px 14px;border-radius:8px;pointer-events:none;box-shadow:0 4px 14px rgba(0,0,0,.3)';
document.documentElement.appendChild(box);document.documentElement.appendChild(tip);var cur=null;
function ok(c){return /^[A-Za-z_][\w-]*$/.test(c)&&!/\d{4,}/.test(c);}
function sel(el){var parts=[];while(el&&el.nodeType===1&&el!==document.body&&el!==document.documentElement&&parts.length<8){
if(el.id&&ok(el.id)){parts.unshift('#'+el.id);break;}var p=el.tagName.toLowerCase();var cls=[].slice.call(el.classList).filter(ok).slice(0,2);
if(cls.length)p+='.'+cls.join('.');var par=el.parentElement;if(par&&!cls.length){var same=[].slice.call(par.children).filter(function(x){return x.tagName===el.tagName});
if(same.length>1)p+=':nth-of-type('+(same.indexOf(el)+1)+')';}parts.unshift(p);el=par;}
if(el===document.body&&parts.length&&parts[0].charAt(0)!=='#')parts.unshift('body');return parts.join(' > ');}
function mv(e){var t=e.target;if(t===box||t===tip)return;cur=t;var r=t.getBoundingClientRect();box.style.left=r.left+'px';box.style.top=r.top+'px';box.style.width=r.width+'px';box.style.height=r.height+'px';}
function stop(){window.__sanagaTool=null;window.__sanagaStopTool=null;document.removeEventListener('mousemove',mv,true);document.removeEventListener('click',ck,true);box.remove();tip.remove();}
function ck(e){e.preventDefault();e.stopPropagation();if(!cur)return;var s=sel(cur);cur.style.setProperty('display','none','important');if(s)window.ipc.postMessage('tool:zap-done:'+s);stop();}
window.__sanagaStopTool=stop;document.addEventListener('mousemove',mv,true);document.addEventListener('click',ck,true);})"#;

const STOP_TOOL_JS: &str = "if(window.__sanagaStopTool)window.__sanagaStopTool();if(document.designMode==='on'){document.designMode='off';}window.__sanagaTool=null;";

use crate::guard::plausible_selector;

fn is_web(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// YouTube video id of a page address.
fn youtube_id(page: &str) -> Option<String> {
    let u = url::Url::parse(page).ok()?;
    let host = u.host_str()?.trim_start_matches("www.").trim_start_matches("m.");
    let id = match host {
        "youtube.com" | "music.youtube.com" => {
            if let Some(v) = u.query_pairs().find(|(k, _)| k == "v").map(|(_, v)| v.to_string()) {
                v
            } else {
                let mut seg = u.path_segments()?;
                match seg.next()? {
                    "shorts" | "embed" | "live" => seg.next()?.to_string(),
                    _ => return None,
                }
            }
        }
        "youtu.be" => u.path_segments()?.next()?.to_string(),
        _ => return None,
    };
    id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_').then_some(id).filter(|i| !i.is_empty())
}

/// Colour of the window border at `t` (0..1 around the window).
fn hue(h: f32) -> Color32 {
    let h = h.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    // softened: pleasant, not neon
    let m = |v: f32| (60.0 + v * 175.0) as u8;
    Color32::from_rgb(m(r), m(g), m(b))
}

impl Sanaga {
    // ------------------------------------------------------------ focus mode

    /// Focus mode: the page alone in the window, nothing else (F9).
    pub(super) fn toggle_focus_mode(&mut self) {
        self.focus_mode = !self.focus_mode;
        self.menu_open = false;
        if self.focus_mode {
            self.notice = Some((tr("Mode focus — F9 pour revenir.").into(), self.ctx.input(|i| i.time)));
        }
    }

    // ------------------------------------------------------------ border

    /// The border around the window (Settings › Appearance): drawn, and its width returned.
    pub(super) fn draw_border(&mut self, ui: &mut Ui, now: f64) -> f32 {
        let st = &self.store.settings;
        if st.border == 0 || self.fullscreen || self.app_mode {
            return 0.0;
        }
        let w = st.border_size.clamp(1, 16) as f32;
        let alpha = (st.border_alpha.clamp(10, 100) as f32 / 100.0 * 255.0) as u8;
        let accent = crate::theme::theme(st.dark, false).accent;
        let animated = st.border == 3 && !self.low_power_active();
        // the four corners' colours: top-left, top-right, bottom-right, bottom-left
        let corners: [Color32; 4] = match st.border {
            1 => [accent; 4],
            2 => {
                let other = hue(0.08 + crate::theme::accent_in_use() as f32 * 0.13);
                [accent, other, accent, other]
            }
            _ => {
                let t = if animated { (now * 0.08) as f32 } else { 0.0 };
                [hue(t), hue(t + 0.25), hue(t + 0.5), hue(t + 0.75)]
            }
        };
        let corners = corners.map(|c| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha));
        let r = ui.max_rect();
        let i = r.shrink(w);
        let mut mesh = egui::Mesh::default();
        // outer 0..4, inner 4..8 (same corner order)
        for p in [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), i.left_top(), i.right_top(), i.right_bottom(), i.left_bottom()].iter().enumerate() {
            mesh.colored_vertex(*p.1, corners[p.0 % 4]);
        }
        for k in 0..4u32 {
            let n = (k + 1) % 4;
            mesh.add_triangle(k, n, 4 + n);
            mesh.add_triangle(k, 4 + n, 4 + k);
        }
        ui.painter().add(egui::Shape::mesh(mesh));
        if animated {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(66));
        }
        w
    }

    // ------------------------------------------------------------ low power

    /// Low power mode is on now (always, or on battery).
    pub(super) fn low_power_active(&self) -> bool {
        match self.store.settings.low_power {
            2 => true,
            1 => self.on_battery.1,
            _ => false,
        }
    }

    /// Check from time to time whether the computer runs on its battery.
    pub(super) fn power_step(&mut self, now: f64) {
        if now - self.on_battery.0 < 30.0 {
            return;
        }
        let was = self.on_battery.1;
        let on = on_battery();
        self.on_battery = (now, on);
        if on && !was && self.store.settings.low_power == 1 && now > 5.0 {
            self.notice = Some((tr("Sur batterie : mode économie d'énergie (onglets inactifs mis en veille plus tôt).").into(), now));
        }
    }

    // ------------------------------------------------------------ page tools

    fn tab_index(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    fn page_of(&self, id: u64) -> Option<&crate::page::PageView> {
        self.tabs.iter().find(|t| t.id == id).and_then(|t| t.web.as_ref())
    }

    fn active_page(&self) -> Option<&crate::page::PageView> {
        self.tabs.get(self.active).and_then(|t| t.web.as_ref()).filter(|_| matches!(self.tabs[self.active].view, View::Web))
    }

    /// A tool asked from a page's right-click menu, or a tool's answer.
    pub(super) fn page_tool(&mut self, tab: u64, what: &str, arg: &str) {
        if std::env::var_os("SANAGA_DEBUG_MENU").is_some() {
            eprintln!("page tool {what} {arg}");
        }
        // a tool the person asked for in a page shows that page; messages a
        // page sends by itself (a video it can't play, an answer, the error
        // page's buttons) never pull a background tab to the front
        if let Some(i) = self.tab_index(tab) {
            if !matches!(what, "zap-done" | "pip-info" | "media-unsupported" | "probe" | "safe" | "tls-allow") {
                self.active = i;
            }
        }
        match what {
            "preview" => self.preview_link(arg),
            "container-link" => self.container_ask = Some(arg.to_string()),
            // the error page's buttons: only while that tab shows Sanaga's
            // error page for this very failure (a certificate one, for this
            // host, to go on despite it)
            "tls-allow" => {
                if self.error_pages.get(&tab).is_some_and(|(k, h)| *k == crate::page::LoadError::Certificate && h == arg) {
                    self.error_pages.remove(&tab);
                    self.accept_certificate(tab, arg);
                }
            }
            "safe" => {
                if self.error_pages.remove(&tab).is_some() {
                    if let Some(i) = self.tab_index(tab) {
                        let back = self.tabs[i].web.as_ref().is_some_and(|w| w.can_go_back().unwrap_or(false));
                        if back {
                            self.go_history(i, -1);
                        } else {
                            self.go_home(i);
                        }
                    }
                }
            }
            // (from the page's right-click menu: web addresses only)
            "private-link" => {
                let page = self.tabs.iter().find(|t| t.id == tab).map(|t| t.url().to_string()).unwrap_or_default();
                if crate::guard::link_may_open_elsewhere(arg, &page) {
                    self.open_tab(arg, true);
                    self.focus_address = false;
                }
            }
            "pip" => self.start_pip(),
            "pip-info" => self.pip_info(tab, arg),
            "zap" => self.start_zapper(),
            "zap-done" => self.zapped(tab, arg),
            "edit-text" => self.toggle_edit_text(),
            "site-settings" => self.site_panel = true,
            "media-unsupported" => self.codec_flag = Some(tab),
            // test aid: what a page answered (SANAGA_LIVE_TEST probe:)
            "probe" if cfg!(debug_assertions) => eprintln!("live probe {arg}"),
            _ => {}
        }
    }

    /// A link opened from a page with Ctrl / middle / Shift: a tab behind
    /// (0) or in front (1), under the page it came from, in its container
    /// and private if the page is; or a new window (2).
    pub(super) fn open_link_from(&mut self, tab: u64, url: &str, how: u8) {
        let Some(from) = self.tabs.iter().position(|t| t.id == tab) else { return };
        let (private, container, group) = (self.tabs[from].private, self.tabs[from].container.clone(), self.tabs[from].group);
        // a new window has the profile's data: from a private tab or a
        // container, the link opens in a tab of the same kind instead
        let how = if how == 2 && (private || !container.is_empty()) { 1 } else { how };
        if how == 2 {
            self.new_window(vec![url.to_string()]);
            return;
        }
        let shown_id = self.tabs[self.active].id;
        let i = self.open_tab(url, private);
        self.focus_address = false;
        self.tabs[i].parent = Some(tab);
        self.tabs[i].container = container;
        self.tabs[i].group = group;
        // next to the page it came from, after the links already opened from it
        let mut to = from + 1;
        while to < self.tabs.len() - 1 && self.tabs[to].parent == Some(tab) && to != i {
            to += 1;
        }
        let to = to.min(self.tabs.len() - 1);
        if to != i {
            self.move_tab(i, to);
        }
        if how == 0 {
            // a tab behind: the page shown stays
            if let Some(k) = self.tabs.iter().position(|t| t.id == shown_id) {
                self.active = k;
            }
        }
        self.save_session();
    }

    /// Escape: stop the tool running in the page. True when one was running.
    pub(super) fn stop_page_tool(&mut self) -> bool {
        let editing = self.editing_text.take();
        self.zap_armed = None;
        if let Some(w) = self.active_page() {
            let _ = w.evaluate_own(STOP_TOOL_JS);
        }
        if let Some(id) = editing {
            if let Some(w) = self.page_of(id) {
                let _ = w.evaluate_own(STOP_TOOL_JS);
            }
            self.notice = Some((tr("Modification du texte terminée.").into(), self.ctx.input(|i| i.time)));
            return true;
        }
        false
    }

    /// Floating video: the page's video in a small window above the others.
    pub(super) fn start_pip(&mut self) {
        let tab = self.tabs.get(self.active).map(|t| t.id);
        match self.active_page() {
            Some(w) => {
                let _ = w.evaluate_own(PIP_JS);
                // its answer is awaited from this tab only
                self.pip_armed = tab;
            }
            None => self.notice = Some((tr("Aucune vidéo dans cette page.").into(), self.ctx.input(|i| i.time))),
        }
    }

    fn pip_info(&mut self, tab: u64, arg: &str) {
        // only the answer to the person's own request (menu ⋮, right-click)
        if self.pip_armed.take() != Some(tab) {
            return;
        }
        let now = self.ctx.input(|i| i.time);
        if arg.is_empty() {
            self.notice = Some((tr("Aucune vidéo dans cette page.").into(), now));
            return;
        }
        let mut f = arg.split('\u{1f}');
        let src = f.next().unwrap_or("").to_string();
        let t: f64 = f.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let vw: f64 = f.next().and_then(|v| v.parse().ok()).filter(|v: &f64| *v > 0.0).unwrap_or(16.0);
        let vh: f64 = f.next().and_then(|v| v.parse().ok()).filter(|v: &f64| *v > 0.0).unwrap_or(9.0);
        let page_url = self.tabs.iter().find(|x| x.id == tab).map(|x| x.url().to_string()).unwrap_or_default();
        let (w, h) = (480, ((480.0 * vh / vw) as i32).clamp(200, 720));
        let engine = self.tab_engine();
        let title = tr("Sanaga — vidéo flottante");
        let opened = if let Some(id) = youtube_id(&page_url) {
            let url = format!("https://www.youtube-nocookie.com/embed/{id}?autoplay=1&start={}", t as u64);
            crate::shell::float_window(engine.as_ref(), title, Some(&url), None, w, h, None, true)
        } else if is_web(&src) || src.starts_with("file://") {
            let esc = src.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;");
            let html = format!("<!doctype html><html><body style=\"margin:0;background:#000;overflow:hidden\"><video src=\"{esc}#t={t:.1}\" autoplay controls style=\"width:100vw;height:100vh;object-fit:contain\"></video></body></html>");
            crate::shell::float_window(engine.as_ref(), title, None, Some((&html, &page_url)), w, h, None, true)
        } else {
            false
        };
        if let Some(p) = self.page_of(tab) {
            if opened {
                let _ = p.evaluate_own("if(window.__sanagaPip)window.__sanagaPip.pause();");
            } else {
                // a stream that only plays inside its page: it floats in the page
                let _ = p.evaluate_own(PIP_IN_PAGE_JS);
                self.notice = Some((tr("Cette vidéo ne peut pas quitter sa page : elle flotte dans un coin pendant que vous lisez.").into(), now));
            }
        }
    }

    /// Remove a part of the page: point at it, click.
    pub(super) fn start_zapper(&mut self) {
        let tab = self.tabs.get(self.active).map(|t| t.id);
        if let Some(w) = self.active_page() {
            let tip = crate::keyring::js_str(tr("Cliquez sur ce qu'il faut retirer de la page — Échap pour annuler"));
            let _ = w.evaluate_own(&format!("{ZAP_JS}({tip});"));
            // the element picked is awaited from this tab only
            self.zap_armed = tab;
            if let Some(p) = self.active_page() {
                p.webview().grab_focus();
            }
        }
    }

    fn zapped(&mut self, tab: u64, selector: &str) {
        // only after the person started the element remover on this tab
        if self.zap_armed.take() != Some(tab) {
            return;
        }
        let Some(url) = self.tabs.iter().find(|t| t.id == tab).map(|t| t.url().to_string()) else { return };
        let host = crate::site_prefs::key(&host_of(&url));
        if host.is_empty() || !plausible_selector(selector) {
            return;
        }
        let mut prefs = crate::site_prefs::for_host(&host);
        if !prefs.hidden.iter().any(|s| s == selector) {
            prefs.hidden.push(selector.to_string());
        }
        self.save_site(&host, prefs, false);
        self.notice = Some((tf("Élément retiré de {} (aussi les prochaines fois). Pour le revoir : Réglages de ce site.", &[&host]), self.ctx.input(|i| i.time)));
    }

    /// Edit the page's text (for a screenshot or a mock-up); Escape ends it.
    pub(super) fn toggle_edit_text(&mut self) {
        let Some(id) = self.tabs.get(self.active).map(|t| t.id) else { return };
        if self.editing_text == Some(id) {
            self.stop_page_tool();
            return;
        }
        if let Some(w) = self.active_page() {
            let _ = w.evaluate_own("document.designMode='on';window.__sanagaTool='edit';");
            w.webview().grab_focus();
            self.editing_text = Some(id);
            self.notice = Some((tr("Modifiez le texte directement dans la page. Échap pour terminer ; tout revient au rechargement.").into(), self.ctx.input(|i| i.time)));
        }
    }

    /// Let this site's text be selected and copied (and keep it so).
    pub(super) fn unlock_copy(&mut self) {
        let url = self.tabs[self.active].url().to_string();
        if !is_web(&url) {
            return;
        }
        let host = crate::site_prefs::key(&host_of(&url));
        let mut prefs = crate::site_prefs::for_host(&host);
        prefs.unlock = !prefs.unlock;
        let on = prefs.unlock;
        self.save_site(&host, prefs, true);
        let msg = if on { tr("Copier-coller débloqué sur ce site.") } else { tr("Copier-coller rendu au site.") };
        self.notice = Some((msg.into(), self.ctx.input(|i| i.time)));
    }

    /// The page as kept by the Internet Archive (gone, changed, or not loading).
    pub(super) fn open_archive(&mut self) {
        let url = self.tabs[self.active].url().to_string();
        if is_web(&url) {
            self.open_tab(&format!("https://web.archive.org/web/{url}"), self.tabs[self.active].private);
        }
    }

    /// Keep the window above the others (with focus mode: a reference page
    /// next to your work).
    pub(super) fn toggle_on_top(&mut self) {
        let want = !self.on_top;
        if crate::desktop::keep_on_top(want, &crate::shell::window_title()) {
            self.on_top = want;
            let msg = if want { tr("La fenêtre reste au premier plan.") } else { tr("La fenêtre ne reste plus au premier plan.") };
            self.notice = Some((msg.into(), self.ctx.input(|i| i.time)));
        } else {
            self.notice = Some((tr("Votre bureau ne permet pas de garder une fenêtre au premier plan.").into(), self.ctx.input(|i| i.time)));
        }
    }

    /// A link previewed in a small window, without leaving the page.
    pub(super) fn preview_link(&mut self, url: &str) {
        if !is_web(url) && !url.starts_with("file://") {
            return;
        }
        if !self.web_ok {
            self.open_tab(url, false);
            return;
        }
        let engine = self.tab_engine();
        crate::shell::float_window(engine.as_ref(), tr("Sanaga — aperçu"), Some(url), None, 920, 680, Some(tr("Ouvrir dans un onglet")), false);
    }

    /// The engine of the tab shown: a private tab's small windows keep
    /// nothing, a container's keep its cookies and logins.
    fn tab_engine(&self) -> Option<crate::page::Engine> {
        let t = self.tabs.get(self.active)?;
        if t.private {
            return Some(crate::page::private_engine(self.web_context.as_ref()));
        }
        if !t.container.is_empty() {
            if let Some(e) = self.page_maker().container_engine(&t.container) {
                return Some(e);
            }
        }
        self.web_context.clone()
    }

    /// Change one site's settings, keep them, and show them on its open pages.
    pub(super) fn save_site(&mut self, host: &str, prefs: SitePrefs, reload: bool) {
        let host = crate::site_prefs::key(host);
        let mut list = crate::site_prefs::all();
        list.retain(|(h, _)| *h != host);
        if !prefs.is_default() {
            list.push((host.clone(), prefs));
        }
        self.store.save_site_prefs(&list);
        if reload {
            for t in &self.tabs {
                if let Some(w) = &t.web {
                    if crate::site_prefs::key(&host_of(t.url())) == host {
                        let _ = w.reload();
                    }
                }
            }
        }
    }
}

/// Does the computer run on its battery now?
fn on_battery() -> bool {
    let Ok(rd) = std::fs::read_dir("/sys/class/power_supply") else { return false };
    let mut battery_discharging = false;
    for e in rd.flatten() {
        let p = e.path();
        let kind = std::fs::read_to_string(p.join("type")).unwrap_or_default();
        if kind.trim() == "Mains" && std::fs::read_to_string(p.join("online")).is_ok_and(|v| v.trim() == "1") {
            return false;
        }
        if kind.trim() == "Battery" && std::fs::read_to_string(p.join("status")).is_ok_and(|v| v.trim() == "Discharging") {
            battery_discharging = true;
        }
    }
    battery_discharging
}

/// Where Sanaga's icons are installed for this user (packaging/install.sh).
fn icon_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    let data = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
    Some(data.join("icons/hicolor"))
}

impl Sanaga {
    /// The application's icon in another colour: Sanaga's own drawing, and
    /// the installed launcher icons (so the dock and app menu follow).
    pub(super) fn set_app_icon(&mut self, style: u8) {
        self.store.settings.app_icon = style;
        self.store.save_settings();
        crate::icons::ICON_STYLE.store(style, std::sync::atomic::Ordering::Relaxed);
        let now = self.ctx.input(|i| i.time);
        let Some(dir) = icon_dir() else { return };
        let svg = dir.join("scalable/apps/sanaga.svg");
        if !svg.exists() {
            self.notice = Some((tr("Icône changée dans Sanaga. Installez Sanaga (packaging/install.sh) pour la voir aussi dans vos applications.").into(), now));
            return;
        }
        let _ = std::fs::write(&svg, crate::icons::logo_svg(style as usize, ""));
        for s in [48u32, 64, 128, 256, 512] {
            let png = dir.join(format!("{s}x{s}/apps/sanaga.png"));
            if png.exists() {
                let icon = crate::icons::window_icon(s);
                let _ = image::save_buffer(&png, &icon.rgba, icon.width, icon.height, image::ColorType::Rgba8);
            }
        }
        let _ = std::process::Command::new("gtk-update-icon-cache").args(["-q", "-t"]).arg(&dir).status();
        self.notice = Some((tr("Icône changée. Le dock et le menu des applications la montrent au prochain démarrage.").into(), now));
    }

    /// This profile in the applications menu: its own launcher, with its
    /// emoji on Sanaga's icon.
    pub(super) fn add_profile_launcher(&mut self) {
        let now = self.ctx.input(|i| i.time);
        let profile = self.store.profile.clone();
        let (Some(home), Ok(exe)) = (std::env::var_os("HOME").map(std::path::PathBuf::from), std::env::current_exe()) else { return };
        let data = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
        let Some(pdir) = self.store.data_dir().cloned() else { return };
        let emoji = self.store.settings.profile_emoji.trim().to_string();
        let icon = pdir.join("launcher-icon.svg");
        let _ = std::fs::write(&icon, crate::icons::logo_svg(self.store.settings.app_icon as usize, &emoji));
        let name = if profile.is_empty() { "Sanaga".to_string() } else { format!("Sanaga — {profile}") };
        let safe: String = profile.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
        let file = data.join("applications").join(if profile.is_empty() { "sanaga-main.desktop".to_string() } else { format!("sanaga-profile-{safe}.desktop") });
        // quoted and escaped as the Desktop Entry specification says
        let args = if profile.is_empty() { String::new() } else { format!(" --profile {}", crate::desktop::exec_arg(&profile)) };
        let shown = if emoji.is_empty() { name.clone() } else { format!("{name} {emoji}") };
        let text = format!(
            "[Desktop Entry]\nType=Application\nName={}\nComment={}\nExec={}{args} %U\nIcon={}\nCategories=Network;WebBrowser;\nStartupNotify=true\n",
            crate::desktop::desktop_value(&shown),
            crate::desktop::desktop_value(tr("Navigateur web Sanaga, avec les données de ce profil")),
            crate::desktop::exec_arg(&exe.to_string_lossy()),
            crate::desktop::desktop_value(&icon.display().to_string())
        );
        let _ = std::fs::create_dir_all(file.parent().unwrap_or(&data));
        match std::fs::write(&file, text) {
            Ok(()) => self.notice = Some((tf("« {} » ajouté à vos applications.", &[&name]), now)),
            Err(e) => self.notice = Some((e.to_string(), now)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_youtube_ids() {
        assert_eq!(youtube_id("https://www.youtube.com/watch?v=abc_DEF-123&t=4").as_deref(), Some("abc_DEF-123"));
        assert_eq!(youtube_id("https://youtu.be/xyz987").as_deref(), Some("xyz987"));
        assert_eq!(youtube_id("https://m.youtube.com/shorts/q1w2e3").as_deref(), Some("q1w2e3"));
        assert_eq!(youtube_id("https://example.com/watch?v=abc"), None);
    }
}
