//! The first start: a short tour that sets Sanaga up the way the person
//! likes — language, look, layout, privacy, import from another browser,
//! intelligence, default browser. Every step can be skipped; everything can
//! be changed later in Settings (General › Replay the tour).

use super::settings::{layout_choice, popup, segmented, switch};
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align2, Color32, CursorIcon, RichText, Sense};

const STEPS: usize = 7;

impl Sanaga {
    pub(super) fn page_onboarding(&mut self, ui: &mut Ui, th: &Theme) {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, th.ntp_bg);
        let step = self.onboarding_step.min(STEPS - 1);
        let mut changed = false;
        let mut next = false;
        let mut back = false;
        let mut finish = false;
        let mut import = false;
        let mut make_default = false;
        let card_w = (full.width() - 48.0).min(620.0);
        let found_ai = crate::ai_cli::installed();
        let sources = crate::import::chrome_family_sources();
        ui.vertical_centered(|ui| {
            ui.add_space((full.height() * 0.06).clamp(16.0, 60.0));
            egui::Frame::new()
                .fill(th.page)
                .corner_radius(CornerRadius::same(22))
                .shadow(egui::Shadow { offset: [0, 10], blur: 40, spread: 0, color: Color32::from_black_alpha(if self.store.settings.dark { 110 } else { 30 }) })
                .inner_margin(egui::Margin::symmetric(36, 30))
                .show(ui, |ui| {
                    ui.set_width(card_w);
                    ui.set_min_height(360.0);
                    let st = &mut self.store.settings;
                    let title = |ui: &mut Ui, icon: Icon, t: &str, sub: &str| {
                        let (r, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                        if icon == Icon::Home {
                            icons::logo(ui.painter(), r.center(), 28.0);
                        } else {
                            ui.painter().rect_filled(r, CornerRadius::same(14), th.accent);
                            icons::paint(ui.painter(), r.shrink(15.0), icon, Color32::WHITE);
                        }
                        ui.add_space(12.0);
                        ui.label(RichText::new(t).size(24.0).family(bold()).color(th.text));
                        ui.add_space(4.0);
                        ui.label(RichText::new(sub).size(14.0).color(th.sub));
                        ui.add_space(22.0);
                    };
                    match step {
                        0 => {
                            title(ui, Icon::Home, tr("Bienvenue dans Sanaga"), tr("Un navigateur rapide, léger et respectueux. Quelques choix et c'est parti."));
                            ui.label(RichText::new("Langue · Language · Idioma · Sprache").size(13.0).color(th.sub));
                            ui.add_space(6.0);
                            let opts: Vec<(&str, &str)> = crate::i18n::LANGUAGES.iter().map(|(c, n)| (*c, *n)).collect();
                            let cur = opts.iter().find(|o| o.0 == st.language).map(|o| o.0).unwrap_or("en");
                            if let Some(code) = popup(ui, "onb-language", cur, &opts) {
                                st.language = code.to_string();
                                changed = true;
                            }
                        }
                        1 => {
                            title(ui, Icon::Palette, tr("Votre style"), tr("Clair, sombre, ou comme le reste de l'ordinateur — et votre couleur."));
                            if let Some(k) = segmented(ui, th, &[tr("Système"), tr("Clair"), tr("Sombre")], st.theme as usize) {
                                st.theme = k as u8;
                                if k > 0 {
                                    st.dark = k == 2;
                                }
                                changed = true;
                            }
                            ui.add_space(18.0);
                            ui.horizontal(|ui| {
                                let total = crate::theme::ACCENTS.len() as f32 * 34.0;
                                ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
                                for (k, (name, _l, _d, swatch)) in crate::theme::ACCENTS.iter().enumerate() {
                                    let col = if k == 0 { crate::theme::ACCENTS[crate::theme::accent_in_use_of_system()].3 } else { *swatch };
                                    let (r, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                                    ui.painter().circle_filled(r.center(), 11.0, col);
                                    if st.accent as usize == k {
                                        ui.painter().circle_stroke(r.center(), 14.5, Stroke::new(2.0, col));
                                    }
                                    if resp.on_hover_cursor(CursorIcon::PointingHand).tip(tr(name)).clicked() {
                                        st.accent = k as u8;
                                        changed = true;
                                    }
                                }
                            });
                        }
                        2 => {
                            title(ui, Icon::Grid, tr("Où mettre les onglets ?"), tr("En haut comme Safari, ou sur le côté pour en garder beaucoup."));
                            ui.horizontal(|ui| {
                                ui.add_space(((ui.available_width() - 4.0 * 100.0) / 2.0).max(0.0));
                                for (v, label) in [(0u8, tr("En haut")), (1, tr("En bas")), (2, tr("À gauche")), (3, tr("À droite"))] {
                                    if layout_choice(ui, th, label, v, 9, st.tabs_pos == v) {
                                        st.tabs_pos = v;
                                        changed = true;
                                    }
                                }
                            });
                            ui.add_space(10.0);
                            ui.label(RichText::new(tr("Barre d'adresse")).size(13.0).color(th.sub));
                            ui.horizontal(|ui| {
                                ui.add_space(((ui.available_width() - 3.0 * 100.0) / 2.0).max(0.0));
                                for (v, label) in [(0u8, tr("En haut")), (1, tr("En bas")), (2, tr("Flottante"))] {
                                    if layout_choice(ui, th, label, 9, v, st.bar_pos == v) {
                                        st.bar_pos = v;
                                        changed = true;
                                    }
                                }
                            });
                        }
                        3 => {
                            title(ui, Icon::Shield, tr("Votre vie privée"), tr("Sanaga n'envoie rien à personne. Choisissez vos protections."));
                            let mut rowsw = |ui: &mut Ui, label: &str, on: &mut bool| {
                                ui.horizontal(|ui| {
                                    ui.set_width(card_w * 0.8);
                                    ui.label(RichText::new(label).size(14.0).color(th.text));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if switch(ui, th, on) {
                                            changed = true;
                                        }
                                    });
                                });
                                ui.add_space(8.0);
                            };
                            rowsw(ui, tr("Empêcher le suivi intersites"), &mut st.itp);
                            let mut ads = self.store.extensions_on.iter().any(|e| e == "bloqueur-de-publicites");
                            let mut banners = self.store.extensions_on.iter().any(|e| e == "sans-bannieres-cookies");
                            let (a0, b0) = (ads, banners);
                            rowsw(ui, tr("Bloqueur de publicités"), &mut ads);
                            rowsw(ui, tr("Sans bannières de cookies"), &mut banners);
                            for (id, now, before) in [("bloqueur-de-publicites", ads, a0), ("sans-bannieres-cookies", banners, b0)] {
                                if now != before {
                                    self.store.extensions_on.retain(|e| e != id);
                                    if now {
                                        self.store.extensions_on.push(id.into());
                                    }
                                    self.store.save_extensions();
                                }
                            }
                            let st = &mut self.store.settings;
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.set_width(card_w * 0.8);
                                ui.label(RichText::new(tr("Moteur de recherche")).size(14.0).color(th.text));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let opts: Vec<(Engine, &str)> = Engine::ALL.iter().map(|e| (*e, e.label())).collect();
                                    if let Some(e) = popup(ui, "onb-engine", st.engine, &opts) {
                                        st.engine = e;
                                        changed = true;
                                    }
                                });
                            });
                        }
                        4 => {
                            title(ui, Icon::Download, tr("Retrouvez vos affaires"), tr("Favoris, historique et mots de passe d'un autre navigateur."));
                            if sources.is_empty() {
                                ui.label(RichText::new(tr("Aucun autre navigateur trouvé sur cet ordinateur.")).color(th.sub));
                            } else {
                                let names: Vec<String> = sources.iter().map(|(n, _)| n.to_string()).collect();
                                ui.label(RichText::new(tf("Trouvés : {}", &[&names.join(", ")])).size(14.0).color(th.text));
                                ui.add_space(12.0);
                                if ui.add(egui::Button::new(RichText::new(tr("Importer maintenant")).color(Color32::WHITE)).fill(th.accent).min_size(vec2(180.0, 34.0)).corner_radius(CornerRadius::same(10))).clicked() {
                                    import = true;
                                }
                            }
                        }
                        5 => {
                            title(ui, Icon::Sparkle, tr("Intelligence"), tr("Résumés de pages, traduction et extensions écrites pour vous."));
                            let mut choice = |ui: &mut Ui, key: &str, label: &str| {
                                let chosen = st.ai == key;
                                let (r, resp) = ui.allocate_exact_size(vec2(card_w * 0.8, 40.0), Sense::click());
                                ui.painter().rect_filled(r, CornerRadius::same(10), if chosen { th.accent.gamma_multiply(0.14) } else if resp.hovered() { th.card } else { Color32::TRANSPARENT });
                                ui.painter().rect_stroke(r, CornerRadius::same(10), Stroke::new(1.0, if chosen { th.accent } else { th.border }), egui::StrokeKind::Inside);
                                ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), th.text);
                                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                                    st.ai = key.to_string();
                                    st.ai_model.clear();
                                    changed = true;
                                }
                                ui.add_space(6.0);
                            };
                            for t in &found_ai {
                                choice(ui, t.key(), &tf("{} (installé sur cet ordinateur)", &[&t.name()]));
                            }
                            choice(ui, "", tr("Sanaga seul — sans IA, rien ne quitte l'ordinateur"));
                            ui.label(RichText::new(tr("Claude avec une clé ou Ollama : dans Paramètres › Intelligence.")).size(12.0).color(th.sub));
                        }
                        _ => {
                            title(ui, Icon::StarFilled, tr("Tout est prêt"), tr("Faites de Sanaga votre navigateur, ou gardez celui de maintenant."));
                            if ui.add(egui::Button::new(RichText::new(tr("Définir par défaut")).color(Color32::WHITE)).fill(th.accent).min_size(vec2(180.0, 34.0)).corner_radius(CornerRadius::same(10))).clicked() {
                                make_default = true;
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new(tr("Astuce : Ctrl+T nouvel onglet, Ctrl+L barre d'adresse, Ctrl+Maj+Y résumer la page.")).size(12.5).color(th.sub));
                        }
                    }
                });
            ui.add_space(20.0);
            // progress and buttons: Back on the left, dots in the middle, Continue on the right
            let (row, _) = ui.allocate_exact_size(vec2(card_w, 36.0), Sense::hover());
            if step > 0 {
                let br = Rect::from_min_size(row.left_top(), vec2(110.0, 34.0));
                if ui.put(br, egui::Button::new(tr("Retour")).corner_radius(CornerRadius::same(10))).clicked() {
                    back = true;
                }
            }
            for k in 0..STEPS {
                let c = pos2(row.center().x + (k as f32 - (STEPS as f32 - 1.0) / 2.0) * 14.0, row.center().y);
                ui.painter().circle_filled(c, if k == step { 4.0 } else { 3.0 }, if k == step { th.accent } else { th.border });
            }
            let label = if step + 1 == STEPS { tr("Commencer") } else { tr("Continuer") };
            let nr = Rect::from_min_size(pos2(row.right() - 130.0, row.top()), vec2(130.0, 34.0));
            if ui.put(nr, egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(th.accent).corner_radius(CornerRadius::same(10))).clicked() {
                if step + 1 == STEPS {
                    finish = true;
                } else {
                    next = true;
                }
            }
            ui.add_space(10.0);
            if step + 1 < STEPS && ui.link(RichText::new(tr("Passer la présentation")).color(th.sub)).clicked() {
                finish = true;
            }
        });
        if changed {
            crate::i18n::set_language(&self.store.settings.language);
            self.store.save_settings();
        }
        if make_default {
            self.make_default_browser_pub();
        }
        if back {
            self.onboarding_step = step.saturating_sub(1);
        }
        if next {
            self.onboarding_step = step + 1;
        }
        if import {
            self.open_tab("sanaga://import", false);
        }
        if finish {
            self.onboarding_step = 0;
            let a = self.active;
            self.navigate(a, "sanaga://newtab".into(), None, true);
        }
    }
}
