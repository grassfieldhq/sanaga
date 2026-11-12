//! "Settings for this website": everything about the site shown, in one
//! panel — JavaScript, web fonts, the ad and tracker blocker, its look (dark,
//! another font, no sticky headers), copy unlocked, the parts removed from
//! it, the identity Sanaga gives it, and its zoom.

use super::*;
use crate::identity::Identity;
use crate::site_prefs::SitePrefs;
use eframe::egui::{Color32, Margin, RichText};

/// Fonts offered for pages (installed on most systems; else the system's
/// sans-serif is used).
const FONTS: [&str; 7] = ["", "sans-serif", "serif", "Inter", "Noto Sans", "Atkinson Hyperlegible", "monospace"];

/// A site's settings in a few words ("no JavaScript · dark · 2 elements removed").
pub(super) fn summary(p: &SitePrefs) -> String {
    let mut what: Vec<String> = Vec::new();
    for (on, label) in [(!p.js, tr("sans JavaScript")), (!p.blocker, tr("bloqueur coupé")), (!p.fonts, tr("sans ses polices")), (p.dark, tr("sombre")), (p.no_sticky, tr("sans bandeaux collants")), (p.unlock, tr("copie débloquée"))] {
        if on {
            what.push(label.to_string());
        }
    }
    if !p.font.is_empty() {
        what.push(p.font.clone());
    }
    if !p.hidden.is_empty() {
        what.push(tf("{} élément(s) retiré(s)", &[&p.hidden.len()]));
    }
    what.join(" · ")
}

