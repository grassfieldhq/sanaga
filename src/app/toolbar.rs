//! Safari-style top of the window: window buttons, sidebar toggle, back/forward,
//! a centred address field, the tab bar below, and the sidebar.

use super::chrome::select_all;
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align, Align2, Color32, Response, RichText, Sense, TextEdit};

/// A toolbar button inside a capsule (macOS 27): a soft round highlight on
/// hover, the accent colour when switched on.
pub(super) fn tool_button(ui: &mut Ui, r: Rect, icon: Icon, enabled: bool, on: bool, th: &Theme) -> Response {
    let resp = ui.interact(r, egui::Id::new(("tool", r.min.x as i32, r.min.y as i32)), if enabled { Sense::click() } else { Sense::hover() });
    let spot = Rect::from_center_size(r.center(), vec2(r.width().min(32.0), 28.0));
    if on {
        ui.painter().rect_filled(spot, CornerRadius::same(14), th.accent.gamma_multiply(0.14));
    } else if enabled && resp.hovered() {
        ui.painter().rect_filled(spot, CornerRadius::same(14), th.track);
    }
    let col = if on { th.accent } else if enabled { th.text.gamma_multiply(0.82) } else { th.sub.gamma_multiply(0.4) };
    icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(17.0, 17.0)), icon, col);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The capsules behind groups of toolbar buttons that touch each other, with
/// a hairline between back and forward (macOS 27).
fn capsules(groups: &[Rect], cy: f32, th: &Theme, dividers: bool) -> Vec<egui::Shape> {
    let mut runs: Vec<Vec<Rect>> = Vec::new();
    let mut sorted = groups.to_vec();
    sorted.sort_by(|a, b| a.left().total_cmp(&b.left()));
    for r in sorted {
        match runs.last_mut() {
            Some(run) if (r.left() - run.last().map(|l| l.right()).unwrap_or(f32::MIN)).abs() < 1.5 => run.push(r),
            _ => runs.push(vec![r]),
        }
    }
    let mut out = Vec::new();
    for run in runs {
        let (l, rgt) = (run[0].left(), run[run.len() - 1].right());
        let cap = Rect::from_min_max(pos2(l, cy - 18.0), pos2(rgt, cy + 18.0));
        let round = CornerRadius::same(18);
        out.push(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: th.capsule_shadow }.as_shape(cap, round).into());
        out.push(egui::Shape::rect_filled(cap, round, th.capsule));
        if dividers {
            for pair in run.windows(2) {
                let x = pair[0].right();
                out.push(egui::Shape::line_segment([pos2(x, cy - 10.0), pos2(x, cy + 10.0)], Stroke::new(1.0, th.hairline)));
            }
        }
    }
    out
}

