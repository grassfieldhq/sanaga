//! The sidebar, as in macOS 27 (Safari, Finder): a floating panel inset from
//! the window's edges, with rounded corners, a light rim and a soft shadow;
//! a search field; sections with small headers that fold (tab groups,
//! favourites, reading list, recent history); 32-pt rows with a neutral
//! selection, icons in the accent colour, and a complete right-click menu on
//! every row. Groups are renamed in place (double-click), favourites too.

use super::chrome::site_badge;
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align, Align2, Color32, CursorIcon, Layout, Margin, RichText, ScrollArea, Sense, TextEdit, UiBuilder};

/// What a row of the sidebar asks for, done after drawing.
enum Do {
    Open(String, bool),
    OpenWindow(String),
    Copy(String),
    SwitchGroup(usize),
    NewGroup,
    RenameGroup(usize, String),
    GroupColor(usize, u8),
    NewTabInGroup(usize),
    Ungroup(usize),
    CloseGroup(usize),
    GroupToWindow(usize),
    RemoveReading(String),
    RemoveBookmark(usize),
    EditBookmark(usize),
    SaveBookmark(usize, String, String),
    Page(&'static str),
}

/// The neutral selection and hover fills of the rows (macOS 27 Finder).
fn select_fill(dark: bool, strong: bool) -> Color32 {
    let a = if strong { 24 } else { 12 };
    if dark { Color32::from_white_alpha(a) } else { Color32::from_black_alpha(a) }
}

impl Sanaga {
    pub(super) fn sidebar(&mut self, ui: &mut Ui, th: &Theme) {
        let full = ui.max_rect();
        let dark = self.store.settings.dark;
        // the floating panel
        let panel = full.shrink(8.0);
        let round = CornerRadius::same(12);
        ui.painter().add(egui::Shadow { offset: [0, 1], blur: 12, spread: 0, color: Color32::from_black_alpha(if dark { 76 } else { 20 }) }.as_shape(panel, round));
        ui.painter().rect_filled(panel, round, th.sidebar);
        ui.painter().rect_stroke(panel, round, Stroke::new(1.0, if dark { Color32::from_white_alpha(20) } else { Color32::WHITE }), egui::StrokeKind::Inside);

        let mut acts: Vec<Do> = Vec::new();
        let mut inner = ui.new_child(UiBuilder::new().max_rect(panel.shrink2(vec2(10.0, 10.0))).layout(Layout::top_down(Align::Min)));
        let ui = &mut inner;
        // search
        Frame::new().fill(select_fill(dark, false)).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(8, 0)).show(ui, |ui| {
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 28.0), Layout::left_to_right(Align::Center), |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                icons::paint(ui.painter(), r, Icon::Search, th.sub);
                ui.add_space(2.0);
                ui.add(TextEdit::singleline(&mut self.sidebar_filter).id(egui::Id::new("sidebar-search")).frame(Frame::NONE).font(FontId::proportional(13.0)).vertical_align(Align::Center).hint_text(RichText::new(tr("Rechercher")).color(th.sub)).desired_width(ui.available_width()));
            });
        });
        ui.add_space(6.0);
        let q = self.sidebar_filter.trim().to_lowercase();
        let matches = |u: &str, t: &str| q.is_empty() || u.to_lowercase().contains(&q) || t.to_lowercase().contains(&q);

        ScrollArea::vertical().id_salt("sidebar-scroll").auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            // ---------------- tab groups ----------------
            if q.is_empty() && self.section_header(ui, th, "sb-groups", tr("Groupes d'onglets"), Some((tr("Nouveau groupe d'onglets"), &mut acts))) {
                for g in 0..self.groups.len() {
                    let n = self.tabs.iter().filter(|t| t.group == g && !t.pinned).count();
                    let current = g == self.current_group;
                    let renaming = self.group_renaming == Some(g);
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                    if current {
                        ui.painter().rect_filled(r, CornerRadius::same(8), select_fill(dark, true));
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(8), select_fill(dark, false));
                    }
                    let ic = Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(16.0, 16.0));
                    if g == 0 {
                        icons::paint(ui.painter(), ic, Icon::Grid, th.accent);
                    } else {
                        // a group: a rounded square in its colour
                        ui.painter().rect_filled(ic.shrink(1.5), CornerRadius::same(4), self.group_color(g));
                    }
                    if renaming {
                        let fr = Rect::from_min_max(pos2(r.left() + 32.0, r.top() + 4.0), pos2(r.right() - 8.0, r.bottom() - 4.0));
                        let mut c = ui.new_child(UiBuilder::new().max_rect(fr));
                        let id = egui::Id::new(("group-rename", g));
                        let te = c.add(TextEdit::singleline(&mut self.groups[g]).id(id).hint_text(tr("Nommer ce groupe")).desired_width(fr.width()));
                        if !te.has_focus() && !te.lost_focus() {
                            te.request_focus();
                        }
                        if te.lost_focus() {
                            let name = self.groups[g].clone();
                            acts.push(Do::RenameGroup(g, name));
                        }
                    } else {
                        let label = if g == 0 { tr("Onglets").to_string() } else { self.group_label(g) };
                        let clip = Rect::from_min_max(pos2(r.left() + 32.0, r.top()), pos2(r.right() - 34.0, r.bottom()));
                        ui.painter().with_clip_rect(clip).text(pos2(clip.left(), r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.0), th.text);
                        ui.painter().text(pos2(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, n.to_string(), FontId::proportional(12.0), th.sub);
                    }
                    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
                    if resp.double_clicked() && g > 0 {
                        self.group_renaming = Some(g);
                    } else if resp.clicked() && !renaming {
                        acts.push(Do::SwitchGroup(g));
                    }
                    resp.menu_on_press(|ui| {
                        ui.set_min_width(220.0);
                        if menu_item(ui, Icon::Grid, tr("Afficher ce groupe")) {
                            acts.push(Do::SwitchGroup(g));
                        }
                        if menu_item(ui, Icon::Plus, tr("Nouvel onglet dans le groupe")) {
                            acts.push(Do::NewTabInGroup(g));
                        }
                        if g > 0 {
                            if menu_item(ui, Icon::Pencil, tr("Renommer")) {
                                self.group_renaming = Some(g);
                            }
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.add_space(6.0);
                                for (k, (name, col)) in super::tabs::GROUP_COLORS.iter().enumerate() {
                                    let (cr, cresp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                                    ui.painter().circle_filled(cr.center(), 7.0, *col);
                                    if self.group_colors.get(g).copied() == Some(k as u8) {
                                        ui.painter().circle_stroke(cr.center(), 9.5, Stroke::new(1.5, *col));
                                    }
                                    if cresp.on_hover_cursor(CursorIcon::PointingHand).tip(tr(name)).clicked() {
                                        acts.push(Do::GroupColor(g, k as u8));
                                    }
                                }
                            });
                            ui.add_space(4.0);
                            ui.separator();
                            if menu_item(ui, Icon::Share, tr("Ouvrir dans une nouvelle fenêtre")) {
                                acts.push(Do::GroupToWindow(g));
                            }
                            if menu_item(ui, Icon::Grid, tr("Dissocier les onglets")) {
                                acts.push(Do::Ungroup(g));
                            }
                            if menu_item(ui, Icon::Close, tr("Fermer le groupe")) {
                                acts.push(Do::CloseGroup(g));
                            }
                        }
                    });
                }
                ui.add_space(8.0);
            }

            // ---------------- favourites ----------------
            let favs: Vec<(usize, String, String)> = self.store.bookmarks.iter().enumerate().filter(|(_, b)| matches(&b.url, &b.title)).map(|(i, b)| (i, b.url.clone(), b.title.clone())).collect();
            if self.section_header(ui, th, "sb-favs", tr("Favoris"), None) {
                if favs.is_empty() {
                    hint(ui, th, if q.is_empty() { tr("Cliquez sur l'étoile de la barre d'adresse pour ajouter une page.") } else { tr("Aucun résultat") });
                }
                for (i, url, title) in favs {
                    if self.bm_editing == Some(i) {
                        // edit in place: name and address
                        Frame::new().fill(select_fill(dark, false)).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(8)).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 6.0;
                            ui.add(TextEdit::singleline(&mut self.bm_edit.0).hint_text(tr("Nom")).desired_width(f32::INFINITY));
                            ui.add(TextEdit::singleline(&mut self.bm_edit.1).hint_text(tr("Adresse")).desired_width(f32::INFINITY));
                            ui.horizontal(|ui| {
                                if ui.add(egui::Button::new(RichText::new(tr("Enregistrer")).color(Color32::WHITE)).fill(th.accent)).clicked() {
                                    acts.push(Do::SaveBookmark(i, self.bm_edit.0.clone(), self.bm_edit.1.clone()));
                                }
                                if ui.button(tr("Annuler")).clicked() {
                                    self.bm_editing = None;
                                }
                            });
                        });
                        ui.add_space(2.0);
                        continue;
                    }
                    let resp = page_row(ui, th, dark, &url, &title, "");
                    link_actions(ui, &resp, &url, &mut acts);
                    resp.menu_on_press(|ui| {
                        ui.set_min_width(220.0);
                        link_menu(ui, &url, &mut acts);
                        ui.separator();
                        if menu_item(ui, Icon::Pencil, tr("Modifier…")) {
                            acts.push(Do::EditBookmark(i));
                        }
                        if menu_item(ui, Icon::Close, tr("Supprimer")) {
                            acts.push(Do::RemoveBookmark(i));
                        }
                    });
                }
                if q.is_empty() && link_row(ui, th, dark, Icon::StarFilled, tr("Gérer les favoris")) {
                    acts.push(Do::Page("sanaga://bookmarks"));
                }
                ui.add_space(8.0);
            }

            // ---------------- reading list ----------------
            let reading: Vec<(String, String)> = self.store.reading.iter().filter(|b| matches(&b.url, &b.title)).map(|b| (b.url.clone(), b.title.clone())).collect();
            if self.section_header(ui, th, "sb-reading", tr("Liste de lecture"), None) {
                if reading.is_empty() {
                    hint(ui, th, if q.is_empty() { tr("Gardez une page pour plus tard : menu ⋮ › Ajouter à la liste de lecture (Ctrl+Maj+D).") } else { tr("Aucun résultat") });
                }
                for (url, title) in reading {
                    let resp = page_row(ui, th, dark, &url, &title, &host_of(&url));
                    // a hovered item can be taken out of the list
                    if ui.rect_contains_pointer(resp.rect) {
                        let xr = Rect::from_center_size(pos2(resp.rect.right() - 16.0, resp.rect.center().y), vec2(20.0, 20.0));
                        let xresp = ui.interact(xr, egui::Id::new(("rl-rm", &url)), Sense::click());
                        ui.painter().circle_filled(xr.center(), 10.0, if xresp.hovered() { select_fill(dark, true) } else { th.sidebar });
                        icons::paint(ui.painter(), xr.shrink(6.0), Icon::Close, th.sub);
                        if xresp.tip(tr("Retirer de la liste de lecture")).clicked() {
                            acts.push(Do::RemoveReading(url.clone()));
                        }
                    }
                    link_actions(ui, &resp, &url, &mut acts);
                    resp.menu_on_press(|ui| {
                        ui.set_min_width(220.0);
                        link_menu(ui, &url, &mut acts);
                        ui.separator();
                        if menu_item(ui, Icon::Close, tr("Retirer de la liste de lecture")) {
                            acts.push(Do::RemoveReading(url.clone()));
                        }
                    });
                }
                ui.add_space(8.0);
            }

            // ---------------- recent history ----------------
            if self.section_header(ui, th, "sb-history", tr("Historique récent"), None) {
                let mut seen = HashSet::new();
                let recent: Vec<(String, String)> = self
                    .store
                    .history
                    .iter()
                    .rev()
                    .filter(|v| matches(&v.url, &v.title) && seen.insert(v.url.clone()))
                    .take(if q.is_empty() { 10 } else { 30 })
                    .map(|v| (v.url.clone(), v.title.clone()))
                    .collect();
                if recent.is_empty() {
                    hint(ui, th, if q.is_empty() { tr("Aucune page visitée") } else { tr("Aucun résultat") });
                }
                for (url, title) in recent {
                    let resp = page_row(ui, th, dark, &url, &title, "");
                    link_actions(ui, &resp, &url, &mut acts);
                    resp.menu_on_press(|ui| {
                        ui.set_min_width(220.0);
                        link_menu(ui, &url, &mut acts);
                    });
                }
                if q.is_empty() && link_row(ui, th, dark, Icon::Rotate, tr("Tout l'historique")) {
                    acts.push(Do::Page("sanaga://history"));
                }
            }
            ui.add_space(10.0);
        });
        self.sidebar_do(acts);
    }

    /// A section's header: its name, and on hover a fold arrow (and an
    /// action, e.g. +). Returns whether the section is open.
    fn section_header(&mut self, ui: &mut Ui, th: &Theme, key: &str, title: &str, plus: Option<(&str, &mut Vec<Do>)>) -> bool {
        let id = egui::Id::new(key);
        let mut open = ui.ctx().data_mut(|d| *d.get_temp_mut_or_insert_with(id, || true));
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
        ui.painter().text(pos2(r.left() + 8.0, r.center().y + 2.0), Align2::LEFT_CENTER, title, FontId::new(11.5, bold()), th.sub);
        let hovered = ui.rect_contains_pointer(r);
        if hovered {
            let ar = Rect::from_center_size(pos2(r.right() - 10.0, r.center().y + 2.0), vec2(10.0, 10.0));
            icons::paint(ui.painter(), ar, if open { Icon::Down } else { Icon::Forward }, th.sub);
        }
        if let Some((tip, acts)) = plus {
            let pr = Rect::from_center_size(pos2(r.right() - 32.0, r.center().y + 2.0), vec2(20.0, 20.0));
            let presp = ui.interact(pr, id.with("plus"), Sense::click());
            if hovered || presp.hovered() {
                if presp.hovered() {
                    ui.painter().circle_filled(pr.center(), 10.0, th.track);
                }
                icons::paint(ui.painter(), pr.shrink(5.0), Icon::Plus, th.sub);
            }
            if presp.on_hover_cursor(CursorIcon::PointingHand).tip(tip).clicked() {
                acts.push(Do::NewGroup);
                return open;
            }
        }
        if resp.clicked() {
            open = !open;
            ui.ctx().data_mut(|d| d.insert_temp(id, open));
        }
        open
    }

    fn sidebar_do(&mut self, acts: Vec<Do>) {
        for a in acts {
            match a {
                Do::Open(u, new_tab) => {
                    if new_tab {
                        self.open_tab(&u, false);
                    } else {
                        self.navigate(self.active, u, None, true);
                    }
                }
                Do::OpenWindow(u) => self.new_window(vec![u]),
                Do::Copy(u) => {
                    self.ctx.copy_text(u);
                    self.notice = Some((tr("Adresse copiée.").into(), self.ctx.input(|i| i.time)));
                }
                Do::SwitchGroup(g) => {
                    self.overview_open = false;
                    self.switch_group(g);
                }
                Do::NewGroup => {
                    self.new_group("");
                    self.group_renaming = Some(self.groups.len() - 1);
                }
                Do::RenameGroup(g, name) => {
                    if let Some(n) = self.groups.get_mut(g) {
                        *n = name.trim().to_string();
                    }
                    self.group_renaming = None;
                    self.save_session();
                }
                Do::GroupColor(g, k) => {
                    self.sync_group_meta();
                    if let Some(c) = self.group_colors.get_mut(g) {
                        *c = k;
                    }
                }
                Do::NewTabInGroup(g) => {
                    self.switch_group(g);
                    self.current_group = g;
                    self.open_tab("sanaga://newtab", false);
                    let a = self.active;
                    self.tabs[a].group = g;
                }
                Do::Ungroup(g) => self.ungroup(g),
                Do::CloseGroup(g) => self.close_group(g),
                Do::GroupToWindow(g) => {
                    let urls: Vec<String> = self.tabs.iter().filter(|t| t.group == g && !t.private).map(|t| t.url().to_string()).collect();
                    if !urls.is_empty() {
                        self.new_window(urls);
                        self.close_group(g);
                    }
                }
                Do::RemoveReading(u) => {
                    self.store.reading.retain(|b| b.url != u);
                    self.store.save_reading();
                }
                Do::RemoveBookmark(i) => self.remove_bookmark(i),
                Do::EditBookmark(i) => {
                    if let Some(b) = self.store.bookmarks.get(i) {
                        self.bm_edit = (b.title.clone(), b.url.clone());
                        self.bm_editing = Some(i);
                    }
                }
                Do::SaveBookmark(i, title, url) => self.save_bookmark_edit(i, title, url),
                Do::Page(p) => self.open_tab(p, false).pipe_unit(),
            }
        }
    }

    /// Take a favourite away, with a way back for a few seconds.
    pub(super) fn remove_bookmark(&mut self, i: usize) {
        if i < self.store.bookmarks.len() {
            let b = self.store.bookmarks.remove(i);
            self.store.save_bookmarks();
            self.bm_undo = Some((i, b, self.ctx.input(|t| t.time)));
        }
    }

    pub(super) fn save_bookmark_edit(&mut self, i: usize, title: String, url: String) {
        if let Some(b) = self.store.bookmarks.get_mut(i) {
            let url = url.trim();
            if !url.is_empty() {
                b.url = input_to_url(url, self.store.settings.engine, &self.store.settings.home);
            }
            b.title = if title.trim().is_empty() { b.url.clone() } else { title.trim().to_string() };
            self.store.save_bookmarks();
        }
        self.bm_editing = None;
    }

    /// The "favourite removed — Undo" bar, at the bottom of the window.
    pub(super) fn undo_bar(&mut self, ctx: &egui::Context, content: Rect, th: &Theme) {
        let Some((i, b, t)) = &self.bm_undo else { return };
        let now = ctx.input(|x| x.time);
        if now - t > 6.0 {
            self.bm_undo = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        let (i, title) = (*i, b.title.clone());
        let mut undo = false;
        egui::Area::new(egui::Id::new("undo-bar")).order(egui::Order::Foreground).fixed_pos(pos2(content.center().x - 190.0, content.bottom() - 64.0)).show(ctx, |ui| {
            Frame::new()
                .fill(if self.store.settings.dark { Color32::from_rgb(0x3A, 0x38, 0x33) } else { Color32::from_rgb(0x2B, 0x29, 0x25) })
                .corner_radius(CornerRadius::same(12))
                .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(60) })
                .inner_margin(Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.set_width(348.0);
                    ui.horizontal(|ui| {
                        let short: String = title.chars().take(34).collect();
                        ui.label(RichText::new(tf("« {} » retiré des favoris", &[&short])).color(Color32::WHITE));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new(tr("Annuler")).color(th.accent.gamma_multiply(1.0)).strong()).frame(false)).clicked() {
                                undo = true;
                            }
                        });
                    });
                });
        });
        if undo {
            if let Some((_, b, _)) = self.bm_undo.take() {
                let at = i.min(self.store.bookmarks.len());
                self.store.bookmarks.insert(at, b);
                self.store.save_bookmarks();
            }
        }
    }
}

