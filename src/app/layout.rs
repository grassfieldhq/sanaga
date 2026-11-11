//! Where the parts of the window go — the person chooses in Settings ›
//! Appearance: tabs on top, at the bottom, on the left or on the right
//! (vertical tabs, for many tabs), the address bar on top, at the bottom, or
//! floating. Also the tab list drawn down the side, the slim title bar used
//! when nothing else holds the window buttons, and the floating address bar.

use super::chrome::site_badge;
use super::toolbar::{spinner, window_buttons, window_drag};
use super::chrome::Fit as _;
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align2, Color32, CursorIcon, Margin, ScrollArea, Sense};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TabsAt {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum BarAt {
    Top,
    Bottom,
    Floating,
}

/// Which part holds the window's round buttons (close, minimise, maximise).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum WindowButtonsIn {
    TabRow,
    Toolbar,
    SideTabs,
    TitleBar,
}

pub(super) const VTABS_W: f32 = 248.0;
/// The toolbar (macOS 27: 36-pt capsules with room around them).
pub(super) const TOOLBAR_H: f32 = 52.0;
/// The tab bar under it (macOS 27: a 28-pt track, 8 under the toolbar).
pub(super) const TABBAR_H: f32 = 38.0;

/// Where a popover attached to `anchor` (a toolbar button, the address
/// field, a tab-bar label) opens: under it, or above it when the anchor is
/// in the lower half of the window (address bar or tabs at the bottom, as
/// in Safari). `left`: the popover's left edge. Gives the position for the
/// area, its pivot, and the height there is room for.
pub(super) fn popover_place(ctx: &egui::Context, anchor: Rect, left: f32, gap: f32) -> (egui::Pos2, Align2, f32) {
    let screen = ctx.content_rect();
    if anchor.center().y > screen.center().y {
        (pos2(left, anchor.top() - gap), Align2::LEFT_BOTTOM, (anchor.top() - gap - screen.top() - 16.0).max(160.0))
    } else {
        (pos2(left, anchor.bottom() + gap), Align2::LEFT_TOP, (screen.bottom() - anchor.bottom() - gap - 16.0).max(160.0))
    }
}

impl Sanaga {
    pub(super) fn tabs_at(&self) -> TabsAt {
        match self.store.settings.tabs_pos {
            1 => TabsAt::Bottom,
            2 => TabsAt::Left,
            3 => TabsAt::Right,
            _ => TabsAt::Top,
        }
    }

    pub(super) fn bar_at(&self) -> BarAt {
        match self.store.settings.bar_pos {
            1 => BarAt::Bottom,
            2 => BarAt::Floating,
            _ => BarAt::Top,
        }
    }

    pub(super) fn window_buttons_in(&self) -> WindowButtonsIn {
        match (self.tabs_at(), self.bar_at()) {
            // macOS 27: the toolbar on top holds them, the tab bar under it
            (_, BarAt::Top) => WindowButtonsIn::Toolbar,
            (TabsAt::Top, _) => WindowButtonsIn::TabRow,
            (TabsAt::Left, _) => WindowButtonsIn::SideTabs,
            _ => WindowButtonsIn::TitleBar,
        }
    }

