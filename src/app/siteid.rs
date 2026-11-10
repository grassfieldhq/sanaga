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

impl Sanaga {
    /// A protected site (Netflix, Disney+, Spotify…) asked in tab `i` (an
    /// address typed, a favourite, a link): nothing is loaded in Sanaga,
    /// where it can't play. When the person chose so, it opens at once in
    /// the app or browser that plays it and the tab stays where it was;
    /// otherwise Sanaga's page offers that in one click.
    pub(super) fn protected_site(&mut self, i: usize, url: &str) {
        let Some(service) = identity::protected_url(url) else { return };
        let now = self.ctx.input(|t| t.time);
        if self.store.settings.drm_auto {
            if let Some(w) = identity::handoffs(service).into_iter().next() {
                identity::open_with(&w, url);
                self.notice = Some((tf("{} s'ouvre dans {}…", &[&service, &w.name]), now));
                // a tab opened only for it (no page in it yet) goes away
                if i < self.tabs.len() && self.tabs[i].entries.is_empty() {
                    if self.tabs.len() > 1 {
                        self.close_tab(i);
                    } else {
                        self.navigate(i, "sanaga://newtab".into(), None, true);
                    }
                } else if i < self.tabs.len() && self.tabs[i].url() == url {
                    // already shown here (a call site found unable to work
                    // once loaded): the tab goes back where it was
                    if self.tabs[i].idx > 0 || self.tabs[i].web.as_ref().is_some_and(|w| w.can_go_back().unwrap_or(false)) {
                        self.go_history(i, -1);
                    } else if self.tabs.len() > 1 {
                        self.close_tab(i);
                    } else {
                        self.go_home(i);
                    }
                }
                return;
            }
        }
        let enc: String = url::form_urlencoded::byte_serialize(url.as_bytes()).collect();
        self.navigate(i, format!("sanaga://protected?u={enc}"), None, true);
        if let Some(t) = self.tabs.get_mut(i) {
            t.address = url.to_string();
        }
    }

    /// The address the protected-site page is about.
    fn protected_target(&self) -> String {
        let u = self.tabs.get(self.active).map(|t| t.url().to_string()).unwrap_or_default();
        url::Url::parse(&u).ok().and_then(|p| p.query_pairs().find(|(k, _)| k == "u").map(|(_, v)| v.to_string())).unwrap_or_default()
    }

