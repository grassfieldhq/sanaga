//! The top of the window: tab strip, toolbar with the address bar, suggestions.

use super::*;
use crate::icons::{self, Icon};
use crate::theme::badge_color;
use eframe::egui::{Align2, Color32, Margin, Response, Sense, TextEdit};

/// The keyboard is on this control (reached with Tab): a ring in the accent
/// colour around it. Sanaga draws most of its controls itself: without
/// this, nothing shows where Tab is, and the interface can't be used
/// without a mouse.
pub(super) fn focus_ring(ui: &Ui, resp: &Response, rect: egui::Rect, radius: f32, th: &Theme) {
    if resp.has_focus() {
        ui.painter().rect_stroke(rect, egui::CornerRadius::same(radius.clamp(0.0, 255.0) as u8), egui::Stroke::new(2.0, th.accent), egui::StrokeKind::Outside);
    }
}

/// A line of text that does not fit ends with "…" — never cut in the
/// middle of a letter, which reads as a mistake. Used in place of
/// `Painter::text` on a painter clipped to the room the text has: the room
/// is what is left of the clip from where the text starts.
pub(super) trait Fit {
    fn fit(&self, pos: egui::Pos2, anchor: Align2, text: impl ToString, font: FontId, color: Color32) -> egui::Rect;
}

impl Fit for egui::Painter {
    fn fit(&self, pos: egui::Pos2, anchor: Align2, text: impl ToString, font: FontId, color: Color32) -> egui::Rect {
        let clip = self.clip_rect();
        let room = match anchor.x() {
            egui::Align::Min => clip.right() - pos.x,
            egui::Align::Max => pos.x - clip.left(),
            egui::Align::Center => 2.0 * (clip.right() - pos.x).min(pos.x - clip.left()),
        };
        let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
        job.wrap = egui::text::TextWrapping { max_width: room.max(0.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
        let galley = self.layout_job(job);
        let rect = anchor.anchor_size(pos, galley.size());
        self.galley(rect.min, galley, color);
        rect
    }
}

/// A button for what can't be undone (erase the history, delete a password,
/// remove an extension): the first press only arms it — it turns red and
/// asks "Confirm?" for a few seconds — the second does it. One slip of the
/// mouse no longer destroys anything.
pub(super) fn confirm_button(ui: &mut Ui, what: impl std::hash::Hash + std::fmt::Debug, label: &str) -> bool {
    let id = egui::Id::new(("confirm-button", what));
    let now = ui.input(|i| i.time);
    let armed = ui.ctx().data(|d| d.get_temp::<f64>(id)).is_some_and(|t| now - t < 4.0);
    let button = if armed { egui::Button::new(egui::RichText::new(tr("Confirmer ?")).color(Color32::WHITE)).fill(Color32::from_rgb(0xC8, 0x3A, 0x2B)) } else { egui::Button::new(label) };
    if !ui.add(button).clicked() {
        return false;
    }
    if armed {
        ui.ctx().data_mut(|d| d.remove::<f64>(id));
        return true;
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, now));
    // (it goes back to its own words by itself)
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(4));
    false
}

pub(super) fn icon_button_small(ui: &mut Ui, icon: Icon, th: &Theme) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    focus_ring(ui, &resp, r, 13.0, th);
    if resp.hovered() {
        ui.painter().circle_filled(r.center(), 12.0, th.omnibox_hover);
    }
    icons::paint(ui.painter(), r.shrink(7.0), icon, th.text);
    resp
}

/// Small round badge with the site's first letter (Sanaga loads no favicons: lighter).
pub(super) fn site_badge(p: &egui::Painter, c: egui::Pos2, r: f32, url: &str, title: &str) {
    if url.starts_with("sanaga://") {
        icons::logo(p, c, r);
    } else if let Some(tex) = crate::favicon::get(url) {
        // the site's own icon, on a small rounded square like Safari
        let rect = egui::Rect::from_center_size(c, egui::vec2(r * 2.0, r * 2.0));
        let radius = (r * 0.35).min(255.0) as u8;
        if r >= 10.0 {
            p.rect_filled(rect.expand(r * 0.18), egui::CornerRadius::same(radius), Color32::WHITE);
        }
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        p.add(egui::epaint::RectShape::filled(rect, egui::CornerRadius::same(radius), Color32::WHITE).with_texture(tex.id(), uv));
    } else {
        p.circle_filled(c, r, badge_color(&host_of(url)));
        p.text(c, Align2::CENTER_CENTER, badge_letter(url, title), FontId::new(r * 1.2, bold()), Color32::WHITE);
    }
}