/// Dragging an empty bar moves the window; double-click maximises it.
pub(super) fn window_drag(ui: &Ui, resp: &Response) {
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

pub(super) fn spinner(p: &egui::Painter, c: egui::Pos2, r: f32, time: f64, col: Color32) {
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
pub(super) fn window_buttons(ui: &mut Ui, x: f32, cy: f32) -> f32 {
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
/// The name of a toolbar button, as shown in the settings and menus.
pub(super) fn toolbar_button_label(key: &str) -> &str {
    match key {
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
        "summary" => tr("Résumer la page"),
        _ => key,
    }
}

/// A toolbar button's place: 0 shown, 1 in the » menu, 2 hidden.
pub(super) fn toolbar_state(toolbar: &str, key: &str) -> usize {
    for k in toolbar.split(',') {
        if k == key {
            return 0;
        }
        if k.strip_prefix('~') == Some(key) {
            return 1;
        }
        if k.strip_prefix('-') == Some(key) {
            return 2;
        }
    }
    2
}

pub(super) fn set_toolbar_state(toolbar: &mut String, key: &str, state: usize) {
    let (off, over) = (format!("-{key}"), format!("~{key}"));
    let mut keys: Vec<String> = toolbar.split(',').filter(|k| !k.is_empty() && *k != key && *k != off && *k != over).map(|k| k.to_string()).collect();
    keys.push(match state {
        0 => key.to_string(),
        1 => over,
        _ => off,
    });
    *toolbar = keys.join(",");
}

/// Tick boxes choosing the toolbar buttons; true when something changed.
pub(super) fn toolbar_checkboxes(ui: &mut Ui, toolbar: &mut String) -> bool {
    let mut changed = false;
    for (key, _) in crate::store::TOOLBAR_BUTTONS {
        let label = toolbar_button_label(key);
        let mut on = toolbar.split(',').any(|k| k == key);
        if ui.checkbox(&mut on, label).changed() {
            set_toolbar_state(toolbar, key, if on { 0 } else { 2 });
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

        // window buttons: in the tab row above (like Chrome), or here when
        // the tabs are elsewhere
        let mut x = full.left() + 12.0;
        if self.window_buttons_in() == super::layout::WindowButtonsIn::Toolbar && self.bar_at() == super::layout::BarAt::Top {
            x = window_buttons(ui, full.left() + 18.0, cy) + 16.0;
        }
        // buttons side by side share a capsule (their rects are kept for it)
        let capsule_layer = ui.painter().add(egui::Shape::Noop);
        let left_slots: std::cell::RefCell<Vec<Rect>> = Default::default();
        let right_slots: std::cell::RefCell<Vec<Rect>> = Default::default();
        let slot = |x: &mut f32, w: f32| {
            let r = Rect::from_min_size(pos2(*x, cy - 16.0), vec2(w, 32.0));
            *x += w;
            left_slots.borrow_mut().push(r);
            r
        };
        // right-click on the toolbar: choose its buttons (Chrome / Firefox)
        bg.menu_on_press(|ui| {
            ui.label(RichText::new(tr("Boutons de la barre d'outils")).strong());
            self.toolbar_menu(ui);
        });
        if self.tb("sidebar") {
            let sb = slot(&mut x, 36.0);
            if tool_button(ui, sb, Icon::Sidebar, true, self.sidebar_open, th).tip(tr("Barre latérale")).clicked() {
                self.sidebar_open = !self.sidebar_open;
            }
            x += 10.0;
        }
        if self.tb("back") {
            let back = slot(&mut x, 36.0);
            if tool_button(ui, back, Icon::Back, can_back, false, th).tip(tr("Page précédente (Alt+←)")).clicked() {
                self.go_history(a, -1);
            }
        }
        if self.tb("forward") {
            let fwd = slot(&mut x, 36.0);
            if tool_button(ui, fwd, Icon::Forward, can_fwd, false, th).tip(tr("Page suivante (Alt+→)")).clicked() {
                self.go_history(a, 1);
            }
        }
        if self.tb("reload_left") {
            let rl = slot(&mut x, 36.0);
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
            let hm = slot(&mut x, 36.0);
            if tool_button(ui, hm, Icon::Home, true, false, th).tip(tr("Page d'accueil")).clicked() {
                self.go_home(a);
            }
        }
        // profile badge (only for extra profiles, like Safari)
        if !self.store.profile.is_empty() {
            let emoji = self.store.settings.profile_emoji.trim();
            let drawable = !emoji.is_empty() && ui.painter().fonts_mut(|f| f.has_glyphs(&FontId::new(12.0, bold()), emoji));
            let name = if drawable { format!("{emoji} {}", self.store.profile) } else { self.store.profile.clone() };
            let g = ui.painter().layout_no_wrap(name.clone(), FontId::new(12.0, bold()), Color32::WHITE);
            let pr = Rect::from_min_size(pos2(x + 6.0, cy - 12.0), vec2(g.size().x + 20.0, 24.0));
            let presp = ui.interact(pr, egui::Id::new("profile-chip"), Sense::click());
            ui.painter().rect_filled(pr, CornerRadius::same(12), crate::theme::badge_color(&self.store.profile));
            ui.painter().galley(pos2(pr.left() + 10.0, cy - g.size().y / 2.0), g, Color32::WHITE);
            if presp.tip(tr("Profil de cette fenêtre — cliquer pour gérer les profils")).clicked() {
                let a = self.active;
                self.navigate(a, "sanaga://profiles".into(), None, true);
            }
            x = pr.right() + 4.0;
        }
        let left_end = x + 8.0;

        // right side, from the edge inwards
        let mut rx = full.right() - 12.0;
        let slot_r = |rx: &mut f32, w: f32| {
            let r = Rect::from_min_size(pos2(*rx - w, cy - 16.0), vec2(w, 32.0));
            *rx -= w;
            right_slots.borrow_mut().push(r);
            r
        };
        let menu_r = slot_r(&mut rx, 36.0);
        let mresp = tool_button(ui, menu_r, Icon::Menu, true, self.menu_open, th).tip(tr("Menu de Sanaga"));
        if mresp.clicked() {
            self.menu_open = !self.menu_open;
            self.overflow_open = false;
        }
        self.menu_rect = mresp.rect;
        // the buttons tucked away (»)
        if self.has_overflow() {
            let r = slot_r(&mut rx, 30.0);
            let resp = ui.interact(r, egui::Id::new("tb-overflow"), Sense::click());
            if resp.hovered() || self.overflow_open {
                ui.painter().rect_filled(r.shrink(2.0), CornerRadius::same(8), th.omnibox_hover);
            }
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "»", FontId::proportional(17.0), th.text);
            if resp.tip(tr("Autres boutons")).clicked() {
                self.overflow_open = !self.overflow_open;
                self.menu_open = false;
            }
            self.overflow_rect = r;
        }
        // the person's own buttons
        for k in (0..self.store.buttons.len()).rev() {
            let r = slot_r(&mut rx, 36.0);
            let resp = ui.interact(r, egui::Id::new(("tb-custom", k)), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r.shrink(2.0), CornerRadius::same(8), th.omnibox_hover);
            }
            super::buttons::paint_symbol(ui.painter(), r, &self.store.buttons[k], th.text);
            let tip = super::buttons::button_tip(&self.store.buttons[k]);
            if resp.tip(tip).clicked() {
                self.run_button(k);
            }
        }
        if self.tb("overview") {
            let ov_r = slot_r(&mut rx, 36.0);
            if tool_button(ui, ov_r, Icon::Grid, true, self.overview_open, th).tip(tr("Aperçu des onglets (Ctrl+Maj+O)")).clicked() {
                self.toggle_overview();
            }
        }
        let has_page = !self.tabs[a].url().starts_with("sanaga://");
        if self.tb("share") {
            let share_r = slot_r(&mut rx, 36.0);
            let sresp = tool_button(ui, share_r, Icon::Share, has_page, self.share_open, th).tip(tr("Partager"));
            if sresp.clicked() {
                self.share_open = !self.share_open;
                self.menu_open = false;
            }
            self.share_rect = sresp.rect;
        } else {
            self.share_rect = self.menu_rect;
        }
        if self.tb("summary") {
            let r = slot_r(&mut rx, 36.0);
            let on = self.summary.is_some();
            let can = matches!(self.tabs[a].view, View::Web);
            let resp = tool_button(ui, r, Icon::Sparkle, can, on || self.ai_menu_open, th).tip(tr("Résumer ou traduire la page"));
            self.ai_menu_rect = resp.rect;
            if resp.clicked() {
                if on && self.summary.as_ref().is_some_and(|s| s.is_for(self.tabs[a].id)) {
                    self.summary = None;
                } else {
                    self.ai_menu_open = !self.ai_menu_open;
                    self.menu_open = false;
                }
            }
        }
        if self.tb("mobile") {
            let r = slot_r(&mut rx, 36.0);
            let on = self.device.is_some();
            if tool_button(ui, r, Icon::Phone, true, on, th).tip(tr("Simulateur mobile (Ctrl+Maj+M)")).clicked() {
                self.toggle_devices();
            }
        }
        if self.tb("print") {
            let r = slot_r(&mut rx, 36.0);
            if tool_button(ui, r, Icon::Print, has_page, false, th).tip(tr("Imprimer… (Ctrl+P)")).clicked() {
                self.print_page();
            }
        }
        if self.tb("reading") {
            let r = slot_r(&mut rx, 36.0);
            if tool_button(ui, r, Icon::Glasses, true, false, th).tip(tr("Liste de lecture")).clicked() {
                self.sidebar_open = !self.sidebar_open;
            }
        }
        if self.tb("bookmarks") {
            let r = slot_r(&mut rx, 36.0);
            if tool_button(ui, r, Icon::StarFilled, true, false, th).tip(tr("Favoris")).clicked() {
                self.open_tab("sanaga://bookmarks", false);
            }
        }
        if self.tb("newtab") {
            let r = slot_r(&mut rx, 36.0);
            if tool_button(ui, r, Icon::Plus, true, false, th).tip(tr("Nouvel onglet (Ctrl+T)")).clicked() {
                self.open_tab("sanaga://newtab", false);
            }
        }
        if self.tb("downloads") && !self.store.downloads.is_empty() {
            let dl = slot_r(&mut rx, 36.0);
            if tool_button(ui, dl, Icon::Download, true, false, th).tip(tr("Téléchargements (Ctrl+J)")).clicked() {
                self.open_tab("sanaga://downloads", false);
            }
        }
        let now = ui.input(|i| i.time);
        if self.tb("memory") {
        if now - self.mem.0 > 2.0 {
            self.mem = (now, net::memory_used_cached());
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
        let rect = Rect::from_min_max(pos2(fl, cy - 18.0), pos2(fr, cy + 18.0));
        // the capsules, drawn under the buttons (not in the floating bar, a
        // capsule itself)
        if self.bar_at() != super::layout::BarAt::Floating {
            let mut shapes = capsules(&left_slots.borrow(), cy, th, true);
            shapes.extend(capsules(&right_slots.borrow(), cy, th, false));
            ui.painter().set(capsule_layer, egui::Shape::Vec(shapes));
        }
        self.address_field(ui, rect, loading, th);
    }

    fn address_field(&mut self, ui: &mut Ui, rect: Rect, loading: bool, th: &Theme) {
        let a = self.active;
        let id = egui::Id::new("address-bar");
        let focused = ui.memory(|m| m.has_focus(id));
        let hovered = ui.rect_contains_pointer(rect);
        if focused {
            ui.painter().rect_stroke(rect.expand(1.0), CornerRadius::same(19), Stroke::new(3.5, th.accent.gamma_multiply(0.35)), egui::StrokeKind::Outside);
            ui.painter().rect_filled(rect, CornerRadius::same(18), th.page);
        } else {
            // a capsule like the buttons' (macOS 27), a plain field in the floating bar
            if self.bar_at() == super::layout::BarAt::Floating {
                ui.painter().rect_filled(rect, CornerRadius::same(18), if hovered { th.omnibox_hover } else { th.omnibox });
            } else {
                ui.painter().add(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: th.capsule_shadow }.as_shape(rect, CornerRadius::same(18)));
                ui.painter().rect_filled(rect, CornerRadius::same(18), if hovered { th.capsule.gamma_multiply(0.85) } else { th.capsule });
            }
        }
        let url = self.tabs[a].url().to_string();
        let is_ntp = url.starts_with("sanaga://newtab");

        // bookmark star on the left, reload / stop on the right
        let star_r = Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(24.0, 24.0));
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("star-rect"), star_r));
        if is_ntp {
            icons::paint(ui.painter(), star_r.shrink(6.0), Icon::Search, th.sub);
        } else if (url.starts_with("sanaga://") && url != "sanaga://welcome") || !self.tb("star") {
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
                self.star_clicked(star_r);
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
}
