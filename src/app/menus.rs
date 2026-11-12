//! Bookmarks bar, the ⋮ menu, the find-in-page bar and the downloads bar.

use super::chrome::{icon_button_small, menu_item, menu_item_keys, site_badge};
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align, Color32, Layout, Margin, RichText, Sense, TextEdit};

impl Sanaga {
    pub(super) fn bookmarks_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let mut go: Option<(String, bool)> = None;
        let mut remove: Option<String> = None;
        let mut edit: Option<usize> = None;
        let mut copy: Option<String> = None;
        let mut hide = false;
        let full = ui.max_rect();
        // right-click on the empty bar, or the small button at its right end, hides it
        let bg = ui.interact(full, egui::Id::new("bookmarks-bar-bg"), Sense::click());
        bg.menu_on_press(|ui| {
            if menu_item_keys(ui, Icon::StarFilled, tr("Masquer la barre des favoris"), tr("Ctrl+Maj+B")) {
                hide = true;
            }
        });
        let hr = Rect::from_center_size(pos2(full.right() - 22.0, full.center().y), vec2(22.0, 22.0));
        let hresp = ui.interact(hr, egui::Id::new("bookmarks-bar-hide"), Sense::click());
        if hresp.hovered() {
            ui.painter().rect_filled(hr, CornerRadius::same(5), th.omnibox_hover);
        }
        icons::paint(ui.painter(), hr.shrink(6.0), Icon::Close, th.sub);
        if hresp.tip(tr("Masquer la barre des favoris (Ctrl+Maj+B)\nPour la réafficher : menu principal ou Ctrl+Maj+B")).clicked() {
            hide = true;
        }
        ui.horizontal_centered(|ui| {
            ui.add_space(10.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            if self.store.bookmarks.is_empty() {
                ui.label(RichText::new(tr("Ajoutez des favoris avec l'étoile de la barre d'adresse.")).size(12.0).color(th.sub));
            }
            for (k, b) in self.store.bookmarks.iter().enumerate() {
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
                    if menu_item(ui, Icon::Plus, tr("Ouvrir dans un nouvel onglet")) {
                        go = Some((b.url.clone(), true));
                    }
                    // (a favourite could be opened or deleted from here, not
                    // renamed nor its address taken)
                    if menu_item(ui, Icon::Pencil, tr("Modifier…")) {
                        edit = Some(k);
                    }
                    if menu_item(ui, Icon::Copy, tr("Copier l'adresse")) {
                        copy = Some(b.url.clone());
                    }
                    if menu_item(ui, Icon::Close, tr("Supprimer")) {
                        remove = Some(b.url.clone());
                    }
                    ui.separator();
                    if menu_item_keys(ui, Icon::StarFilled, tr("Masquer la barre des favoris"), tr("Ctrl+Maj+B")) {
                        hide = true;
                    }
                });
            }
        });
        if hide {
            self.store.settings.show_bookmarks_bar = false;
            self.store.save_settings();
        }
        self.apply_go(go);
        if let Some(u) = copy {
            ui.ctx().copy_text(u);
            self.say(tr("Adresse copiée."));
        }
        if let Some(k) = edit {
            // on the favourites page, its line open for editing
            if let Some(b) = self.store.bookmarks.get(k) {
                self.bm_edit = (b.title.clone(), b.url.clone());
                self.bm_editing = Some(k);
                self.open_tab("sanaga://bookmarks", false);
            }
        }
        if let Some(u) = remove {
            self.store.bookmarks.retain(|b| b.url != u);
            self.store.save_bookmarks();
        }
    }

    /// Find-in-page bar, shown as a strip under the toolbar.
    pub(super) fn find_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let web = matches!(self.tabs[self.active].view, View::Web);
        let mut go: Option<isize> = None;
        let mut close = false;
        let tab = self.tabs[self.active].id;
        // what was found: "3/12" in Sanaga's own pages (it knows where it
        // is), the number of matches in a web page (the engine tells only
        // that), and in words when there is none
        let (count, none) = if self.find_query.is_empty() {
            (String::new(), false)
        } else if web {
            match self.find_web.filter(|(t, _)| *t == tab) {
                None => (String::new(), false),
                Some((_, 0)) => (tr("Aucun résultat").to_string(), true),
                // (the place of the match shown, when it is known for sure)
                Some((_, n)) if self.find_at.is_some_and(|(t, _)| t == tab) => (format!("{}/{n}", self.find_at.map(|(_, at)| at).unwrap_or(1)), false),
                Some((_, 1)) => (tr("1 résultat").to_string(), false),
                Some((_, n)) if n >= 999 => (tr("Plus de 999 résultats").to_string(), false),
                Some((_, n)) => (tf("{} résultats", &[&n]), false),
            }
        } else if self.find_hits.is_empty() {
            (tr("Aucun résultat").to_string(), true)
        } else {
            (format!("{}/{}", self.find_pos + 1, self.find_hits.len()), false)
        };
        let mut fresh = false;
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
                // the words of last time are selected: typing replaces them,
                // Enter looks for them again
                if let Some(mut state) = TextEdit::load_state(ui.ctx(), id) {
                    let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(self.find_query.chars().count()));
                    state.cursor.set_char_range(Some(all));
                    state.store(ui.ctx(), id);
                }
                if web && !self.find_query.is_empty() {
                    go = Some(1);
                    fresh = true;
                }
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                go = Some(if ui.input(|i| i.modifiers.shift) { -1 } else { 1 });
                resp.request_focus();
            }
            if web && resp.changed() {
                go = Some(1);
                fresh = true;
            }
            ui.label(RichText::new(&count).size(12.0).color(if none { Color32::from_rgb(0xC8, 0x3A, 0x2B) } else { th.sub }));
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
            self.close_find();
            return;
        }
        let Some(d) = go else { return };
        if web {
            self.web_find(d < 0, fresh);
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