impl Sanaga {
    pub(super) fn site_settings_panel(&mut self, ctx: &egui::Context, th: &Theme) {
        if !self.site_panel {
            return;
        }
        let url = self.tabs[self.active].url().to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            self.site_panel = false;
            self.notice = Some((tr("Les réglages de site concernent les pages web.").into(), ctx.input(|i| i.time)));
            return;
        }
        let host = crate::site_prefs::key(&host_of(&url));
        let before = crate::site_prefs::for_host(&host);
        let mut p = before.clone();
        let mut identity = crate::identity::for_host(&host);
        let identity_before = identity;
        // (nothing to reset on a site left as it came)
        let changed = before != SitePrefs::default() || identity != Identity::Auto;
        let mut close = false;
        let mut reset = false;
        let mut zap = false;
        let screen = ctx.content_rect();
        let dark = self.store.settings.dark;
        // near the address bar: under it, or above it when it is at the bottom
        let (at, pivot) = if self.bar_at() == super::layout::BarAt::Bottom {
            (pos2(screen.center().x - 200.0, screen.bottom() - 70.0), egui::Align2::LEFT_BOTTOM)
        } else {
            (pos2(screen.center().x - 200.0, screen.top() + 70.0), egui::Align2::LEFT_TOP)
        };
        egui::Area::new(egui::Id::new("site-panel")).order(egui::Order::Foreground).pivot(pivot).fixed_pos(at).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.border))
                .corner_radius(CornerRadius::same(14))
                .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(if dark { 120 } else { 60 }) })
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(368.0);
                    ui.horizontal(|ui| {
                        let (badge, _) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
                        super::chrome::site_badge(ui.painter(), badge.center(), 9.0, &url, &host);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.add(egui::Label::new(RichText::new(&host).size(15.0).family(bold()).color(th.text)).truncate());
                            ui.label(RichText::new(tr("Réglages de ce site")).size(12.0).color(th.sub));
                        });
                    });
                    ui.add_space(10.0);
                    // The same language as the Settings: cards of rows, the
                    // words on the left (as many lines as they need), the
                    // control against the right edge, a hairline between rows.
                    let card = |ui: &mut Ui, rows: &mut dyn FnMut(&mut Ui)| {
                        Frame::new().fill(th.card).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(12, 4)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 0.0;
                            rows(ui);
                        });
                        ui.add_space(8.0);
                    };
                    let line = |ui: &mut Ui, first: bool, label: &str, sub: &str, room: f32, control: &mut dyn FnMut(&mut Ui)| {
                        if !first {
                            let y = ui.cursor().top();
                            ui.painter().hline(ui.max_rect().x_range(), y, Stroke::new(0.6, th.border));
                        }
                        ui.add_space(7.0);
                        ui.horizontal(|ui| {
                            ui.set_min_height(24.0);
                            let w = (ui.available_width() - room).max(80.0);
                            let name = egui::Label::new(RichText::new(label).size(13.5).color(th.text)).wrap();
                            if sub.is_empty() {
                                // (alone on its line: level with its control, not above it)
                                ui.allocate_ui_with_layout(vec2(w, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.set_width(w);
                                    ui.add(name);
                                });
                            } else {
                                ui.vertical(|ui| {
                                    ui.set_width(w);
                                    ui.spacing_mut().item_spacing.y = 1.0;
                                    ui.add(name);
                                    ui.add(egui::Label::new(RichText::new(sub).size(11.0).color(th.sub)).wrap());
                                });
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
                        });
                        ui.add_space(7.0);
                    };
                    let switch = |ui: &mut Ui, first: bool, label: &str, sub: &str, on: &mut bool| {
                        line(ui, first, label, sub, 50.0, &mut |ui| {
                            super::settings::switch(ui, th, on);
                        });
                    };
                    // (a small window: the panel scrolls rather than leaving the screen)
                    egui::ScrollArea::vertical().max_height((screen.height() - 190.0).max(160.0)).show(ui, |ui| {
                        card(ui, &mut |ui| {
                            switch(ui, true, tr("JavaScript"), tr("Désactivé : pages plus légères, mais certains sites ne marchent plus"), &mut p.js);
                            switch(ui, false, tr("Bloqueur de publicités et de traceurs"), "", &mut p.blocker);
                            switch(ui, false, tr("Polices du site"), tr("Désactivé : les polices de votre système, plus légères"), &mut p.fonts);
                        });
                        card(ui, &mut |ui| {
                            switch(ui, true, tr("Mode sombre"), "", &mut p.dark);
                            switch(ui, false, tr("Sans bandeaux collants"), tr("Les en-têtes et barres fixés à l'écran défilent avec la page"), &mut p.no_sticky);
                            switch(ui, false, tr("Copier-coller débloqué"), tr("Sélection, copie et clic droit même si le site les interdit"), &mut p.unlock);
                            line(ui, false, tr("Police des pages"), "", 170.0, &mut |ui| {
                                let label = |f: &str| if f.is_empty() { tr("Celle du site").to_string() } else { f.to_string() };
                                egui::ComboBox::from_id_salt("site-font").selected_text(label(&p.font)).width(150.0).show_ui(ui, |ui| {
                                    for f in FONTS {
                                        if ui.selectable_label(p.font == f, label(f)).clicked() {
                                            p.font = f.to_string();
                                        }
                                    }
                                });
                            });
                            line(ui, false, tr("Se présenter comme"), "", 170.0, &mut |ui| {
                                egui::ComboBox::from_id_salt("site-identity").selected_text(identity.label()).width(150.0).show_ui(ui, |ui| {
                                    for i in Identity::ALL {
                                        if ui.selectable_label(identity == i, i.label()).clicked() {
                                            identity = i;
                                        }
                                    }
                                });
                            });
                        });
                        card(ui, &mut |ui| {
                            let n = p.hidden.len();
                            let text = if n == 0 { tr("Aucun élément retiré").to_string() } else { tf("{} élément(s) retiré(s)", &[&n]) };
                            line(ui, true, &text, "", if n > 0 { 250.0 } else { 150.0 }, &mut |ui| {
                                if ui.button(tr("Retirer un élément…")).clicked() {
                                    zap = true;
                                }
                                if n > 0 && ui.button(tr("Tout remettre")).clicked() {
                                    p.hidden.clear();
                                }
                            });
                        });
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        // (it undoes every choice made for this site: asked twice)
                        if changed && super::chrome::confirm_button(ui, ("site-reset", &host), tr("Réinitialiser")) {
                            reset = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new(tr("Terminé")).color(Color32::WHITE)).fill(th.accent)).clicked() {
                                close = true;
                            }
                        });
                    });
                });
        });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        if reset {
            p = SitePrefs::default();
            identity = Identity::Auto;
        }
        if p != before {
            self.save_site(&host, p, true);
        }
        if identity != identity_before {
            self.set_identity(&host, if identity == Identity::Auto { None } else { Some(identity) });
        }
        if zap {
            self.site_panel = false;
            self.start_zapper();
            return;
        }
        if close {
            self.site_panel = false;
        }
    }
}