pub(super) fn select_all(ctx: &egui::Context, id: egui::Id, len: usize) {
    if let Some(mut state) = TextEdit::load_state(ctx, id) {
        let range = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(len));
        state.cursor.set_char_range(Some(range));
        state.store(ctx, id);
    }
}

impl Sanaga {
    pub(super) fn address_suggestions(&mut self, ui: &mut Ui, addr: Rect, th: &Theme) {
        let a = self.active;
        let focused = ui.memory(|m| m.has_focus(egui::Id::new("address-bar")));
        // while moving through the list with the arrows, the list stays the one
        // for what was typed
        // (the field may hold more than what was typed: the completed site's name)
        let q = if self.suggest_sel.is_some() {
            self.suggest_typed.trim().to_string()
        } else if self.inline_done && self.tabs[a].address.starts_with(&self.inline_typed) {
            self.inline_typed.trim().to_string()
        } else {
            self.tabs[a].address.trim().to_string()
        };
        if !self.suggest_open || q.is_empty() || addr == Rect::NOTHING {
            self.suggest_items.clear();
            self.suggest_sel = None;
            return;
        }
        // Pressing on a suggestion takes the focus from the field before the
        // click ends: keep the list while the pointer is over it. (Not
        // `ui.rect_contains_pointer`: the list is on a higher layer than the
        // toolbar, so that would always say no and close it mid-click.)
        let over_list = ui.input(|i| i.pointer.latest_pos()).is_some_and(|p| self.suggest_rect.contains(p));
        if !focused && !over_list {
            self.suggest_open = false;
            self.suggest_items.clear();
            self.suggest_sel = None;
            return;
        }
        // The pages of the history that answer what is typed are asked for
        // when the text changes, on a thread of their own. (They were
        // looked for through every visit, at every frame: what kept the
        // history to 5,000 visits.)
        let ql = q.to_lowercase();
        let typed = ql.clone();
        self.history_ask(|app| &mut app.hist.suggest, ql.clone(), move |h| h.suggest(&typed, 12));
        // (the address the field holds: what was typed, completed with a site's name)
        let held = if self.inline_done { self.tabs[a].address.trim().to_string() } else { q.clone() };
        // The list is put together again when what it is made of changes
        // — the text, the history's answer, the tabs, the favourites — not
        // at every frame.
        let made = super::hist::SuggestMade { typed: q.clone(), held: held.clone(), answers: self.hist.suggest.answers, tabs: self.tabs.len(), active: a, favourites: self.store.bookmarks.len() };
        if made != self.hist.suggest_made || self.suggest_items.is_empty() {
            self.suggest_items = self.suggestion_lines(&q, &ql, &held);
            self.hist.suggest_made = made;
            if std::env::var_os("SANAGA_DEBUG_SUGGEST").is_some() {
                eprintln!("suggestions for {q:?} at frame {}: {}", self.frame, self.suggest_items.iter().map(|i| i.0.as_str()).collect::<Vec<_>>().join(" | "));
            }
        }
        if self.suggest_sel.is_some_and(|k| k >= self.suggest_items.len()) {
            self.suggest_sel = None;
        }
        // (taken out while it is drawn, put back after: not copied at every frame)
        let items = std::mem::take(&mut self.suggest_items);
        let sel = self.suggest_sel;
        let mut go: Option<String> = None;
        // under the field, or above it when the address bar is at the bottom
        let (at, pivot, _) = super::layout::popover_place(ui.ctx(), addr, addr.left(), 4.0);
        let area = egui::Area::new(egui::Id::new("suggestions")).order(egui::Order::Foreground).pivot(pivot).fixed_pos(at).show(ui.ctx(), |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.border))
                .corner_radius(CornerRadius::same(14))
                .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(40) })
                .inner_margin(Margin::symmetric(0, 6))
                .show(ui, |ui| {
                    ui.set_width(addr.width());
                    for (k, (url, label, search)) in items.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(vec2(addr.width(), 34.0), Sense::click());
                        // (for a screen reader: what the line is, and whether it is the one picked)
                        resp.widget_info(|| {
                            let name = if *search || label == url || url.starts_with("tab:") { label.clone() } else { format!("{label} — {}", super::toolbar::display_url(url, false).0) };
                            egui::WidgetInfo::selected(egui::Role::ListBoxOption, true, sel == Some(k), name)
                        });
                        if sel == Some(k) {
                            ui.painter().rect_filled(r, 0.0, th.accent.gamma_multiply(0.16));
                            ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(3.0, r.height())), 0.0, th.accent);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(r, 0.0, th.omnibox);
                        }
                        let ic = Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), vec2(15.0, 15.0));
                        if *search {
                            icons::paint(ui.painter(), ic, Icon::Search, th.sub);
                        } else if url.starts_with("tab:") {
                            icons::paint(ui.painter(), ic, Icon::Grid, th.accent);
                        } else {
                            site_badge(ui.painter(), ic.center(), 7.5, url, label);
                        }
                        let clip = Rect::from_min_max(pos2(ic.right() + 12.0, r.top()), pos2(r.right() - 12.0, r.bottom()));
                        let p = ui.painter().with_clip_rect(clip);
                        let g = p.fit(pos2(clip.left(), r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), th.text);
                        if !*search && label != url && !url.starts_with("tab:") {
                            // (as in the address bar: decoded, no https:// nor www.)
                            let (site, rest) = super::toolbar::display_url(url, false);
                            p.fit(pos2(g.right() + 10.0, r.center().y), Align2::LEFT_CENTER, format!("— {site}{rest}"), FontId::proportional(13.0), th.link);
                        }
                        if resp.clicked() {
                            go = Some(url.clone());
                        }
                    }
                });
        });
        self.suggest_items = items;
        self.suggest_rect = area.response.rect;
        if let Some(u) = go {
            self.activate_suggestion(&u);
        }
    }

    /// The lines offered under the address bar for what is typed (`q`; `ql`
    /// in lower case; `held`: what the field holds), each (address, words
    /// shown, is it a search): a calculation, a search shortcut, open
    /// tabs, the address itself, the search — then favourites and visited
    /// pages, the best first.
    fn suggestion_lines(&self, q: &str, ql: &str, held: &str) -> Vec<(String, String, bool)> {
        let a = self.active;
        let engine = self.store.settings.engine;
        let mut items: Vec<(String, String, bool)> = Vec::new();
        // calculator answer ("calc:" = copy the result)
        if let Some(v) = super::plus::calculate(q) {
            let r = super::plus::format_number(v);
            items.push((format!("calc:{r}"), tf("= {}   (cliquer pour copier)", &[&r]), true));
        }
        // search shortcut ("w ", "yt ", "m "...)
        if let Some((u, site)) = super::plus::keyword_search(q) {
            let rest = q.split_once(' ').map(|x| x.1).unwrap_or("").trim().to_string();
            items.push((u, tf("Rechercher « {} » sur {}", &[&rest, &site]), true));
        }
        // open tabs: switch to them instead of opening the page again
        for (i, t) in self.tabs.iter().enumerate() {
            if i == a || items.len() >= 4 {
                continue;
            }
            let title = t.title();
            if title.to_lowercase().contains(ql) || t.url().to_lowercase().contains(ql) {
                items.push((format!("tab:{i}"), tf("Passer à l'onglet : {}", &[&title]), false));
            }
        }
        // (a completed site's name leads where the site was really visited)
        let direct = self.completed_address(a).unwrap_or_else(|| input_to_url(held, engine, ""));
        if direct != engine.search_url(q) && super::plus::keyword_search(q).is_none() {
            items.push((direct.clone(), direct, false));
        }
        items.push((engine.search_url(q), tf("{} — Rechercher avec {}", &[&q, &short_engine(engine)]), true));
        // Visited pages and favourites, the best first: sites whose name
        // starts with what is typed, then those with a part of their name
        // starting so, then pages that hold it in their address or title;
        // among equals, the most visited lately (see `history::frecency`).
        // While the answer to this very text is on its way, the lines of
        // the last answer that still answer it stay.
        let words = crate::history::words_of(ql);
        let site = |url: &str| crate::history::site_of(url).0;
        let mut pages: Vec<crate::history::Hit> =
            (self.hist.suggest.got.iter()).filter_map(|h| crate::history::rank(&words, &site(&h.url), &h.url, &h.title).map(|rank| crate::history::Hit { rank, ..h.clone() })).collect();
        // (a favourite counts as a page visited three times lately, on top of its visits)
        const FAVOURITE: u64 = 300;
        for b in &self.store.bookmarks {
            let Some(rank) = crate::history::rank(&words, &site(&b.url), &b.url, &b.title) else { continue };
            let key = same_page_key(&b.url);
            match pages.iter_mut().find(|h| same_page_key(&h.url) == key) {
                Some(h) => h.score = h.score.max(FAVOURITE + crate::history::frecency(h.visits, h.last, crate::store::now())),
                None => pages.push(crate::history::Hit { url: b.url.clone(), title: b.title.clone(), visits: 0, last: 0, rank, score: FAVOURITE }),
            }
        }
        pages.sort_by_key(|h| (h.rank, std::cmp::Reverse(h.score), std::cmp::Reverse(h.last)));
        // one line per page: the same page written two ways (é or %C3%A9, with
        // or without a last "/") is not listed twice, nor the address typed
        let mut seen: HashSet<String> = items.iter().map(|i| same_page_key(&i.0)).collect();
        for h in pages {
            if items.len() >= 8 {
                break;
            }
            if seen.insert(same_page_key(&h.url)) {
                items.push((h.url.clone(), if h.title.is_empty() { h.url } else { h.title }, false));
            }
        }
        items
    }

    /// What the address field shows while a suggestion is picked with the arrows.
    pub(super) fn suggestion_text(&self, k: usize) -> String {
        let Some((url, _, search)) = self.suggest_items.get(k) else { return self.suggest_typed.clone() };
        if let Some(i) = url.strip_prefix("tab:").and_then(|n| n.parse::<usize>().ok()) {
            return self.tabs.get(i).map(|t| t.url().to_string()).unwrap_or_default();
        }
        if *search || url.starts_with("calc:") {
            return self.suggest_typed.clone();
        }
        url.clone()
    }

    /// Typing forward at the end of the address field: look for the site
    /// to complete what is typed with ("wiki" → "wikipedia.org") — the one
    /// that counts most among the sites visited (`history::frecency`) and
    /// the favourites whose name starts that way. Asked of the history on
    /// a thread of its own (it was a walk through 4,000 visits at every
    /// key); the field is completed when the answer comes, a moment later
    /// (`complete_address`).
    pub(super) fn ask_completion(&mut self, typed: &str) {
        self.hist.completed = None;
        let t = typed.to_lowercase();
        if t.chars().count() < 2 || t.contains(|c: char| c.is_whitespace() || c == '/') {
            return;
        }
        // (a favourite's site counts as one visited once lately; among
        // favourites, the shortest name)
        let favourite = (self.store.bookmarks.iter())
            .filter_map(|b| {
                let (host, origin) = crate::history::site_of(&b.url);
                (!host.is_empty() && host.starts_with(&t)).then(|| crate::history::Completion { text: host, url: format!("{origin}/"), score: 100 })
            })
            .min_by_key(|c| c.text.len());
        let history = self.store.history.clone();
        let (tab, typed) = (self.tabs[self.active].id, typed.to_string());
        let asked = typed.clone();
        self.off_thread(
            move || history.complete_site(&asked),
            move |app, visited| {
                let best = match (visited, favourite) {
                    (Some(v), Some(f)) => Some(if f.score > v.score { f } else { v }),
                    (v, f) => v.or(f),
                };
                if let Some(site) = best {
                    app.complete_address(tab, &typed, site);
                }
            },
        );
    }

    /// Complete what was typed in the address field with a site's name,
    /// the added part selected: the next letter replaces it, Enter goes
    /// there — where the site was really visited (`site.url`:
    /// "http://nas.lan:8080/"), not always "https://" and its name.
    fn complete_address(&mut self, tab: u64, typed: &str, site: crate::history::Completion) {
        let a = self.active;
        let id = egui::Id::new("address-bar");
        // (the person typed on, or left the field: this answers a text that is gone)
        if self.tabs[a].id != tab || self.tabs[a].address != typed || self.inline_typed != typed || self.inline_done || !self.ctx.memory(|m| m.has_focus(id)) {
            return;
        }
        let n = typed.chars().count();
        let Some(mut state) = TextEdit::load_state(&self.ctx, id) else { return };
        if !state.cursor.char_range().is_some_and(|r| r.primary.index == egui::text::CharIndex(n) && r.secondary.index == r.primary.index) {
            return;
        }
        // (the rest of the name: what follows as many letters as were
        // typed — a typed letter may take more bytes than the name's, e.g.
        // the Kelvin sign for "k")
        let rest: String = site.text.chars().skip(n).collect();
        let full = format!("{typed}{rest}");
        let (from, to) = (egui::text::CCursor::new(n), egui::text::CCursor::new(full.chars().count()));
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(from, to)));
        state.store(&self.ctx, id);
        self.tabs[a].address = full.clone();
        self.inline_done = true;
        // the site's address is looked up now (DNS), so the page starts
        // sooner on Enter; never for a private tab
        if !self.tabs[a].private {
            if let (Some(e), Some(host)) = (&self.web_context, url::Url::parse(&site.url).ok().and_then(|u| u.host_str().map(str::to_string))) {
                e.session.prefetch_dns(&host);
            }
        }
        self.hist.completed = Some((full, site.url));
        self.ctx.request_repaint();
    }

    /// Where the address field leads when it holds a site's name it was
    /// completed with (see `complete_address`).
    pub(super) fn completed_address(&self, a: usize) -> Option<String> {
        self.hist.completed.as_ref().filter(|(text, _)| text == self.tabs[a].address.trim()).map(|(_, url)| url.clone())
    }

    /// Open a suggestion (click, or Enter on the one picked with the arrows).
    pub(super) fn activate_suggestion(&mut self, u: &str) {
        let a = self.active;
        let u = u.to_string();
        {
            self.suggest_open = false;
            self.suggest_sel = None;
            if let Some(i) = u.strip_prefix("tab:").and_then(|n| n.parse::<usize>().ok()) {
                if i < self.tabs.len() {
                    self.tabs[a].address = self.tabs[a].url().to_string();
                    self.current_group = self.tabs[i].group;
                    self.active = i;
                }
            } else if let Some(r) = u.strip_prefix("calc:") {
                self.ctx.copy_text(r.to_string());
                self.notice = Some((tf("{} copié", &[&r]), self.ctx.input(|i| i.time)));
            } else {
                self.navigate(a, u, None, true);
            }
        }
    }
}