/// Small helper: ignore a value (an index returned by `open_tab`).
trait PipeUnit {
    fn pipe_unit(self);
}
impl PipeUnit for usize {
    fn pipe_unit(self) {}
}

/// A row with the site's icon, its title, and a second line (host) if given.
fn page_row(ui: &mut Ui, th: &Theme, dark: bool, url: &str, title: &str, sub: &str) -> egui::Response {
    let h = if sub.is_empty() { 32.0 } else { 40.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(8), select_fill(dark, false));
    }
    site_badge(ui.painter(), pos2(r.left() + 16.0, r.center().y), 8.0, url, title);
    let clip = Rect::from_min_max(pos2(r.left() + 32.0, r.top()), pos2(r.right() - 8.0, r.bottom()));
    let p = ui.painter().with_clip_rect(clip);
    let t = if title.is_empty() { url } else { title };
    if sub.is_empty() {
        p.text(pos2(clip.left(), r.center().y), Align2::LEFT_CENTER, t, FontId::proportional(13.0), th.text);
    } else {
        p.text(pos2(clip.left(), r.top() + 13.0), Align2::LEFT_CENTER, t, FontId::proportional(13.0), th.text);
        p.text(pos2(clip.left(), r.top() + 29.0), Align2::LEFT_CENTER, sub, FontId::proportional(11.5), th.sub);
    }
    resp.tip(url).on_hover_cursor(CursorIcon::PointingHand)
}

