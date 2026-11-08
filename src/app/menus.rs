//! Bookmarks bar, the ⋮ menu, the find-in-page bar and the downloads bar.

use super::chrome::{icon_button_small, site_badge};
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align, Align2, Color32, Layout, Margin, RichText, Sense, TextEdit};

impl Sanaga {
    pub(super) fn bookmarks_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let mut go: Option<(String, bool)> = None;
        let mut remove: Option<String> = None;
        let mut hide = false;
        let full = ui.max_rect();
        // right-click on the empty bar, or the small button at its right end, hides it
        let bg = ui.interact(full, egui::Id::new("bookmarks-bar-bg"), Sense::click());
        bg.menu_on_press(|ui| {
            if ui.button(tr("Masquer la barre des favoris (Ctrl+Maj+B)")).clicked() {
                hide = true;
                ui.close();
            }
        });
        let hr = Rect::from_center_size(pos2(full.right() - 22.0, full.center().y), vec2(22.0, 22.0));
        let hresp = ui.interact(hr, egui::Id::new("bookmarks-bar-hide"), Sense::click());
        if hresp.hovered() {
            ui.painter().rect_filled(hr, CornerRadius::same(5), th.omnibox_hover);
        }
        icons::paint(ui.painter(), hr.shrink(6.0), Icon::Close, th.sub);
        if hresp.tip(tr("Masquer la barre des favoris (Ctrl+Maj+B)\nPour la réafficher : menu ⋮ ou Ctrl+Maj+B")).clicked() {
            hide = true;
        }
        ui.horizontal_centered(|ui| {
            ui.add_space(10.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            if self.store.bookmarks.is_empty() {
                ui.label(RichText::new(tr("Ajoutez des favoris avec l'étoile de la barre d'adresse.")).size(12.0).color(th.sub));
            }
            for b in &self.store.bookmarks {
                let title: String = if b.title.chars().count() > 22 { format!("{}…", b.title.chars().take(21).collect::<String>()) } else { b.title.clone() };
                let galley = ui.painter().layout_no_wrap(title, FontId::proportional(12.5), th.text);
                let w = galley.size().x + 34.0;
                if ui.available_width() < w + 40.0 {
                    break;
                }
                let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(12), th.omnibox_hover);
                }
                site_badge(ui.painter(), pos2(r.left() + 13.0, r.center().y), 7.0, &b.url, &b.title);
                ui.painter().galley(pos2(r.left() + 25.0, r.center().y - galley.size().y / 2.0), galley, th.text);
                let resp = resp.tip(format!("{}\n{}", b.title, b.url));
                if resp.middle_clicked() || (resp.clicked() && ui.input(|i| i.modifiers.command)) {
                    go = Some((b.url.clone(), true));
                } else if resp.clicked() {
                    go = Some((b.url.clone(), false));
                }
                resp.menu_on_press(|ui| {
                    if ui.button(tr("Ouvrir dans un nouvel onglet")).clicked() {
                        go = Some((b.url.clone(), true));
                        ui.close();
                    }
                    if ui.button(tr("Supprimer")).clicked() {
                        remove = Some(b.url.clone());
                        ui.close();
                    }
                    if ui.button(tr("Masquer la barre des favoris")).clicked() {
                        hide = true;
                        ui.close();
                    }
                });
            }
        });
        if hide {
            self.store.settings.show_bookmarks_bar = false;
            self.store.save_settings();
        }
        self.apply_go(go);
        if let Some(u) = remove {
            self.store.bookmarks.retain(|b| b.url != u);
            self.store.save_bookmarks();
        }
    }

    pub(super) fn main_menu(&mut self, ctx: &egui::Context, th: &Theme) {
        if !self.menu_open {
            return;
        }
        let w = 290.0;
        let pos = pos2(self.menu_rect.right() - w, self.menu_rect.bottom() + 4.0);
        let mut act: Option<&'static str> = None;
        let mut zoom_act: Option<&'static str> = None;
        let zoom = self.tabs[self.active].zoom;
        let bm_bar = self.store.settings.show_bookmarks_bar;
        let cur_url = self.tabs[self.active].url().to_string();
        let in_list = self.store.in_reading_list(&cur_url);
        let reading = self.tabs[self.active].reader;
        let images = self.store.settings.images;
        let dark = self.store.settings.dark;
        let max_h = (ctx.content_rect().bottom() - pos.y - 24.0).max(200.0);
        let area = egui::Area::new(egui::Id::new("main-menu")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.border))
                .corner_radius(CornerRadius::same(12))
                .shadow(egui::Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(50) })
                .inner_margin(Margin::symmetric(0, 8))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(max_h).min_scrolled_height(max_h).show(ui, |ui| {
                    ui.set_width(w);
                    let mut item = |ui: &mut Ui, label: &str, key: &str, id: &'static str| {
                        let (r, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
                        let (tc, kc) = if resp.hovered() {
                            ui.painter().rect_filled(r.shrink2(vec2(6.0, 1.0)), CornerRadius::same(6), th.accent);
                            (Color32::WHITE, Color32::from_white_alpha(200))
                        } else {
                            (th.text, th.sub)
                        };
                        ui.painter().text(pos2(r.left() + 18.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.5), tc);
                        ui.painter().text(pos2(r.right() - 16.0, r.center().y), Align2::RIGHT_CENTER, key, FontId::proportional(12.0), kc);
                        if resp.clicked() {
                            act = Some(id);
                        }
                    };
                    let sep = |ui: &mut Ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(w, 9.0), Sense::hover());
                        ui.painter().line_segment([pos2(r.left(), r.center().y), pos2(r.right(), r.center().y)], Stroke::new(1.0, th.border));
                    };
                    item(ui, tr("Nouvel onglet"), tr("Ctrl+T"), "new");
                    item(ui, tr("Nouvelle fenêtre"), tr("Ctrl+N"), "window");
                    item(ui, tr("Nouvel onglet privé"), tr("Ctrl+Maj+N"), "private");
                    item(ui, tr("Nouveau groupe d'onglets"), "", "newgroup");
                    item(ui, tr("Aperçu des onglets"), tr("Ctrl+Maj+O"), "overview");
                    item(ui, tr("Rechercher dans les onglets"), tr("Ctrl+Maj+A"), "tabsearch");
                    sep(ui);
                    item(ui, tr("Historique"), tr("Ctrl+H"), "history");
                    item(ui, tr("Téléchargements"), tr("Ctrl+J"), "downloads");
                    item(ui, if reading { tr("Quitter le Lecteur") } else { tr("Afficher dans le Lecteur") }, tr("Ctrl+Maj+R"), "reader");
                    item(ui, if in_list { tr("Retirer de la liste de lecture") } else { tr("Ajouter à la liste de lecture") }, tr("Ctrl+Maj+D"), "readinglist");
                    item(ui, tr("Traduire la page (Google Traduction)"), "", "translate");
                    item(ui, tr("Favoris"), "", "bookmarks");
                    item(ui, if bm_bar { tr("Masquer la barre des favoris") } else { tr("Afficher la barre des favoris") }, tr("Ctrl+Maj+B"), "bmbar");
                    sep(ui);
                    let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
                    ui.painter().text(pos2(r.left() + 18.0, r.center().y), Align2::LEFT_CENTER, tr("Zoom"), FontId::proportional(14.0), th.text);
                    let plus = Rect::from_center_size(pos2(r.right() - 30.0, r.center().y), vec2(28.0, 26.0));
                    let minus = Rect::from_center_size(pos2(r.right() - 110.0, r.center().y), vec2(28.0, 26.0));
                    ui.painter().text(pos2(r.right() - 70.0, r.center().y), Align2::CENTER_CENTER, format!("{:.0} %", zoom * 100.0), FontId::proportional(13.0), th.text);
                    for (rr, id, is_plus) in [(minus, "zout", false), (plus, "zin", true)] {
                        let resp = ui.interact(rr, egui::Id::new(id), Sense::click());
                        ui.painter().rect_filled(rr, CornerRadius::same(6), if resp.hovered() { th.omnibox_hover } else { th.omnibox });
                        if is_plus {
                            icons::paint(ui.painter(), rr.shrink(7.0), Icon::Plus, th.text);
                        } else {
                            ui.painter().line_segment([pos2(rr.center().x - 6.0, rr.center().y), pos2(rr.center().x + 6.0, rr.center().y)], Stroke::new(1.8, th.text));
                        }
                        if resp.clicked() {
                            zoom_act = Some(id);
                        }
                    }
                    sep(ui);
                    item(ui, tr("Résumer la page"), tr("Ctrl+Maj+Y"), "summary");
                    item(ui, tr("Rechercher dans la page"), tr("Ctrl+F"), "find");
                    item(ui, tr("Imprimer…"), tr("Ctrl+P"), "print");
                    item(ui, tr("Enregistrer la page"), tr("Ctrl+S"), "savepage");
                    item(ui, tr("Plein écran"), tr("F11"), "fullscreen");
                    sep(ui);
                    item(ui, tr("Afficher le code source"), tr("Ctrl+U"), "source");
                    item(ui, tr("Inspecteur web"), tr("Ctrl+Maj+I"), "inspector");
                    item(ui, tr("Créer une extension…"), "", "make-extension");
                    item(ui, tr("Taille d'écran (téléphone, tablette…)"), tr("Ctrl+Maj+M"), "devices");
                    item(ui, tr("Personnaliser la barre d'outils…"), "", "settings");
                    sep(ui);
                    item(ui, if images { tr("Masquer les images") } else { tr("Afficher les images") }, "", "images");
                    item(ui, if dark { tr("Thème clair") } else { tr("Thème sombre") }, "", "dark");
                    sep(ui);
                    item(ui, tr("Remplir avec mes coordonnées"), tr("Ctrl+Maj+F"), "contacts");
                    item(ui, tr("Mots de passe"), "", "passwords");
                    item(ui, tr("Paramètres des sites"), "", "sites");
                    item(ui, tr("Importer depuis un autre navigateur…"), "", "import");
                    item(ui, tr("Effacer les données de navigation…"), tr("Ctrl+Maj+Suppr"), "cleardata");
                    item(ui, tr("Extensions"), "", "extensions");
                    item(ui, tr("Profils"), "", "profiles");
                    sep(ui);
                    item(ui, tr("Paramètres"), "", "settings");
                    item(ui, tr("À propos de Sanaga"), "", "about");
                    item(ui, tr("Quitter"), tr("Ctrl+Q"), "quit");
                    });
                });
        });
        let menu_rect = self.menu_rect;
        let clicked_outside = ctx.input(|i| i.pointer.any_pressed())
            && !ctx.pointer_interact_pos().map(|p| area.response.rect.contains(p) || menu_rect.contains(p)).unwrap_or(false);
        if clicked_outside {
            self.menu_open = false;
        }
        let Some(id) = act.or(zoom_act) else { return };
        if id != "zin" && id != "zout" {
            self.menu_open = false;
        }
        match id {
            "newgroup" => {
                self.sidebar_open = true;
                let n = self.groups.len();
                self.new_group(&tf("Groupe {}", &[&n]));
            }
            "overview" => self.toggle_overview(),
            "contacts" => self.fill_contacts(),
            "tabsearch" => {
                self.tab_search_open = true;
                self.tab_search_q.clear();
            }
            "savepage" => self.save_page(),
            "import" => {
                self.open_tab("sanaga://import", false);
            }
            "sites" => {
                self.open_tab("sanaga://sites", false);
            }
            "cleardata" => {
                self.open_tab("sanaga://clear", false);
            }
            "passwords" => {
                self.open_tab("sanaga://passwords", false);
            }
            "extensions" => {
                self.open_tab("sanaga://extensions", false);
            }
            "profiles" => {
                self.open_tab("sanaga://profiles", false);
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
            "dark" => {
                // a fixed choice from now on (light or dark), not the system's
                let st = &mut self.store.settings;
                st.dark = !st.dark;
                st.theme = if st.dark { 2 } else { 1 };
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

    /// Find-in-page bar, shown as a strip under the toolbar.
    pub(super) fn find_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let web = matches!(self.tabs[self.active].view, View::Web);
        let mut go: Option<isize> = None;
        let mut close = false;
        let count = if web || self.find_query.is_empty() {
            String::new()
        } else if self.find_hits.is_empty() {
            "0/0".into()
        } else {
            format!("{}/{}", self.find_pos + 1, self.find_hits.len())
        };
        ui.horizontal_centered(|ui| {
            ui.add_space(12.0);
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            icons::paint(ui.painter(), r, Icon::Search, th.sub);
            ui.add_space(6.0);
            let id = egui::Id::new("find-input");
            let resp = ui.add(
                TextEdit::singleline(&mut self.find_query)
                    .id(id)
                    .hint_text(tr("Rechercher dans la page"))
                    .desired_width(260.0)
                    .background_color(th.omnibox),
            );
            if self.focus_find {
                resp.request_focus();
                self.focus_find = false;
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                go = Some(if ui.input(|i| i.modifiers.shift) { -1 } else { 1 });
                resp.request_focus();
            }
            if web && resp.changed() && !self.find_query.is_empty() {
                go = Some(1);
            }
            ui.label(RichText::new(&count).size(12.0).color(th.sub));
            if icon_button_small(ui, Icon::Up, th).tip(tr("Précédent (Maj+Entrée)")).clicked() {
                go = Some(-1);
            }
            if icon_button_small(ui, Icon::Down, th).tip(tr("Suivant (Entrée)")).clicked() {
                go = Some(1);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if icon_button_small(ui, Icon::Close, th).tip(tr("Fermer (Échap)")).clicked() {
                    close = true;
                }
            });
        });
        if close {
            self.find_open = false;
            self.find_query.clear();
        }
        let Some(d) = go else { return };
        if web {
            self.web_find(d < 0);
        } else if !self.find_hits.is_empty() {
            let n = self.find_hits.len() as isize;
            self.find_pos = (self.find_pos as isize + d).rem_euclid(n) as usize;
            let b = self.find_hits[self.find_pos];
            self.tabs[self.active].target_block = Some((b, 4));
        }
    }

    pub(super) fn downloads_panel(&mut self, ui: &mut Ui, th: &Theme) {
        let mut close = false;
        let mut show_all = false;
        ui.horizontal_centered(|ui| {
            ui.add_space(10.0);
            for d in self.store.downloads.iter().rev().take(4) {
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(10, 4)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                        icons::paint(ui.painter(), r, Icon::Download, if d.ok { th.accent } else { Color32::from_rgb(0xD0, 0x40, 0x30) });
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&d.name).size(12.5).color(th.text));
                            let s = if d.ok { net::human_bytes(d.bytes) } else { tf("Échec : {}", &[&d.error]) };
                            ui.label(RichText::new(s).size(11.0).color(th.sub));
                        });
                    });
                });
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                if icon_button_small(ui, Icon::Close, th).clicked() {
                    close = true;
                }
                if ui.button(tr("Tout afficher")).clicked() {
                    show_all = true;
                }
            });
        });
        if close {
            self.downloads_bar = false;
        }
        if show_all {
            self.open_tab("sanaga://downloads", false);
        }
    }
}
