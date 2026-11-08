//! Bookmarks bar, the ⋮ menu, the find-in-page bar and the downloads bar.

use super::chrome::{icon_button_small, site_badge};
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align, Color32, Layout, Margin, RichText, Sense, TextEdit};

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
