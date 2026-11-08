//! The ⋮ menu, kept short like a macOS menu: three quick buttons (new tab,
//! new window, private tab), the zoom, the everyday actions with their symbols
//! and shortcuts, and the rest under "More tools", which opens in place.

use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align2, Color32, Margin, Sense};

impl Sanaga {
    pub(super) fn main_menu(&mut self, ctx: &egui::Context, th: &Theme) {
        if !self.menu_open {
            return;
        }
        let w = 300.0;
        let pos = pos2(self.menu_rect.right() - w, self.menu_rect.bottom() + 6.0);
        let act: std::cell::Cell<Option<&'static str>> = std::cell::Cell::new(None);
        let zoom = self.tabs[self.active].zoom;
        let cur_url = self.tabs[self.active].url().to_string();
        let in_list = self.store.in_reading_list(&cur_url);
        let reading = self.tabs[self.active].reader;
        let images = self.store.settings.images;
        let bm_bar = self.store.settings.show_bookmarks_bar;
        let more_id = egui::Id::new("menu-more");
        let more = ctx.data(|d| d.get_temp::<bool>(more_id)).unwrap_or(false);
        let max_h = (ctx.content_rect().bottom() - pos.y - 24.0).max(200.0);
        let dark = self.store.settings.dark;
        let (focus, on_top) = (self.focus_mode, self.on_top);
        let editing = self.editing_text == Some(self.tabs[self.active].id);
        let unlocked = crate::site_prefs::for_host(&host_of(&cur_url)).unlock;
        let area = egui::Area::new(egui::Id::new("main-menu")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.hairline.gamma_multiply(0.6)))
                .corner_radius(CornerRadius::same(14))
                .shadow(egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if dark { 110 } else { 45 }) })
                .inner_margin(Margin::same(6))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(max_h).min_scrolled_height(max_h).auto_shrink([true, true]).show(ui, |ui| {
                        ui.set_width(w - 12.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let iw = w - 12.0;
                        // three quick buttons
                        let (row, _) = ui.allocate_exact_size(vec2(iw, 58.0), Sense::hover());
                        let bw = (iw - 12.0) / 3.0;
                        for (k, (icon, label, id)) in [(Icon::Plus, tr("Onglet"), "new"), (Icon::Grid, tr("Fenêtre"), "window"), (Icon::Private, tr("Privé"), "private")].into_iter().enumerate() {
                            let r = Rect::from_min_size(pos2(row.left() + k as f32 * (bw + 6.0), row.top() + 2.0), vec2(bw, 54.0));
                            let resp = ui.interact(r, egui::Id::new(("menu-quick", k)), Sense::click());
                            ui.painter().rect_filled(r, CornerRadius::same(10), if resp.hovered() { th.track.gamma_multiply(1.5) } else { th.card });
                            icons::paint(ui.painter(), Rect::from_center_size(pos2(r.center().x, r.top() + 20.0), vec2(16.0, 16.0)), icon, th.accent);
                            ui.painter().text(pos2(r.center().x, r.bottom() - 13.0), Align2::CENTER_CENTER, label, FontId::proportional(12.0), th.text);
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                act.set(Some(id));
                            }
                        }
                        ui.add_space(6.0);
                        // zoom: − 100 % +  and full screen
                        let (r, _) = ui.allocate_exact_size(vec2(iw, 36.0), Sense::hover());
                        ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, tr("Zoom"), FontId::proportional(13.5), th.text);
                        let fs = Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(30.0, 28.0));
                        let plus = Rect::from_center_size(pos2(r.right() - 58.0, r.center().y), vec2(30.0, 28.0));
                        let minus = Rect::from_center_size(pos2(r.right() - 142.0, r.center().y), vec2(30.0, 28.0));
                        let pct = Rect::from_center_size(pos2(r.right() - 100.0, r.center().y), vec2(50.0, 28.0));
                        let presp = ui.interact(pct, egui::Id::new("menu-zoom-reset"), Sense::click());
                        ui.painter().text(pct.center(), Align2::CENTER_CENTER, format!("{:.0} %", zoom * 100.0), FontId::proportional(13.0), if presp.hovered() { th.accent } else { th.text });
                        if presp.tip(tr("Réinitialiser le zoom")).clicked() {
                            act.set(Some("z0"));
                        }
                        for (rr, id, icon, tip) in [(minus, "zout", None, tr("Réduire (Ctrl+−)")), (plus, "zin", Some(Icon::Plus), tr("Agrandir (Ctrl++)")), (fs, "fullscreen", Some(Icon::Grid), tr("Plein écran (F11)"))] {
                            let resp = ui.interact(rr, egui::Id::new(("menu-z", id)), Sense::click());
                            ui.painter().rect_filled(rr, CornerRadius::same(8), if resp.hovered() { th.track.gamma_multiply(1.5) } else { th.card });
                            match (icon, id) {
                                (_, "fullscreen") => {
                                    // four corners: full screen
                                    let c = rr.center();
                                    let s = Stroke::new(1.5, th.text);
                                    for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                                        let corner = pos2(c.x + dx * 6.0, c.y + dy * 5.0);
                                        ui.painter().line_segment([corner, pos2(corner.x - dx * 4.0, corner.y)], s);
                                        ui.painter().line_segment([corner, pos2(corner.x, corner.y - dy * 4.0)], s);
                                    }
                                }
                                (Some(i), _) => icons::paint(ui.painter(), rr.shrink(8.0), i, th.text),
                                (None, _) => {
                                    ui.painter().line_segment([pos2(rr.center().x - 5.5, rr.center().y), pos2(rr.center().x + 5.5, rr.center().y)], Stroke::new(1.7, th.text));
                                }
                            }
                            if resp.tip(tip).clicked() {
                                act.set(Some(id));
                            }
                        }
                        sep(ui, th, iw);
                        let item = |ui: &mut Ui, icon: Icon, label: &str, key: &str, id: &'static str| {
                            if menu_row(ui, th, iw, icon, label, key) {
                                act.set(Some(id));
                            }
                        };
                        item(ui, Icon::Rotate, tr("Historique"), tr("Ctrl+H"), "history");
                        item(ui, Icon::Download, tr("Téléchargements"), tr("Ctrl+J"), "downloads");
                        item(ui, Icon::StarFilled, tr("Favoris"), "", "bookmarks");
                        item(ui, Icon::Lock, tr("Mots de passe"), "", "passwords");
                        sep(ui, th, iw);
                        item(ui, Icon::Search, tr("Rechercher dans la page"), tr("Ctrl+F"), "find");
                        item(ui, Icon::Reader, if reading { tr("Quitter le Lecteur") } else { tr("Lecteur") }, tr("Ctrl+Maj+R"), "reader");
                        item(ui, Icon::Sparkle, tr("Résumer la page"), tr("Ctrl+Maj+Y"), "summary");
                        item(ui, Icon::Mail, tr("Traduire la page"), "", "translate");
                        item(ui, Icon::Print, tr("Imprimer…"), tr("Ctrl+P"), "print");
                        sep(ui, th, iw);
                        // more tools, opening in place
                        let (r, resp) = ui.allocate_exact_size(vec2(iw, 30.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(7), th.track.gamma_multiply(0.9));
                        }
                        icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(14.0, 14.0)), Icon::More, th.text);
                        ui.painter().text(pos2(r.left() + 34.0, r.center().y), Align2::LEFT_CENTER, tr("Plus d'outils"), FontId::proportional(13.5), th.text);
                        icons::paint(ui.painter(), Rect::from_center_size(pos2(r.right() - 14.0, r.center().y), vec2(10.0, 10.0)), if more { Icon::Down } else { Icon::Forward }, th.sub);
                        if resp.clicked() {
                            ctx.data_mut(|d| d.insert_temp(more_id, !more));
                        }
                        if more {
                            let sub = |ui: &mut Ui, icon: Icon, label: &str, key: &str, id: &'static str| {
                                ui.horizontal(|ui| {
                                    ui.add_space(12.0);
                                    if menu_row(ui, th, iw - 12.0, icon, label, key) {
                                        act.set(Some(id));
                                    }
                                });
                            };
                            sub(ui, Icon::Glasses, if in_list { tr("Retirer de la liste de lecture") } else { tr("Ajouter à la liste de lecture") }, tr("Ctrl+Maj+D"), "readinglist");
                            sub(ui, Icon::Private, tr("Nouvel onglet dans un conteneur…"), "", "container");
                            sub(ui, Icon::Reader, if focus { tr("Quitter le mode focus") } else { tr("Mode focus") }, "F9", "focus");
                            sub(ui, Icon::Grid, if on_top { tr("Ne plus garder au premier plan") } else { tr("Garder au premier plan") }, "", "ontop");
                            sub(ui, Icon::Camera, tr("Vidéo flottante"), "", "pip");
                            sub(ui, Icon::Shield, tr("Réglages de ce site…"), "", "site");
                            sub(ui, Icon::Close, tr("Supprimer un élément de la page…"), "", "zap");
                            sub(ui, Icon::Reader, if editing { tr("Terminer la modification du texte") } else { tr("Modifier le texte de la page") }, "", "edittext");
                            sub(ui, Icon::Lock, if unlocked { tr("Rendre le copier-coller au site") } else { tr("Débloquer le copier-coller") }, "", "unlock");
                            sub(ui, Icon::Rotate, tr("Version archivée de la page"), "", "archive");
                            sub(ui, Icon::Download, tr("Enregistrer la page"), tr("Ctrl+S"), "savepage");
                            sub(ui, Icon::Grid, tr("Aperçu des onglets"), tr("Ctrl+Maj+O"), "overview");
                            sub(ui, Icon::Search, tr("Rechercher dans les onglets"), tr("Ctrl+Maj+A"), "tabsearch");
                            sub(ui, Icon::Folder, tr("Nouveau groupe d'onglets"), "", "newgroup");
                            sub(ui, Icon::Grid, tr("Rouvrir la fenêtre fermée"), "", "reopenwindow");
                            sub(ui, Icon::Shield, tr("Se présenter à ce site comme…"), "", "identity");
                            sub(ui, Icon::Reader, tr("Afficher le code source"), tr("Ctrl+U"), "source");
                            sub(ui, Icon::Info, tr("Inspecteur web"), tr("Ctrl+Maj+I"), "inspector");
                            sub(ui, Icon::Phone, tr("Simulateur mobile"), tr("Ctrl+Maj+M"), "devices");
                            sub(ui, Icon::Puzzle, tr("Créer une extension…"), "", "make-extension");
                            sub(ui, Icon::Mail, tr("Remplir avec mes coordonnées"), tr("Ctrl+Maj+F"), "contacts");
                            sub(ui, Icon::StarFilled, if bm_bar { tr("Masquer la barre des favoris") } else { tr("Afficher la barre des favoris") }, tr("Ctrl+Maj+B"), "bmbar");
                            sub(ui, Icon::Camera, if images { tr("Masquer les images") } else { tr("Afficher les images") }, "", "images");
                            sub(ui, Icon::Close, tr("Effacer les données de navigation…"), tr("Ctrl+Maj+Suppr"), "cleardata");
                        }
                        sep(ui, th, iw);
                        item(ui, Icon::Home, tr("Paramètres"), tr("Ctrl+,"), "settings");
                        item(ui, Icon::Info, tr("À propos de Sanaga"), "", "about");
                        item(ui, Icon::Close, tr("Quitter"), tr("Ctrl+Q"), "quit");
                    });
                });
        });
        let menu_rect = self.menu_rect;
        let clicked_outside = ctx.input(|i| i.pointer.any_pressed()) && !ctx.pointer_interact_pos().map(|p| area.response.rect.contains(p) || menu_rect.contains(p)).unwrap_or(false);
        if clicked_outside || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.menu_open = false;
        }
        let Some(id) = act.get() else { return };
        if !matches!(id, "zin" | "zout" | "z0") {
            self.menu_open = false;
        }
        match id {
            "newgroup" => {
                self.new_group("");
                self.sidebar_open = true;
                self.group_renaming = Some(self.groups.len() - 1);
            }
            "overview" => self.toggle_overview(),
            "container" | "focus" | "ontop" | "pip" | "site" | "zap" | "edittext" | "unlock" | "archive" => self.run_shortcut(id),
            "contacts" => self.fill_contacts(),
            "tabsearch" => {
                self.tab_search_open = true;
                self.tab_search_q.clear();
            }
            "savepage" => self.save_page(),
            "cleardata" => {
                self.open_tab("sanaga://clear", false);
            }
            "passwords" => {
                self.open_tab("sanaga://passwords", false);
            }
            "translate" => self.translate_page(),
            "print" => self.print_page(),
            "summary" => self.summarize_page(),
            "make-extension" => {
                self.open_tab("sanaga://create-extension", false);
            }
            "fullscreen" => self.toggle_fullscreen(),
            "source" => self.view_source(),
            "inspector" => self.open_inspector(),
            "devices" => self.toggle_devices(),
            "new" => {
                self.open_tab("sanaga://newtab", false);
            }
            "window" => self.new_window(Vec::new()),
            "private" => {
                self.open_tab("sanaga://newtab", true);
            }
            "reopenwindow" => self.reopen_window(),
            "identity" => self.identity_ask = true,
            "history" => {
                self.open_tab("sanaga://history", false);
            }
            "downloads" => {
                self.open_tab("sanaga://downloads", false);
            }
            "bookmarks" => {
                self.open_tab("sanaga://bookmarks", false);
            }
            "zin" => self.zoom_by(0.1),
            "zout" => self.zoom_by(-0.1),
            "z0" => {
                let a = self.active;
                self.tabs[a].zoom = 1.0;
            }
            "find" => {
                self.find_open = true;
                self.focus_find = true;
            }
            "images" => {
                self.store.settings.images = !self.store.settings.images;
                self.store.save_settings();
            }
            "reader" => self.toggle_reader(self.active),
            "readinglist" => self.toggle_reading_list(),
            "bmbar" => {
                self.store.settings.show_bookmarks_bar = !self.store.settings.show_bookmarks_bar;
                self.store.save_settings();
            }
            "settings" => {
                self.open_tab("sanaga://settings", false);
            }
            "about" => {
                self.open_tab("sanaga://about", false);
            }
            "quit" => self.quit_app(),
            _ => {}
        }
    }
}

fn sep(ui: &mut Ui, th: &Theme, w: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(w, 11.0), Sense::hover());
    ui.painter().line_segment([pos2(r.left() + 10.0, r.center().y), pos2(r.right() - 10.0, r.center().y)], Stroke::new(1.0, th.hairline.gamma_multiply(0.6)));
}

/// One line of the menu: symbol, words, shortcut; the accent behind it on hover.
fn menu_row(ui: &mut Ui, th: &Theme, w: f32, icon: Icon, label: &str, key: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.painter().rect_filled(r, CornerRadius::same(7), th.accent);
    }
    let (tc, kc) = if hovered { (Color32::WHITE, Color32::from_white_alpha(210)) } else { (th.text, th.sub) };
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(14.0, 14.0)), icon, if hovered { Color32::WHITE } else { th.text.gamma_multiply(0.8) });
    ui.painter().text(pos2(r.left() + 34.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.5), tc);
    ui.painter().text(pos2(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, key, FontId::proportional(12.0), kc);
    resp.clicked()
}