/// A row leading to a page (icon in the accent colour).
fn link_row(ui: &mut Ui, th: &Theme, dark: bool, icon: Icon, label: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(8), select_fill(dark, false));
    }
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(14.0, 14.0)), icon, th.accent);
    ui.painter().text(pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.0), th.text);
    resp.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

fn hint(ui: &mut Ui, th: &Theme, text: &str) {
    Frame::new().inner_margin(Margin { left: 8, right: 6, top: 2, bottom: 6 }).show(ui, |ui| {
        ui.add(egui::Label::new(RichText::new(text).size(12.0).color(th.sub)).wrap());
    });
}

/// Click opens here, middle-click / Ctrl+click in a new tab.
fn link_actions(ui: &Ui, resp: &egui::Response, url: &str, acts: &mut Vec<Do>) {
    if resp.middle_clicked() || (resp.clicked() && ui.input(|i| i.modifiers.command)) {
        acts.push(Do::Open(url.to_string(), true));
    } else if resp.clicked() {
        acts.push(Do::Open(url.to_string(), false));
    }
}

/// The menu items every link has.
fn link_menu(ui: &mut Ui, url: &str, acts: &mut Vec<Do>) {
    if menu_item(ui, Icon::Forward, tr("Ouvrir")) {
        acts.push(Do::Open(url.to_string(), false));
    }
    if menu_item(ui, Icon::Plus, tr("Ouvrir dans un nouvel onglet")) {
        acts.push(Do::Open(url.to_string(), true));
    }
    if menu_item(ui, Icon::Share, tr("Ouvrir dans une nouvelle fenêtre")) {
        acts.push(Do::OpenWindow(url.to_string()));
    }
    if menu_item(ui, Icon::Mail, tr("Copier l'adresse")) {
        acts.push(Do::Copy(url.to_string()));
    }
}

/// A menu item with an icon (macOS menus show symbols beside the words).
pub(super) fn menu_item(ui: &mut Ui, icon: Icon, label: &str) -> bool {
    // a steady width: the menu keeps its size whatever its widest line
    let w = ui.min_rect().width().max(228.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
    let accent = ui.visuals().selection.stroke.color;
    let hovered = resp.hovered();
    if hovered {
        ui.painter().rect_filled(r, CornerRadius::same(6), accent);
    }
    let col = if hovered { Color32::WHITE } else { ui.visuals().text_color() };
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 14.0, r.center().y), vec2(13.0, 13.0)), icon, col);
    ui.painter().text(pos2(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.0), col);
    if resp.clicked() {
        ui.close();
        return true;
    }
    false
}