/// What tells two addresses of the same page apart from two pages: the
/// address decoded, without its #part and last "/".
pub(super) fn same_page_key(url: &str) -> String {
    let u = url.split('#').next().unwrap_or(url);
    let u = percent_encoding::percent_decode_str(u).decode_utf8().map(|d| d.to_string()).unwrap_or_else(|_| u.to_string());
    u.trim_end_matches('/').replace("://www.", "://")
}

/// What stands at the left of a menu line.
enum Mark {
    Icon(Icon),
    /// A dot of a group's or a container's colour.
    Dot(Color32),
}

/// A line of a right-click menu, as macOS draws them: a symbol, the words,
/// the keys that do the same at the right. Every menu Sanaga draws uses
/// it (some were the toolkit's bare buttons: no symbol, no keys). True
/// when chosen — the menu closes.
pub(super) fn menu_item(ui: &mut Ui, icon: Icon, label: &str) -> bool {
    menu_line(ui, Mark::Icon(icon), label, "")
}

pub(super) fn menu_item_keys(ui: &mut Ui, icon: Icon, label: &str, keys: &str) -> bool {
    menu_line(ui, Mark::Icon(icon), label, keys)
}

pub(super) fn menu_item_dot(ui: &mut Ui, dot: Color32, label: &str) -> bool {
    menu_line(ui, Mark::Dot(dot), label, "")
}

