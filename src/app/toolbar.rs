//! Safari-style top of the window: window buttons, sidebar toggle, back/forward,
//! a centred address field, the tab bar below, and the sidebar.

use super::chrome::{select_all, site_badge};
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align, Align2, Color32, Response, RichText, ScrollArea, Sense, TextEdit};

/// A rounded toolbar button (grey square on hover, tinted when switched on).
pub(super) fn tool_button(ui: &mut Ui, r: Rect, icon: Icon, enabled: bool, on: bool, th: &Theme) -> Response {
    let resp = ui.interact(r, egui::Id::new(("tool", r.min.x as i32, r.min.y as i32)), if enabled { Sense::click() } else { Sense::hover() });
    if on {
        ui.painter().rect_filled(r, CornerRadius::same(7), th.omnibox);
    } else if enabled && resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(7), th.omnibox_hover);
    }
    let col = if enabled { th.text.gamma_multiply(0.85) } else { th.sub.gamma_multiply(0.45) };
    icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(16.0, 16.0)), icon, col);
    resp
}

/// Dragging an empty bar moves the window; double-click maximises it.
fn window_drag(ui: &Ui, resp: &Response) {
    if resp.drag_started_by(egui::PointerButton::Primary) && !crate::desktop::begin_move() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if resp.drag_stopped() {
        crate::desktop::end_move();
    }
    if resp.double_clicked() {
        crate::desktop::toggle_maximize(ui.ctx());
    }
}

fn spinner(p: &egui::Painter, c: egui::Pos2, r: f32, time: f64, col: Color32) {
    let a0 = (time * 6.0) as f32;
    let pts: Vec<egui::Pos2> = (0..=16)
        .map(|k| {
            let a = a0 + k as f32 / 16.0 * 4.5;
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.8, col)));
}


/// The three round window buttons; returns where they end on the right.
fn window_buttons(ui: &mut Ui, x: f32, cy: f32) -> f32 {
    let hover_lights = ui.rect_contains_pointer(Rect::from_min_size(pos2(x - 10.0, cy - 10.0), vec2(64.0, 20.0)));
    let lights = [Color32::from_rgb(0xFF, 0x5F, 0x57), Color32::from_rgb(0xFE, 0xBC, 0x2E), Color32::from_rgb(0x28, 0xC8, 0x40)];
    for (k, col) in lights.into_iter().enumerate() {
        let c = pos2(x + k as f32 * 20.0, cy);
        let resp = ui.interact(Rect::from_center_size(c, vec2(16.0, 16.0)), egui::Id::new(("winbtn", k)), Sense::click());
        ui.painter().circle_filled(c, 6.5, col);
        ui.painter().circle_stroke(c, 6.5, Stroke::new(0.6, Color32::from_black_alpha(40)));
        if hover_lights {
            let g = Stroke::new(1.2, Color32::from_black_alpha(140));
            match k {
                0 => {
                    ui.painter().line_segment([pos2(c.x - 2.5, c.y - 2.5), pos2(c.x + 2.5, c.y + 2.5)], g);
                    ui.painter().line_segment([pos2(c.x + 2.5, c.y - 2.5), pos2(c.x - 2.5, c.y + 2.5)], g);
                }
                1 => {
                    ui.painter().line_segment([pos2(c.x - 3.0, c.y), pos2(c.x + 3.0, c.y)], g);
                }
                _ => {
                    ui.painter().line_segment([pos2(c.x - 3.0, c.y), pos2(c.x + 3.0, c.y)], g);
                    ui.painter().line_segment([pos2(c.x, c.y - 3.0), pos2(c.x, c.y + 3.0)], g);
                }
            }
        }
        let tip = [tr("Fermer"), tr("Réduire"), tr("Agrandir")][k];
        if resp.tip(tip).clicked() {
            match k {
                0 => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                1 => crate::desktop::minimize(ui.ctx()),
                _ => crate::desktop::toggle_maximize(ui.ctx()),
            }
        }
    }
    x + 47.0
}
enum GroupAction {
    New(String),
    Switch(usize),
    Rename(usize, String),
    Close(usize),
}

/// Tick boxes choosing the toolbar buttons; true when something changed.
pub(super) fn toolbar_checkboxes(ui: &mut Ui, toolbar: &mut String) -> bool {
    let mut changed = false;
    for (key, _) in crate::store::TOOLBAR_BUTTONS {
        let label = match key {
            "sidebar" => tr("Barre latérale"),
            "back" => tr("Précédent"),
            "forward" => tr("Suivant"),
            "reload_left" => tr("Actualiser, à côté de Suivant (comme Chrome)"),
            "home" => tr("Accueil"),
            "star" => tr("Étoile des favoris, dans la barre d'adresse"),
            "reader" => tr("Lecteur, dans la barre d'adresse"),
            "reload" => tr("Actualiser, dans la barre d'adresse"),
            "newtab" => tr("Nouvel onglet"),
            "bookmarks" => tr("Favoris"),
            "reading" => tr("Liste de lecture"),
            "print" => tr("Imprimer"),
            "downloads" => tr("Téléchargements"),
            "memory" => tr("Mémoire utilisée"),
            "share" => tr("Partager"),
            "overview" => tr("Aperçu des onglets"),
            "mobile" => tr("Simulateur mobile"),
            _ => key,
        };
        let mut on = toolbar.split(',').any(|k| k == key);
        if ui.checkbox(&mut on, label).changed() {
            let off = format!("-{key}");
            let mut keys: Vec<String> = toolbar.split(',').filter(|k| !k.is_empty() && *k != key && *k != off).map(|k| k.to_string()).collect();
            keys.push(if on { key.to_string() } else { off });
            *toolbar = keys.join(",");
            changed = true;
        }
    }
    ui.separator();
    if ui.button(tr("Rétablir les boutons d'origine")).clicked() {
        *toolbar = crate::store::default_toolbar();
        changed = true;
    }
    changed
}

