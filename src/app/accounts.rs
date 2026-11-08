//! Passwords (saved in the system keyring, filled on request), contact AutoFill,
//! extensions and profiles — the parts of Safari that look after the person's
//! own data.

use super::*;
use crate::icons::{self, Icon};
use crate::keyring;
use eframe::egui::{Align, Color32, Layout, Margin, RichText, Sense, TextEdit};

/// Runs in every page: notices logins and fills fields when asked.
pub(super) const PASSWORD_JS: &str = include_str!("../../assets/passwords.js");

pub(super) enum PwPrompt {
    Save { tab: u64, site: String, user: String, pass: String },
    Fill { tab: u64, site: String, users: Vec<String> },
}

fn decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s).decode_utf8_lossy().to_string()
}

/// A page's origin as its `location.origin` says it (`https://site:port`, `file://`).
pub(super) fn origin_of(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) if u.scheme() == "file" => "file://".into(),
        Ok(u) => u.origin().ascii_serialization(),
        Err(_) => String::new(),
    }
}

impl Sanaga {
    /// Messages from the page script: a login was sent, a login form appeared,
    /// or an account was picked in the list under the field.
    pub(super) fn on_password_msg(&mut self, tab: u64, msg: &str) {
        if !self.store.settings.passwords {
            return;
        }
        let Some(t) = self.tabs.iter().find(|t| t.id == tab) else { return };
        let private = t.private;
        let parts: Vec<String> = msg.split('\n').map(decode).collect();
        // The page names its site in the message, and any page script can
        // send one: it must be the site the tab really shows, so a page only
        // ever reaches the passwords of its own site.
        let real = t.web.as_ref().and_then(|w| w.uri()).map(|u| origin_of(&u)).unwrap_or_else(|| origin_of(t.url()));
        if parts.get(1).is_none_or(|s| *s != real) {
            return;
        }
        match parts.first().map(|s| s.as_str()) {
            Some("pw:save") if parts.len() >= 4 && !private => {
                let (site, user, pass) = (parts[1].clone(), parts[2].clone(), parts[3].clone());
                let profile = self.store.profile.clone();
                let known = self.store.logins.iter().any(|(s, u)| *s == site && *u == user);
                if known && keyring::lookup(&profile, &site, &user).as_deref() == Some(pass.as_str()) {
                    return;
                }
                self.pw_prompt = Some(PwPrompt::Save { tab, site, user, pass });
            }
            Some("pw:get") if parts.len() >= 2 => {
                let site = parts[1].clone();
                let users: Vec<String> = self.store.logins.iter().filter(|(s, _)| *s == site).map(|(_, u)| u.clone()).collect();
                if users.is_empty() {
                    return;
                }
                // the names go to the page, for its list under the field
                let list = format!("[{}]", users.iter().map(|u| keyring::js_str(u)).collect::<Vec<_>>().join(","));
                if let Some(w) = self.tabs.iter().find(|t| t.id == tab).and_then(|t| t.web.as_ref()) {
                    let _ = w.evaluate_script(&format!("window.__sanagaLogins && window.__sanagaLogins({list});"));
                }
                if users.len() == 1 {
                    // one account for this site: fill it at once (empty fields only)
                    self.fill_login(tab, &site, &users[0], true);
                } else if !matches!(self.pw_prompt, Some(PwPrompt::Save { .. })) {
                    self.pw_prompt = Some(PwPrompt::Fill { tab, site, users });
                }
            }
            Some("pw:fill") if parts.len() >= 3 => {
                let (site, user) = (parts[1].clone(), parts[2].clone());
                if self.store.logins.iter().any(|(s, u)| *s == site && *u == user) {
                    self.fill_login(tab, &site, &user, false);
                    if matches!(self.pw_prompt, Some(PwPrompt::Fill { .. })) {
                        self.pw_prompt = None;
                    }
                }
            }
            _ => {}
        }
    }

    /// Put a saved login in the page's fields (`only_empty`: keep what was typed).
    fn fill_login(&mut self, tab: u64, site: &str, user: &str, only_empty: bool) {
        let Some(pass) = keyring::lookup(&self.store.profile, site, user) else {
            self.notice = Some((tr("Mot de passe introuvable dans le trousseau.").into(), self.ctx.input(|i| i.time)));
            return;
        };
        if let Some(w) = self.tabs.iter().find(|t| t.id == tab).and_then(|t| t.web.as_ref()) {
            let _ = w.evaluate_script(&format!("window.__sanagaFill && window.__sanagaFill({}, {}, {only_empty});", keyring::js_str(user), keyring::js_str(&pass)));
        }
    }