/// The width of a menu's lines: that of its widest line, as measured at
/// the last frame — every line the same, and no wider than needed.
fn menu_width(ui: &Ui, need: f32) -> f32 {
    let id = ui.id().with("menu-width");
    let pass = ui.ctx().cumulative_pass_nr();
    let (at, now, before) = ui.data(|d| d.get_temp::<(u64, f32, f32)>(id)).unwrap_or((pass, 0.0, 0.0));
    let (now, before) = if at == pass { (now.max(need), before) } else { (need, now) };
    ui.data_mut(|d| d.insert_temp(id, (pass, now, before)));
    need.max(before).max(228.0)
}

fn menu_line(ui: &mut Ui, mark: Mark, label: &str, keys: &str) -> bool {
    let words = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), Color32::PLACEHOLDER);
    let keys_w = if keys.is_empty() { 0.0 } else { ui.painter().layout_no_wrap(keys.to_string(), FontId::proportional(12.0), Color32::PLACEHOLDER).size().x + 26.0 };
    let w = menu_width(ui, 30.0 + words.size().x + keys_w + 12.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let accent = ui.visuals().selection.stroke.color;
    let hovered = resp.hovered() || resp.has_focus();
    if hovered {
        ui.painter().rect_filled(r, CornerRadius::same(6), accent);
    }
    let col = if hovered { Color32::WHITE } else { ui.visuals().text_color() };
    match mark {
        Mark::Icon(icon) => icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 14.0, r.center().y), vec2(13.0, 13.0)), icon, col),
        Mark::Dot(dot) => {
            ui.painter().circle_filled(pos2(r.left() + 14.0, r.center().y), 5.0, dot);
        }
    }
    ui.painter().galley(pos2(r.left() + 30.0, r.center().y - words.size().y / 2.0), words, col);
    if !keys.is_empty() {
        let dim = if hovered { Color32::from_white_alpha(215) } else { ui.visuals().weak_text_color() };
        ui.painter().text(pos2(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, keys, FontId::proportional(12.0), dim);
    }
    if resp.clicked() {
        ui.close();
        return true;
    }
    false
}