    /// A slim bar on top with the window buttons and the page's title (when
    /// tabs and address bar are both elsewhere).
    pub(super) fn title_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let full = ui.max_rect();
        let bg = ui.interact(full, egui::Id::new("titlebar-bg"), Sense::click_and_drag());
        window_drag(ui, &bg);
        window_buttons(ui, full.left() + 18.0, full.center().y);
        let title = self.tabs[self.active].title();
        let clip = full.shrink2(vec2(90.0, 0.0));
        ui.painter().with_clip_rect(clip).fit(full.center(), Align2::CENTER_CENTER, title, FontId::proportional(12.5), th.sub);
    }

    /// The floating address bar: a rounded card over the bottom of the window.
    pub(super) fn floating_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let full = ui.max_rect();
        let w = (full.width() - 32.0).min(880.0);
        let card = Rect::from_center_size(pos2(full.center().x, full.center().y), vec2(w, 46.0));
        ui.painter().add(egui::Shadow { offset: [0, 6], blur: 22, spread: 0, color: Color32::from_black_alpha(if self.store.settings.dark { 90 } else { 38 }) }.as_shape(card, CornerRadius::same(23)));
        ui.painter().rect_filled(card, CornerRadius::same(23), th.toolbar);
        ui.painter().rect_stroke(card, CornerRadius::same(23), Stroke::new(1.0, th.border), egui::StrokeKind::Inside);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(card.shrink2(vec2(6.0, 0.0))).id_salt("floating-bar"));
        self.toolbar(&mut child, th);
    }

    /// The tabs as a list down the side of the window (left or right):
    /// pinned sites on top, then the groups and their tabs.
    pub(super) fn side_tabs(&mut self, ui: &mut Ui, th: &Theme) {
        let full = ui.max_rect();
        let time = ui.input(|i| i.time);
        let mut select: Option<usize> = None;
        let mut close: Option<usize> = None;
        let mut fold: Option<usize> = None;
        let mut drop_at: Option<(usize, usize)> = None;
        let mut new_tab = false;
        let mut menu_done = false;

        // header: window buttons (left side only) and the new-tab button
        let head = Rect::from_min_size(full.min, vec2(full.width(), 46.0));
        let hb = ui.interact(head, egui::Id::new("vtabs-head"), Sense::click_and_drag());
        window_drag(ui, &hb);
        let mut x = head.left() + 14.0;
        if self.window_buttons_in() == WindowButtonsIn::SideTabs {
            x = window_buttons(ui, head.left() + 18.0, head.center().y) + 16.0;
        } else {
            icons::logo(ui.painter(), pos2(x + 9.0, head.center().y), 9.0);
            ui.painter().text(pos2(x + 26.0, head.center().y), Align2::LEFT_CENTER, "Sanaga", FontId::new(13.5, bold()), th.text);
        }
        let _ = x;
        let plus = Rect::from_center_size(pos2(head.right() - 22.0, head.center().y), vec2(28.0, 28.0));
        let presp = ui.interact(plus, egui::Id::new("vtabs-plus"), Sense::click());
        if presp.hovered() {
            ui.painter().rect_filled(plus, CornerRadius::same(7), th.omnibox_hover);
        }
        icons::paint(ui.painter(), plus.shrink(7.0), Icon::Plus, th.text);
        if presp.tip(tr("Nouvel onglet (Ctrl+T)")).clicked() {
            new_tab = true;
        }

        // pinned sites: a grid of icons
        let order = self.visible_tabs();
        let pinned: Vec<usize> = order.iter().copied().filter(|&i| self.tabs[i].pinned).collect();
        let mut y = head.bottom() + 2.0;
        if !pinned.is_empty() {
            let cols = ((full.width() - 20.0) / 52.0).floor().max(1.0) as usize;
            let cw = (full.width() - 20.0) / cols as f32;
            for (k, &i) in pinned.iter().enumerate() {
                let r = Rect::from_min_size(pos2(full.left() + 10.0 + (k % cols) as f32 * cw, y + (k / cols) as f32 * 46.0), vec2(cw - 6.0, 40.0));
                let resp = ui.interact(r, egui::Id::new(("vpin", self.tabs[i].id)), Sense::click());
                let fill = if i == self.active { th.accent.gamma_multiply(0.18) } else if resp.hovered() { th.omnibox_hover } else { th.omnibox };
                ui.painter().rect_filled(r, CornerRadius::same(10), fill);
                site_badge(ui.painter(), r.center(), 9.0, self.tabs[i].url(), &self.tabs[i].title());
                let title = self.tabs[i].title();
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
                if resp.clicked() {
                    select = Some(i);
                }
            }
            y += pinned.len().div_ceil(cols) as f32 * 46.0 + 6.0;
            ui.painter().line_segment([pos2(full.left() + 14.0, y), pos2(full.right() - 14.0, y)], Stroke::new(1.0, th.border));
            y += 6.0;
        }

        // the other tabs, by group
        let list_rect = Rect::from_min_max(pos2(full.left(), y), pos2(full.right(), full.bottom() - 44.0));
        let rest: Vec<usize> = order.iter().copied().filter(|&i| !self.tabs[i].pinned).collect();
        // tree tabs: each tab under the one it was opened from
        let tree: Vec<(usize, usize, bool)> = if self.store.settings.tree_tabs { self.tree_order(&rest) } else { rest.iter().map(|&i| (i, 0, false)).collect() };
        let mut tree_fold: Option<usize> = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list_rect).id_salt("vtabs-list"));
        ScrollArea::vertical().id_salt("vtabs-scroll").auto_shrink(false).show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let mut last_group = usize::MAX;
            let mut rows: Vec<(usize, Rect)> = Vec::new();
            for &(i, depth, has_kids) in &tree {
                let g = self.tabs[i].group;
                if g != last_group && g > 0 {
                    // group header: colour, name, number of tabs; click folds it
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
                    let col = self.group_color(g);
                    let r = r.shrink2(vec2(8.0, 0.0));
                    if resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(7), th.omnibox_hover);
                    }
                    ui.painter().circle_filled(pos2(r.left() + 12.0, r.center().y), 5.0, col);
                    let name = if self.groups[g].is_empty() { tf("Groupe {}", &[&g]) } else { self.groups[g].clone() };
                    ui.painter().text(pos2(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, &name, FontId::new(12.5, bold()), col);
                    let n = self.tabs.iter().filter(|t| t.group == g).count();
                    let folded = self.group_folded(g);
                    ui.painter().text(pos2(r.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, format!("{n} {}", if folded { "▸" } else { "▾" }), FontId::proportional(11.5), th.sub);
                    let resp = resp.tip(tr("Cliquer pour replier le groupe ; clic droit pour le gérer"));
                    resp.menu_on_press(|ui| {
                        if self.group_menu(ui, g) {
                            menu_done = true;
                            ui.close();
                        }
                    });
                    if menu_done {
                        return;
                    }
                    if resp.clicked() {
                        fold = Some(g);
                    }
                }
                last_group = g;
                if g > 0 && self.group_folded(g) && i != self.active {
                    continue;
                }
                let indent = if g > 0 { 10.0 } else { 0.0 } + depth.min(5) as f32 * 14.0;
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click_and_drag());
                let r = Rect::from_min_max(pos2(r.left() + 8.0 + indent, r.top()), pos2(r.right() - 8.0, r.bottom()));
                rows.push((i, r));
                let tab = &self.tabs[i];
                let active = i == self.active;
                let tth = theme(self.store.settings.dark, tab.private);
                if active {
                    ui.painter().rect_filled(r, CornerRadius::same(9), th.toolbar);
                    ui.painter().rect_stroke(r, CornerRadius::same(9), Stroke::new(1.0, th.border), egui::StrokeKind::Inside);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(9), th.omnibox_hover);
                }
                if g > 0 {
                    ui.painter().rect_filled(Rect::from_min_size(pos2(r.left() - 6.0, r.top() + 8.0), vec2(2.5, r.height() - 16.0)), 1.0, self.group_color(g));
                }
                if has_kids {
                    // the branch's fold arrow, just left of the icon
                    let fr = Rect::from_center_size(pos2(r.left() + 2.0, r.center().y), vec2(12.0, 16.0));
                    let fresp = ui.interact(fr, egui::Id::new(("vtree", tab.id)), Sense::click());
                    let c = fr.center();
                    let col = if fresp.hovered() { th.text } else { th.sub };
                    let pts = if tab.tree_folded { vec![pos2(c.x - 2.0, c.y - 4.0), pos2(c.x + 3.0, c.y), pos2(c.x - 2.0, c.y + 4.0)] } else { vec![pos2(c.x - 4.0, c.y - 2.0), pos2(c.x + 4.0, c.y - 2.0), pos2(c.x, c.y + 3.0)] };
                    ui.painter().add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
                    if fresp.tip(if tab.tree_folded { tr("Déplier") } else { tr("Replier") }).clicked() {
                        tree_fold = Some(i);
                    }
                }
                if let Some(col) = self.container_color(&tab.container) {
                    ui.painter().rect_filled(Rect::from_min_size(pos2(r.right() - 4.0, r.top() + 7.0), vec2(3.0, r.height() - 14.0)), 1.5, col);
                }
                let ic = pos2(r.left() + 16.0, r.center().y);
                let title = tab.title();
                // close on the left: it takes the icon's place while the tab is hovered
                let close_left = !self.store.settings.close_right && ui.rect_contains_pointer(r);
                if close_left {
                    let cr = Rect::from_center_size(ic, vec2(20.0, 20.0));
                    let cresp = ui.interact(cr, egui::Id::new(("vclose-left", tab.id)), Sense::click());
                    ui.painter().rect_filled(cr, CornerRadius::same(6), if cresp.hovered() { th.border } else { th.omnibox_hover });
                    icons::paint(ui.painter(), cr.shrink(5.0), Icon::Close, th.sub);
                    if cresp.tip(tr("Fermer l'onglet (Ctrl+W)")).clicked() {
                        close = Some(i);
                    }
                } else if tab.loading {
                    spinner(ui.painter(), ic, 6.5, time, th.accent);
                } else if tab.private {
                    icons::paint(ui.painter(), Rect::from_center_size(ic, vec2(15.0, 15.0)), Icon::Private, tth.accent);
                } else {
                    site_badge(ui.painter(), ic, 8.0, tab.url(), &title);
                }
                let show_close = self.store.settings.close_right && (active || ui.rect_contains_pointer(r));
                let text_right = r.right() - if show_close { 30.0 } else { 10.0 };
                let clip = Rect::from_min_max(pos2(ic.x + 14.0, r.top()), pos2(text_right, r.bottom()));
                ui.painter().with_clip_rect(clip).fit(pos2(clip.left(), r.center().y), Align2::LEFT_CENTER, &title, FontId::proportional(13.0), if active { th.text } else { th.text.gamma_multiply(0.85) });
                if tab.audio || tab.muted {
                    icons::paint(ui.painter(), Rect::from_center_size(pos2(text_right - 6.0, r.center().y), vec2(12.0, 12.0)), if tab.muted { Icon::SpeakerMuted } else { Icon::Speaker }, th.sub);
                }
                if show_close {
                    let cr = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(20.0, 20.0));
                    let cresp = ui.interact(cr, egui::Id::new(("vclose", tab.id)), Sense::click());
                    if cresp.hovered() {
                        ui.painter().rect_filled(cr, CornerRadius::same(6), th.border);
                    }
                    icons::paint(ui.painter(), cr.shrink(5.0), Icon::Close, th.sub);
                    if cresp.tip(tr("Fermer l'onglet (Ctrl+W)")).clicked() {
                        close = Some(i);
                    }
                }
                if resp.dragged_by(egui::PointerButton::Primary) {
                    if let Some(p) = ui.ctx().pointer_interact_pos() {
                        if let Some(&(t, _)) = rows.iter().find(|(_, rr)| p.y >= rr.top() && p.y < rr.bottom()) {
                            if t != i {
                                drop_at = Some((i, t));
                            }
                        }
                    }
                }
                let resp = resp.tip(&title).on_hover_cursor(CursorIcon::PointingHand);
                resp.menu_on_press(|ui| {
                    if self.tab_menu(ui, i) {
                        menu_done = true;
                        ui.close();
                    }
                });
                if menu_done {
                    return;
                }
                if resp.middle_clicked() {
                    close = Some(i);
                } else if resp.clicked() && close.is_none() {
                    select = Some(i);
                }
            }
            // the place below the tabs opens a new one (like a tab bar's empty end)
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
            let r = r.shrink2(vec2(8.0, 0.0));
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(9), th.omnibox_hover);
            }
            icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(13.0, 13.0)), Icon::Plus, th.sub);
            ui.painter().text(pos2(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, tr("Nouvel onglet"), FontId::proportional(13.0), th.sub);
            if resp.clicked() {
                new_tab = true;
            }
        });
        if menu_done {
            return;
        }

        // bottom: number of tabs and the tab overview
        let foot = Rect::from_min_max(pos2(full.left(), full.bottom() - 44.0), full.right_bottom());
        ui.painter().line_segment([foot.left_top() + vec2(14.0, 0.0), foot.right_top() - vec2(14.0, 0.0)], Stroke::new(1.0, th.border));
        ui.painter().text(pos2(foot.left() + 18.0, foot.center().y), Align2::LEFT_CENTER, if self.tabs.len() == 1 { tr("1 onglet").to_string() } else { tf("{} onglets", &[&self.tabs.len()]) }, FontId::proportional(12.0), th.sub);
        let ov = Rect::from_center_size(pos2(foot.right() - 22.0, foot.center().y), vec2(28.0, 28.0));
        let oresp = ui.interact(ov, egui::Id::new("vtabs-overview"), Sense::click());
        if oresp.hovered() {
            ui.painter().rect_filled(ov, CornerRadius::same(7), th.omnibox_hover);
        }
        icons::paint(ui.painter(), ov.shrink(7.0), Icon::Grid, th.text);
        if oresp.tip(tr("Aperçu des onglets (Ctrl+Maj+O)")).clicked() {
            self.toggle_overview();
        }
        let sr = Rect::from_center_size(pos2(foot.right() - 54.0, foot.center().y), vec2(28.0, 28.0));
        let sresp = ui.interact(sr, egui::Id::new("vtabs-search"), Sense::click());
        if sresp.hovered() {
            ui.painter().rect_filled(sr, CornerRadius::same(7), th.omnibox_hover);
        }
        icons::paint(ui.painter(), sr.shrink(7.0), Icon::Search, th.text);
        if sresp.tip(tr("Rechercher dans les onglets (Ctrl+Maj+A)")).clicked() {
            self.tab_search_open = true;
        }

        if let Some(g) = fold {
            self.toggle_group_fold(g);
        }
        if let Some(i) = tree_fold {
            self.tabs[i].tree_folded = !self.tabs[i].tree_folded;
        }
        if let Some(i) = select {
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
        if new_tab {
            self.overview_open = false;
            self.open_tab("sanaga://newtab", false);
        }
    }

    /// The parts of the window around the page, where the person put them.
    pub(super) fn window_parts(&mut self, ui: &mut Ui, th: &Theme, base_th: &Theme) {
        let tabs = self.tabs_at();
        let bar = self.bar_at();
        let side = tabs == TabsAt::Left || tabs == TabsAt::Right;
        let buttons = self.window_buttons_in();
        // top edge, from the outside in
        if buttons == WindowButtonsIn::TitleBar && !self.app_mode {
            egui::Panel::top("titlebar").exact_size(34.0).show_separator_line(false).frame(Frame::NONE.fill(base_th.strip)).show(ui, |ui| {
                self.title_bar(ui, base_th);
            });
        }
        // tabs down the side: the whole height of the window
        if side {
            let frame = Frame::NONE.fill(base_th.strip).inner_margin(Margin::ZERO);
            let panel = if tabs == TabsAt::Left { egui::Panel::left("vtabs") } else { egui::Panel::right("vtabs") };
            panel.default_size(VTABS_W).size_range(180.0..=380.0).show_separator_line(true).frame(frame).show(ui, |ui| {
                self.side_tabs(ui, base_th);
            });
        }
        // macOS 27 (Safari, Finder): the toolbar, then the tab bar under it,
        // in one band of the window's colour
        // compact mode: lower bars, more room for the page
        let (toolbar_h, tabbar_h) = if self.store.settings.compact { (TOOLBAR_H - 10.0, TABBAR_H - 6.0) } else { (TOOLBAR_H, TABBAR_H) };
        if bar == BarAt::Top {
            egui::Panel::top("toolbar").exact_size(toolbar_h).show_separator_line(false).frame(Frame::NONE.fill(th.toolbar)).show(ui, |ui| {
                self.toolbar(ui, th);
            });
        }
        // the favourites bar: under the toolbar, above the tabs (Safari)
        if self.store.settings.show_bookmarks_bar && !self.app_mode {
            egui::Panel::top("bookmarks").exact_size(28.0).show_separator_line(false).frame(Frame::NONE.fill(th.toolbar)).show(ui, |ui| {
                self.bookmarks_bar(ui, th);
            });
        }
        if tabs == TabsAt::Top {
            egui::Panel::top("tabstrip").exact_size(tabbar_h).show_separator_line(false).frame(Frame::NONE.fill(th.toolbar)).show(ui, |ui| {
                self.tab_strip(ui, th);
            });
        }
        // bottom edge, from the outside in
        match bar {
            BarAt::Bottom => {
                egui::Panel::bottom("toolbar").exact_size(toolbar_h).show_separator_line(false).frame(Frame::NONE.fill(th.toolbar)).show(ui, |ui| {
                    self.toolbar(ui, th);
                });
            }
            BarAt::Floating => {
                egui::Panel::bottom("toolbar-floating").exact_size(64.0).show_separator_line(false).frame(Frame::NONE.fill(th.page)).show(ui, |ui| {
                    self.floating_bar(ui, th);
                });
            }
            BarAt::Top => {}
        }
        if tabs == TabsAt::Bottom {
            egui::Panel::bottom("tabstrip").exact_size(tabbar_h).show_separator_line(false).frame(Frame::NONE.fill(th.toolbar)).show(ui, |ui| {
                self.tab_strip(ui, th);
            });
        }
    }
}
