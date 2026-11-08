//! Features found in Chrome that complete Sanaga: memory saver (sleeping
//! tabs), tab search, site permissions, clearing browsing data, saving a page,
//! the page's QR code, address-bar shortcuts and calculator.

use super::chrome::site_badge;
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align, Align2, Color32, Layout, Margin, RichText, Sense, TextEdit};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use gtk::glib::prelude::*;
use webkit6::prelude::*;

/// A site asking for the camera, microphone, location or notifications.
pub(super) struct PermAsk {
    pub tab: u64,
    pub site: String,
    pub kind: &'static str,
    pub req: webkit6::PermissionRequest,
}

pub(super) type PermQueue = Rc<RefCell<VecDeque<PermAsk>>>;

pub(super) fn kind_label(kind: &str) -> &'static str {
    match kind {
        "camera" => tr("votre caméra"),
        "micro" => tr("votre micro"),
        "camera+micro" => tr("votre caméra et votre micro"),
        "location" => tr("votre position"),
        "notifications" => tr("vous envoyer des notifications"),
        _ => tr("un accès"),
    }
}

/// Listen to WebKit's permission requests for one tab.
pub(super) fn watch_permissions(w: &crate::page::PageView, tab: u64, queue: PermQueue, ctx: egui::Context) {
    w.webview().connect_permission_request(move |wv, req| {
        let kind: &'static str = if let Some(m) = req.downcast_ref::<webkit6::UserMediaPermissionRequest>() {
            match (m.is_for_video_device(), m.is_for_audio_device()) {
                (true, true) => "camera+micro",
                (true, false) => "camera",
                (false, true) => "micro",
                _ => return false,
            }
        } else if req.is::<webkit6::GeolocationPermissionRequest>() {
            "location"
        } else if req.is::<webkit6::NotificationPermissionRequest>() {
            "notifications"
        } else {
            return false; // anything else: WebKit's default (refused)
        };
        let site = wv.uri().map(|u| {
            let u = u.to_string();
            url::Url::parse(&u).ok().map(|p| p.origin().ascii_serialization()).unwrap_or(u)
        });
        queue.borrow_mut().push_back(PermAsk { tab, site: site.unwrap_or_default(), kind, req: req.clone() });
        ctx.request_repaint();
        true
    });
}

/// Address-bar shortcuts: "w sanaga", "yt musique", "m Douala"...
pub fn keyword_search(q: &str) -> Option<(String, String)> {
    let (k, rest) = q.trim().split_once(' ')?;
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    let enc: String = url::form_urlencoded::byte_serialize(rest.as_bytes()).collect();
    let (base, name) = match k.to_lowercase().as_str() {
        "w" | "wiki" => (wiki_search(), "Wikipédia"),
        "yt" => ("https://www.youtube.com/results?search_query=", "YouTube"),
        "g" => ("https://www.google.com/search?q=", "Google"),
        "b" => ("https://www.bing.com/search?q=", "Bing"),
        "dd" => ("https://html.duckduckgo.com/html/?q=", "DuckDuckGo"),
        "m" | "carte" => ("https://www.openstreetmap.org/search?query=", "OpenStreetMap"),
        "tr" => (translate_text(), tr("Traduction")),
        "gh" => ("https://github.com/search?q=", "GitHub"),
        _ => return None,
    };
    Some((format!("{base}{enc}"), name.to_string()))
}

fn wiki_search() -> &'static str {
    match crate::i18n::language() {
        "en" => "https://en.wikipedia.org/w/index.php?search=",
        "es" => "https://es.wikipedia.org/w/index.php?search=",
        "pt" => "https://pt.wikipedia.org/w/index.php?search=",
        "de" => "https://de.wikipedia.org/w/index.php?search=",
        _ => "https://fr.wikipedia.org/w/index.php?search=",
    }
}

fn translate_text() -> &'static str {
    match crate::i18n::language() {
        "en" => "https://translate.google.com/?sl=auto&tl=en&text=",
        "es" => "https://translate.google.com/?sl=auto&tl=es&text=",
        "pt" => "https://translate.google.com/?sl=auto&tl=pt&text=",
        "de" => "https://translate.google.com/?sl=auto&tl=de&text=",
        _ => "https://translate.google.com/?sl=auto&tl=fr&text=",
    }
}

