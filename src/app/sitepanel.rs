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
        let mut close = false;
        let mut reset = false;
        let mut zap = false;
        let screen = ctx.content_rect();
        let dark = self.store.settings.dark;
        egui::Area::new(egui::Id::new("site-panel")).order(egui::Order::Foreground).fixed_pos(pos2(screen.center().x - 200.0, screen.top() + 70.0)).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.border))
                .corner_radius(CornerRadius::same(14))
                .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(if dark { 120 } else { 60 }) })
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(368.0);
                    ui.horizontal(|ui| {
                        super::chrome::site_badge(ui.painter(), ui.cursor().min + vec2(10.0, 11.0), 9.0, &url, &host);
                        ui.add_space(26.0);
                        ui.label(RichText::new(&host).size(15.0).family(bold()).color(th.text));
                    });
                    ui.label(RichText::new(tr("Réglages de ce site")).size(12.0).color(th.sub));
                    ui.add_space(10.0);
                    let row = |ui: &mut Ui, label: &str, sub: &str, on: &mut bool| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(290.0);
                                ui.label(RichText::new(label).size(13.5).color(th.text));
                                if !sub.is_empty() {
                                    ui.label(RichText::new(sub).size(11.0).color(th.sub));
                                }
                            });
                            super::settings::switch(ui, th, on);
                        });
                        ui.add_space(4.0);
                    };
                    row(ui, tr("JavaScript"), tr("Désactivé : pages plus légères, mais certains sites ne marchent plus"), &mut p.js);
                    row(ui, tr("Bloqueur de publicités et de traceurs"), "", &mut p.blocker);
                    row(ui, tr("Polices du site"), tr("Désactivé : les polices de votre système, plus légères"), &mut p.fonts);
                    ui.separator();
                    row(ui, tr("Mode sombre"), "", &mut p.dark);
                    row(ui, tr("Sans bandeaux collants"), tr("Les en-têtes et barres fixés à l'écran défilent avec la page"), &mut p.no_sticky);
                    row(ui, tr("Copier-coller débloqué"), tr("Sélection, copie et clic droit même si le site les interdit"), &mut p.unlock);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("Police des pages")).size(13.5).color(th.text));
                        ui.add_space(8.0);
                        let label = |f: &str| if f.is_empty() { tr("Celle du site").to_string() } else { f.to_string() };
                        egui::ComboBox::from_id_salt("site-font").selected_text(label(&p.font)).width(170.0).show_ui(ui, |ui| {
                            for f in FONTS {
                                if ui.selectable_label(p.font == f, label(f)).clicked() {
                                    p.font = f.to_string();
                                }
                            }
                        });
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("Se présenter comme")).size(13.5).color(th.text));
                        ui.add_space(8.0);
                        egui::ComboBox::from_id_salt("site-identity").selected_text(identity.label()).width(150.0).show_ui(ui, |ui| {
                            for i in Identity::ALL {
                                if ui.selectable_label(identity == i, i.label()).clicked() {
                                    identity = i;
                                }
                            }
                        });
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let n = p.hidden.len();
                        let text = if n == 0 { tr("Aucun élément retiré").to_string() } else { tf("{} élément(s) retiré(s)", &[&n]) };
                        ui.label(RichText::new(text).size(12.5).color(th.sub));
                        if n > 0 && ui.small_button(tr("Tout remettre")).clicked() {
                            p.hidden.clear();
                        }
                        if ui.small_button(tr("Retirer un élément…")).clicked() {
                            zap = true;
                        }
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr("Réinitialiser")).clicked() {
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
