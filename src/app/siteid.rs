//! The bar shown on sites whose videos or music need a DRM WebKitGTK cannot
//! play (Netflix, Spotify…): it says so plainly and opens the page, in one
//! click, in the service's app or in a browser that has the DRM. And the
//! small chooser of how Sanaga introduces itself to the site shown.

use super::*;
use crate::icons::{self, Icon};
use crate::identity::{self, Identity};
use eframe::egui::{Align, Color32, Layout, RichText, Sense};

impl Sanaga {
    /// The DRM service of the page shown, unless the bar was dismissed.
    pub(super) fn drm_bar_service(&self) -> Option<&'static str> {
        let t = self.tabs.get(self.active)?;
        if !matches!(t.view, View::Web) {
            return None;
        }
        let host = host_of(t.url());
        if self.drm_dismissed.contains(&host) {
            return None;
        }
        identity::drm_service(&host)
    }

    pub(super) fn drm_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let Some(service) = self.drm_bar_service() else { return };
        let url = self.tabs[self.active].url().to_string();
        let ways = identity::handoffs(service);
        let mut open: Option<identity::Handoff> = None;
        let mut dismiss = false;
        let mut always = false;
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            icons::paint(ui.painter(), r, Icon::Info, th.accent);
            ui.add_space(6.0);
            let text = if ways.is_empty() {
                tf("{} protège ses contenus avec un DRM (Widevine) que le moteur de Sanaga ne peut pas lire. Installez Chromium ou Firefox pour les regarder.", &[&service])
            } else {
                tf("{} protège ses contenus avec un DRM (Widevine) que le moteur de Sanaga ne peut pas lire.", &[&service])
            };
            ui.label(RichText::new(text).size(13.0).color(th.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.button(tr("Ignorer")).clicked() {
                    dismiss = true;
                }
                if !ways.is_empty() && ui.button(tr("Toujours ouvrir ainsi")).on_hover_text(tr("Les sites protégés s'ouvriront directement dans ce navigateur (Paramètres › Général)")).clicked() {
                    always = true;
                }
                for (k, w) in ways.iter().take(2).enumerate() {
                    let b = if k == 0 { egui::Button::new(RichText::new(&w.label).color(Color32::WHITE)).fill(th.accent) } else { egui::Button::new(&w.label) };
                    if ui.add(b.corner_radius(CornerRadius::same(8))).clicked() {
                        open = Some(w.clone());
                    }
                }
            });
        });
        if let Some(w) = open {
            identity::open_with(&w, &url);
            self.notice = Some((format!("{}…", w.label), self.ctx.input(|i| i.time)));
        }
        if dismiss {
            self.drm_dismissed.insert(host_of(&url));
        }
        if always {
            self.store.settings.drm_auto = true;
            self.store.save_settings();
        }
    }

    /// "Introduce Sanaga to this site as…": Automatic, Safari, Chrome, Firefox.
    pub(super) fn identity_chooser(&mut self, ctx: &egui::Context, th: &Theme) {
        if !self.identity_ask {
            return;
        }
        let url = self.tabs[self.active].url().to_string();
        let host = host_of(&url);
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            self.identity_ask = false;
            return;
        }
        let current = identity::for_host(&host);
        let mut pick: Option<Identity> = None;
        let mut close = false;
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("identity-ask")).order(egui::Order::Foreground).fixed_pos(pos2(screen.center().x - 200.0, screen.top() + 120.0)).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .corner_radius(CornerRadius::same(16))
                .stroke(Stroke::new(1.0, th.hairline.gamma_multiply(0.6)))
                .shadow(egui::Shadow { offset: [0, 10], blur: 34, spread: 0, color: Color32::from_black_alpha(60) })
                .inner_margin(egui::Margin::same(20))
                .show(ui, |ui| {
                    ui.set_width(400.0);
                    ui.label(RichText::new(tf("Se présenter à {} comme…", &[&host])).size(16.0).family(bold()).color(th.text));
                    ui.add_space(4.0);
                    ui.label(RichText::new(tr("Certains sites refusent WebKit alors qu'ils fonctionnent avec lui : choisir Chrome les fait s'ouvrir. La page se recharge.")).size(12.0).color(th.sub));
                    ui.add_space(12.0);
                    for i in Identity::ALL {
                        let (r, resp) = ui.allocate_exact_size(vec2(400.0, 36.0), Sense::click());
                        let chosen = i == current;
                        ui.painter().rect_filled(r, CornerRadius::same(9), if chosen { th.accent.gamma_multiply(0.14) } else if resp.hovered() { th.card } else { Color32::TRANSPARENT });
                        ui.painter().text(pos2(r.left() + 14.0, r.center().y), egui::Align2::LEFT_CENTER, i.label(), FontId::proportional(14.0), th.text);
                        if chosen {
                            ui.painter().text(pos2(r.right() - 14.0, r.center().y), egui::Align2::RIGHT_CENTER, "✓", FontId::proportional(14.0), th.accent);
                        }
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            pick = Some(i);
                        }
                    }
                    ui.add_space(8.0);
                    if ui.button(tr("Annuler")).clicked() {
                        close = true;
                    }
                });
        });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        if let Some(i) = pick {
            self.identity_ask = false;
            self.set_identity(&host, if i == Identity::Auto { None } else { Some(i) });
            let a = self.active;
            self.reload(a);
        } else if close {
            self.identity_ask = false;
        }
    }

    /// Remember (or forget) how to introduce Sanaga to a site.
    pub(super) fn set_identity(&mut self, host: &str, i: Option<Identity>) {
        let host = host.trim_start_matches("www.").to_string();
        self.store.identities.retain(|(h, _)| *h != host);
        if let Some(i) = i {
            self.store.identities.push((host, i));
        }
        self.store.save_identities();
    }
}