    /// AutoFill the form of the page with the person's contact details.
    pub(super) fn fill_contacts(&mut self) {
        let s = &self.store.settings;
        if s.fill_name.is_empty() && s.fill_email.is_empty() && s.fill_phone.is_empty() && s.fill_address.is_empty() {
            self.notice = Some((tr("Ajoutez d'abord vos coordonnées dans Paramètres › Remplissage automatique.").into(), self.ctx.input(|i| i.time)));
            return;
        }
        let js = format!(
            "window.__sanagaContact && window.__sanagaContact({{name:{},email:{},phone:{},address:{}}});",
            keyring::js_str(&s.fill_name),
            keyring::js_str(&s.fill_email),
            keyring::js_str(&s.fill_phone),
            keyring::js_str(&s.fill_address)
        );
        if let Some(w) = self.tabs.get(self.active).and_then(|t| t.web.as_ref()) {
            let _ = w.evaluate_script(&js);
        }
    }

    /// The bar under the toolbar asking to save or fill a password.
    pub(super) fn password_bar_visible(&self) -> bool {
        let id = self.tabs.get(self.active).map(|t| t.id);
        match &self.pw_prompt {
            Some(PwPrompt::Save { tab, .. }) | Some(PwPrompt::Fill { tab, .. }) => Some(*tab) == id,
            None => false,
        }
    }