/// A small calculator for the address bar: "12*(3+4)", "2^10", "15% * 80".
pub fn calculate(q: &str) -> Option<f64> {
    let s: String = q.trim().trim_start_matches('=').replace(',', ".").replace('×', "*").replace('÷', "/").chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() || !s.chars().any(|c| "+-*/^%".contains(c)) || !s.chars().all(|c| c.is_ascii_digit() || "+-*/^%.()".contains(c)) {
        return None;
    }
    let b = s.as_bytes();
    let mut i = 0;
    let v = expr(b, &mut i)?;
    (i == b.len() && v.is_finite()).then_some(v)
}

fn expr(b: &[u8], i: &mut usize) -> Option<f64> {
    let mut v = term(b, i)?;
    while *i < b.len() && (b[*i] == b'+' || b[*i] == b'-') {
        let op = b[*i];
        *i += 1;
        let r = term(b, i)?;
        v = if op == b'+' { v + r } else { v - r };
    }
    Some(v)
}

fn term(b: &[u8], i: &mut usize) -> Option<f64> {
    let mut v = power(b, i)?;
    while *i < b.len() && (b[*i] == b'*' || b[*i] == b'/') {
        let op = b[*i];
        *i += 1;
        let r = power(b, i)?;
        v = if op == b'*' { v * r } else { v / r };
    }
    Some(v)
}

fn power(b: &[u8], i: &mut usize) -> Option<f64> {
    let base = factor(b, i)?;
    if *i < b.len() && b[*i] == b'^' {
        *i += 1;
        let e = power(b, i)?;
        return Some(base.powf(e));
    }
    Some(base)
}

fn factor(b: &[u8], i: &mut usize) -> Option<f64> {
    if *i >= b.len() {
        return None;
    }
    if b[*i] == b'-' {
        *i += 1;
        return factor(b, i).map(|v| -v);
    }
    let mut v = if b[*i] == b'(' {
        *i += 1;
        let v = expr(b, i)?;
        if *i >= b.len() || b[*i] != b')' {
            return None;
        }
        *i += 1;
        v
    } else {
        let start = *i;
        while *i < b.len() && (b[*i].is_ascii_digit() || b[*i] == b'.') {
            *i += 1;
        }
        std::str::from_utf8(&b[start..*i]).ok()?.parse::<f64>().ok()?
    };
    if *i < b.len() && b[*i] == b'%' {
        *i += 1;
        v /= 100.0;
    }
    Some(v)
}

pub fn format_number(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 && v.abs() < 1e15 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.10}");
        s.trim_end_matches('0').trim_end_matches('.').replace('.', ",")
    }
}

impl Sanaga {
    /// Memory saver: tabs left alone for a while are put to sleep (their page
    /// is closed and reloaded when you come back), like Chrome's.
    pub(super) fn memory_saver_step(&mut self, now: f64) {
        let active = self.active;
        if let Some(req) = self.tabs.get_mut(active).and_then(|t| t.lite_pending.take()) {
            let private = self.tabs[active].private;
            self.tabs[active].loading = true;
            let agent = if private { self.private_agent.clone() } else { self.agent.clone() };
            net::fetch_page(agent, req, self.tx.clone(), self.ctx.clone());
        }
        if let Some(t) = self.tabs.get_mut(active) {
            t.last_active = now;
            if t.sleeping && matches!(t.view, View::Web) && t.web.is_none() && t.web_pending.is_none() {
                t.sleeping = false;
                t.loading = true;
                t.web_pending = Some(t.url().to_string());
            }
        }
        // (minutes before a tab sleeps, most pages kept running at once)
        let level = if self.low_power_active() { 3 } else { self.store.settings.memory_saver };
        let (minutes, cap) = match level {
            1 => (60.0, 12),
            2 => (20.0, 6),
            3 => (5.0, 3),
            _ => return,
        };
        let mut running: Vec<(f64, usize)> = Vec::new();
        for (i, t) in self.tabs.iter_mut().enumerate() {
            // never asleep: the tab shown, a page playing sound, a call (camera
            // or micro on), a pinned tab (mail, chat…), a form being filled in
            let calling = t.web.as_ref().is_some_and(|w| {
                let v = w.webview();
                v.camera_capture_state() != webkit6::MediaCaptureState::None || v.microphone_capture_state() != webkit6::MediaCaptureState::None
            });
            if i == active || t.web.is_none() || t.audio || calling || t.pinned || t.form_dirty {
                continue;
            }
            if t.last_active > 0.0 && now - t.last_active > minutes * 60.0 {
                put_to_sleep(t);
            } else {
                running.push((t.last_active, i));
            }
        }
        // too many pages running: the ones seen longest ago go to sleep now
        if running.len() + 1 > cap {
            running.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let extra = running.len() + 1 - cap;
            for &(_, i) in running.iter().take(extra) {
                put_to_sleep(&mut self.tabs[i]);
            }
        }
    }

