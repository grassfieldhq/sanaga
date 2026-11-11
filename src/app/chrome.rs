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
    let button = if armed {
        egui::Button::new(egui::RichText::new(tr("Confirmer ?")).color(Color32::WHITE)).fill(Color32::from_rgb(0xC8, 0x3A, 0x2B))
    } else {
        egui::Button::new(label)
    };
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
        let ql = q.to_lowercase();
        let engine = self.store.settings.engine;
        let mut items: Vec<(String, String, bool)> = Vec::new(); // (url, label, is_search)
        // calculator answer ("calc:" = copy the result)
        if let Some(v) = super::plus::calculate(&q) {
            let r = super::plus::format_number(v);
            items.push((format!("calc:{r}"), tf("= {}   (cliquer pour copier)", &[&r]), true));
        }
        // search shortcut ("w ", "yt ", "m "...)
        if let Some((u, site)) = super::plus::keyword_search(&q) {
            let rest = q.split_once(' ').map(|x| x.1).unwrap_or("").trim().to_string();
            items.push((u, tf("Rechercher « {} » sur {}", &[&rest, &site]), true));
        }
        // open tabs: switch to them instead of opening the page again
        for (i, t) in self.tabs.iter().enumerate() {
            if i == a || items.len() >= 4 {
                continue;
            }
            let title = t.title();
            if title.to_lowercase().contains(&ql) || t.url().to_lowercase().contains(&ql) {
                items.push((format!("tab:{i}"), tf("Passer à l'onglet : {}", &[&title]), false));
            }
        }
        // (the address the field holds: what was typed, completed with a site's name)
        let held = if self.inline_done { self.tabs[a].address.trim().to_string() } else { q.clone() };
        let direct = input_to_url(&held, engine, "");
        if direct != engine.search_url(&q) && super::plus::keyword_search(&q).is_none() {
            items.push((direct.clone(), direct, false));
        }
        items.push((engine.search_url(&q), tf("{} — Rechercher avec {}", &[&q, &short_engine(engine)]), true));
        // one line per page: the same page written two ways (é or %C3%A9, with
        // or without a last "/") is not listed twice, nor the address typed
        let mut seen: HashSet<String> = items.iter().map(|i| same_page_key(&i.0)).collect();
        let bm = self.store.bookmarks.iter().map(|b| (&b.url, &b.title));
        let hist = self.store.history.iter().rev().map(|v| (&v.url, &v.title));
        for (u, t) in bm.chain(hist) {
            if items.len() >= 8 {
                break;
            }
            if (u.to_lowercase().contains(&ql) || t.to_lowercase().contains(&ql)) && seen.insert(same_page_key(u)) {
                items.push((u.clone(), if t.is_empty() { u.clone() } else { t.clone() }, false));
            }
        }
        self.suggest_items = items.clone();
        if self.suggest_sel.is_some_and(|k| k >= items.len()) {
            self.suggest_sel = None;
        }
        let sel = self.suggest_sel;
        let mut go: Option<String> = None;
        // under the field, or above it when the address bar is at the bottom
        let (at, pivot, _) = super::layout::popover_place(ui.ctx(), addr, addr.left(), 4.0);
        let area = egui::Area::new(egui::Id::new("suggestions"))
            .order(egui::Order::Foreground)
            .pivot(pivot)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
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
        self.suggest_rect = area.response.rect;
        if let Some(u) = go {
            self.activate_suggestion(&u);
        }
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

    /// Open a suggestion (click, or Enter on the one picked with the arrows).
    /// The site to complete what is being typed with ("wiki" → "wikipedia.org"):
    /// the one visited most among favourites and recent history whose name
    /// starts that way.
    pub(super) fn site_completion(&self, typed: &str) -> Option<String> {
        let t = typed.to_lowercase();
        if t.chars().count() < 2 || t.contains(|c: char| c.is_whitespace() || c == '/' || c == ':') {
            return None;
        }
        let mut count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let urls = self.store.bookmarks.iter().map(|b| &b.url).chain(self.store.history.iter().rev().take(4000).map(|v| &v.url));
        for u in urls {
            let host = host_of(u);
            let host = host.strip_prefix("www.").unwrap_or(&host);
            if host.len() > t.len() && host.starts_with(&t) {
                *count.entry(host.to_string()).or_default() += 1;
            }
        }
        count.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.len().cmp(&a.0.len()))).map(|(h, _)| h)
    }

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