/// A menu line that opens a second menu beside it (its arrow at the
/// right), drawn like the other lines.
pub(super) fn menu_sub<R>(ui: &mut Ui, icon: Icon, label: &str, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let accent = ui.visuals().selection.stroke.color;
    let text = ui.visuals().text_color();
    let w = menu_width(ui, 30.0 + ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), Color32::PLACEHOLDER).size().x + 34.0);
    let (resp, inner) = ui
        .scope(|ui| {
            let v = ui.visuals_mut();
            for look in [&mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                look.weak_bg_fill = accent;
                look.bg_fill = accent;
                look.fg_stroke.color = Color32::WHITE;
                look.bg_stroke = Stroke::NONE;
                look.corner_radius = CornerRadius::same(6);
                look.expansion = 0.0;
            }
            v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            v.widgets.inactive.bg_stroke = Stroke::NONE;
            ui.spacing_mut().button_padding = vec2(8.0, 4.0);
            // (the symbol's place is kept empty in the button, and painted below)
            let button = egui::Button::new((
                egui::Atom::custom(egui::IdSalt::new("menu-symbol"), vec2(18.0, 13.0)),
                egui::RichText::new(label).size(13.0),
                egui::Atom::grow(),
                egui::containers::menu::SubMenuButton::arrow_atom(None),
            ))
            .min_size(vec2(w, 26.0));
            egui::containers::menu::SubMenuButton::from_button(button).ui(ui, add)
        })
        .inner;
    let lit = resp.hovered() || resp.has_focus() || inner.is_some();
    icons::paint(ui.painter(), Rect::from_center_size(pos2(resp.rect.left() + 14.0, resp.rect.center().y), vec2(13.0, 13.0)), icon, if lit { Color32::WHITE } else { text });
    inner.map(|i| i.inner)
}