    /// Answer queued permission requests from remembered choices; keep the
    /// first unknown one for the prompt bar.
    pub(super) fn permission_step(&mut self) {
        if self.perm_current.is_some() {
            return;
        }
        loop {
            let Some(ask) = self.perm_queue.borrow_mut().pop_front() else { return };
            if !self.tabs.iter().any(|t| t.id == ask.tab) {
                ask.req.deny();
                continue;
            }
            match self.store.permission(&ask.site, ask.kind) {
                Some(true) => ask.req.allow(),
                Some(false) => ask.req.deny(),
                // notification requests: refused quietly unless the person
                // lets sites ask (Settings › Privacy)
                None if ask.kind == "notifications" && !self.store.settings.notif_prompts => {
                    ask.req.deny();
                    self.notice = Some((tf("Notifications de {} bloquées (Paramètres › Confidentialité).", &[&host_of(&ask.site)]), self.ctx.input(|i| i.time)));
                }
                None => {
                    self.perm_current = Some(ask);
                    return;
                }
            }
        }
    }

    pub(super) fn permission_bar_visible(&self) -> bool {
        self.perm_current.is_some()
    }

    pub(super) fn permission_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let Some(ask) = &self.perm_current else { return };
        let mut answer: Option<(bool, bool)> = None; // (allow, remember)
        let text = tf("{} souhaite accéder à {}.", &[&host_of(&ask.site), &kind_label(ask.kind)]);
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            icons::paint(ui.painter(), r, Icon::Shield, th.accent);
            ui.add_space(6.0);
            ui.label(RichText::new(text).size(13.0).color(th.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.button(tr("Bloquer")).clicked() {
                    answer = Some((false, true));
                }
                if ui.button(tr("Autoriser cette fois")).clicked() {
                    answer = Some((true, false));
                }
                let b = egui::Button::new(RichText::new(tr("Toujours autoriser")).color(Color32::WHITE)).fill(th.accent);
                if ui.add(b).clicked() {
                    answer = Some((true, true));
                }
            });
        });
        if let Some((allow, remember)) = answer {
            if let Some(ask) = self.perm_current.take() {
                if allow {
                    ask.req.allow();
                } else {
                    ask.req.deny();
                }
                if remember {
                    self.store.set_permission(&ask.site, ask.kind, Some(allow));
                }
            }
        }
    }

    /// Safari's question when a site starts a download by itself, or
    /// several at once: allowed, its downloads go on (and the site may
    /// download until Sanaga quits); refused, they are cancelled.
    pub(super) fn download_ask_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let Some(site) = self.download_asks.first().cloned() else { return };
        let mut answer: Option<bool> = None;
        let text = tf("Voulez-vous autoriser les téléchargements sur « {} » ?", &[&site]);
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            icons::paint(ui.painter(), r, Icon::Shield, th.accent);
            ui.add_space(6.0);
            ui.label(RichText::new(text).size(13.0).color(th.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.button(tr("Ne pas autoriser")).clicked() {
                    answer = Some(false);
                }
                let b = egui::Button::new(RichText::new(tr("Autoriser")).color(Color32::WHITE)).fill(th.accent);
                if ui.add(b).clicked() {
                    answer = Some(true);
                }
            });
        });
        if let Some(allow) = answer {
            self.download_asks.retain(|s| *s != site);
            crate::page::answer_downloads(&site, allow);
        }
    }

    /// Ctrl+S: keep a copy of the page (one .mhtml file) in Téléchargements.
    pub(super) fn save_page(&mut self) {
        let t = &self.tabs[self.active];
        let Some(w) = t.web.as_ref().filter(|_| matches!(t.view, View::Web)) else {
            self.notice = Some((tr("Enregistrer la page fonctionne avec le moteur WebKit.").into(), self.ctx.input(|i| i.time)));
            return;
        };
        let name: String = t.title().chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
        let name = if name.trim().is_empty() { "page".to_string() } else { name.trim().chars().take(80).collect() };
        let path = net::download_path_named(&format!("{name}.mhtml"));
        let tx = self.web_tx.clone();
        let p2 = path.clone();
        w.webview().save_to_file(&webkit6::gio::File::for_path(&path), webkit6::SaveMode::Mhtml, None::<&webkit6::gio::Cancellable>, move |res| {
            let _ = tx.send(web::WebEvent::DownloadDone { url: String::new(), path: Some(p2), ok: res.is_ok() });
        });
        self.notice = Some((tf("Page enregistrée dans {}", &[&path.display()]), self.ctx.input(|i| i.time)));
    }

    /// Erase history and site data for a period (seconds; 0 = all time).
    pub(super) fn clear_browsing_data(&mut self, period: u64, history: bool, cookies: bool, cache: bool) {
        let since = if period == 0 { 0 } else { crate::store::now().saturating_sub(period) };
        if history {
            self.store.clear_history_since(since);
            self.visited = self.store.history.iter().map(|v| v.url.clone()).collect();
            self.store.downloads.retain(|d| d.time < since);
        }
        let mut types = webkit6::WebsiteDataTypes::empty();
        if cookies {
            types |= webkit6::WebsiteDataTypes::COOKIES
                | webkit6::WebsiteDataTypes::LOCAL_STORAGE
                | webkit6::WebsiteDataTypes::SESSION_STORAGE
                | webkit6::WebsiteDataTypes::INDEXEDDB_DATABASES
                | webkit6::WebsiteDataTypes::SERVICE_WORKER_REGISTRATIONS
                | webkit6::WebsiteDataTypes::DOM_CACHE;
        }
        if cache {
            types |= webkit6::WebsiteDataTypes::MEMORY_CACHE | webkit6::WebsiteDataTypes::DISK_CACHE;
        }
        if !types.is_empty() {
            let span = glib_timespan(period);
            match self.tabs.iter().find_map(|t| t.web.as_ref()).and_then(|w| w.webview().network_session()).and_then(|s| s.website_data_manager()) {
                Some(dm) => dm.clear(types, span, None::<&webkit6::gio::Cancellable>, |_| {}),
                None => self.pending_clear = Some((types.bits(), period)),
            }
        }
        self.notice = Some((tr("Données de navigation effacées.").into(), self.ctx.input(|i| i.time)));
    }

    /// Data clearing asked while no web page was open: done with the first one.
    pub(super) fn run_pending_clear(&mut self, w: &crate::page::PageView) {
        if let Some((bits, period)) = self.pending_clear.take() {
            if let Some(dm) = w.webview().network_session().and_then(|s| s.website_data_manager()) {
                dm.clear(webkit6::WebsiteDataTypes::from_bits_truncate(bits), glib_timespan(period), None::<&webkit6::gio::Cancellable>, |_| {});
            }
        }
    }

    pub(super) fn page_clear_data(&mut self, ui: &mut Ui, th: &Theme) {
        let mut go = false;
        let periods = [(3600u64, tr("La dernière heure")), (86_400, tr("Les dernières 24 heures")), (7 * 86_400, tr("Les 7 derniers jours")), (28 * 86_400, tr("Les 4 dernières semaines")), (0, tr("Depuis toujours"))];
        super::pages::centered_column(ui, |ui| {
            super::pages::page_title(ui, th, tr("Effacer les données de navigation"));
            Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(tr("Période"));
                    let current = periods.iter().find(|p| p.0 == self.clear_period).map(|p| p.1).unwrap_or(tr("La dernière heure"));
                    egui::ComboBox::from_id_salt("clear-period").selected_text(current).width(230.0).show_ui(ui, |ui| {
                        for (v, l) in periods {
                            ui.selectable_value(&mut self.clear_period, v, l);
                        }
                    });
                });
                ui.add_space(8.0);
                ui.checkbox(&mut self.clear_history, tr("Historique de navigation et téléchargements"));
                ui.checkbox(&mut self.clear_cookies, tr("Cookies et données des sites (vous serez déconnecté de la plupart des sites)"));
                ui.checkbox(&mut self.clear_cache, tr("Images et fichiers en cache"));
                ui.add_space(10.0);
                let b = egui::Button::new(RichText::new(tr("Effacer les données")).color(Color32::WHITE)).fill(Color32::from_rgb(0xD0, 0x40, 0x30));
                if ui.add(b).clicked() {
                    go = true;
                }
            });
            ui.add_space(8.0);
            ui.label(RichText::new(tr("Les mots de passe, favoris et la liste de lecture ne sont pas touchés.")).size(12.0).color(th.sub));
        });
        if go {
            let (p, h, c, k) = (self.clear_period, self.clear_history, self.clear_cookies, self.clear_cache);
            self.clear_browsing_data(p, h, c, k);
        }
    }

    /// Site settings: permissions given or refused, and remembered zoom.
    pub(super) fn page_sites(&mut self, ui: &mut Ui, th: &Theme) {
        let mut forget_perm: Option<(String, String)> = None;
        let mut forget_zoom: Option<String> = None;
        let perms = self.store.permissions.clone();
        let mut zooms: Vec<(String, f32)> = self.store.site_zoom.iter().map(|(h, z)| (h.clone(), *z)).collect();
        zooms.sort_by(|a, b| a.0.cmp(&b.0));
        let site_prefs = crate::site_prefs::all();
        let mut reset_site: Option<String> = None;
        super::pages::centered_column(ui, |ui| {
            super::pages::page_title(ui, th, tr("Paramètres des sites"));
            // JavaScript, fonts, blocker, look… site by site
            ui.label(RichText::new(tr("Réglages par site")).size(15.0).family(bold()).color(th.text));
            ui.add_space(6.0);
            if site_prefs.is_empty() {
                ui.label(RichText::new(tr("JavaScript, polices, bloqueur, mode sombre, police, éléments retirés : site par site. Clic droit dans une page › Réglages de ce site…")).size(12.5).color(th.sub));
            }
            for (host, p) in &site_prefs {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(host).color(th.text));
                    ui.label(RichText::new(super::sitepanel::summary(p)).color(th.sub));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(tr("Réinitialiser")).clicked() {
                            reset_site = Some(host.clone());
                        }
                    });
                });
            }
            ui.add_space(18.0);
            ui.label(RichText::new(tr("Autorisations")).size(15.0).family(bold()).color(th.text));
            ui.add_space(6.0);
            if perms.is_empty() {
                ui.label(RichText::new(tr("Quand un site demande votre caméra, votre micro, votre position ou des notifications, votre choix apparaît ici.")).size(12.5).color(th.sub));
            }
            for (site, kind, allow) in &perms {
                ui.horizontal(|ui| {
                    site_badge(ui.painter(), pos2(ui.cursor().left() + 9.0, ui.cursor().center().y + 6.0), 8.0, site, site);
                    ui.add_space(22.0);
                    ui.label(RichText::new(host_of(site)).color(th.text));
                    ui.label(RichText::new(format!("— {} : {}", kind_label(kind), if *allow { tr("autorisé") } else { tr("bloqué") })).color(th.sub));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(tr("Oublier")).clicked() {
                            forget_perm = Some((site.clone(), kind.to_string()));
                        }
                    });
                });
            }
            ui.add_space(18.0);
            ui.label(RichText::new(tr("Zoom par site")).size(15.0).family(bold()).color(th.text));
            ui.add_space(6.0);
            if zooms.is_empty() {
                ui.label(RichText::new(tr("Le zoom choisi sur un site (Ctrl + / Ctrl -) est retenu et apparaît ici.")).size(12.5).color(th.sub));
            }
            for (host, z) in &zooms {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(host).color(th.text));
                    ui.label(RichText::new(format!("{:.0} %", z * 100.0)).color(th.sub));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(tr("Réinitialiser")).clicked() {
                            forget_zoom = Some(host.clone());
                        }
                    });
                });
            }
        });
        if let Some((s, k)) = forget_perm {
            self.store.set_permission(&s, &k, None);
        }
        if let Some(h) = reset_site {
            self.save_site(&h, crate::site_prefs::SitePrefs::default(), true);
        }
        if let Some(h) = forget_zoom {
            let z = self.store.settings.zoom;
            self.store.set_site_zoom(&h, z);
        }
    }

    /// Ctrl+Shift+A: find an open tab by its title or address (all groups).
    pub(super) fn tab_search(&mut self, ctx: &egui::Context, th: &Theme) {
        if !self.tab_search_open {
            return;
        }
        let w = 460.0;
        let screen = ctx.content_rect();
        let pos = pos2(screen.center().x - w / 2.0, screen.top() + 60.0);
        let q = self.tab_search_q.to_lowercase();
        let mut pick: Option<usize> = None;
        let area = egui::Area::new(egui::Id::new("tab-search")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.border))
                .corner_radius(CornerRadius::same(12))
                .shadow(egui::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(60) })
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(w - 20.0);
                    let id = egui::Id::new("tab-search-input");
                    let resp = ui.add(TextEdit::singleline(&mut self.tab_search_q).id(id).hint_text(tr("Rechercher dans les onglets")).desired_width(w - 20.0));
                    if !resp.has_focus() && self.tab_search_q.is_empty() {
                        resp.request_focus();
                    }
                    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    ui.add_space(6.0);
                    let mut first: Option<usize> = None;
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        for (i, t) in self.tabs.iter().enumerate() {
                            let title = t.title();
                            if !q.is_empty() && !title.to_lowercase().contains(&q) && !t.url().to_lowercase().contains(&q) {
                                continue;
                            }
                            first.get_or_insert(i);
                            let (r, rr) = ui.allocate_exact_size(vec2(w - 20.0, 38.0), Sense::click());
                            if rr.hovered() || i == self.active {
                                ui.painter().rect_filled(r, CornerRadius::same(6), if rr.hovered() { th.omnibox_hover } else { th.omnibox });
                            }
                            site_badge(ui.painter(), pos2(r.left() + 16.0, r.center().y), 8.0, t.url(), &title);
                            let clip = Rect::from_min_max(pos2(r.left() + 32.0, r.top()), pos2(r.right() - 8.0, r.bottom()));
                            let p = ui.painter().with_clip_rect(clip);
                            p.text(pos2(clip.left(), r.center().y - 7.0), Align2::LEFT_CENTER, &title, FontId::proportional(13.0), th.text);
                            let mut sub = host_of(t.url());
                            if self.groups.len() > 1 {
                                sub = format!("{sub} · {}", self.groups.get(t.group).cloned().unwrap_or_default());
                            }
                            if t.sleeping {
                                sub.push_str(" · en veille");
                            }
                            p.text(pos2(clip.left(), r.center().y + 9.0), Align2::LEFT_CENTER, sub, FontId::proportional(11.5), th.sub);
                            if rr.clicked() {
                                pick = Some(i);
                            }
                        }
                    });
                    if enter {
                        pick = pick.or(first);
                    }
                });
        });
        if ctx.input(|i| i.pointer.any_pressed()) && !ctx.pointer_interact_pos().map(|p| area.response.rect.contains(p)).unwrap_or(false) {
            self.tab_search_open = false;
        }
        if let Some(i) = pick {
            self.tab_search_open = false;
            self.tab_search_q.clear();
            self.current_group = self.tabs[i].group;
            self.active = i;
            self.overview_open = false;
        }
    }

    /// The page's address as a QR code (to open it on a phone).
    pub(super) fn qr_texture(&mut self, url: &str) -> Option<TextureHandle> {
        if let Some((u, t)) = &self.qr {
            if u == url {
                return Some(t.clone());
            }
        }
        let code = qrcode::QrCode::new(url.as_bytes()).ok()?;
        let n = code.width();
        let colors = code.to_colors();
        let (scale, margin) = (6usize, 4usize);
        let size = (n + margin * 2) * scale;
        let mut px = vec![Color32::WHITE; size * size];
        for y in 0..n {
            for x in 0..n {
                if colors[y * n + x] == qrcode::Color::Dark {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            px[((y + margin) * scale + dy) * size + (x + margin) * scale + dx] = Color32::BLACK;
                        }
                    }
                }
            }
        }
        let img = egui::ColorImage { size: [size, size], pixels: px, source_size: vec2(size as f32, size as f32) };
        let tex = self.ctx.load_texture("page-qr", img, egui::TextureOptions::NEAREST);
        self.qr = Some((url.to_string(), tex.clone()));
        Some(tex)
    }
}

fn put_to_sleep(t: &mut Tab) {
    // WebKit keeps a closed page's process around: end it explicitly so the
    // memory really comes back (each tab's page runs in its own process).
    if let Some(w) = t.web.take() {
        w.webview().terminate_web_process();
        drop(w);
    }
    t.sleeping = true;
    t.web_shown = false;
}

fn glib_timespan(period_secs: u64) -> webkit6::glib::TimeSpan {
    webkit6::glib::TimeSpan::from_seconds(period_secs as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculator() {
        assert_eq!(calculate("12*(3+4)"), Some(84.0));
        assert_eq!(calculate("2^10"), Some(1024.0));
        assert_eq!(calculate("15% * 80"), Some(12.0));
        assert_eq!(calculate("1,5 + 1"), Some(2.5));
        assert_eq!(calculate("wikipedia.org"), None);
        assert_eq!(calculate("2024"), None);
        assert_eq!(format_number(2.5), "2,5");
    }

    #[test]
    fn shortcuts() {
        assert!(keyword_search("w Sanaga").unwrap().0.contains("wikipedia.org"));
        assert!(keyword_search("bonjour le monde").is_none());
    }
}
