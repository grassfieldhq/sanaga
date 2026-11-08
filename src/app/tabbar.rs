//! The tab bar, as in macOS 27 (Safari's and the Finder's): a rounded track
//! holding equal-width tabs, the tab shown as a lighter pill inside it, the
//! title centred, the close button on the left of a hovered tab, and a round
//! plus button after the track. Pinned tabs are small icon pills at the start. A tab of
//! a group carries the group's colour in a small dot. Tabs drag to reorder and
//! have a right-click menu; very many tabs scroll with the wheel.

use super::chrome::site_badge;
use super::toolbar::{spinner, window_buttons, window_drag};
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Color32, Sense};

/// Height of the track and of the + button (macOS 27: 28).
const TRACK_H: f32 = 28.0;
/// The pill sits 2 inside the track.
const PILL_INSET: f32 = 2.0;
/// Width of a pinned tab.
const PIN_W: f32 = 40.0;
/// Below this width a tab shows only its icon.
const MIN_W: f32 = 96.0;

impl Sanaga {
    pub(super) fn tab_strip(&mut self, ui: &mut Ui, th: &Theme) {
        let full = ui.max_rect();
        self.sync_group_meta();
        let time = ui.input(|i| i.time);
        let bg = ui.interact(full, egui::Id::new("tabbar-bg"), Sense::click_and_drag());
        window_drag(ui, &bg);
        bg.menu_on_press(|ui| {
            if ui.button(tr("Nouvel onglet")).clicked() {
                self.open_tab("sanaga://newtab", false);
                ui.close();
            }
            if ui.button(tr("Nouveau groupe d'onglets")).clicked() {
                self.new_group("");
                let g = self.groups.len() - 1;
                self.group_editing = Some(g);
                ui.close();
            }
            if !self.closed.is_empty() && ui.button(tr("Rouvrir l'onglet fermé (Ctrl+Maj+T)")).clicked() {
                self.run_shortcut("reopen");
                ui.close();
            }
        });

        let cy = full.center().y;
        let mut left = full.left() + 8.0;
        if self.window_buttons_in() == super::layout::WindowButtonsIn::TabRow {
            left = window_buttons(ui, full.left() + 18.0, cy) + 14.0;
        }
        let plus_d = TRACK_H;
        let right = full.right() - 8.0 - plus_d - 6.0;
        let track = Rect::from_min_max(pos2(left, cy - TRACK_H / 2.0), pos2(right.max(left + 60.0), cy + TRACK_H / 2.0));
        let round = CornerRadius::same((TRACK_H / 2.0) as u8);
        ui.painter().rect_filled(track, round, th.track);

        // the tabs shown: pinned first, a folded group keeps only the tab shown
        let order = self.strip_tabs();
        let pinned_n = order.iter().filter(|&&i| self.tabs[i].pinned).count();
        let normal_n = (order.len() - pinned_n).max(1) as f32;
        let room = track.width() - pinned_n as f32 * PIN_W;
        // equal widths; very many tabs shrink to their icon, then scroll
        let w = (room / normal_n).max(44.0);
        let total = pinned_n as f32 * PIN_W + normal_n * w;
        let max_scroll = (total - track.width()).max(0.0);
        if max_scroll > 0.0 && ui.rect_contains_pointer(full) {
            let d = ui.input(|i| i.smooth_scroll_delta);
            self.tab_scroll -= if d.x.abs() > d.y.abs() { d.x } else { d.y };
        }
        let mut x = track.left() - self.tab_scroll;
        let rects: Vec<Rect> = order
            .iter()
            .map(|&i| {
                let tw = if self.tabs[i].pinned { PIN_W } else { w };
                let r = Rect::from_min_size(pos2(x, track.top()), vec2(tw, TRACK_H));
                x += tw;
                r
            })
            .collect();
        // keep the shown tab in view when it changes
        if let Some(p) = order.iter().position(|&i| i == self.active) {
            if p != self.tab_scroll_seen.0 || self.tabs.len() != self.tab_scroll_seen.1 {
                let ax = rects[p].left() - (track.left() - self.tab_scroll);
                if ax < self.tab_scroll {
                    self.tab_scroll = ax;
                } else if ax + rects[p].width() > self.tab_scroll + track.width() {
                    self.tab_scroll = ax + rects[p].width() - track.width();
                }
                self.tab_scroll_seen = (p, self.tabs.len());
            }
        }
        self.tab_scroll = self.tab_scroll.clamp(0.0, max_scroll);

        let saved_clip = ui.clip_rect();
        ui.set_clip_rect(track.intersect(saved_clip));
        let mut close: Option<usize> = None;
        let mut select: Option<usize> = None;
        let mut mute: Option<usize> = None;
        let mut drop_at: Option<(usize, usize)> = None;
        let mut menu_done = false;
        let active_pos = order.iter().position(|&i| i == self.active);
        let hovered_pos = rects.iter().position(|r| ui.rect_contains_pointer(*r));

        // thin separators between tabs that are neither shown nor hovered
        for k in 0..rects.len().saturating_sub(1) {
            let near = |p: Option<usize>| p == Some(k) || p == Some(k + 1);
            if !near(active_pos) && !near(hovered_pos) {
                let xr = rects[k].right();
                ui.painter().line_segment([pos2(xr, track.top() + 8.0), pos2(xr, track.bottom() - 8.0)], Stroke::new(1.0, th.hairline));
            }
        }

        for (k, &i) in order.iter().enumerate() {
            let rect = rects[k];
            if !track.intersects(rect) {
                continue;
            }
            let tab = &self.tabs[i];
            let resp = ui.interact(rect, ui.id().with(("tab", tab.id)), Sense::click_and_drag());
            let active = i == self.active;
            let hovered = resp.hovered();
            let tth = theme(self.store.settings.dark, tab.private);
            let pill = rect.shrink(PILL_INSET);
            let p = ui.painter();
            if active {
                // the shown tab: a light pill, lifted by a faint shadow
                p.add(egui::Shadow { offset: [0, 1], blur: 4, spread: 0, color: Color32::from_black_alpha(if self.store.settings.dark { 60 } else { 22 }) }.as_shape(pill, CornerRadius::same((pill.height() / 2.0) as u8)));
                p.rect_filled(pill, CornerRadius::same((pill.height() / 2.0) as u8), if tab.private { tth.tab_active } else { th.pill });
            } else if hovered {
                p.rect_filled(pill, CornerRadius::same((pill.height() / 2.0) as u8), th.pill.gamma_multiply(0.45));
            }
            let title = tab.title();
            let text_col = if active { tth.text } else { th.sub };
            let icon_only = tab.pinned || rect.width() < MIN_W - 0.5;
            // favicon + title, centred together (Safari); the icon alone when narrow
            let font = FontId::proportional(12.0);
            let g = ui.painter().layout_no_wrap(title.clone(), font.clone(), text_col);
            let max_text = (rect.width() - 2.0 * (16.0 + 10.0) - 20.0).max(0.0);
            let tw = g.size().x.min(max_text);
            let group_dot = (tab.group > 0 && !tab.pinned).then(|| self.group_color(tab.group));
            let lead = 16.0 + 6.0 + if group_dot.is_some() { 10.0 } else { 0.0 };
            let block = if icon_only { 16.0 } else { lead + tw };
            let x0 = rect.center().x - block / 2.0;
            let ic = pos2(x0 + 8.0, rect.center().y);
            let p = ui.painter();
            if tab.loading {
                spinner(p, ic, 6.0, time, th.accent);
            } else if tab.private {
                icons::paint(p, Rect::from_center_size(ic, vec2(14.0, 14.0)), Icon::Private, tth.accent);
            } else {
                site_badge(p, ic, 7.5, tab.url(), &title);
            }
            if !icon_only {
                let mut tx = x0 + 22.0;
                if let Some(col) = group_dot {
                    p.circle_filled(pos2(tx + 3.0, rect.center().y), 3.5, col);
                    tx += 10.0;
                }
                let clip = Rect::from_min_max(pos2(tx, rect.top()), pos2(tx + tw, rect.bottom()));
                p.with_clip_rect(clip.intersect(track)).galley(pos2(tx, rect.center().y - g.size().y / 2.0), g.clone(), text_col);
                if g.size().x > max_text {
                    // fade a long title out instead of cutting it
                    let fade_bg = if active { th.pill } else { Color32::TRANSPARENT };
                    if fade_bg != Color32::TRANSPARENT {
                        let fr = Rect::from_min_max(pos2(clip.right() - 14.0, rect.top() + 6.0), pos2(clip.right(), rect.bottom() - 6.0));
                        let mut mesh = egui::Mesh::default();
                        mesh.colored_vertex(fr.left_top(), Color32::TRANSPARENT);
                        mesh.colored_vertex(fr.right_top(), fade_bg);
                        mesh.colored_vertex(fr.right_bottom(), fade_bg);
                        mesh.colored_vertex(fr.left_bottom(), Color32::TRANSPARENT);
                        mesh.add_triangle(0, 1, 2);
                        mesh.add_triangle(0, 2, 3);
                        p.add(egui::Shape::mesh(mesh));
                    }
                }
            }
            // sound: a small speaker on the right, click to mute
            if (tab.audio || tab.muted) && !tab.pinned {
                let sr = Rect::from_center_size(pos2(pill.right() - 14.0, rect.center().y), vec2(18.0, 18.0));
                let sresp = ui.interact(sr, ui.id().with(("sound", tab.id)), Sense::click());
                if sresp.hovered() {
                    ui.painter().circle_filled(sr.center(), 9.0, th.track);
                }
                icons::paint(ui.painter(), sr.shrink(4.0), if tab.muted { Icon::SpeakerMuted } else { Icon::Speaker }, if tab.muted { th.sub } else { th.accent });
                if sresp.tip(if tab.muted { tr("Rétablir le son") } else { tr("Couper le son") }).clicked() {
                    mute = Some(i);
                }
            }
            // close button on the left of a hovered tab (macOS)
            if hovered && !tab.pinned {
                let cr = Rect::from_center_size(pos2(pill.left() + 13.0, rect.center().y), vec2(18.0, 18.0));
                let cresp = ui.interact(cr, ui.id().with(("close", tab.id)), Sense::click());
                ui.painter().circle_filled(cr.center(), 9.0, if cresp.hovered() { th.track } else { Color32::TRANSPARENT });
                icons::paint(ui.painter(), cr.shrink(5.0), Icon::Close, th.sub);
                if cresp.tip(tr("Fermer l'onglet (Ctrl+W)")).clicked() {
                    close = Some(i);
                }
            }
            // drag a tab to another place (pinned and normal tabs stay apart)
            if resp.dragged_by(egui::PointerButton::Primary) {
                if let Some(px) = ui.ctx().pointer_interact_pos() {
                    if let Some(t) = rects.iter().position(|r| px.x >= r.left() && px.x < r.right()) {
                        if t != k && self.tabs[order[t]].pinned == self.tabs[i].pinned {
                            drop_at = Some((i, order[t]));
                        }
                    }
                }
            }
            let resp = resp.tip(&title);
            resp.menu_on_press(|ui| {
                if self.tab_menu(ui, i) {
                    menu_done = true;
                    ui.close();
                }
            });
            if menu_done {
                ui.set_clip_rect(saved_clip);
                return;
            }
            if resp.middle_clicked() && !self.tabs[i].pinned {
                close = Some(i);
            } else if resp.clicked() && close.is_none() && mute.is_none() {
                select = Some(i);
            }
        }
        ui.set_clip_rect(saved_clip);

        // the round + after the track
        let pr = Rect::from_center_size(pos2(full.right() - 8.0 - plus_d / 2.0, cy), vec2(plus_d, plus_d));
        let presp = ui.interact(pr, egui::Id::new("tabbar-plus"), Sense::click());
        ui.painter().circle_filled(pr.center(), plus_d / 2.0, if presp.hovered() { th.track.gamma_multiply(1.6) } else { th.track });
        icons::paint(ui.painter(), pr.shrink(8.0), Icon::Plus, th.text);
        if presp.tip(tr("Nouvel onglet (Ctrl+T)")).clicked() {
            self.overview_open = false;
            self.open_tab("sanaga://newtab", false);
        }

        if let Some(i) = mute {
            self.toggle_mute(i);
        } else if let Some(i) = select {
            self.active = i;
            self.current_group = self.tabs[i].group;
            self.suggest_open = false;
        }
        if let Some((from, to)) = drop_at {
            self.move_tab(from, to);
        }
        if let Some(i) = close {
            self.close_tab(i);
        }
    }
}
