//! Favourites, made to feel like Safari's: clicking the star adds the page
//! and opens a bubble to name it (or remove it); the Favourites page lists
//! them in a rounded group with a search, each row editable in place
//! (double-click or the pencil), reordered by dragging its handle, removed
//! with a way back (Undo), with a complete right-click menu.

use super::chrome::site_badge;
use super::sidebar::menu_item;
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align, Align2, Color32, CursorIcon, Layout, Margin, RichText, Sense, TextEdit};

impl Sanaga {
    /// The star of the address field: add the page (and open the bubble), or
    /// open the bubble of a page already in the favourites.
    pub(super) fn star_clicked(&mut self, star: Rect) {
        let t = &self.tabs[self.active];
        if t.entries.is_empty() || t.url().starts_with("sanaga://newtab") {
            return;
        }
        let (u, ti) = (t.url().to_string(), t.title());
        if !self.store.is_bookmarked(&u) {
            self.store.toggle_bookmark(&u, &ti);
        }
        self.bm_pop_title = self.store.bookmarks.iter().find(|b| b.url == u).map(|b| b.title.clone()).unwrap_or(ti);
        self.bm_popover = true;
        self.ctx.data_mut(|d| d.insert_temp(egui::Id::new("star-rect"), star));
    }

    /// The bubble under the star: the favourite's name, Remove, Done.
    pub(super) fn star_popover(&mut self, ctx: &egui::Context, th: &Theme) {
        if !self.bm_popover {
            return;
        }
        let url = self.tabs[self.active].url().to_string();
        let Some(i) = self.store.bookmarks.iter().position(|b| b.url == url) else {
            self.bm_popover = false;
            return;
        };
        let star: Rect = ctx.data(|d| d.get_temp(egui::Id::new("star-rect"))).unwrap_or(Rect::from_min_size(pos2(400.0, 40.0), vec2(20.0, 20.0)));
        let mut done = false;
        let mut remove = false;
        let area = egui::Area::new(egui::Id::new("star-popover")).order(egui::Order::Foreground).fixed_pos(pos2(star.left() - 16.0, star.bottom() + 10.0)).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.hairline.gamma_multiply(0.6)))
                .corner_radius(CornerRadius::same(14))
                .shadow(egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if self.store.settings.dark { 110 } else { 45 }) })
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(320.0);
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                        icons::paint(ui.painter(), r, Icon::StarFilled, th.accent);
                        ui.label(RichText::new(tr("Ajouté aux favoris")).size(14.5).family(bold()).color(th.text));
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(9), th.card);
                        site_badge(ui.painter(), r.center(), 9.0, &url, &self.bm_pop_title);
                        ui.vertical(|ui| {
                            let id = egui::Id::new("bm-pop-name");
                            let te = ui.add(TextEdit::singleline(&mut self.bm_pop_title).id(id).desired_width(260.0).hint_text(tr("Nom")));
                            if !ui.ctx().memory(|m| m.has_focus(id)) && ui.ctx().data(|d| d.get_temp::<bool>(id.with("focused"))).is_none() {
                                te.request_focus();
                                ui.ctx().data_mut(|d| d.insert_temp(id.with("focused"), true));
                            }
                            if te.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                done = true;
                            }
                            let short: String = url.chars().take(44).collect();
                            ui.label(RichText::new(short).size(11.5).color(th.sub));
                        });
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new(tr("Supprimer")).color(Color32::from_rgb(0xC0, 0x30, 0x2A))).fill(th.card).corner_radius(CornerRadius::same(8))).clicked() {
                            remove = true;
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new(tr("OK")).color(Color32::WHITE)).fill(th.accent).min_size(vec2(72.0, 28.0)).corner_radius(CornerRadius::same(8))).clicked() {
                                done = true;
                            }
                        });
                    });
                });
        });
        // a click elsewhere or Escape closes it (keeping the name)
        let outside = ctx.input(|i| i.pointer.any_pressed()) && !ctx.pointer_interact_pos().is_some_and(|p| area.response.rect.contains(p) || star.contains(p));
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
        if remove {
            self.bm_popover = false;
            self.remove_bookmark(i);
        } else if done || outside || esc {
            let name = self.bm_pop_title.trim().to_string();
            if !name.is_empty() {
                self.store.bookmarks[i].title = name;
                self.store.save_bookmarks();
            }
            self.bm_popover = false;
        }
        if !self.bm_popover {
            ctx.data_mut(|d| d.remove::<bool>(egui::Id::new("bm-pop-name").with("focused")));
        }
    }

    /// sanaga://bookmarks
    pub(super) fn page_bookmarks(&mut self, ui: &mut Ui, th: &Theme) {
        let mut go: Option<(String, bool)> = None;
        let mut window: Option<String> = None;
        let mut copy: Option<String> = None;
        let mut remove: Option<usize> = None;
        let mut edit: Option<usize> = None;
        let mut save: Option<(usize, String, String)> = None;
        let mut moved: Option<(usize, usize)> = None;
        let mut import: Option<std::path::PathBuf> = None;
        let mut pick_file = false;
        let mut open_import = false;
        let sources = crate::import::chrome_family_sources();
        let dark = self.store.settings.dark;
        let q = self.bm_filter.trim().to_lowercase();
        super::pages::centered_column(ui, |ui| {
            ui.add_space(26.0);
            // title, number, search and import
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Favoris")).size(26.0).family(bold()).color(th.text));
                ui.add_space(8.0);
                ui.label(RichText::new(self.store.bookmarks.len().to_string()).size(15.0).color(th.sub));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.menu_button(RichText::new(tr("Importer…")), |ui| {
                        ui.set_min_width(240.0);
                        for (name, path) in &sources {
                            if menu_item(ui, Icon::Download, &tf("Depuis {}", &[name])) {
                                import = Some(path.clone());
                            }
                        }
                        if menu_item(ui, Icon::Folder, tr("Un fichier HTML (Firefox, Safari…)")) {
                            pick_file = true;
                        }
                        ui.separator();
                        if menu_item(ui, Icon::Forward, tr("Importer depuis un autre navigateur (Chrome, Firefox…)")) {
                            open_import = true;
                        }
                    });
                    ui.add_space(8.0);
                    Frame::new().fill(th.track).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(8, 0)).show(ui, |ui| {
                        ui.allocate_ui_with_layout(vec2(220.0, 28.0), Layout::left_to_right(Align::Center), |ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                            icons::paint(ui.painter(), r, Icon::Search, th.sub);
                            ui.add(TextEdit::singleline(&mut self.bm_filter).frame(Frame::NONE).hint_text(RichText::new(tr("Rechercher")).color(th.sub)).vertical_align(Align::Center).desired_width(ui.available_width()));
                        });
                    });
                });
            });
            ui.add_space(14.0);
            if self.store.bookmarks.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                    icons::paint(ui.painter(), r, Icon::Star, th.sub);
                    ui.add_space(8.0);
                    ui.label(RichText::new(tr("Aucun favori pour l'instant")).size(15.0).family(bold()).color(th.text));
                    ui.label(RichText::new(tr("Cliquez sur l'étoile de la barre d'adresse pour ajouter une page.")).color(th.sub));
                });
                return;
            }
            let rows: Vec<(usize, String, String)> = self.store.bookmarks.iter().enumerate().filter(|(_, b)| q.is_empty() || b.url.to_lowercase().contains(&q) || b.title.to_lowercase().contains(&q)).map(|(i, b)| (i, b.url.clone(), b.title.clone())).collect();
            if rows.is_empty() {
                ui.label(RichText::new(tr("Aucun résultat")).color(th.sub));
            }
            let mut rects: Vec<(usize, Rect)> = Vec::new();
            Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::symmetric(0, 2)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                for (n, (i, url, title)) in rows.iter().enumerate() {
                    let i = *i;
                    if self.bm_editing == Some(i) {
                        Frame::new().inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 6.0;
                            ui.add(TextEdit::singleline(&mut self.bm_edit.0).hint_text(tr("Nom")).desired_width(f32::INFINITY));
                            ui.add(TextEdit::singleline(&mut self.bm_edit.1).hint_text(tr("Adresse")).desired_width(f32::INFINITY));
                            ui.horizontal(|ui| {
                                if ui.add(egui::Button::new(RichText::new(tr("Enregistrer")).color(Color32::WHITE)).fill(th.accent).corner_radius(CornerRadius::same(7))).clicked() || ui.input(|x| x.key_pressed(Key::Enter)) {
                                    save = Some((i, self.bm_edit.0.clone(), self.bm_edit.1.clone()));
                                }
                                if ui.button(tr("Annuler")).clicked() || ui.input(|x| x.key_pressed(Key::Escape)) {
                                    self.bm_editing = None;
                                }
                            });
                        });
                        continue;
                    }
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click_and_drag());
                    rects.push((i, r));
                    if n > 0 {
                        ui.painter().line_segment([pos2(r.left() + 54.0, r.top()), pos2(r.right() - 14.0, r.top())], Stroke::new(1.0, th.hairline.gamma_multiply(0.6)));
                    }
                    let over = ui.rect_contains_pointer(r);
                    let dragging = self.bm_drag == Some(i);
                    if dragging {
                        ui.painter().rect_filled(r.shrink(2.0), CornerRadius::same(10), th.accent.gamma_multiply(0.12));
                    } else if over {
                        ui.painter().rect_filled(r.shrink2(vec2(4.0, 2.0)), CornerRadius::same(8), if dark { Color32::from_white_alpha(10) } else { Color32::from_black_alpha(8) });
                    }
                    let icon_box = Rect::from_center_size(pos2(r.left() + 30.0, r.center().y), vec2(32.0, 32.0));
                    ui.painter().rect_filled(icon_box, CornerRadius::same(8), th.page);
                    site_badge(ui.painter(), icon_box.center(), 9.0, url, title);
                    let text_right = r.right() - if over { 96.0 } else { 16.0 };
                    let clip = Rect::from_min_max(pos2(r.left() + 56.0, r.top()), pos2(text_right, r.bottom()));
                    let p = ui.painter().with_clip_rect(clip);
                    p.text(pos2(clip.left(), r.top() + 18.0), Align2::LEFT_CENTER, if title.is_empty() { url } else { title }, FontId::proportional(13.5), th.text);
                    p.text(pos2(clip.left(), r.top() + 36.0), Align2::LEFT_CENTER, url, FontId::proportional(11.5), th.sub);
                    if over {
                        // the row's actions: edit, more, and the handle to drag it
                        let mut bx = r.right() - 14.0;
                        for (k, icon, tip) in [(0, Icon::Menu, tr("Glisser pour déplacer")), (1, Icon::More, tr("Plus")), (2, Icon::Pencil, tr("Modifier"))] {
                            let br = Rect::from_center_size(pos2(bx - 12.0, r.center().y), vec2(24.0, 24.0));
                            bx -= 28.0;
                            let bresp = ui.interact(br, egui::Id::new(("bm-act", i, k)), if k == 0 { Sense::drag() } else { Sense::click() });
                            if bresp.hovered() {
                                ui.painter().circle_filled(br.center(), 12.0, th.track);
                            }
                            icons::paint(ui.painter(), br.shrink(6.0), icon, th.sub);
                            let bresp = bresp.tip(tip);
                            match k {
                                0 => {
                                    if bresp.drag_started() {
                                        self.bm_drag = Some(i);
                                    }
                                    if bresp.dragged() {
                                        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                                    } else {
                                        bresp.on_hover_cursor(CursorIcon::Grab);
                                    }
                                }
                                1 => {
                                    bresp.menu_on_press(|ui| {
                                        ui.set_min_width(220.0);
                                        bookmark_menu(ui, i, url, &mut go, &mut window, &mut copy, &mut edit, &mut remove);
                                    });
                                }
                                _ => {
                                    if bresp.clicked() {
                                        edit = Some(i);
                                    }
                                }
                            }
                        }
                    }
                    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
                    if resp.double_clicked() {
                        edit = Some(i);
                    } else if resp.middle_clicked() || (resp.clicked() && ui.input(|x| x.modifiers.command)) {
                        go = Some((url.clone(), true));
                    } else if resp.clicked() {
                        go = Some((url.clone(), false));
                    }
                    resp.menu_on_press(|ui| {
                        ui.set_min_width(220.0);
                        bookmark_menu(ui, i, url, &mut go, &mut window, &mut copy, &mut edit, &mut remove);
                    });
                }
            });
            // dragging a row: where it would land
            if let Some(from) = self.bm_drag {
                if let Some(p) = ui.ctx().pointer_interact_pos() {
                    if let Some(&(to, r)) = rects.iter().find(|(_, r)| p.y >= r.top() && p.y < r.bottom()) {
                        let y = if to > from { r.bottom() } else { r.top() };
                        ui.painter().line_segment([pos2(r.left() + 14.0, y), pos2(r.right() - 14.0, y)], Stroke::new(2.0, th.accent));
                        if ui.input(|x| x.pointer.any_released()) {
                            moved = Some((from, to));
                        }
                    }
                }
                if ui.input(|x| x.pointer.any_released()) {
                    self.bm_drag = None;
                }
            }
            ui.add_space(8.0);
            ui.label(RichText::new(tr("Double-cliquez pour renommer · glissez la poignée pour réordonner · clic droit pour plus d'actions")).size(11.5).color(th.sub));
        });

        if let Some((from, to)) = moved {
            if from != to && from < self.store.bookmarks.len() && to < self.store.bookmarks.len() {
                let b = self.store.bookmarks.remove(from);
                self.store.bookmarks.insert(to, b);
                self.store.save_bookmarks();
            }
            self.bm_drag = None;
        }
        if let Some((i, t, u)) = save {
            self.save_bookmark_edit(i, t, u);
        }
        if let Some(i) = edit {
            if let Some(b) = self.store.bookmarks.get(i) {
                self.bm_edit = (b.title.clone(), b.url.clone());
                self.bm_editing = Some(i);
            }
        }
        if let Some(i) = remove {
            self.remove_bookmark(i);
        }
        if let Some(u) = copy {
            self.ctx.copy_text(u);
            self.notice = Some((tr("Adresse copiée.").into(), ui.input(|i| i.time)));
        }
        if let Some(u) = window {
            self.new_window(vec![u]);
        }
        if open_import {
            self.navigate(self.active, "sanaga://import".into(), None, true);
        }
        if pick_file {
            import = self.choose_file(tr("Importer des favoris (fichier HTML)"), &["*.html", "*.htm"], tr("Favoris (HTML)"));
        }
        if let Some(path) = import {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let found = if text.trim_start().starts_with('{') { crate::import::parse_chrome(&text) } else { crate::import::parse_html(&text) };
            let mut added = 0;
            for (url, title) in found {
                if !self.store.is_bookmarked(&url) {
                    let title = if title.trim().is_empty() { url.clone() } else { title };
                    self.store.bookmarks.push(crate::store::Bookmark { url, title });
                    added += 1;
                }
            }
            self.store.save_bookmarks();
            self.notice = Some((tf("Favoris importés : {}", &[&added]), ui.input(|i| i.time)));
        }
        self.apply_go(go);
    }
}

#[allow(clippy::too_many_arguments)]
fn bookmark_menu(ui: &mut Ui, i: usize, url: &str, go: &mut Option<(String, bool)>, window: &mut Option<String>, copy: &mut Option<String>, edit: &mut Option<usize>, remove: &mut Option<usize>) {
    if menu_item(ui, Icon::Forward, tr("Ouvrir")) {
        *go = Some((url.to_string(), false));
    }
    if menu_item(ui, Icon::Plus, tr("Ouvrir dans un nouvel onglet")) {
        *go = Some((url.to_string(), true));
    }
    if menu_item(ui, Icon::Share, tr("Ouvrir dans une nouvelle fenêtre")) {
        *window = Some(url.to_string());
    }
    if menu_item(ui, Icon::Mail, tr("Copier l'adresse")) {
        *copy = Some(url.to_string());
    }
    ui.separator();
    if menu_item(ui, Icon::Pencil, tr("Modifier…")) {
        *edit = Some(i);
    }
    if menu_item(ui, Icon::Close, tr("Supprimer")) {
        *remove = Some(i);
    }
}
