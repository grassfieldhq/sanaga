//! Safari-style tab features: pinned tabs, tab groups, sound control,
//! duplicate / close others, and the tab overview with page thumbnails.

use super::chrome::site_badge;
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align2, Color32, CursorIcon, RichText, ScrollArea, Sense};
use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebViewExt};

/// Turn a WebKit snapshot into a small picture for the overview.
fn surface_to_image(s: cairo::Surface) -> Option<egui::ColorImage> {
    let mut img = cairo::ImageSurface::try_from(s).ok()?;
    let (w, h, stride) = (img.width() as usize, img.height() as usize, img.stride() as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let data = img.data().ok()?;
    let step = (w / 480).max(1);
    let (ow, oh) = (w / step, (h / step).min(ow_limit(w / step)));
    let mut px = Vec::with_capacity(ow * oh * 4);
    for y in 0..oh {
        for x in 0..ow {
            let i = y * step * stride + x * step * 4;
            // cairo ARGB32 is stored B, G, R, A on little-endian machines
            px.extend_from_slice(&[data[i + 2], data[i + 1], data[i], 255]);
        }
    }
    Some(egui::ColorImage::from_rgba_unmultiplied([ow, oh], &px))
}

/// Full-size picture of the page (shown under menus).
pub(super) fn surface_to_full_image(s: cairo::Surface) -> Option<egui::ColorImage> {
    let mut img = cairo::ImageSurface::try_from(s).ok()?;
    let (w, h, stride) = (img.width() as usize, img.height() as usize, img.stride() as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let data = img.data().ok()?;
    let mut px = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = &data[y * stride..y * stride + w * 4];
        for c in row.as_chunks::<4>().0 {
            px.extend_from_slice(&[c[2], c[1], c[0], 255]);
        }
    }
    Some(egui::ColorImage::from_rgba_unmultiplied([w, h], &px))
}

fn ow_limit(w: usize) -> usize {
    // keep a card-like ratio: no taller than 3/4 of the width
    (w * 3 / 4).max(1)
}

/// Chrome's group colours.
pub(super) const GROUP_COLORS: [(&str, Color32); 9] = [
    ("Gris", Color32::from_rgb(0x5F, 0x63, 0x68)),
    ("Bleu", Color32::from_rgb(0x1A, 0x73, 0xE8)),
    ("Rouge", Color32::from_rgb(0xD9, 0x30, 0x25)),
    ("Jaune", Color32::from_rgb(0xE3, 0x74, 0x00)),
    ("Vert", Color32::from_rgb(0x18, 0x80, 0x38)),
    ("Rose", Color32::from_rgb(0xD0, 0x18, 0x84)),
    ("Violet", Color32::from_rgb(0xA1, 0x42, 0xF4)),
    ("Cyan", Color32::from_rgb(0x00, 0x7B, 0x83)),
    ("Orange", Color32::from_rgb(0xFA, 0x90, 0x3E)),
];

impl Sanaga {
    /// Tabs of the current group, pinned first (indices into `self.tabs`).
    /// All tabs in tab-bar order: pinned first, then each group's tabs kept
    /// together where its first tab is (like Chrome).
    pub(super) fn visible_tabs(&self) -> Vec<usize> {
        let first = |g: usize| self.tabs.iter().position(|t| t.group == g).unwrap_or(0);
        let mut v: Vec<usize> = (0..self.tabs.len()).collect();
        v.sort_by_key(|&i| {
            let t = &self.tabs[i];
            (!t.pinned, if t.group == 0 || t.pinned { i } else { first(t.group) }, i)
        });
        v
    }

    /// The tabs shown in the tab bar (a folded group shows only its label;
    /// the shown tab always stays visible).
    pub(super) fn strip_tabs(&self) -> Vec<usize> {
        self.visible_tabs().into_iter().filter(|&i| i == self.active || !self.group_folded(self.tabs[i].group)).collect()
    }

    /// A group's name for lists ("Groupe 2" when it has none).
    pub(super) fn group_label(&self, g: usize) -> String {
        match self.groups.get(g) {
            _ if g == 0 => tr("Onglets sans groupe").to_string(),
            Some(n) if !n.trim().is_empty() => n.clone(),
            _ => tf("Groupe {}", &[&g]),
        }
    }

    pub(super) fn group_folded(&self, g: usize) -> bool {
        g > 0 && self.group_collapsed.get(g).copied().unwrap_or(false)
    }

    /// The colour of a group (Chrome's palette).
    pub(super) fn group_color(&self, g: usize) -> Color32 {
        let k = self.group_colors.get(g).copied().unwrap_or(g as u8);
        GROUP_COLORS[k as usize % GROUP_COLORS.len()].1
    }

    /// Keep the per-group details (colour, folded) as long as the group list.
    pub(super) fn sync_group_meta(&mut self) {
        let n = self.groups.len();
        while self.group_colors.len() < n {
            let g = self.group_colors.len();
            self.group_colors.push((g % GROUP_COLORS.len()) as u8);
        }
        self.group_colors.truncate(n);
        self.group_collapsed.resize(n, false);
    }

    fn drop_group_meta(&mut self, g: usize) {
        if g < self.group_colors.len() {
            self.group_colors.remove(g);
        }
        if g < self.group_collapsed.len() {
            self.group_collapsed.remove(g);
        }
    }

    /// Show a group: its first tab (the group unfolds).
    pub(super) fn switch_group(&mut self, g: usize) {
        if g >= self.groups.len() {
            return;
        }
        self.sync_group_meta();
        self.group_collapsed[g] = false;
        match self.visible_tabs().into_iter().find(|&i| self.tabs[i].group == g) {
            Some(i) => self.active = i,
            None => {
                self.current_group = g;
                self.open_tab("sanaga://nouvel-onglet", false);
                let a = self.active;
                self.tabs[a].group = g;
            }
        }
        self.save_session();
    }

    /// A new group with the shown tab in it (or a new tab, if the shown one is
    /// already in a group).
    pub(super) fn new_group(&mut self, name: &str) {
        let name = name.trim().to_string();
        self.groups.push(name);
        self.sync_group_meta();
        let g = self.groups.len() - 1;
        let a = self.active;
        if self.tabs[a].group == 0 && !self.tabs[a].pinned {
            self.tabs[a].group = g;
        } else {
            self.current_group = g;
            self.open_tab("sanaga://nouvel-onglet", false);
            let a = self.active;
            self.tabs[a].group = g;
        }
        self.current_group = g;
        self.save_session();
    }

    /// Put tab `i` in a new group of its own.
    pub(super) fn tab_to_new_group(&mut self, i: usize) {
        self.groups.push(String::new());
        self.sync_group_meta();
        let g = self.groups.len() - 1;
        self.tabs[i].pinned = false;
        self.tabs[i].group = g;
        self.group_editing = Some(g);
        self.save_session();
    }

    /// Close a whole group with its tabs.
    pub(super) fn close_group(&mut self, g: usize) {
        if g == 0 || g >= self.groups.len() {
            return;
        }
        let active_id = self.tabs.get(self.active).map(|t| t.id);
        let mut i = 0;
        while i < self.tabs.len() {
            if self.tabs[i].group == g {
                let t = self.tabs.remove(i);
                if !t.private && !t.entries.is_empty() {
                    self.closed.push(t.url().to_string());
                }
            } else {
                if self.tabs[i].group > g {
                    self.tabs[i].group -= 1;
                }
                i += 1;
            }
        }
        self.groups.remove(g);
        self.drop_group_meta(g);
        self.current_group = 0;
        if self.tabs.is_empty() {
            self.open_tab("sanaga://nouvel-onglet", false);
        }
        self.active = active_id.and_then(|id| self.tabs.iter().position(|t| t.id == id)).or_else(|| self.strip_tabs().first().copied()).unwrap_or(0);
        self.save_session();
    }

    /// Take the tabs out of a group (they stay open) and remove the group.
    pub(super) fn ungroup(&mut self, g: usize) {
        if g == 0 || g >= self.groups.len() {
            return;
        }
        for t in &mut self.tabs {
            if t.group == g {
                t.group = 0;
            } else if t.group > g {
                t.group -= 1;
            }
        }
        self.groups.remove(g);
        self.drop_group_meta(g);
        self.current_group = self.tabs.get(self.active).map(|t| t.group).unwrap_or(0);
        self.save_session();
    }

    /// Fold or unfold a group (Chrome: click on its label). Folding the group of
    /// the shown tab shows the next tab outside it.
    pub(super) fn toggle_group_fold(&mut self, g: usize) {
        self.sync_group_meta();
        if g == 0 || g >= self.groups.len() {
            return;
        }
        let fold = !self.group_collapsed[g];
        self.group_collapsed[g] = fold;
        if fold && self.tabs[self.active].group == g {
            let order = self.visible_tabs();
            let pos = order.iter().position(|&i| i == self.active).unwrap_or(0);
            let next = order.iter().skip(pos + 1).chain(order.iter().take(pos).rev()).copied().find(|&i| self.tabs[i].group != g);
            match next {
                Some(i) => self.active = i,
                None => {
                    self.current_group = 0;
                    self.open_tab("sanaga://nouvel-onglet", false);
                    let a = self.active;
                    self.tabs[a].group = 0;
                }
            }
        }
    }

    /// New tab just to the right of tab `i` (in its group), like Chrome.
    pub(super) fn new_tab_right(&mut self, i: usize) {
        let g = self.tabs[i].group;
        let id = self.tabs[i].id;
        self.open_tab("sanaga://nouvel-onglet", false);
        let n = self.active;
        self.tabs[n].group = g;
        if let Some(pos) = self.tabs.iter().position(|t| t.id == id) {
            self.move_tab(n, (pos + 1).min(self.tabs.len() - 1));
        }
    }

    pub(super) fn toggle_overview(&mut self) {
        self.overview_open = !self.overview_open;
        self.share_open = false;
        self.menu_open = false;
        if self.overview_open {
            self.request_thumbnails();
        }
    }

    pub(super) fn move_to_group(&mut self, i: usize, g: usize) {
        if i < self.tabs.len() && g < self.groups.len() {
            self.tabs[i].group = g;
            if g > 0 {
                self.tabs[i].pinned = false;
                self.sync_group_meta();
                self.group_collapsed[g] = false;
            }
            self.save_session();
        }
    }

    pub(super) fn toggle_pin(&mut self, i: usize) {
        if let Some(t) = self.tabs.get_mut(i) {
            t.pinned = !t.pinned;
        }
        self.save_session();
    }

    pub(super) fn duplicate_tab(&mut self, i: usize) {
        let Some(t) = self.tabs.get(i) else { return };
        let (url, private) = (t.url().to_string(), t.private);
        self.open_tab(&url, private);
    }

    pub(super) fn toggle_mute(&mut self, i: usize) {
        let Some(t) = self.tabs.get_mut(i) else { return };
        t.muted = !t.muted;
        if let Some(w) = &t.web {
            w.webview().set_is_muted(t.muted);
        }
    }

    /// Close every tab of the group except `keep` (pinned tabs stay, like Safari).
    pub(super) fn close_others(&mut self, keep: usize, only_right: bool) {
        let keep_id = self.tabs[keep].id;
        let order = self.visible_tabs();
        let pos = order.iter().position(|&i| i == keep).unwrap_or(0);
        let doomed: Vec<u64> = order
            .iter()
            .enumerate()
            .filter(|(k, &i)| self.tabs[i].id != keep_id && !self.tabs[i].pinned && (!only_right || *k > pos))
            .map(|(_, &i)| self.tabs[i].id)
            .collect();
        for id in doomed {
            if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
                self.close_tab(i);
            }
        }
        if let Some(i) = self.tabs.iter().position(|t| t.id == keep_id) {
            self.active = i;
        }
    }

    /// Move a tab to another place in the tab bar (drag and drop).
    pub(super) fn move_tab(&mut self, from: usize, to: usize) {
        if from == to || from >= self.tabs.len() || to >= self.tabs.len() {
            return;
        }
        let active_id = self.tabs[self.active].id;
        let t = self.tabs.remove(from);
        self.tabs.insert(to, t);
        if let Some(i) = self.tabs.iter().position(|t| t.id == active_id) {
            self.active = i;
        }
    }

    /// The menu shown on a tab's right-click (Chrome's items).
    pub(super) fn tab_menu(&mut self, ui: &mut Ui, i: usize) -> bool {
        let mut done = false;
        let t = &self.tabs[i];
        let (pinned, muted, web, group) = (t.pinned, t.muted, t.web.is_some(), t.group);
        if ui.button(tr("Nouvel onglet à droite")).clicked() {
            self.new_tab_right(i);
            return true;
        }
        if self.groups.len() <= 1 {
            if ui.button(tr("Ajouter l'onglet à un nouveau groupe")).clicked() {
                self.tab_to_new_group(i);
                done = true;
            }
        } else {
            ui.menu_button(tr("Ajouter l'onglet au groupe"), |ui| {
                if ui.button(tr("Nouveau groupe")).clicked() {
                    self.tab_to_new_group(i);
                    done = true;
                }
                ui.separator();
                for g in 1..self.groups.len() {
                    if g == group {
                        continue;
                    }
                    let name = if self.groups[g].is_empty() { tf("Groupe {}", &[&g]) } else { self.groups[g].clone() };
                    let col = self.group_color(g);
                    if ui.button(RichText::new(format!("● {name}")).color(col)).clicked() {
                        self.move_to_group(i, g);
                        done = true;
                    }
                }
            });
        }
        if group > 0 && ui.button(tr("Retirer du groupe")).clicked() {
            self.move_to_group(i, 0);
            done = true;
        }
        ui.separator();
        if ui.button(tr("Actualiser")).clicked() {
            self.reload(i);
            done = true;
        }
        if ui.button(tr("Dupliquer")).clicked() {
            self.duplicate_tab(i);
            done = true;
        }
        if self.tabs.len() > 1 && !self.tabs[i].private && ui.button(tr("Déplacer vers une nouvelle fenêtre")).clicked() {
            // the page opens again in the new window, and leaves this one
            let url = self.tabs[i].url().to_string();
            self.new_window(vec![url]);
            self.close_tab(i);
            return true;
        }
        if ui.button(if pinned { tr("Désépingler") } else { tr("Épingler") }).clicked() {
            self.toggle_pin(i);
            done = true;
        }
        if web && ui.button(if muted { tr("Réactiver le son du site") } else { tr("Couper le son du site") }).clicked() {
            self.toggle_mute(i);
            done = true;
        }
        ui.separator();
        if ui.button(tr("Fermer")).clicked() {
            self.close_tab(i);
            return true;
        }
        if ui.button(tr("Fermer les autres onglets")).clicked() {
            self.close_others(i, false);
            done = true;
        }
        if ui.button(tr("Fermer les onglets à droite")).clicked() {
            self.close_others(i, true);
            done = true;
        }
        if !self.closed.is_empty() {
            ui.separator();
            if ui.button(tr("Rouvrir l'onglet fermé (Ctrl+Maj+T)")).clicked() {
                self.run_shortcut("reopen");
                done = true;
            }
        }
        done
    }

    /// The menu shown on a group label's right-click.
    pub(super) fn group_menu(&mut self, ui: &mut Ui, g: usize) -> bool {
        let mut done = false;
        ui.set_min_width(240.0);
        let resp = ui.add(egui::TextEdit::singleline(&mut self.groups[g]).hint_text(tr("Nommer ce groupe")).desired_width(220.0));
        if self.group_editing == Some(g) {
            resp.request_focus();
            self.group_editing = None;
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.save_session();
            done = true;
        }
        ui.horizontal(|ui| {
            for (k, (name, col)) in GROUP_COLORS.iter().enumerate() {
                let (r, cresp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                let chosen = self.group_colors.get(g).copied() == Some(k as u8);
                ui.painter().circle_filled(r.center(), 8.0, *col);
                if chosen {
                    ui.painter().circle_stroke(r.center(), 10.5, egui::Stroke::new(2.0, *col));
                }
                if cresp.tip(tr(name)).clicked() {
                    self.sync_group_meta();
                    self.group_colors[g] = k as u8;
                }
            }
        });
        ui.separator();
        if ui.button(tr("Nouvel onglet dans le groupe")).clicked() {
            if let Some(last) = self.visible_tabs().into_iter().rfind(|&i| self.tabs[i].group == g) {
                self.group_collapsed[g] = false;
                self.new_tab_right(last);
            }
            done = true;
        }
        if ui.button(tr("Dissocier")).clicked() {
            self.ungroup(g);
            return true;
        }
        if ui.button(tr("Fermer le groupe")).clicked() {
            self.close_group(g);
            return true;
        }
        done
    }

    /// Ask WebKit for pictures of the pages (for the overview).
    pub(super) fn request_thumbnails(&mut self) {
        for t in &self.tabs {
            let Some(w) = &t.web else { continue };
            let tx = self.thumb_tx.clone();
            let id = t.id;
            let ctx = self.ctx.clone();
            w.webview().snapshot(SnapshotRegion::Visible, SnapshotOptions::NONE, None::<&webkit2gtk::gio::Cancellable>, move |res| {
                if let Some(img) = res.ok().and_then(surface_to_image) {
                    let _ = tx.send((id, img));
                    ctx.request_repaint();
                }
            });
        }
    }

    pub(super) fn poll_tab_extras(&mut self) {
        while let Ok((id, img)) = self.thumb_rx.try_recv() {
            if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                t.thumb = Some(self.ctx.load_texture(format!("thumb-{id}"), img, egui::TextureOptions::LINEAR));
            }
        }
        for t in &mut self.tabs {
            t.audio = t.web.as_ref().map(|w| w.webview().is_playing_audio()).unwrap_or(false);
        }
    }

    /// All tabs of the group as cards, like Safari's tab overview.
    pub(super) fn tab_overview(&mut self, ui: &mut Ui, th: &Theme) {
        ui.painter().rect_filled(ui.max_rect(), 0.0, th.ntp_bg);
        let order = self.visible_tabs();
        let mut select: Option<usize> = None;
        let mut close: Option<usize> = None;
        let mut new_tab = false;
        let full_w = ui.available_width();
        let (card, gap) = (vec2(250.0, 196.0), 22.0);
        let cols = (((full_w - 64.0 + gap) / (card.x + gap)).floor() as usize).max(1).min(order.len() + 1);
        let row_w = cols as f32 * card.x + (cols as f32 - 1.0) * gap;
        let left = ((full_w - row_w) / 2.0).max(16.0);
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.add_space(22.0);
            ui.horizontal(|ui| {
                ui.add_space(left);
                ui.label(RichText::new(tr("Onglets")).size(24.0).family(bold()).color(th.text));
                ui.label(RichText::new(tf("  {} onglet{}", &[&order.len(), &if order.len() > 1 { "s" } else { "" }])).size(14.0).color(th.sub));
            });
            ui.add_space(18.0);
            let items: Vec<Option<usize>> = order.iter().map(|&i| Some(i)).chain(std::iter::once(None)).collect();
            for row in items.chunks(cols) {
                let (rr, _) = ui.allocate_exact_size(vec2(full_w, card.y + gap), Sense::hover());
                for (k, item) in row.iter().enumerate() {
                    let r = Rect::from_min_size(pos2(rr.left() + left + k as f32 * (card.x + gap), rr.top()), card);
                    let thumb = Rect::from_min_size(r.min, vec2(card.x, card.y - 34.0));
                    let Some(i) = *item else {
                        // "+" card: new tab
                        let resp = ui.interact(thumb, egui::Id::new("ov-new"), Sense::click());
                        ui.painter().rect_stroke(thumb, CornerRadius::same(12), Stroke::new(1.5, if resp.hovered() { th.accent } else { th.border }), egui::StrokeKind::Inside);
                        icons::paint(ui.painter(), Rect::from_center_size(thumb.center(), vec2(30.0, 30.0)), Icon::Plus, th.sub);
                        if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            new_tab = true;
                        }
                        continue;
                    };
                    let tab = &self.tabs[i];
                    let resp = ui.interact(r, egui::Id::new(("ov-card", tab.id)), Sense::click());
                    let active = i == self.active;
                    let p = ui.painter();
                    p.rect_filled(thumb.translate(vec2(0.0, 2.0)), CornerRadius::same(12), Color32::from_black_alpha(20));
                    match &tab.thumb {
                        Some(tex) => {
                            p.rect_filled(thumb, CornerRadius::same(12), th.card);
                            egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), thumb.size()))
                                .corner_radius(CornerRadius::same(12))
                                .paint_at(ui, thumb);
                        }
                        None => {
                            p.rect_filled(thumb, CornerRadius::same(12), th.card);
                            site_badge(p, thumb.center(), 24.0, tab.url(), &tab.title());
                        }
                    }
                    if tab.sleeping || (i != self.active && tab.web.is_none() && tab.web_pending.is_some()) {
                        let badge = Rect::from_min_size(pos2(thumb.right() - 78.0, thumb.top() + 8.0), vec2(70.0, 20.0));
                        ui.painter().rect_filled(badge, CornerRadius::same(10), Color32::from_black_alpha(150));
                        ui.painter().text(badge.center(), Align2::CENTER_CENTER, tr("En veille"), FontId::proportional(11.0), Color32::WHITE);
                    }
                    let ring = if active { Stroke::new(3.0, th.accent) } else if resp.hovered() { Stroke::new(2.0, th.border) } else { Stroke::new(0.8, th.border) };
                    ui.painter().rect_stroke(thumb, CornerRadius::same(12), ring, egui::StrokeKind::Outside);
                    let title = tab.title();
                    site_badge(ui.painter(), pos2(r.left() + 9.0, thumb.bottom() + 17.0), 7.0, tab.url(), &title);
                    let clip = Rect::from_min_max(pos2(r.left() + 22.0, thumb.bottom()), pos2(r.right(), r.bottom()));
                    ui.painter().with_clip_rect(clip).text(pos2(clip.left(), thumb.bottom() + 17.0), Align2::LEFT_CENTER, title, FontId::proportional(13.0), th.text);
                    if resp.hovered() {
                        let cr = Rect::from_center_size(pos2(thumb.left() + 16.0, thumb.top() + 16.0), vec2(22.0, 22.0));
                        let cresp = ui.interact(cr, egui::Id::new(("ov-close", tab.id)), Sense::click());
                        ui.painter().circle_filled(cr.center(), 11.0, Color32::from_black_alpha(150));
                        icons::paint(ui.painter(), cr.shrink(6.0), Icon::Close, Color32::WHITE);
                        if cresp.clicked() {
                            close = Some(i);
                        }
                    }
                    if resp.clicked() && close.is_none() {
                        select = Some(i);
                    }
                }
            }
        });
        if let Some(i) = close {
            self.close_tab(i);
        } else if let Some(i) = select {
            self.active = i;
            self.overview_open = false;
        } else if new_tab {
            self.overview_open = false;
            self.open_tab("sanaga://nouvel-onglet", false);
        }
    }
}
