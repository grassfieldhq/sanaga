//! Tabs, navigation, back/forward, forms, network results and keyboard shortcuts.

use super::*;
use crate::html::{Block, FieldKind};
use crate::store;
use eframe::egui::{KeyboardShortcut, Modifiers};

const IMAGE_BUDGET: usize = 96 * 1024 * 1024;

fn strip_fragment(u: &str) -> &str {
    u.split('#').next().unwrap_or(u)
}

impl Sanaga {
    pub(super) fn open_tab(&mut self, url: &str, private: bool) -> usize {
        self.overview_open = false;
        self.tab_search_open = false;
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab {
            id,
            private,
            entries: Vec::new(),
            idx: 0,
            view: View::Internal(Internal::NewTab),
            nav: 0,
            loading: false,
            address: String::new(),
            forms: Vec::new(),
            heights: Vec::new(),
            layout_key: (0, 0),
            scroll_to: None,
            target_block: None,
            restore_scroll: None,
            scroll_y: 0.0,
            zoom: self.store.settings.zoom,
            full: false,
            web: None,
            web_pending: None,
            web_shown: false,
            web_zoom: 1.0,
            reader: false,
            reader_skip: Vec::new(),
            pinned: false,
            // a new tab joins the group shown (Safari's tab groups)
            group: self.current_group,
            muted: false,
            audio: false,
            thumb: None,
            last_active: 0.0,
            sleeping: false,
            form_dirty: false,
            lite_pending: None,
            container: String::new(),
            parent: None,
            tree_folded: false,
        });
        let i = self.tabs.len() - 1;
        self.active = i;
        self.navigate(i, url.to_string(), None, true);
        if url.starts_with("sanaga://newtab") {
            self.focus_address = true;
        }
        i
    }

    /// Open in a background tab (the current tab stays in front).
    pub(super) fn open_background(&mut self, url: &str, private: bool) {
        let cur = self.active;
        self.open_tab(url, private);
        self.active = cur;
        self.focus_address = false;
    }

    pub(super) fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        // like Chrome: closing the shown tab shows its right neighbour (else the
        // left one) in the same group; closing another tab keeps the shown one
        let active_id = self.tabs.get(self.active).map(|t| t.id);
        let closing_active = i == self.active;
        let order = self.visible_tabs();
        let neighbour = order.iter().position(|&x| x == i).and_then(|p| order.get(p + 1).or(if p > 0 { order.get(p - 1) } else { None })).map(|&x| self.tabs[x].id);
        let t = self.tabs.remove(i);
        if !t.private && !t.entries.is_empty() {
            self.closed.push(t.url().to_string());
        }
        let want = if closing_active { neighbour } else { active_id };
        match want.and_then(|id| self.tabs.iter().position(|t| t.id == id)) {
            Some(k) => self.active = k,
            None => {
                self.active = 0;
                self.open_tab("sanaga://newtab", false);
            }
        }
        self.save_session();
    }

    pub(super) fn save_session(&self) {
        if self.screenshot.is_some() || self.app_mode {
            return;
        }
        // one line per tab: address, group name, pinned, title, container, and
        // the place of the tab it was opened from (tree tabs)
        let kept: Vec<&Tab> = self.tabs.iter().filter(|t| !t.private && !t.entries.is_empty()).collect();
        let urls: Vec<String> = kept
            .iter()
            .map(|t| {
                let parent = t.parent.and_then(|p| kept.iter().position(|k| k.id == p)).map(|k| k.to_string()).unwrap_or_default();
                format!("{}\t{}\t{}\t{}\t{}\t{}", t.url(), self.groups.get(t.group).map(|g| g.replace('\t', " ")).unwrap_or_default(), if t.pinned { "1" } else { "0" }, t.title().replace(['\t', '\n'], " "), t.container.replace('\t', " "), parent)
            })
            .collect();
        self.store.save_session(self.slot, &urls);
    }

    /// Go to `url` in tab `ti`. `push` adds a history step (false = reload / back / forward).
    pub(super) fn navigate(&mut self, ti: usize, url: String, post: Option<String>, push: bool) {
        let url = super::english_address(&url);
        self.suggest_open = false;
        self.menu_open = false;
        let is_web_url = url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://");
        let use_web = self.web_ok && self.store.settings.webkit && is_web_url && post.is_none();
        let Some(tab) = self.tabs.get_mut(ti) else { return };
        // a new page leaves the Reader; the Reader always uses the light engine
        if push {
            tab.reader = false;
            let host = host_of(&url);
            tab.zoom = self.store.site_zoom.get(&host).copied().unwrap_or(self.store.settings.zoom);
        }
        let use_web = use_web && !tab.reader;
        let url = if url == "about:blank" { "sanaga://newtab".to_string() } else { url };
        // Inside a WebKit page, WebKit keeps the back/forward list itself.
        let push = push && !(use_web && matches!(tab.view, View::Web));
        if push {
            if let Some(e) = tab.entries.get_mut(tab.idx) {
                e.scroll = tab.scroll_y;
            }
            // Same page, different #anchor: just scroll.
            if post.is_none() && url.contains('#') {
                if let View::Doc(d) = &tab.view {
                    if strip_fragment(&d.url) == strip_fragment(&url) {
                        let doc = d.clone();
                        let frag = url.split('#').nth(1).unwrap_or("").to_string();
                        tab.entries.truncate(tab.idx + 1);
                        tab.entries.push(Entry { url: url.clone(), title: doc.title.clone(), scroll: 0.0, doc: Some(doc.clone()) });
                        tab.idx = tab.entries.len() - 1;
                        tab.address = url;
                        if let Some(&b) = doc.anchors.get(&frag) {
                            tab.target_block = Some((b, 4));
                        }
                        return;
                    }
                }
            }
            if !tab.entries.is_empty() {
                tab.entries.truncate(tab.idx + 1);
            }
            tab.entries.push(Entry { url: url.clone(), title: String::new(), scroll: 0.0, doc: None });
            tab.idx = tab.entries.len() - 1;
            tab.full = false;
        } else if let Some(e) = tab.entries.get_mut(tab.idx) {
            e.url = url.clone();
        }
        tab.nav += 1;
        tab.loading = false;
        tab.target_block = None;
        tab.address = if url.starts_with("sanaga://newtab") { String::new() } else { url.clone() };

        if let Some(i) = internal_from_url(&url) {
            tab.view = View::Internal(i);
            tab.scroll_to = Some(0.0);
            return;
        }
        if url.trim_end_matches('/') == "sanaga://welcome" {
            let doc = Arc::new(crate::html::parse(welcome_html(), "sanaga://welcome"));
            let (id, nav) = (tab.id, tab.nav);
            self.on_page(id, nav, Ok(doc));
            return;
        }
        if let Some(inner) = url.strip_prefix("view-source:") {
            // page source: taken from the open page when there is one (same
            // content as shown, logged-in pages too), else asked to the site
            tab.loading = true;
            tab.view = View::Loading;
            let (id, nav) = (tab.id, tab.nav);
            use webkit6::prelude::*;
            let open_page = self.tabs.iter().filter_map(|t| t.web.as_ref()).map(|w| w.webview()).find(|w| w.uri().map(|u| u.as_str() == inner).unwrap_or(false));
            if let Some(res) = open_page.and_then(|w| w.main_resource()) {
                let (tx, ctx, u) = (self.tx.clone(), self.ctx.clone(), inner.to_string());
                res.data(None::<&webkit6::gio::Cancellable>, move |r| {
                    let result = r.map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|e| e.to_string());
                    let _ = tx.send(NetMsg::Source { tab: id, nav, url: u, result });
                    ctx.request_repaint();
                });
                return;
            }
            let Some(tab) = self.tabs.get_mut(ti) else { return };
            let agent = if tab.private { self.private_agent.clone() } else { self.agent.clone() };
            net::fetch_page(agent, net::Request { tab: tab.id, nav: tab.nav, url: inner.to_string(), post_body: None, full: true, source: true }, self.tx.clone(), self.ctx.clone());
            return;
        }
        if use_web {
            tab.view = View::Web;
            tab.loading = true;
            tab.web_pending = Some(url);
            return;
        }
        if !is_web_url {
            tab.view = View::Error(tf("Sanaga ne sait pas ouvrir ce type d'adresse : {}", &[&url]));
            return;
        }
        tab.loading = true;
        if !matches!(tab.view, View::Doc(_)) {
            tab.view = View::Loading;
        }
        let agent = if tab.private { self.private_agent.clone() } else { self.agent.clone() };
        let req = net::Request { tab: tab.id, nav: tab.nav, url, post_body: post, full: tab.full && !tab.reader, source: false };
        if ti != self.active || self.restoring {
            // background tab: the page is fetched when the tab is shown
            tab.loading = false;
            tab.lite_pending = Some(req);
            return;
        }
        net::fetch_page(agent, req, self.tx.clone(), self.ctx.clone());
    }

    pub(super) fn on_page(&mut self, tab_id: u64, nav: u64, result: Result<Arc<Document>, String>) {
        let Some(ti) = self.tabs.iter().position(|t| t.id == tab_id) else { return };
        let tab = &mut self.tabs[ti];
        if tab.nav != nav {
            return;
        }
        tab.loading = false;
        match result {
            Ok(doc) => {
                self.pages_loaded += 1;
                self.bytes_loaded += doc.bytes as u64;
                let e = &mut tab.entries[tab.idx];
                let frag = e.url.split_once('#').map(|(_, f)| f.to_string());
                e.url = match &frag {
                    Some(f) if !doc.url.contains('#') => format!("{}#{}", doc.url, f),
                    _ => doc.url.clone(),
                };
                e.title = doc.title.clone();
                e.doc = Some(doc.clone());
                tab.address = e.url.clone();
                tab.forms = doc.forms.clone();
                tab.heights.clear();
                tab.scroll_to = Some(tab.restore_scroll.take().unwrap_or(0.0));
                if let Some(f) = frag.and_then(|f| doc.anchors.get(&f).copied()) {
                    tab.target_block = Some((f, 4));
                }
                tab.view = View::Doc(doc.clone());
                // Keep memory low: only pages next to the current one stay in memory.
                let idx = tab.idx;
                for (i, en) in tab.entries.iter_mut().enumerate() {
                    if (i as isize - idx as isize).abs() > 2 {
                        en.doc = None;
                    }
                }
                let url = tab.entries[idx].url.clone();
                if !tab.private {
                    self.store.add_visit(&url, &doc.title);
                    self.visited.insert(url);
                }
                self.save_session();
            }
            Err(msg) => tab.view = View::Error(msg),
        }
    }

    pub(super) fn go_history(&mut self, ti: usize, d: isize) {
        let Some(tab) = self.tabs.get_mut(ti) else { return };
        if let (View::Web, Some(w)) = (&tab.view, &tab.web) {
            if d < 0 && w.can_go_back().unwrap_or(false) {
                let _ = w.go_back();
                return;
            }
            if d > 0 && w.can_go_forward().unwrap_or(false) {
                let _ = w.go_forward();
                return;
            }
        }
        let ni = tab.idx as isize + d;
        if ni < 0 || ni as usize >= tab.entries.len() {
            return;
        }
        tab.entries[tab.idx].scroll = tab.scroll_y;
        tab.idx = ni as usize;
        let e = &tab.entries[tab.idx];
        let url = e.url.clone();
        let scroll = e.scroll;
        if let Some(doc) = e.doc.clone() {
            tab.nav += 1;
            tab.loading = false;
            tab.forms = doc.forms.clone();
            tab.heights.clear();
            tab.view = View::Doc(doc);
            tab.scroll_to = Some(scroll);
            tab.address = url;
        } else {
            tab.restore_scroll = Some(scroll);
            self.navigate(ti, url, None, false);
        }
    }

    pub(super) fn reload(&mut self, ti: usize) {
        if let Some(tab) = self.tabs.get_mut(ti) {
            if tab.entries.is_empty() {
                return;
            }
            if let (View::Web, Some(w)) = (&tab.view, &tab.web) {
                let _ = w.reload();
                return;
            }
            tab.restore_scroll = Some(tab.scroll_y);
            let url = tab.url().to_string();
            self.navigate(ti, url, None, false);
        }
    }

    pub(super) fn stop(&mut self, ti: usize) {
        if let Some(tab) = self.tabs.get_mut(ti) {
            if let (View::Web, Some(w)) = (&tab.view, &tab.web) {
                let _ = w.evaluate_script("window.stop();");
            }
            tab.nav += 1;
            tab.loading = false;
            if matches!(tab.view, View::Loading) {
                tab.view = View::Error(tr("Chargement interrompu.").into());
            }
        }
    }

    pub(super) fn go_home(&mut self, ti: usize) {
        let h = self.store.settings.home.clone();
        self.navigate(ti, h, None, true);
    }

    pub(super) fn submit(&mut self, ti: usize, form: usize, button: Option<(String, String)>) {
        let Some(tab) = self.tabs.get(ti) else { return };
        let Some(f) = tab.forms.get(form) else { return };
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        for field in &f.fields {
            if field.name.is_empty() || (field.kind == FieldKind::Checkbox && !field.checked) {
                continue;
            }
            ser.append_pair(&field.name, &field.value);
        }
        if let Some((n, v)) = button {
            if !n.is_empty() {
                ser.append_pair(&n, &v);
            }
        }
        let query = ser.finish();
        if f.post {
            let action = f.action.clone();
            self.navigate(ti, action, Some(query), true);
        } else if let Ok(mut u) = url::Url::parse(&f.action) {
            u.set_fragment(None);
            u.set_query(if query.is_empty() { None } else { Some(&query) });
            self.navigate(ti, u.to_string(), None, true);
        }
    }

    pub(super) fn poll_network(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                NetMsg::Page { tab, nav, result } => self.on_page(tab, nav, result),
                NetMsg::Source { tab, nav, url, result } => {
                    let web_ok = self.web_ok;
                    let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab && t.nav == nav) else { continue };
                    match result {
                        Ok(text) if web_ok => {
                            // shown by WebKit: colours, line numbers, wrapping
                            t.view = View::Web;
                            t.loading = true;
                            t.entries[t.idx].title = format!("view-source:{url}");
                            t.web_pending = Some(format!("{}{}", super::source::HTML_MARK, super::source::page(&url, &text)));
                        }
                        Ok(text) => {
                            let mut d = crate::html::plain(&text, &url);
                            d.title = tf("Source de {}", &[&url]);
                            self.on_page(tab, nav, Ok(Arc::new(d)));
                        }
                        Err(e) => self.on_page(tab, nav, Err(e)),
                    }
                }
                NetMsg::Image { url, result } => {
                    let slot = match result {
                        Ok(img) => {
                            let size = vec2(img.size[0] as f32, img.size[1] as f32);
                            self.img_bytes += img.size[0] * img.size[1] * 4;
                            let tex = self.ctx.load_texture(&url, img, egui::TextureOptions::LINEAR);
                            ImgSlot::Ready(tex, size, self.frame)
                        }
                        Err(_) => ImgSlot::Failed,
                    };
                    self.images.insert(url, slot);
                    self.trim_images();
                }
                NetMsg::Download { tab, nav, name, path, bytes, result } => {
                    if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab) {
                        if t.nav == nav {
                            t.loading = false;
                            // the page we were on stays; drop the empty history step
                            if t.entries.len() > 1 && t.entries[t.idx].doc.is_none() {
                                t.entries.remove(t.idx);
                                t.idx = t.idx.saturating_sub(1).min(t.entries.len() - 1);
                                t.address = t.url().to_string();
                                if let Some(d) = t.entries[t.idx].doc.clone() {
                                    t.view = View::Doc(d);
                                } else if let Some(i) = internal_from_url(t.url()) {
                                    t.view = View::Internal(i);
                                }
                            } else if matches!(t.view, View::Loading) {
                                t.view = View::Internal(Internal::Downloads);
                            }
                        }
                    }
                    let (ok, error) = match result {
                        Ok(()) => (true, String::new()),
                        Err(e) => (false, e),
                    };
                    self.store.downloads.push(store::Download { name, path, bytes, ok, time: store::now(), error });
                    self.downloads_bar = true;
                }
            }
        }
    }

    /// Free the least recently drawn pictures when they use too much memory.
    fn trim_images(&mut self) {
        while self.img_bytes > IMAGE_BUDGET {
            let oldest = self
                .images
                .iter()
                .filter_map(|(k, v)| if let ImgSlot::Ready(_, _, f) = v { Some((k.clone(), *f)) } else { None })
                .min_by_key(|(_, f)| *f);
            let Some((k, _)) = oldest else { break };
            if let Some(ImgSlot::Ready(_, s, _)) = self.images.remove(&k) {
                self.img_bytes = self.img_bytes.saturating_sub((s.x * s.y * 4.0) as usize);
            }
        }
    }

    pub(super) fn request_image(&mut self, url: String, private: bool) {
        if self.images.contains_key(&url) {
            return;
        }
        self.images.insert(url.clone(), ImgSlot::Loading);
        let agent = if private { self.private_agent.clone() } else { self.agent.clone() };
        net::fetch_image(agent, url, self.tx.clone(), self.ctx.clone());
    }

    pub(super) fn zoom_by(&mut self, d: f32) {
        let t = &mut self.tabs[self.active];
        t.zoom = (((t.zoom + d) * 10.0).round() / 10.0).clamp(0.5, 3.0);
        self.remember_zoom();
    }

    pub(super) fn toggle_bookmark(&mut self) {
        let t = &self.tabs[self.active];
        if t.entries.is_empty() || t.url().starts_with("sanaga://newtab") {
            return;
        }
        let (u, ti) = (t.url().to_string(), t.title());
        self.store.toggle_bookmark(&u, &ti);
    }

    pub(super) fn update_find(&mut self) {
        let tab = &self.tabs[self.active];
        let View::Doc(doc) = &tab.view else {
            self.find_hits.clear();
            return;
        };
        let q = if self.find_open { self.find_query.to_lowercase() } else { String::new() };
        let key = (q.clone(), tab.id * 1_000_000 + tab.nav, doc.blocks.len());
        if key == self.find_key {
            return;
        }
        self.find_key = key;
        self.find_hits.clear();
        self.find_pos = 0;
        if q.is_empty() {
            return;
        }
        for (i, b) in doc.blocks.iter().enumerate() {
            let text = match b {
                Block::Heading(_, s) | Block::Para(s) | Block::Quote(s) | Block::ListItem { spans: s, .. } => Document::plain_text(s),
                Block::Pre(t) => t.clone(),
                Block::Image { alt, .. } => alt.clone(),
                _ => continue,
            };
            let n = text.to_lowercase().matches(&q).count();
            self.find_hits.extend(std::iter::repeat_n(i, n));
        }
        if let Some(&b) = self.find_hits.first() {
            self.tabs[self.active].target_block = Some((b, 4));
        }
    }

    pub(super) fn shortcuts(&mut self, ui: &mut Ui) {
        let sc = |m: Modifiers, k: Key| KeyboardShortcut::new(m, k);
        let cs = Modifiers::COMMAND | Modifiers::SHIFT;
        let c = Modifiers::COMMAND;
        let mut todo: Vec<&'static str> = Vec::new();
        ui.input_mut(|i| {
            let list = [
                (sc(cs, Key::N), "private"),
                (sc(cs, Key::T), "reopen"),
                (sc(cs, Key::B), "bmbar"),
                (sc(cs, Key::R), "reader"),
                (sc(cs, Key::Backslash), "overview"),
                (sc(cs, Key::O), "overview"),
                (sc(cs, Key::F), "contacts"),
                (sc(cs, Key::A), "tabsearch"),
                (sc(cs, Key::Delete), "cleardata"),
                (sc(c, Key::S), "savepage"),
                (sc(cs, Key::L), "sidebar"),
                (sc(Modifiers::COMMAND | Modifiers::ALT, Key::I), "inspector"),
                (sc(cs, Key::I), "inspector"),
                (sc(Modifiers::NONE, Key::F12), "inspector"),
                (sc(cs, Key::M), "devices"),
                (sc(cs, Key::Y), "summary"),
                (sc(Modifiers::NONE, Key::F9), "focus"),
                (sc(c, Key::P), "print"),
                (sc(c, Key::U), "source"),
                (sc(Modifiers::NONE, Key::F11), "fullscreen"),
                (sc(cs, Key::D), "readinglist"),
                (sc(cs, Key::Tab), "prev"),
                (sc(c, Key::T), "new"),
                (sc(c, Key::Comma), "settings"),
                (sc(c, Key::N), "window"),
                (sc(c, Key::W), "close"),
                (sc(c, Key::Tab), "next"),
                (sc(c, Key::PageDown), "tabright"),
                (sc(c, Key::PageUp), "tableft"),
                (sc(c, Key::L), "addr"),
                (sc(Modifiers::NONE, Key::F6), "addr"),
                (sc(c, Key::R), "reload"),
                (sc(Modifiers::NONE, Key::F5), "reload"),
                (sc(Modifiers::ALT, Key::ArrowLeft), "back"),
                (sc(Modifiers::ALT, Key::ArrowRight), "fwd"),
                (sc(Modifiers::ALT, Key::Home), "home"),
                (sc(c, Key::D), "bookmark"),
                (sc(c, Key::H), "history"),
                (sc(c, Key::J), "downloads"),
                (sc(c, Key::F), "find"),
                (sc(c, Key::Plus), "zin"),
                (sc(c, Key::Equals), "zin"),
                (sc(c, Key::Minus), "zout"),
                (sc(c, Key::Num0), "zreset"),
            ];
            for (s, name) in list {
                if i.consume_shortcut(&s) {
                    todo.push(name);
                }
            }
            if i.key_pressed(Key::Escape) {
                todo.push("esc");
            }
        });
        for t in todo {
            self.run_shortcut(t);
        }
    }

    pub(super) fn run_shortcut(&mut self, t: &str) {
        {
            let a = self.active.min(self.tabs.len() - 1);
            match t {
                "private" => {
                    self.open_tab("sanaga://newtab", true);
                }
                "reopen" => {
                    if let Some(u) = self.closed.pop() {
                        self.open_tab(&u, false);
                    }
                }
                "bmbar" => {
                    self.store.settings.show_bookmarks_bar = !self.store.settings.show_bookmarks_bar;
                    self.store.save_settings();
                }
                "new" => {
                    self.open_tab("sanaga://newtab", false);
                }
                "window" => self.new_window(Vec::new()),
                "settings" => {
                    self.open_tab("sanaga://settings", false);
                }
                "translate" => self.translate_page(),
                "close" => self.close_tab(a),
                // Ctrl+Tab: the tabs used last, as pictures (or the tab bar's order)
                "next" | "prev" if self.store.settings.tab_switcher => self.switch_tabs(if t == "next" { 1 } else { -1 }),
                "next" | "tabright" => self.tab_bar_step(true),
                "prev" | "tableft" => self.tab_bar_step(false),
                "addr" => self.focus_address = true,
                "reload" => self.reload(a),
                "back" => self.go_history(a, -1),
                "fwd" => self.go_history(a, 1),
                "home" => self.go_home(a),
                "bookmark" => {
                    // Ctrl+D: as a click on the star (add, then name it in the bubble)
                    let star = self.ctx.data(|d| d.get_temp::<Rect>(egui::Id::new("star-rect"))).unwrap_or(Rect::NOTHING);
                    self.star_clicked(star);
                }
                "reader" => self.toggle_reader(a),
                "overview" => self.toggle_overview(),
                "contacts" => self.fill_contacts(),
                "tabsearch" => {
                    self.tab_search_open = !self.tab_search_open;
                    self.tab_search_q.clear();
                }
                "savepage" => self.save_page(),
                "cleardata" => {
                    self.open_tab("sanaga://clear", false);
                }
                "sidebar" => self.sidebar_open = !self.sidebar_open,
                "focus" => self.toggle_focus_mode(),
                "container" => self.container_ask = Some(String::new()),
                "ontop" => self.toggle_on_top(),
                "pip" => self.start_pip(),
                "zap" => self.start_zapper(),
                "edittext" => self.toggle_edit_text(),
                "unlock" => self.unlock_copy(),
                "archive" => self.open_archive(),
                "site" => self.site_panel = true,
                "inspector" => self.open_inspector(),
                "summary" => self.summarize_page(),
                "devices" => self.toggle_devices(),
                "print" => self.print_page(),
                "source" => self.view_source(),
                "fullscreen" => self.toggle_fullscreen(),
                "readinglist" => self.toggle_reading_list(),
                "history" => {
                    self.open_tab("sanaga://history", false);
                }
                "downloads" => {
                    self.open_tab("sanaga://downloads", false);
                }
                "find" => {
                    self.find_open = true;
                    self.focus_find = true;
                }
                "zin" => self.zoom_by(0.1),
                "zout" => self.zoom_by(-0.1),
                "zreset" => {
                    let z = self.store.settings.zoom;
                    self.tabs[a].zoom = z;
                    self.remember_zoom();
                }
                "esc" => {
                    if self.stop_page_tool() {
                    } else if self.focus_mode && !self.menu_open {
                        self.toggle_focus_mode();
                    } else if self.tab_search_open {
                        self.tab_search_open = false;
                    } else if self.overview_open || self.share_open {
                        self.overview_open = false;
                        self.share_open = false;
                    } else if self.menu_open {
                        self.menu_open = false;
                    } else if self.suggest_open {
                        self.suggest_open = false;
                    } else if self.find_open {
                        self.find_open = false;
                    } else if self.tabs[a].loading {
                        self.stop(a);
                    }
                }
                _ => {}
            }
        }
    }

    /// Reader button: show this page as clean text, or go back to the normal page.
    pub(super) fn toggle_reader(&mut self, ti: usize) {
        let Some(tab) = self.tabs.get_mut(ti) else { return };
        if tab.entries.is_empty() {
            return;
        }
        let url = tab.url().to_string();
        if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://")) {
            return;
        }
        tab.reader = !tab.reader;
        tab.restore_scroll = Some(0.0);
        self.navigate(ti, url, None, false);
    }

    /// Add the current page to the reading list (or take it out).
    pub(super) fn toggle_reading_list(&mut self) {
        let t = &self.tabs[self.active];
        if t.entries.is_empty() || t.url().starts_with("sanaga://") {
            return;
        }
        let (u, ti) = (t.url().to_string(), t.title());
        self.store.toggle_reading(&u, &ti);
    }

    pub(super) fn apply_go(&mut self, go: Option<(String, bool)>) {
        if let Some((u, new)) = go {
            if new {
                self.open_background(&u, false);
            } else {
                self.navigate(self.active, u, None, true);
            }
        }
    }
}
