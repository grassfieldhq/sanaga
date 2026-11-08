//! Start page (new tab), in the spirit of Safari's: favourites as tiles,
//! frequently visited sites, and a sobriety report instead of a privacy report.

use super::chrome::site_badge;
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align2, Color32, CursorIcon, RichText, Sense};

fn section_title(ui: &mut Ui, th: &Theme, text: &str) {
    ui.label(RichText::new(text).size(19.0).family(bold()).color(th.text));
    ui.add_space(12.0);
}

fn card_bg(p: &egui::Painter, r: Rect, radius: u8, th: &Theme, hovered: bool) {
    p.rect_filled(r.translate(vec2(0.0, 1.0)), CornerRadius::same(radius), Color32::from_black_alpha(14));
    p.rect_filled(r, CornerRadius::same(radius), if hovered { th.omnibox_hover } else if th.page == Color32::WHITE { th.page } else { th.card });
    p.rect_stroke(r, CornerRadius::same(radius), Stroke::new(0.6, th.border), egui::StrokeKind::Inside);
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    } else {
        s.to_string()
    }
}

impl Sanaga {
    pub(super) fn page_newtab(&mut self, ui: &mut Ui, th: &Theme) {
        let a = self.active;
        ui.painter().rect_filled(ui.clip_rect(), 0.0, th.ntp_bg);
        if self.tabs[a].private {
            self.private_start(ui, th);
            return;
        }
        let mut go: Option<String> = None;
        let full_w = ui.available_width();
        let cw = (full_w - 80.0).clamp(300.0, 760.0);
        let left = ((full_w - cw) / 2.0).max(20.0);
        let favs: Vec<(String, String)> = self.store.bookmarks.iter().filter(|b| !b.url.starts_with("sanaga://")).take(12).map(|b| (b.url.clone(), b.title.clone())).collect();
        let reading: Vec<(String, String)> = self.store.reading.iter().take(5).map(|b| (b.url.clone(), b.title.clone())).collect();
        // frequently visited: most visited addresses not already in favourites
        let mut counts: HashMap<String, (usize, String)> = HashMap::new();
        for v in &self.store.history {
            let e = counts.entry(v.url.clone()).or_insert((0, String::new()));
            e.0 += 1;
            if !v.title.is_empty() {
                e.1 = v.title.clone();
            }
        }
        let mut freq: Vec<(String, usize, String)> = counts.into_iter().filter(|(u, _)| !self.store.is_bookmarked(u)).map(|(u, (n, t))| (u, n, t)).collect();
        freq.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
        freq.truncate(8);
        let webkit_on = self.store.settings.webkit && (self.web_ok || self.screenshot.is_some());
        let mut show_reading = self.store.settings.ntp_reading;
        let mut show_frequent = self.store.settings.ntp_frequent;
        let mut show_reports = self.store.settings.ntp_reports;
        let mut show_bar = self.store.settings.show_bookmarks_bar;
        let customize = self.customize_open;
        let mut toggle_customize = false;
        let mut changed = false;
        let itp = self.store.settings.itp;
        let now = ui.input(|i| i.time);
        self.refresh_privacy(now);
        let privacy = self.privacy.clone();
        let devices = self.sync_status.devices.clone();
        let engine_txt = if webkit_on { tr("WebKit, sites complets") } else { tr("Sanaga Léger, pages simplifiées") };
        let report = [
            tf("{} de mémoire", &[&net::human_bytes(self.mem.1)]),
            tf("Pages actives : {} sur {} onglets", &[&self.tabs.iter().filter(|t| t.web.is_some()).count().max(1), &self.tabs.len()]),
            tf("Moteur d'affichage : {}.", &[&engine_txt]),
            tf("Pages chargées depuis le démarrage : {}", &[&self.pages_loaded]),
        ];

        ui.horizontal(|ui| {
            ui.add_space(left);
            ui.vertical(|ui| {
                ui.set_width(cw);
                ui.add_space((ui.available_height() * 0.08).clamp(24.0, 70.0));
                // brand
                let (r, _) = ui.allocate_exact_size(vec2(cw, 56.0), Sense::hover());
                let g = ui.painter().layout_no_wrap("Sanaga".into(), FontId::new(30.0, bold()), th.text);
                let total = 44.0 + 12.0 + g.size().x;
                let x0 = r.center().x - total / 2.0;
                icons::logo(ui.painter(), pos2(x0 + 22.0, r.center().y), 22.0);
                ui.painter().galley(pos2(x0 + 56.0, r.center().y - g.size().y / 2.0), g, th.text);
                ui.add_space(26.0);

                // favourites
                section_title(ui, th, tr("Favoris"));
                let tile = vec2(100.0, 108.0);
                let per_row = ((cw / tile.x).floor() as usize).max(1);
                for row in favs.chunks(per_row) {
                    let (rr, _) = ui.allocate_exact_size(vec2(cw, tile.y), Sense::hover());
                    for (k, (url, title)) in row.iter().enumerate() {
                        let r = Rect::from_min_size(pos2(rr.left() + k as f32 * tile.x, rr.top()), tile);
                        let resp = ui.interact(r, egui::Id::new(("fav", url)), Sense::click());
                        let sq = Rect::from_center_size(pos2(r.center().x, r.top() + 38.0), vec2(68.0, 68.0));
                        card_bg(ui.painter(), sq, 16, th, resp.hovered());
                        site_badge(ui.painter(), sq.center(), 17.0, url, title);
                        ui.painter().text(pos2(r.center().x, r.top() + 90.0), Align2::CENTER_CENTER, short(title, 14), FontId::proportional(12.5), th.text);
                        if resp.tip(url).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            go = Some(url.clone());
                        }
                    }
                }
                if favs.is_empty() {
                    ui.label(RichText::new(tr("Ajoutez des favoris avec l'étoile de la barre d'adresse.")).color(th.sub));
                }
                ui.add_space(22.0);

                // reading list (Safari-style), when there is something in it
                if !reading.is_empty() && show_reading {
                    section_title(ui, th, tr("Liste de lecture"));
                    for (url, title) in &reading {
                        let (r, _) = ui.allocate_exact_size(vec2(cw, 50.0), Sense::hover());
                        let r = r.shrink2(vec2(0.0, 3.0));
                        let resp = ui.interact(r, egui::Id::new(("reading", url)), Sense::click());
                        card_bg(ui.painter(), r, 10, th, resp.hovered());
                        icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), vec2(18.0, 18.0)), Icon::Glasses, th.accent);
                        let clip = Rect::from_min_max(pos2(r.left() + 44.0, r.top()), pos2(r.right() - 12.0, r.bottom()));
                        let p = ui.painter().with_clip_rect(clip);
                        p.text(pos2(clip.left(), r.center().y - 8.0), Align2::LEFT_CENTER, title, FontId::new(13.0, bold()), th.text);
                        p.text(pos2(clip.left(), r.center().y + 9.0), Align2::LEFT_CENTER, host_of(url), FontId::proportional(12.0), th.sub);
                        if resp.tip(url).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            go = Some(url.clone());
                        }
                    }
                    ui.add_space(22.0);
                }