    /// `sanaga://protected`: "Netflix opens in Google Chrome" — the button,
    /// the other ways, "always this way", or staying in Sanaga anyway.
    pub(super) fn page_protected(&mut self, ui: &mut Ui, th: &Theme) {
        let target = self.protected_target();
        let host = host_of(&target);
        // a protected video site, or a video call this engine can't make
        let call = identity::drm_service(&host).is_none() && identity::call_service(&host).is_some();
        let service = identity::drm_service(&host).or_else(|| identity::call_service(&host)).unwrap_or("");
        let ways = identity::handoffs(service);
        let mut open: Option<identity::Handoff> = None;
        let mut stay = false;
        let mut copy: Option<String> = None;
        super::pages::centered_column(ui, |ui| {
            ui.add_space(64.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(540.0);
                let (r, _) = ui.allocate_exact_size(vec2(84.0, 84.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(24), th.card);
                icons::paint(ui.painter(), r.shrink(20.0), Icon::Lock, th.accent);
                ui.add_space(22.0);
                let title = match ways.first() {
                    Some(w) if w.app => tf("{} s'ouvre dans son application", &[&service]),
                    Some(w) => tf("{} s'ouvre dans {}", &[&service, &w.name]),
                    None => tf("{} ne peut pas être lu dans Sanaga", &[&service]),
                };
                ui.label(RichText::new(title).size(26.0).family(bold()).color(th.text));
                ui.add_space(10.0);
                let why = if call {
                    tf("{} a besoin des appels vidéo (WebRTC), que le moteur web installé sur ce système ne fournit pas. Sanaga ouvre donc la réunion dans un navigateur qui les gère, avec vos comptes habituels.", &[&service])
                } else {
                    tf("{} protège ses vidéos avec Widevine, un verrou que Google réserve à Chrome, Firefox et aux navigateurs de leur famille. Le moteur de Sanaga n'y a pas droit : Sanaga vous ouvre donc la page là où elle se lit, avec vos comptes habituels.", &[&service])
                };
                ui.label(RichText::new(why).size(14.0).color(th.sub));
                ui.add_space(26.0);
                if let Some(first) = ways.first() {
                    let b = egui::Button::new(RichText::new(&first.label).size(15.0).color(Color32::WHITE)).fill(th.accent).corner_radius(CornerRadius::same(20)).min_size(vec2(260.0, 42.0));
                    if ui.add(b).clicked() {
                        open = Some(first.clone());
                    }
                    if ways.len() > 1 {
                        ui.add_space(12.0);
                        // the other ways, side by side, centred
                        let labels: Vec<&identity::Handoff> = ways.iter().skip(1).take(3).collect();
                        let widths: f32 = labels.iter().map(|w| ui.painter().layout_no_wrap(w.label.clone(), FontId::proportional(13.0), th.text).size().x + 34.0).sum::<f32>() + 8.0 * (labels.len() as f32 - 1.0);
                        ui.horizontal(|ui| {
                            ui.add_space(((ui.available_width() - widths) / 2.0).max(0.0));
                            ui.spacing_mut().item_spacing.x = 8.0;
                            for w in labels {
                                if ui.add(egui::Button::new(RichText::new(&w.label).size(13.0)).corner_radius(CornerRadius::same(16)).min_size(vec2(0.0, 32.0))).clicked() {
                                    open = Some(w.clone());
                                }
                            }
                        });
                    }
                    ui.add_space(20.0);
                    ui.checkbox(&mut self.drm_always_pick, RichText::new(if call { tr("Toujours ouvrir ainsi les appels vidéo et les sites protégés, sans passer par cette page") } else { tr("Toujours ouvrir Netflix, Disney+, Spotify… ainsi, sans passer par cette page") }).size(13.0).color(th.text));
                } else {
                    ui.label(RichText::new(tr("Installez Firefox ou Google Chrome : Sanaga y ouvrira ces sites d'un clic, directement.")).size(14.0).color(th.text));
                    if let Some(cmd) = identity::browser_install_hint() {
                        ui.add_space(12.0);
                        Frame::new().fill(th.code_bg).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
                            ui.label(RichText::new(&cmd).monospace().size(13.0).color(th.text));
                        });
                        ui.add_space(8.0);
                        if ui.button(tr("Copier la commande")).clicked() {
                            copy = Some(cmd);
                        }
                    }
                }
                ui.add_space(30.0);
                let b = egui::Button::new(RichText::new(if call { tr("Ouvrir quand même dans Sanaga (l'appel ne pourra pas se connecter)") } else { tr("Ouvrir quand même dans Sanaga (les vidéos ne se liront pas)") }).size(12.5).color(th.sub)).frame(false);
                if ui.add(b).clicked() {
                    stay = true;
                }
            });
        });
        let a = self.active;
        let now = self.ctx.input(|t| t.time);
        if let Some(w) = open {
            identity::open_with(&w, &target);
            if self.drm_always_pick && !self.store.settings.drm_auto {
                self.store.settings.drm_auto = true;
                self.store.save_settings();
            }
            self.notice = Some((tf("{} s'ouvre dans {}…", &[&service, &w.name]), now));
            // this tab goes back to where it was
            if self.tabs[a].idx > 0 {
                self.go_history(a, -1);
            } else if self.tabs.len() > 1 {
                self.close_tab(a);
            } else {
                self.go_home(a);
            }
        }
        if stay {
            if let Some(s) = identity::drm_service(&host) {
                identity::stay_here(s);
            }
            self.navigate(a, target, None, true);
        }
        if let Some(c) = copy {
            self.ctx.copy_text(c);
            self.notice = Some((tr("Commande copiée").into(), now));
        }
    }
}