    pub(super) fn password_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let Some(prompt) = self.pw_prompt.take() else { return };
        let mut keep = true;
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            icons::paint(ui.painter(), r, Icon::Lock, th.accent);
            ui.add_space(6.0);
            match &prompt {
                PwPrompt::Save { tab: _, site, user, pass } => {
                    let who = if user.is_empty() { String::new() } else { tf(" de « {} »", &[&user]) };
                    ui.label(RichText::new(tf("Enregistrer le mot de passe{} pour {} dans votre trousseau ?", &[&who, &host_of(site)])).size(13.0).color(th.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        if ui.button(tr("Pas maintenant")).clicked() {
                            keep = false;
                        }
                        let save = egui::Button::new(RichText::new(tr("Enregistrer")).color(Color32::WHITE)).fill(th.accent);
                        if ui.add(save).clicked() {
                            keep = false;
                            if keyring::store(&self.store.profile, site, user, pass) {
                                if !self.store.logins.iter().any(|(s, u)| s == site && u == user) {
                                    self.store.logins.push((site.clone(), user.clone()));
                                    self.store.save_logins();
                                }
                                self.notice = Some((tr("Mot de passe enregistré dans le trousseau.").into(), ui.input(|i| i.time)));
                            } else {
                                self.notice = Some((tr("Le trousseau du système n'est pas disponible.").into(), ui.input(|i| i.time)));
                            }
                        }
                    });
                }
                PwPrompt::Fill { tab, site, users } => {
                    ui.label(RichText::new(tf("Mot de passe enregistré pour {}", &[&host_of(site)])).size(13.0).color(th.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        if ui.button(tr("Ignorer")).clicked() {
                            keep = false;
                        }
                        for u in users.iter().take(3) {
                            let label = if u.is_empty() { tr("Remplir").to_string() } else { tf("Remplir : {}", &[&u]) };
                            let b = egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(th.accent);
                            if ui.add(b).clicked() {
                                keep = false;
                                let (tab, site, u) = (*tab, site.clone(), u.clone());
                                self.fill_login(tab, &site, &u, false);
                            }
                        }
                    });
                }
            }
        });
        if keep && self.pw_prompt.is_none() {
            self.pw_prompt = Some(prompt);
        }
    }

    /// Exchange favourites, reading list and open tabs with the sync folder.
    pub(super) fn sync_now(&mut self) {
        let tabs: Vec<(String, String)> = self
            .tabs
            .iter()
            .filter(|t| !t.private && !t.entries.is_empty() && t.url().starts_with("http"))
            .map(|t| (t.url().to_string(), t.title()))
            .collect();
        let mut status = std::mem::take(&mut self.sync_status);
        if crate::sync::sync(&self.store, &tabs, &mut status) {
            self.store.reload_lists();
        }
        self.sync_status = status;
    }

    pub(super) fn page_passwords(&mut self, ui: &mut Ui, th: &Theme) {
        let mut remove: Option<usize> = None;
        let mut reveal: Option<usize> = None;
        let mut copy: Option<usize> = None;
        let filter = self.pw_filter.to_lowercase();
        super::pages::centered_column(ui, |ui| {
            super::pages::page_title(ui, th, tr("Mots de passe"));
            ui.label(RichText::new(tr("Les mots de passe sont gardés dans le trousseau de votre système, pas dans les fichiers de Sanaga. Sanaga propose de les enregistrer quand vous vous connectez à un site, et de les remplir la fois suivante.")).size(13.0).color(th.sub));
            if !keyring::available() {
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Le trousseau du système (Secret Service) est introuvable : les mots de passe ne peuvent pas être enregistrés.")).size(13.0).color(Color32::from_rgb(0xD0, 0x40, 0x30)));
            }
            ui.add_space(12.0);
            ui.add(TextEdit::singleline(&mut self.pw_filter).hint_text(tr("Rechercher un site")).desired_width(320.0));
            ui.add_space(10.0);
            if self.store.logins.is_empty() {
                ui.label(RichText::new(tr("Aucun mot de passe enregistré pour l'instant.")).color(th.sub));
            }
            for (i, (site, user)) in self.store.logins.iter().enumerate() {
                if !filter.is_empty() && !site.to_lowercase().contains(&filter) && !user.to_lowercase().contains(&filter) {
                    continue;
                }
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        super::chrome::site_badge(ui.painter(), pos2(ui.cursor().left() + 10.0, ui.cursor().center().y + 6.0), 9.0, site, &host_of(site));
                        ui.add_space(26.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(host_of(site)).size(14.0).family(bold()).color(th.text));
                            let shown = if self.pw_shown.as_ref().map(|(k, _)| *k == i).unwrap_or(false) {
                                self.pw_shown.as_ref().map(|(_, p)| p.clone()).unwrap_or_default()
                            } else {
                                "••••••••".into()
                            };
                            ui.label(RichText::new(format!("{}   {}", if user.is_empty() { "(sans identifiant)" } else { user }, shown)).size(12.5).color(th.sub));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button(tr("Supprimer")).clicked() {
                                remove = Some(i);
                            }
                            if ui.button(tr("Copier")).clicked() {
                                copy = Some(i);
                            }
                            if ui.button(tr("Afficher")).clicked() {
                                reveal = Some(i);
                            }
                        });
                    });
                });
                ui.add_space(6.0);
            }
        });
        let profile = self.store.profile.clone();
        if let Some(i) = reveal {
            let (s, u) = self.store.logins[i].clone();
            self.pw_shown = keyring::lookup(&profile, &s, &u).map(|p| (i, p));
        }
        if let Some(i) = copy {
            let (s, u) = self.store.logins[i].clone();
            if let Some(p) = keyring::lookup(&profile, &s, &u) {
                self.ctx.copy_text(p);
                self.notice = Some((tr("Mot de passe copié.").into(), self.ctx.input(|i| i.time)));
            }
        }
        if let Some(i) = remove {
            let (s, u) = self.store.logins.remove(i);
            keyring::delete(&profile, &s, &u);
            self.store.save_logins();
            self.pw_shown = None;
        }
    }

    pub(super) fn page_extensions(&mut self, ui: &mut Ui, th: &Theme) {
        let list = crate::extensions::all();
        let mut toggle: Option<String> = None;
        let mut toggle_mobile = false;
        let mut open_folder = false;
        let on = self.store.extensions_on.clone();
        super::pages::centered_column(ui, |ui| {
            super::pages::page_title(ui, th, tr("Extensions"));
            ui.label(RichText::new(tr("Les extensions ajoutent des fonctions aux pages web. Les changements s'appliquent aux nouveaux onglets.")).size(13.0).color(th.sub));
            ui.add_space(12.0);
            // built-in, native: the mobile simulator
            Frame::new().fill(th.card).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                    crate::icons::paint(ui.painter(), r, crate::icons::Icon::Phone, th.accent);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(tr("Simulateur mobile")).size(14.0).family(bold()).color(th.text));
                        ui.label(RichText::new(tr("Voir et tester un site comme sur un téléphone ou une tablette : plus de 35 appareils, cadre, rotation, zoom, version mobile des sites, écran tactile et captures. Bouton téléphone de la barre d'outils ou Ctrl+Maj+M.")).size(12.5).color(th.sub));
                        ui.label(RichText::new(tr("Intégrée à Sanaga")).size(11.5).color(th.sub));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut enabled = self.tb("mobile");
                        let label = if enabled { tr("Activée") } else { tr("Désactivée") };
                        if ui.checkbox(&mut enabled, label).changed() {
                            toggle_mobile = true;
                        }
                    });
                });
            });
            ui.add_space(6.0);
            for e in &list {
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&e.name).size(14.0).family(bold()).color(th.text));
                            ui.label(RichText::new(&e.description).size(12.5).color(th.sub));
                            if !e.builtin {
                                ui.label(RichText::new(if e.matches.is_empty() { tr("Tous les sites").to_string() } else { e.matches.join(", ") }).size(11.5).color(th.sub));
                            }
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let mut enabled = on.contains(&e.id);
                            let label = if enabled { tr("Activée") } else { tr("Désactivée") };
                            if ui.checkbox(&mut enabled, label).changed() {
                                toggle = Some(e.id.clone());
                            }
                        });
                    });
                });
                ui.add_space(6.0);
            }
            ui.add_space(10.0);
            ui.label(RichText::new(tr("Vos propres extensions : déposez des fichiers .js ou .css dans le dossier des extensions.")).size(12.5).color(th.sub));
            if ui.button(tr("Ouvrir le dossier des extensions")).clicked() {
                open_folder = true;
            }
        });
        if toggle_mobile {
            let on = self.tb("mobile");
            let mut keys: Vec<String> = self.store.settings.toolbar.split(',').filter(|k| !k.is_empty() && *k != "mobile" && *k != "-mobile").map(|k| k.to_string()).collect();
            keys.push(if on { "-mobile".into() } else { "mobile".into() });
            self.store.settings.toolbar = keys.join(",");
            self.store.save_settings();
            if on {
                self.device = None;
            }
        }
        if let Some(id) = toggle {
            if let Some(p) = self.store.extensions_on.iter().position(|x| *x == id) {
                self.store.extensions_on.remove(p);
            } else {
                self.store.extensions_on.push(id);
            }
            self.store.save_extensions();
        }
        if open_folder {
            let dir = crate::extensions::prepare_folder();
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
    }

    pub(super) fn page_profiles(&mut self, ui: &mut Ui, th: &Theme) {
        let current = self.store.profile.clone();
        let profiles = crate::store::Store::profiles();
        let mut open: Option<String> = None;
        let mut create = false;
        super::pages::centered_column(ui, |ui| {
            super::pages::page_title(ui, th, tr("Profils"));
            ui.label(RichText::new(tr("Chaque profil a ses propres favoris, historique, mots de passe, cookies et onglets — par exemple « Personnel » et « Travail ». Un profil s'ouvre dans sa propre fenêtre.")).size(13.0).color(th.sub));
            ui.add_space(12.0);
            let all: Vec<String> = std::iter::once(String::new()).chain(profiles.iter().cloned()).collect();
            for p in &all {
                let name = if p.is_empty() { tr("Personnel").to_string() } else { p.clone() };
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 13.0, crate::theme::badge_color(&name));
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, name.chars().next().unwrap_or('P').to_uppercase().to_string(), FontId::new(13.0, bold()), Color32::WHITE);
                        ui.label(RichText::new(&name).size(14.0).family(bold()).color(th.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if *p == current {
                                ui.label(RichText::new(tr("Profil de cette fenêtre")).size(12.5).color(th.accent));
                            } else if ui.button(tr("Ouvrir dans une nouvelle fenêtre")).clicked() {
                                open = Some(p.clone());
                            }
                        });
                    });
                });
                ui.add_space(6.0);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let resp = ui.add(TextEdit::singleline(&mut self.new_profile_name).hint_text(tr("Nom du nouveau profil (ex. Travail)")).desired_width(280.0));
                if (resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) | ui.button(tr("Créer le profil")).clicked() {
                    create = true;
                }
            });
        });
        if create {
            let name: String = self.new_profile_name.trim().chars().filter(|c| !matches!(c, '/' | '\\' | '\t' | '\n')).collect();
            if !name.is_empty() && name != "Personnel" {
                let _ = std::fs::create_dir_all(crate::store::Store::root_dir().join("profils").join(&name));
                self.new_profile_name.clear();
                open = Some(name);
            }
        }
        if let Some(p) = open {
            if let Ok(exe) = std::env::current_exe() {
                let mut cmd = std::process::Command::new(exe);
                if !p.is_empty() {
                    cmd.arg("--profil").arg(&p);
                }
                let _ = cmd.spawn();
            }
        }
    }
}