                // frequently visited
                if show_frequent {
                section_title(ui, th, tr("Consultés fréquemment"));
                if freq.is_empty() {
                    ui.label(RichText::new(tr("Les sites que vous visitez souvent apparaîtront ici.")).color(th.sub));
                } else {
                    let cols = ((cw + 12.0) / 182.0).floor().max(1.0) as usize;
                    let card_w = (cw - (cols as f32 - 1.0) * 12.0) / cols as f32;
                    for row in freq.chunks(cols) {
                        let (rr, _) = ui.allocate_exact_size(vec2(cw, 64.0), Sense::hover());
                        for (k, (url, _, title)) in row.iter().enumerate() {
                            let r = Rect::from_min_size(pos2(rr.left() + k as f32 * (card_w + 12.0), rr.top()), vec2(card_w, 58.0));
                            let resp = ui.interact(r, egui::Id::new(("freq", url)), Sense::click());
                            card_bg(ui.painter(), r, 12, th, resp.hovered());
                            site_badge(ui.painter(), pos2(r.left() + 24.0, r.center().y), 11.0, url, title);
                            let clip = Rect::from_min_max(pos2(r.left() + 44.0, r.top()), pos2(r.right() - 10.0, r.bottom()));
                            let p = ui.painter().with_clip_rect(clip);
                            let t = if title.is_empty() { host_of(url) } else { title.clone() };
                            p.text(pos2(clip.left(), r.center().y - 8.0), Align2::LEFT_CENTER, t, FontId::new(13.0, bold()), th.text);
                            p.text(pos2(clip.left(), r.center().y + 9.0), Align2::LEFT_CENTER, host_of(url), FontId::proportional(12.0), th.sub);
                            if resp.tip(url).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                                go = Some(url.clone());
                            }
                        }
                    }
                }
                ui.add_space(22.0);
                }

                // tabs open on the person's other computers (sync folder)
                if !devices.is_empty() {
                    section_title(ui, th, tr("Onglets sur vos autres appareils"));
                    for (device, tabs) in &devices {
                        ui.label(RichText::new(device).size(13.0).family(bold()).color(th.sub));
                        ui.add_space(4.0);
                        for (url, title) in tabs.iter().take(8) {
                            let (r, _) = ui.allocate_exact_size(vec2(cw, 34.0), Sense::hover());
                            let resp = ui.interact(r, egui::Id::new(("device-tab", device, url)), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(r, CornerRadius::same(8), th.card);
                            }
                            site_badge(ui.painter(), pos2(r.left() + 16.0, r.center().y), 8.0, url, title);
                            let clip = Rect::from_min_max(pos2(r.left() + 32.0, r.top()), pos2(r.right() - 8.0, r.bottom()));
                            let t = if title.is_empty() { host_of(url) } else { title.clone() };
                            let g = ui.painter().with_clip_rect(clip).text(pos2(clip.left(), r.center().y), Align2::LEFT_CENTER, t, FontId::proportional(13.0), th.text);
                            ui.painter().with_clip_rect(clip).text(pos2(g.right() + 10.0, r.center().y), Align2::LEFT_CENTER, host_of(url), FontId::proportional(12.0), th.sub);
                            if resp.tip(url).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                                go = Some(url.clone());
                            }
                        }
                        ui.add_space(8.0);
                    }
                    ui.add_space(14.0);
                }

                // privacy and sobriety reports, side by side when there is room
                if show_reports {
                section_title(ui, th, tr("Rapports"));
                let two = cw >= 640.0;
                let card_w = if two { (cw - 14.0) / 2.0 } else { cw };
                let (rr, _) = ui.allocate_exact_size(vec2(cw, 132.0), Sense::hover());
                let pr = Rect::from_min_size(rr.min, vec2(card_w, 132.0));
                card_bg(ui.painter(), pr, 14, th, false);
                let blue = Color32::from_rgb(0x0A, 0x84, 0xFF);
                let pic = Rect::from_center_size(pos2(pr.left() + 36.0, pr.center().y), vec2(36.0, 36.0));
                ui.painter().circle_filled(pic.center(), 22.0, blue.gamma_multiply(0.14));
                icons::paint(ui.painter(), pic.shrink(7.0), Icon::Shield, blue);
                let lines: Vec<String> = if !itp {
                    vec![tr("Rapport de confidentialité").into(), tr("La protection contre le suivi intersites est désactivée.").into(), tr("Activez-la dans les Paramètres.").into()]
                } else if privacy.is_empty() {
                    vec![tr("Rapport de confidentialité").into(), tr("Sanaga empêche les traceurs de vous suivre").into(), tr("d'un site à l'autre. Aucun repéré pour l'instant.").into()]
                } else {
                    let (d, n) = &privacy[0];
                    vec![
                        tr("Rapport de confidentialité").into(),
                        tf("Traceurs empêchés de vous profiler : {}", &[&privacy.len()]),
                        tf("Le plus présent : {} (sites : {})", &[&d, &n]),
                    ]
                };
                let pclip = ui.painter().with_clip_rect(pr.shrink(4.0));
                for (k, line) in lines.iter().enumerate() {
                    let (size, col, font) = if k == 0 { (14.0, th.text, bold()) } else { (12.5, th.sub, FontFamily::Proportional) };
                    pclip.text(pos2(pr.left() + 72.0, pr.top() + 30.0 + k as f32 * 22.0), Align2::LEFT_CENTER, line, FontId::new(size, font), col);
                }
                let r = if two {
                    Rect::from_min_size(pos2(pr.right() + 14.0, rr.top()), vec2(card_w, 132.0))
                } else {
                    let (r2, _) = ui.allocate_exact_size(vec2(cw, 144.0), Sense::hover());
                    Rect::from_min_size(pos2(r2.left(), r2.top() + 12.0), vec2(cw, 132.0))
                };
                card_bg(ui.painter(), r, 14, th, false);
                let ic = Rect::from_center_size(pos2(r.left() + 36.0, r.center().y), vec2(36.0, 36.0));
                ui.painter().circle_filled(ic.center(), 22.0, Color32::from_rgb(0x34, 0xC7, 0x59).gamma_multiply(0.16));
                icons::paint(ui.painter(), ic.shrink(6.0), Icon::Leaf, Color32::from_rgb(0x34, 0xC7, 0x59));
                let sclip = ui.painter().with_clip_rect(r.shrink(4.0));
                sclip.text(pos2(r.left() + 72.0, r.top() + 30.0), Align2::LEFT_CENTER, tr("Rapport de sobriété"), FontId::new(14.0, bold()), th.text);
                for (k, line) in report.iter().enumerate() {
                    sclip.text(pos2(r.left() + 72.0, r.top() + 52.0 + k as f32 * 19.0), Align2::LEFT_CENTER, line, FontId::proportional(12.5), th.sub);
                }
                }
                // choose what the start page shows
                ui.add_space(26.0);
                ui.horizontal(|ui| {
                    if ui.link(RichText::new(if customize { tr("Terminer") } else { tr("Personnaliser la page d'accueil") }).size(13.0).color(th.accent)).clicked() {
                        toggle_customize = true;
                    }
                });
                if customize {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        changed |= ui.checkbox(&mut show_reading, tr("Liste de lecture")).changed();
                        changed |= ui.checkbox(&mut show_frequent, tr("Consultés fréquemment")).changed();
                        changed |= ui.checkbox(&mut show_reports, tr("Rapports")).changed();
                        changed |= ui.checkbox(&mut show_bar, tr("Barre des favoris")).changed();
                    });
                }
                ui.add_space(40.0);
            });
        });
        if toggle_customize {
            self.customize_open = !self.customize_open;
        }
        if changed {
            let s = &mut self.store.settings;
            s.ntp_reading = show_reading;
            s.ntp_frequent = show_frequent;
            s.ntp_reports = show_reports;
            s.show_bookmarks_bar = show_bar;
            self.store.save_settings();
        }
        if let Some(u) = go {
            self.navigate(a, u, None, true);
        }
    }

    fn private_start(&mut self, ui: &mut Ui, th: &Theme) {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.2).clamp(30.0, 160.0));
            let (r, _) = ui.allocate_exact_size(vec2(84.0, 84.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 42.0, th.card);
            icons::paint(ui.painter(), r.shrink(22.0), Icon::Private, th.accent);
            ui.add_space(18.0);
            ui.label(RichText::new(tr("Navigation privée")).size(26.0).family(bold()).color(th.text));
            ui.add_space(10.0);
            let text = tr("Sanaga ne garde ni l'historique, ni les cookies, ni les données de cet onglet : tout est effacé dès que vous le fermez. Les favoris et les téléchargements sont conservés.");
            let g = ui.painter().layout(text.to_string(), FontId::proportional(14.0), th.sub, 520.0);
            let (r, _) = ui.allocate_exact_size(vec2(520.0, g.size().y), Sense::hover());
            ui.painter().galley(pos2(r.center().x - g.size().x / 2.0, r.top()), g, th.sub);
        });
    }
}
