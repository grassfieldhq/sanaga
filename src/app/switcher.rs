//! The tab switcher: hold Ctrl and press Tab to see the tabs you used last,
//! as pictures of their pages, most recent first; let go of Ctrl to show the
//! one picked (a quick Ctrl+Tab goes back to the previous tab). Escape
//! cancels; a click picks. Ctrl+Page Down / Page Up still follow the tab bar.

use super::chrome::site_badge;
use super::*;
use eframe::egui::{Align2, Color32, Margin, Sense};

pub(super) struct Switcher {
    /// Tab ids, most recently used first.
    order: Vec<u64>,
    sel: usize,
    opened: f64,
}

const CARD: Vec2 = Vec2::new(212.0, 160.0);
const SHOWN: usize = 6;

impl Sanaga {
    /// Keep the order in which tabs were shown (most recent first).
    pub(super) fn note_recent(&mut self) {
        let Some(id) = self.tabs.get(self.active).map(|t| t.id) else { return };
        if self.recent.first() != Some(&id) {
            self.recent.retain(|x| *x != id);
            self.recent.insert(0, id);
            if !self.restoring {
                self.save_session();
            }
        }
        let alive: Vec<u64> = self.tabs.iter().map(|t| t.id).collect();
        self.recent.retain(|x| alive.contains(x));
    }

    /// Ctrl+Tab (1) / Ctrl+Shift+Tab (-1): open the switcher, or move in it.
    pub(super) fn switch_tabs(&mut self, step: i32) {
        if let Some(s) = self.switcher.as_mut() {
            let n = s.order.len() as i32;
            s.sel = (s.sel as i32 + step).rem_euclid(n) as usize;
            return;
        }
        let mut order = self.recent.clone();
        for t in &self.tabs {
            if !order.contains(&t.id) {
                order.push(t.id);
            }
        }
        if order.len() < 2 {
            return;
        }
        let sel = if step > 0 { 1 } else { order.len() - 1 };
        self.switcher = Some(Switcher { order, sel, opened: self.ctx.input(|i| i.time) });
        self.request_thumbnails();
        // the keyboard comes to Sanaga's interface (to see Ctrl let go)
        crate::shell::focus_interface();
    }

    fn switch_to(&mut self, id: u64) {
        if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
            self.active = i;
            self.current_group = self.tabs[i].group;
            self.switcher_refocus = true;
        }
    }

    pub(super) fn tab_switcher(&mut self, ctx: &egui::Context, th: &Theme) {
        // the page shown gets the keyboard back once it is on screen
        if self.switcher_refocus && self.switcher.is_none() {
            if let Some(p) = self.shown_page() {
                use gtk::prelude::WidgetExt;
                p.grab_focus();
                self.switcher_refocus = false;
            }
        }
        if self.switcher.is_none() {
            return;
        }
        let now = ctx.input(|i| i.time);
        // the keyboard's real state (egui only knows what reached it)
        let held = crate::shell::ctrl_held().unwrap_or_else(|| ctx.input(|i| i.modifiers.ctrl));
        let (enter, esc, left, right) = ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape), i.key_pressed(Key::ArrowLeft), i.key_pressed(Key::ArrowRight)));
        if esc {
            self.switcher = None;
            self.switcher_refocus = true;
            return;
        }
        if left || right {
            self.switch_tabs(if right { 1 } else { -1 });
        }
        let Some(s) = self.switcher.as_ref() else { return };
        if (!held && now - s.opened > 0.08) || enter {
            let id = s.order[s.sel];
            self.switcher = None;
            self.switch_to(id);
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(40));
        // the cards around the one picked
        let n = s.order.len();
        let first = s.sel.saturating_sub(SHOWN / 2).min(n.saturating_sub(SHOWN));
        let shown: Vec<(usize, u64)> = s.order.iter().copied().enumerate().skip(first).take(SHOWN).collect();
        let sel = s.sel;
        let screen = ctx.content_rect();
        let width = shown.len() as f32 * (CARD.x + 12.0) + 28.0;
        let mut pick: Option<u64> = None;
        let dark = self.store.settings.dark;
        egui::Area::new(egui::Id::new("tab-switcher")).order(egui::Order::Foreground).fixed_pos(pos2(screen.center().x - width / 2.0, screen.center().y - CARD.y / 2.0 - 40.0)).show(ctx, |ui| {
            Frame::new()
                .fill(th.page.gamma_multiply(0.97))
                .stroke(Stroke::new(1.0, th.border))
                .corner_radius(CornerRadius::same(18))
                .shadow(egui::Shadow { offset: [0, 12], blur: 36, spread: 0, color: Color32::from_black_alpha(if dark { 140 } else { 70 }) })
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        for (k, id) in &shown {
                            let Some(t) = self.tabs.iter().find(|t| t.id == *id) else { continue };
                            let (r, resp) = ui.allocate_exact_size(CARD + vec2(0.0, 26.0), Sense::click());
                            let pic = Rect::from_min_size(r.min, CARD);
                            let chosen = *k == sel;
                            ui.painter().rect_filled(pic, CornerRadius::same(10), th.card);
                            match &t.thumb {
                                Some(tex) => {
                                    let size = tex.size_vec2();
                                    let scale = (pic.width() / size.x).max(pic.height() / size.y);
                                    let uv_w = (pic.width() / (size.x * scale)).min(1.0);
                                    let uv_h = (pic.height() / (size.y * scale)).min(1.0);
                                    egui::Image::new(tex).uv(Rect::from_min_max(pos2(0.0, 0.0), pos2(uv_w, uv_h))).corner_radius(CornerRadius::same(10)).paint_at(ui, pic);
                                }
                                None => site_badge(ui.painter(), pic.center(), 22.0, t.url(), &t.title()),
                            }
                            let stroke = if chosen { Stroke::new(3.0, th.accent) } else { Stroke::new(1.0, th.border) };
                            ui.painter().rect_stroke(pic, CornerRadius::same(10), stroke, egui::StrokeKind::Outside);
                            site_badge(ui.painter(), pos2(r.left() + 9.0, r.bottom() - 11.0), 7.5, t.url(), &t.title());
                            let clip = Rect::from_min_max(pos2(r.left() + 22.0, r.bottom() - 22.0), r.right_bottom());
                            ui.painter().with_clip_rect(clip).text(pos2(clip.left(), clip.center().y), Align2::LEFT_CENTER, t.title(), FontId::proportional(12.5), if chosen { th.text } else { th.sub });
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                pick = Some(*id);
                            }
                        }
                    });
                    if n > SHOWN {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(tf("{} sur {}", &[&(sel + 1), &n])).size(11.5).color(th.sub));
                    }
                });
        });
        if let Some(id) = pick {
            self.switcher = None;
            self.switch_to(id);
        }
    }

    /// Ctrl+Page Down / Page Up: the next / previous tab of the tab bar.
    pub(super) fn tab_bar_step(&mut self, next: bool) {
        let a = self.active;
        let order = self.visible_tabs();
        if let Some(p) = order.iter().position(|&x| x == a) {
            let n = order.len();
            self.active = order[if next { (p + 1) % n } else { (p + n - 1) % n }];
        }
    }
}