impl Sanaga {
    /// Tab row at the top of the window, in the style of Chrome: window buttons
    /// on the left, tabs aligned left with a rounded top, the active tab joined
    /// to the toolbar by two small curves, site icon on the left, close button
    /// on the right, and a + right after the last tab. Pinned tabs are small
    /// squares; tabs can be dragged and have a right-click menu.
    pub(super) fn tab_strip(&mut self, ui: &mut Ui, th: &Theme) {
        let full = ui.max_rect();
        self.sync_group_meta();
        let all = self.visible_tabs();
        let time = ui.input(|i| i.time);
        let bg = ui.interact(full, egui::Id::new("tabbar-bg"), Sense::click_and_drag());
        window_drag(ui, &bg);
        bg.menu_on_press(|ui| {
            if ui.button(tr("Nouvel onglet")).clicked() {
                self.open_tab("sanaga://nouvel-onglet", false);
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

        // window buttons
        let left = window_buttons(ui, full.left() + 18.0, full.center().y + 2.0) + 14.0;

        let top = full.top() + 6.0;
        let bottom = full.bottom();
        let pin_w = 44.0;
        // group labels (Chrome): before each group's first tab; a folded group
        // shows only its label
        let label_w = |name: &String| if name.is_empty() { 22.0 } else { ui.painter().layout_no_wrap(name.clone(), FontId::new(12.0, bold()), Color32::WHITE).size().x.min(140.0) + 20.0 } + 6.0;
        let mut order: Vec<usize> = Vec::new();
        let mut chip_before: Vec<Vec<(usize, f32)>> = Vec::new();
        let mut pending: Vec<(usize, f32)> = Vec::new();
        let mut last_group = usize::MAX;
        let mut chips_w = 0.0;
        for &i in &all {
            let g = if self.tabs[i].pinned { 0 } else { self.tabs[i].group };
            if g > 0 && g != last_group {
                let w = label_w(&self.groups[g]);
                pending.push((g, w));
                chips_w += w;
            }
            last_group = g;
            if i == self.active || !self.group_folded(g) {
                order.push(i);
                chip_before.push(std::mem::take(&mut pending));
            }
        }
        let chip_tail = pending;
        let pinned_n = order.iter().filter(|&&i| self.tabs[i].pinned).count();
        let normal_n = (order.len() - pinned_n).max(1) as f32;
        let room = (full.right() - left - 44.0 - 70.0 - pinned_n as f32 * pin_w - chips_w).max(60.0);
        // many tabs: they shrink down to just their icon (like Chrome)...
        let w = (room / normal_n).clamp(36.0, 240.0);
        // ...and when even that doesn't fit, the row scrolls with the wheel
        let total = normal_n * w + pinned_n as f32 * pin_w + chips_w;
        let visible = (full.right() - 44.0 - 70.0 - left).max(60.0);
        let max_scroll = (total - visible).max(0.0);
        if max_scroll > 0.0 && ui.rect_contains_pointer(full) {
            let d = ui.input(|i| i.smooth_scroll_delta);
            self.tab_scroll -= if d.x.abs() > d.y.abs() { d.x } else { d.y };
        }
        let mut x = left - self.tab_scroll;
        let mut chips: Vec<(usize, Rect)> = Vec::new();
        let rects: Vec<Rect> = order
            .iter()
            .enumerate()
            .map(|(k, &i)| {
                for &(g, cw) in &chip_before[k] {
                    chips.push((g, Rect::from_min_max(pos2(x + 3.0, top + 5.0), pos2(x + cw - 3.0, bottom - 9.0))));
                    x += cw;
                }
                let tw = if self.tabs[i].pinned { pin_w } else { w };
                let r = Rect::from_min_max(pos2(x, top), pos2(x + tw, bottom));
                x += tw;
                r
            })
            .collect();
        for &(g, cw) in &chip_tail {
            chips.push((g, Rect::from_min_max(pos2(x + 3.0, top + 5.0), pos2(x + cw - 3.0, bottom - 9.0))));
            x += cw;
        }
        if let Some(p) = order.iter().position(|&i| i == self.active) {
            if p != self.tab_scroll_seen.0 || self.tabs.len() != self.tab_scroll_seen.1 {
                // keep the shown tab in view when it changes
                let ax = rects[p].left() - (left - self.tab_scroll);
                let aw = rects[p].width();
                if ax < self.tab_scroll {
                    self.tab_scroll = ax;
                } else if ax + aw > self.tab_scroll + visible {
                    self.tab_scroll = ax + aw - visible;
                }
                self.tab_scroll_seen = (p, self.tabs.len());
            }
        }
        self.tab_scroll = self.tab_scroll.clamp(0.0, max_scroll);
        let tabs_end = x.min(left + visible);
        let strip_clip = Rect::from_min_max(pos2(left - if self.tab_scroll > 0.0 { 0.0 } else { 9.0 }, full.top()), pos2(left + visible + 9.0, full.bottom()));
        let saved_clip = ui.clip_rect();
        ui.set_clip_rect(strip_clip.intersect(saved_clip));

        let mut close: Option<usize> = None;
        let mut select: Option<usize> = None;
        let mut mute: Option<usize> = None;
        let mut drop_at: Option<(usize, usize)> = None;
        let mut menu_done = false;
        let active_pos = order.iter().position(|&i| i == self.active);
        let hovered_pos = rects.iter().position(|r| ui.rect_contains_pointer(*r));

        // separators between inactive tabs (hidden next to the active or hovered tab)
        for k in 0..rects.len().saturating_sub(1) {
            let near = |p: Option<usize>| p == Some(k) || p == Some(k + 1);
            if !near(active_pos) && !near(hovered_pos) {
                let xr = rects[k].right();
                ui.painter().line_segment([pos2(xr, top + 9.0), pos2(xr, bottom - 9.0)], Stroke::new(1.0, th.border));
            }
        }

        // group labels: click folds / unfolds the group, right-click manages it
        let mut fold: Option<usize> = None;
        for &(g, cr) in &chips {
            if !strip_clip.intersects(cr) {
                continue;
            }
            let col = self.group_color(g);
            let folded = self.group_folded(g);
            let cresp = ui.interact(cr, egui::Id::new(("group-label", g)), Sense::click());
            let name = self.groups[g].clone();
            if name.is_empty() {
                ui.painter().circle_filled(cr.center(), 7.0, col);
            } else {
                let fill = if cresp.hovered() { col.gamma_multiply(0.85) } else { col };
                ui.painter().rect_filled(cr, CornerRadius::same(7), fill);
                let painter = ui.painter().with_clip_rect(cr.shrink2(vec2(8.0, 0.0)));
                painter.text(pos2(cr.left() + 10.0, cr.center().y), Align2::LEFT_CENTER, name, FontId::new(12.0, bold()), Color32::WHITE);
            }
            let n = self.tabs.iter().filter(|t| t.group == g).count();
            let help = if folded { tf("Groupe replié ({} onglets) — cliquer pour l'ouvrir", &[&n]) } else { tr("Cliquer pour replier le groupe ; clic droit pour le gérer").to_string() };
            let cresp = cresp.tip(help);
            if cresp.clicked() {
                fold = Some(g);
            }
            cresp.menu_on_press(|ui| {
                if self.group_menu(ui, g) {
                    menu_done = true;
                    ui.close();
                }
            });
            if menu_done {
                return;
            }
            // just created: its menu opens under the label to name it
            let cmd = (self.group_editing == Some(g)).then_some(egui::SetOpenCommand::Bool(true));
            let mut close_edit = false;
            egui::Popup::new(egui::Id::new(("group-edit", g)), ui.ctx().clone(), egui::PopupAnchor::from(cr), ui.layer_id())
                .kind(egui::PopupKind::Menu)
                .open_memory(cmd)
                .show(|ui| {
                    if self.group_menu(ui, g) {
                        close_edit = true;
                        ui.close();
                    }
                });
            if close_edit {
                return;
            }
        }
        // a coloured line under the tabs of each group
        for (k, &i) in order.iter().enumerate() {
            let g = self.tabs[i].group;
            if g > 0 && !self.tabs[i].pinned && strip_clip.intersects(rects[k]) {
                let r = rects[k];
                let start = chips.iter().find(|c| c.0 == g && c.1.right() <= r.left() + 1.0 && r.left() - c.1.right() < 8.0).map(|c| c.1.left()).unwrap_or(r.left());
                ui.painter().rect_filled(Rect::from_min_max(pos2(start + 2.0, bottom - 3.0), pos2(r.right() - 2.0, bottom - 1.0)), 1.0, self.group_color(g));
            }
        }

        // draw inactive tabs first and the active one last (its curves overlap)
        let mut draw_order: Vec<usize> = (0..order.len()).collect();
        if let Some(a) = active_pos {
            draw_order.retain(|&k| k != a);
            draw_order.push(a);
        }
        for k in draw_order {
            let i = order[k];
            let rect = rects[k];
            if !strip_clip.intersects(rect) {
                continue; // scrolled out of view
            }
            let tab = &self.tabs[i];
            let resp = ui.interact(rect, ui.id().with(("tab", tab.id)), Sense::click_and_drag());
            let active = i == self.active;
            let tth = theme(self.store.settings.dark, tab.private);
            let p = ui.painter();
            if active {
                let fill = tth.toolbar;
                p.rect_filled(rect, CornerRadius { nw: 9, ne: 9, sw: 0, se: 0 }, fill);
                // the two little curves joining the tab to the toolbar
                let r = 8.0;
                for (side, sx) in [(-1.0f32, rect.left()), (1.0, rect.right())] {
                    let foot = Rect::from_min_max(pos2(sx.min(sx + side * r), bottom - r), pos2(sx.max(sx + side * r), bottom));
                    p.rect_filled(foot, 0.0, fill);
                    p.circle_filled(pos2(sx + side * r, bottom - r), r, th.strip);
                }
            } else if resp.hovered() {
                p.rect_filled(Rect::from_min_max(pos2(rect.left() + 2.0, top + 2.0), pos2(rect.right() - 2.0, bottom - 4.0)), CornerRadius::same(8), th.tab_hover);
            }
            if tab.private && !active {
                p.rect_filled(Rect::from_min_size(pos2(rect.left() + 12.0, bottom - 5.0), vec2(rect.width() - 24.0, 2.0)), 1.0, Color32::from_rgb(0x7B, 0x5C, 0xD6));
            }
            let text_col = if active { tth.text } else { th.sub };
            let title = tab.title();
            let cy = (top + bottom) / 2.0 - 1.0;
            if tab.pinned {
                let ic = pos2(rect.center().x, cy);
                if tab.loading {
                    spinner(p, ic, 6.0, time, th.accent);
                } else {
                    site_badge(p, ic, 8.0, tab.url(), &title);
                }
            } else {
                // icon on the left (centred, alone, on very narrow tabs)
                let narrow = rect.width() < 70.0;
                let ic = if narrow { pos2(rect.center().x, cy) } else { pos2(rect.left() + 20.0, cy) };
                if narrow && active {
                    // narrow shown tab: only its close button, like Chrome
                } else if tab.loading {
                    spinner(p, ic, 6.5, time, th.accent);
                } else if tab.private {
                    icons::paint(p, Rect::from_center_size(ic, vec2(15.0, 15.0)), Icon::Private, tth.accent);
                } else {
                    site_badge(p, ic, 8.0, tab.url(), &title);
                }
                let show_close = if narrow { active } else { active || resp.hovered() || rect.width() > 110.0 };
                let sound = (tab.audio || tab.muted) && !narrow;
                let mut text_right = rect.right() - if show_close { 30.0 } else { 10.0 };
                if sound {
                    let sr = Rect::from_center_size(pos2(text_right - 10.0, cy), vec2(20.0, 20.0));
                    let sresp = ui.interact(sr, ui.id().with(("sound", tab.id)), Sense::click());
                    if sresp.hovered() {
                        ui.painter().circle_filled(sr.center(), 10.0, th.border);
                    }
                    let icon = if tab.muted { Icon::SpeakerMuted } else { Icon::Speaker };
                    icons::paint(ui.painter(), sr.shrink(4.0), icon, if tab.muted { th.sub } else { th.accent });
                    if sresp.tip(if tab.muted { tr("Rétablir le son") } else { tr("Couper le son") }).clicked() {
                        mute = Some(i);
                    }
                    text_right = sr.left() - 2.0;
                }
                if rect.width() > 64.0 {
                    let clip = Rect::from_min_max(pos2(ic.x + 14.0, top), pos2(text_right, bottom));
                    let painter = ui.painter().with_clip_rect(clip);
                    painter.text(pos2(clip.left(), cy), Align2::LEFT_CENTER, &title, FontId::proportional(12.5), text_col);
                    // fade long titles out instead of cutting them
                    let fade_bg = if active { tth.toolbar } else if resp.hovered() { th.tab_hover } else { th.strip };
                    let fr = Rect::from_min_max(pos2(clip.right() - 18.0, top + 6.0), pos2(clip.right(), bottom - 6.0));
                    let mut mesh = egui::Mesh::default();
                    mesh.colored_vertex(fr.left_top(), fade_bg.gamma_multiply(0.0));
                    mesh.colored_vertex(fr.right_top(), fade_bg);
                    mesh.colored_vertex(fr.right_bottom(), fade_bg);
                    mesh.colored_vertex(fr.left_bottom(), fade_bg.gamma_multiply(0.0));
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    painter.add(egui::Shape::mesh(mesh));
                }
                if show_close {
                    let cr = Rect::from_center_size(pos2(if narrow { rect.center().x } else { rect.right() - 17.0 }, cy), vec2(18.0, 18.0));
                    let cresp = ui.interact(cr, ui.id().with(("close", tab.id)), Sense::click());
                    if cresp.hovered() {
                        ui.painter().circle_filled(cr.center(), 9.0, th.border);
                    }
                    icons::paint(ui.painter(), cr.shrink(4.5), Icon::Close, text_col);
                    if cresp.tip(tr("Fermer l'onglet (Ctrl+W)")).clicked() {
                        close = Some(i);
                    }
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
                return;
            }
            if resp.middle_clicked() && !self.tabs[i].pinned {
                close = Some(i);
            } else if resp.clicked() && close.is_none() && mute.is_none() {
                select = Some(i);
            }
        }

        ui.set_clip_rect(saved_clip);
        // + right after the last tab
        let pr = Rect::from_center_size(pos2(tabs_end + 20.0, (top + bottom) / 2.0 - 1.0), vec2(28.0, 28.0));
        let presp = ui.interact(pr, egui::Id::new("tabbar-plus"), Sense::click());
        if presp.hovered() {
            ui.painter().circle_filled(pr.center(), 14.0, th.tab_hover);
        }
        icons::paint(ui.painter(), pr.shrink(7.0), Icon::Plus, th.text);
        if presp.tip(tr("Nouvel onglet (Ctrl+T)")).clicked() {
            self.overview_open = false;
            self.open_tab("sanaga://nouvel-onglet", false);
        }

        if let Some(g) = fold {
            self.toggle_group_fold(g);
        }
        if let Some(i) = mute {
            self.toggle_mute(i);
        } else if let Some(i) = select {
            self.active = i;
            self.suggest_open = false;
        }
        if let Some((from, to)) = drop_at {
            self.move_tab(from, to);
        }
        if let Some(i) = close {
            self.close_tab(i);
        }
    }

    pub(super) fn toolbar(&mut self, ui: &mut Ui, th: &Theme) {
        let a = self.active;
        let full = ui.max_rect();
        let cy = full.center().y;
        let web = self.tabs[a].web.as_ref().filter(|_| matches!(self.tabs[a].view, View::Web));
        let can_back = self.tabs[a].idx > 0 || web.map(|w| w.can_go_back().unwrap_or(false)).unwrap_or(false);
        let can_fwd = self.tabs[a].idx + 1 < self.tabs[a].entries.len() || web.map(|w| w.can_go_forward().unwrap_or(false)).unwrap_or(false);
        let loading = self.tabs[a].loading;

        let bg = ui.interact(full, egui::Id::new("toolbar-bg"), Sense::click_and_drag());
        window_drag(ui, &bg);

        // (window buttons are in the tab row above, like Chrome)
        let mut x = full.left() + 12.0;
        let slot = |x: &mut f32, w: f32| {
            let r = Rect::from_min_size(pos2(*x, cy - 14.0), vec2(w, 28.0));
            *x += w + 4.0;
            r
        };
        // right-click on the toolbar: choose its buttons (Chrome / Firefox)
        bg.menu_on_press(|ui| {
            ui.label(RichText::new(tr("Boutons de la barre d'outils")).strong());
            self.toolbar_menu(ui);
        });
        if self.tb("sidebar") {
            let sb = slot(&mut x, 32.0);
            if tool_button(ui, sb, Icon::Sidebar, true, self.sidebar_open, th).tip(tr("Barre latérale")).clicked() {
                self.sidebar_open = !self.sidebar_open;
            }
            x += 6.0;
        }
        if self.tb("back") {
            let back = slot(&mut x, 30.0);
            if tool_button(ui, back, Icon::Back, can_back, false, th).tip(tr("Page précédente (Alt+←)")).clicked() {
                self.go_history(a, -1);
            }
        }
        if self.tb("forward") {
            let fwd = slot(&mut x, 30.0);
            if tool_button(ui, fwd, Icon::Forward, can_fwd, false, th).tip(tr("Page suivante (Alt+→)")).clicked() {
                self.go_history(a, 1);
            }
        }
        if self.tb("reload_left") {
            let rl = slot(&mut x, 30.0);
            let tip = if loading { tr("Arrêter (Échap)") } else { tr("Actualiser (Ctrl+R)") };
            if tool_button(ui, rl, if loading { Icon::Stop } else { Icon::Reload }, true, false, th).tip(tip).clicked() {
                if loading {
                    self.stop(a);
                } else {
                    self.reload(a);
                }
            }
        }
        if self.tb("home") {
            let hm = slot(&mut x, 30.0);
            if tool_button(ui, hm, Icon::Home, true, false, th).tip(tr("Page d'accueil")).clicked() {
                self.go_home(a);
            }
        }
        // profile badge (only for extra profiles, like Safari)
        if !self.store.profile.is_empty() {
            let name = self.store.profile.clone();
            let g = ui.painter().layout_no_wrap(name.clone(), FontId::new(12.0, bold()), Color32::WHITE);
            let pr = Rect::from_min_size(pos2(x + 6.0, cy - 12.0), vec2(g.size().x + 20.0, 24.0));
            let presp = ui.interact(pr, egui::Id::new("profile-chip"), Sense::click());
            ui.painter().rect_filled(pr, CornerRadius::same(12), crate::theme::badge_color(&name));
            ui.painter().galley(pos2(pr.left() + 10.0, cy - g.size().y / 2.0), g, Color32::WHITE);
            if presp.tip(tr("Profil de cette fenêtre — cliquer pour gérer les profils")).clicked() {
                let a = self.active;
                self.navigate(a, "sanaga://profils".into(), None, true);
            }
            x = pr.right() + 4.0;
        }
        let left_end = x + 8.0;

        // right side, from the edge inwards
        let mut rx = full.right() - 12.0;
        let slot_r = |rx: &mut f32, w: f32| {
            let r = Rect::from_min_size(pos2(*rx - w, cy - 14.0), vec2(w, 28.0));
            *rx -= w + 4.0;
            r
        };
        let menu_r = slot_r(&mut rx, 30.0);
        let mresp = tool_button(ui, menu_r, Icon::Menu, true, self.menu_open, th).tip(tr("Menu de Sanaga"));
        if mresp.clicked() {
            self.menu_open = !self.menu_open;
        }
        self.menu_rect = mresp.rect;
        if self.tb("overview") {
            let ov_r = slot_r(&mut rx, 30.0);
            if tool_button(ui, ov_r, Icon::Grid, true, self.overview_open, th).tip(tr("Aperçu des onglets (Ctrl+Maj+O)")).clicked() {
                self.toggle_overview();
            }
        }
        let has_page = !self.tabs[a].url().starts_with("sanaga://");
        if self.tb("share") {
            let share_r = slot_r(&mut rx, 30.0);
            let sresp = tool_button(ui, share_r, Icon::Share, has_page, self.share_open, th).tip(tr("Partager"));
            if sresp.clicked() {
                self.share_open = !self.share_open;
                self.menu_open = false;
            }
            self.share_rect = sresp.rect;
        } else {
            self.share_rect = self.menu_rect;
        }
        if self.tb("mobile") {
            let r = slot_r(&mut rx, 30.0);
            let on = self.device.is_some();
            if tool_button(ui, r, Icon::Phone, true, on, th).tip(tr("Simulateur mobile (Ctrl+Maj+M)")).clicked() {
                self.toggle_devices();
            }
        }
        if self.tb("print") {
            let r = slot_r(&mut rx, 30.0);
            if tool_button(ui, r, Icon::Print, has_page, false, th).tip(tr("Imprimer… (Ctrl+P)")).clicked() {
                self.print_page();
            }
        }
        if self.tb("reading") {
            let r = slot_r(&mut rx, 30.0);
            if tool_button(ui, r, Icon::Glasses, true, false, th).tip(tr("Liste de lecture")).clicked() {
                self.sidebar_open = !self.sidebar_open;
            }
        }
        if self.tb("bookmarks") {
            let r = slot_r(&mut rx, 30.0);
            if tool_button(ui, r, Icon::StarFilled, true, false, th).tip(tr("Favoris")).clicked() {
                self.open_tab("sanaga://favoris", false);
            }
        }
        if self.tb("newtab") {
            let r = slot_r(&mut rx, 30.0);
            if tool_button(ui, r, Icon::Plus, true, false, th).tip(tr("Nouvel onglet (Ctrl+T)")).clicked() {
                self.open_tab("sanaga://nouvel-onglet", false);
            }
        }
        if self.tb("downloads") && !self.store.downloads.is_empty() {
            let dl = slot_r(&mut rx, 30.0);
            if tool_button(ui, dl, Icon::Download, true, false, th).tip(tr("Téléchargements (Ctrl+J)")).clicked() {
                self.open_tab("sanaga://telechargements", false);
            }
        }
        let now = ui.input(|i| i.time);
        if self.tb("memory") {
        if now - self.mem.0 > 2.0 {
            self.mem = (now, net::memory_used().unwrap_or(0));
        }
        let mg = ui.painter().layout_no_wrap(tf("{} Mo", &[&format!("{:.0}", self.mem.1 as f64 / 1048576.0)]), FontId::proportional(11.5), th.sub);
        let mr = slot_r(&mut rx, mg.size().x + 30.0);
        let mresp = ui.interact(mr, egui::Id::new("memchip"), Sense::hover());
        if mresp.hovered() {
            ui.painter().rect_filled(mr, CornerRadius::same(7), th.omnibox_hover);
        }
        icons::paint(ui.painter(), Rect::from_center_size(pos2(mr.left() + 12.0, cy), vec2(12.0, 12.0)), Icon::Leaf, Color32::from_rgb(0x34, 0xC7, 0x59));
        ui.painter().galley(pos2(mr.left() + 22.0, cy - mg.size().y / 2.0), mg, th.sub);
        mresp.tip(tr("Mémoire utilisée par Sanaga et son moteur, en ce moment"));
        }
        let right_start = rx - 8.0;

        // centred address field
        let want = (full.width() * 0.46).clamp(340.0, 760.0);
        let mut fl = full.center().x - want / 2.0;
        let mut fr = full.center().x + want / 2.0;
        if fl < left_end {
            fr += left_end - fl;
            fl = left_end;
        }
        fr = fr.min(right_start).max(fl + 120.0);
        let rect = Rect::from_min_max(pos2(fl, cy - 17.0), pos2(fr, cy + 17.0));
        self.address_field(ui, rect, loading, th);
    }

    fn address_field(&mut self, ui: &mut Ui, rect: Rect, loading: bool, th: &Theme) {
        let a = self.active;
        let id = egui::Id::new("address-bar");
        let focused = ui.memory(|m| m.has_focus(id));
        let hovered = ui.rect_contains_pointer(rect);
        if focused {
            ui.painter().rect_stroke(rect.expand(1.0), CornerRadius::same(11), Stroke::new(3.0, th.accent.gamma_multiply(0.5)), egui::StrokeKind::Outside);
            ui.painter().rect_filled(rect, CornerRadius::same(10), th.page);
        } else {
            ui.painter().rect_filled(rect, CornerRadius::same(10), if hovered { th.omnibox_hover } else { th.omnibox });
        }
        let url = self.tabs[a].url().to_string();
        let is_ntp = url.starts_with("sanaga://nouvel-onglet");

        // bookmark star on the left, reload / stop on the right
        let star_r = Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(24.0, 24.0));
        if is_ntp {
            icons::paint(ui.painter(), star_r.shrink(6.0), Icon::Search, th.sub);
        } else if (url.starts_with("sanaga://") && url != "sanaga://bienvenue") || !self.tb("star") {
            // Sanaga's own pages: nothing to bookmark
        } else {
            let bookmarked = self.store.is_bookmarked(&url);
            let sresp = ui.interact(star_r, egui::Id::new("star"), Sense::click()).tip(if bookmarked {
                tr("Retirer des favoris (Ctrl+D)")
            } else {
                tr("Ajouter aux favoris (Ctrl+D)")
            });
            let (icon, col) = if bookmarked { (Icon::StarFilled, th.accent) } else { (Icon::Star, th.sub) };
            icons::paint(ui.painter(), star_r.shrink(5.0), icon, if sresp.hovered() && !bookmarked { th.text } else { col });
            if sresp.clicked() {
                self.toggle_bookmark();
            }
        }
        let mut text_right = rect.right() - 6.0;
        if !is_ntp && self.tb("reload") && !self.tb("reload_left") {
            let rr = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), vec2(24.0, 24.0));
            let rresp = ui.interact(rr, egui::Id::new("reload"), Sense::click());
            if rresp.hovered() {
                ui.painter().rect_filled(rr, CornerRadius::same(6), th.border);
            }
            icons::paint(ui.painter(), rr.shrink(6.0), if loading { Icon::Stop } else { Icon::Reload }, th.sub);
            if rresp.tip(if loading { tr("Arrêter (Échap)") } else { tr("Actualiser (Ctrl+R)") }).clicked() {
                if loading {
                    self.stop(a);
                } else {
                    self.reload(a);
                }
            }
            text_right = rr.left() - 2.0;
        }
        // Reader button, as in Safari's address field
        let can_read = (url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://")) && self.tb("reader");
        if can_read {
            let reading = self.tabs[a].reader;
            let rd = Rect::from_center_size(pos2(text_right - 12.0, rect.center().y), vec2(24.0, 24.0));
            let resp = ui.interact(rd, egui::Id::new("reader-btn"), Sense::click());
            if reading {
                ui.painter().rect_filled(rd, CornerRadius::same(6), th.accent);
            } else if resp.hovered() {
                ui.painter().rect_filled(rd, CornerRadius::same(6), th.border);
            }
            icons::paint(ui.painter(), rd.shrink(6.0), Icon::Reader, if reading { Color32::WHITE } else { th.sub });
            let tip = if reading { tr("Quitter le Lecteur (Ctrl+Maj+R)") } else { tr("Afficher dans le Lecteur : la page seule, facile à lire (Ctrl+Maj+R)") };
            if resp.tip(tip).clicked() {
                self.toggle_reader(a);
            }
            text_right = rd.left() - 2.0;
        }
        let zoom = self.tabs[a].zoom;
        if (zoom - 1.0).abs() > 0.01 {
            let zr = Rect::from_min_max(pos2(text_right - 46.0, rect.top() + 8.0), pos2(text_right - 2.0, rect.bottom() - 8.0));
            let zresp = ui.interact(zr, egui::Id::new("zoomchip"), Sense::click()).tip(tr("Réinitialiser le zoom"));
            ui.painter().rect_filled(zr, CornerRadius::same(5), th.border);
            ui.painter().text(zr.center(), Align2::CENTER_CENTER, format!("{:.0} %", zoom * 100.0), FontId::proportional(11.0), th.text);
            if zresp.clicked() {
                self.tabs[a].zoom = 1.0;
            }
            text_right = zr.left() - 4.0;
        }

        // when not editing: just the site's name, centred, with a padlock
        if !focused {
            let (label, col, lock) = if is_ntp || url.is_empty() {
                (tr("Rechercher ou saisir le nom d'un site").to_string(), th.sub, false)
            } else if url.starts_with("sanaga://") {
                (self.tabs[a].title(), th.text, false)
            } else {
                (host_of(&url), th.text, url.starts_with("https://"))
            };
            let g = ui.painter().layout_no_wrap(label, FontId::proportional(14.0), col);
            let gw = g.size().x.min(text_right - star_r.right() - 30.0).max(0.0);
            let lock_w = if lock { 17.0 } else { 0.0 };
            let x0 = rect.center().x - (gw + lock_w) / 2.0;
            if lock {
                icons::paint(ui.painter(), Rect::from_center_size(pos2(x0 + 5.0, rect.center().y), vec2(11.0, 11.0)), Icon::Lock, th.sub);
            }
            let clip = Rect::from_min_max(pos2(x0 + lock_w, rect.top()), pos2(x0 + lock_w + gw, rect.bottom()));
            ui.painter().with_clip_rect(clip).galley(pos2(clip.left(), rect.center().y - g.size().y / 2.0), g, col);
        }
        let te_rect = Rect::from_min_max(pos2(star_r.right() + 4.0, rect.top()), pos2(text_right, rect.bottom()));
        let engine = self.store.settings.engine;
        let home = self.store.settings.home.clone();
        // arrows move through the suggestions (the field shows the one picked),
        // Escape goes back to what was typed
        let n = self.suggest_items.len();
        if focused && self.suggest_open && n > 0 {
            let (down, up) = ui.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, Key::ArrowDown), i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)));
            let esc = self.suggest_sel.is_some() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
            if down || up || esc {
                if self.suggest_sel.is_none() {
                    self.suggest_typed = self.tabs[a].address.clone();
                }
                self.suggest_sel = if esc {
                    None
                } else {
                    match (self.suggest_sel, down) {
                        (None, true) => Some(0),
                        (None, false) => Some(n - 1),
                        (Some(k), true) => (k + 1 < n).then_some(k + 1),
                        (Some(k), false) => k.checked_sub(1),
                    }
                };
                let text = match self.suggest_sel {
                    Some(k) => self.suggestion_text(k),
                    None => self.suggest_typed.clone(),
                };
                self.tabs[a].address = text;
                // caret at the end
                if let Some(mut state) = TextEdit::load_state(ui.ctx(), id) {
                    let end = egui::text::CCursor::new(self.tabs[a].address.chars().count());
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
                    state.store(ui.ctx(), id);
                }
            }
        }
        let picked = self.suggest_sel.and_then(|k| self.suggest_items.get(k).map(|x| x.0.clone()));
        let tab = &mut self.tabs[a];
        let before = tab.address.clone();
        let mut te = TextEdit::singleline(&mut tab.address).id(id).frame(Frame::NONE).font(FontId::proportional(14.0)).vertical_align(Align::Center).desired_width(te_rect.width());
        te = if focused {
            te.text_color(th.text).hint_text(RichText::new(tf("Rechercher avec {} ou saisir une adresse", &[&short_engine(engine)])).color(th.sub))
        } else {
            te.text_color(Color32::TRANSPARENT)
        };
        let resp = ui.new_child(egui::UiBuilder::new().max_rect(te_rect).layout(egui::Layout::left_to_right(Align::Center))).add(te);
        if self.focus_address {
            resp.request_focus();
            self.focus_address = false;
            select_all(ui.ctx(), id, tab.address.chars().count());
        }
        if resp.gained_focus() {
            select_all(ui.ctx(), id, tab.address.chars().count());
        }
        if tab.address != before {
            self.suggest_open = true;
            self.suggest_sel = None;
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            match picked {
                Some(u) => self.activate_suggestion(&u),
                None => {
                    let u = input_to_url(&tab.address, engine, &home);
                    self.navigate(a, u, None, true);
                }
            }
        }
        // progress along the bottom of the field
        if loading {
            let t = ui.input(|i| i.time) as f32;
            let w = rect.width() * 0.3;
            let x = rect.left() + (t * 0.8).fract() * (rect.width() + w) - w;
            let bar = Rect::from_min_max(pos2(x.max(rect.left() + 8.0), rect.bottom() - 2.5), pos2((x + w).min(rect.right() - 8.0), rect.bottom() - 0.5));
            if bar.width() > 0.0 {
                ui.painter().rect_filled(bar, 1.0, th.accent);
            }
        }
        self.address_suggestions(ui, rect, th);
    }

    /// Is this toolbar button shown?
    pub(super) fn tb(&self, key: &str) -> bool {
        self.store.settings.toolbar.split(',').any(|k| k == key)
    }

    /// Tick boxes for the toolbar buttons (toolbar right-click).
    pub(super) fn toolbar_menu(&mut self, ui: &mut Ui) {
        if toolbar_checkboxes(ui, &mut self.store.settings.toolbar) {
            self.store.save_settings();
        }
    }

    /// Sidebar with bookmarks and recent history.
    pub(super) fn sidebar(&mut self, ui: &mut Ui, th: &Theme) {
        let mut go: Option<(String, bool)> = None;
        let mut open_page: Option<&'static str> = None;
        let mut remove_reading: Option<String> = None;
        let mut group_action: Option<GroupAction> = None;
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.add_space(10.0);
            let section = |ui: &mut Ui, title: &str| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.label(RichText::new(title).size(11.0).family(bold()).color(th.sub));
                });
                ui.add_space(2.0);
            };
            let row = |ui: &mut Ui, url: &str, title: &str, go: &mut Option<(String, bool)>| {
                let w = ui.available_width();
                let (r, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
                let r2 = r.shrink2(vec2(8.0, 1.0));
                if resp.hovered() {
                    ui.painter().rect_filled(r2, CornerRadius::same(6), th.omnibox_hover);
                }
                site_badge(ui.painter(), pos2(r2.left() + 14.0, r.center().y), 7.0, url, title);
                let clip = Rect::from_min_max(pos2(r2.left() + 28.0, r.top()), pos2(r2.right() - 6.0, r.bottom()));
                let t = if title.is_empty() { url } else { title };
                ui.painter().with_clip_rect(clip).text(pos2(clip.left(), r.center().y), Align2::LEFT_CENTER, t, FontId::proportional(13.0), th.text);
                let resp = resp.tip(url);
                if resp.middle_clicked() || (resp.clicked() && ui.input(|i| i.modifiers.command)) {
                    *go = Some((url.to_string(), true));
                } else if resp.clicked() {
                    *go = Some((url.to_string(), false));
                }
                r
            };
            section(ui, tr("LISTE DE LECTURE"));
            if self.store.reading.is_empty() {
                egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 12, top: 0, bottom: 0 }).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(tr("Gardez une page pour plus tard : menu ⋮ › Ajouter à la liste de lecture (Ctrl+Maj+D).")).size(12.0).color(th.sub)).wrap());
                });
            }
            for b in &self.store.reading {
                let r = row(ui, &b.url, &b.title, &mut go);
                if ui.rect_contains_pointer(r) {
                    let xr = Rect::from_center_size(pos2(r.right() - 22.0, r.center().y), vec2(20.0, 20.0));
                    let xresp = ui.interact(xr, egui::Id::new(("rl-rm", &b.url)), Sense::click());
                    ui.painter().rect_filled(xr, CornerRadius::same(5), if xresp.hovered() { th.border } else { th.sidebar });
                    icons::paint(ui.painter(), xr.shrink(5.0), Icon::Close, th.sub);
                    if xresp.tip(tr("Retirer de la liste de lecture")).clicked() {
                        remove_reading = Some(b.url.clone());
                    }
                }
            }
            section(ui, tr("GROUPES D'ONGLETS"));
            let mut switch: Option<usize> = None;
            let mut rename: Option<(usize, String)> = None;
            for g in 0..self.groups.len() {
                let n = self.tabs.iter().filter(|t| t.group == g).count();
                let w = ui.available_width();
                let (r, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
                let r2 = r.shrink2(vec2(8.0, 1.0));
                let current = g == self.current_group;
                if current {
                    ui.painter().rect_filled(r2, CornerRadius::same(6), th.accent.gamma_multiply(0.18));
                } else if resp.hovered() {
                    ui.painter().rect_filled(r2, CornerRadius::same(6), th.omnibox_hover);
                }
                if g == 0 {
                    icons::paint(ui.painter(), Rect::from_center_size(pos2(r2.left() + 14.0, r.center().y), vec2(13.0, 13.0)), Icon::Grid, if current { th.accent } else { th.sub });
                } else {
                    ui.painter().circle_filled(pos2(r2.left() + 14.0, r.center().y), 5.5, self.group_color(g));
                }
                ui.painter().text(pos2(r2.left() + 28.0, r.center().y), Align2::LEFT_CENTER, self.group_label(g), FontId::proportional(13.0), th.text);
                ui.painter().text(pos2(r2.right() - 6.0, r.center().y), Align2::RIGHT_CENTER, n.to_string(), FontId::proportional(12.0), th.sub);
                if resp.clicked() {
                    switch = Some(g);
                }
                if g > 0 {
                    resp.menu_on_press(|ui| {
                        if ui.button(tr("Renommer avec le nom saisi ci-dessous")).clicked() && !self.new_group_name.trim().is_empty() {
                            rename = Some((g, self.new_group_name.trim().to_string()));
                            ui.close();
                        }
                        if ui.button(tr("Fermer le groupe et ses onglets")).clicked() {
                            rename = Some((g, String::new()));
                            ui.close();
                        }
                    });
                }
            }
            let mut create = false;
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                let resp = ui.add(egui::TextEdit::singleline(&mut self.new_group_name).hint_text(tr("Nom du nouveau groupe")).desired_width(ui.available_width() - 60.0));
                if (resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) | ui.button("+").tip(tr("Nouveau groupe d'onglets")).clicked() {
                    create = true;
                }
            });
            if create {
                let name = std::mem::take(&mut self.new_group_name);
                group_action = Some(GroupAction::New(name));
            } else if let Some((g, name)) = rename {
                group_action = Some(if name.is_empty() { GroupAction::Close(g) } else { GroupAction::Rename(g, name) });
            } else if let Some(g) = switch {
                group_action = Some(GroupAction::Switch(g));
            }
            section(ui, tr("FAVORIS"));
            for b in &self.store.bookmarks {
                row(ui, &b.url, &b.title, &mut go);
            }
            section(ui, tr("HISTORIQUE RÉCENT"));
            let mut seen = HashSet::new();
            for v in self.store.history.iter().rev() {
                if seen.len() >= 15 {
                    break;
                }
                if seen.insert(v.url.clone()) {
                    row(ui, &v.url, &v.title, &mut go);
                }
            }
            if self.store.history.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.label(RichText::new(tr("Aucune page visitée")).size(12.5).color(th.sub));
                });
            }
            ui.add_space(12.0);
            for (label, page) in [(tr("Tout l'historique"), "sanaga://historique"), (tr("Téléchargements"), "sanaga://telechargements"), (tr("Paramètres"), "sanaga://parametres")] {
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    if ui.link(RichText::new(label).size(12.5).color(th.accent)).clicked() {
                        open_page = Some(page);
                    }
                });
            }
        });
        match group_action {
            Some(GroupAction::New(name)) => self.new_group(&name),
            Some(GroupAction::Switch(g)) => {
                self.overview_open = false;
                self.switch_group(g);
            }
            Some(GroupAction::Rename(g, name)) => {
                self.groups[g] = name;
                self.save_session();
            }
            Some(GroupAction::Close(g)) => self.close_group(g),
            None => {}
        }
        if let Some(u) = remove_reading {
            self.store.reading.retain(|b| b.url != u);
            self.store.save_reading();
        }
        self.apply_go(go);
        if let Some(p) = open_page {
            self.navigate(self.active, p.to_string(), None, true);
        }
    }
}
