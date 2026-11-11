//! Passwords (saved in the system keyring, filled on request), contact AutoFill,
//! extensions and profiles — the parts of Safari that look after the person's
//! own data.

use super::*;
use crate::icons::{self, Icon};
use crate::keyring;
use eframe::egui::{Align, Color32, Layout, Margin, RichText, Sense, TextEdit};

/// Runs in every page (in Sanaga's world, top frame): notices logins and
/// sign-in fields, and fills them when the person picks an account in
/// Sanaga's bar.
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
    /// Messages from the page script: a login was sent, or sign-in fields
    /// appeared (the saved accounts are then offered in Sanaga's bar).
    pub(super) fn on_password_msg(&mut self, tab: u64, msg: &str) {
        // (a window without a profile folder, such as a guest window, keeps
        // no password and never reaches another profile's keyring entries)
        if !self.store.settings.passwords || self.store.data_dir().is_none() {
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
                if !known {
                    self.pw_prompt = Some(PwPrompt::Save { tab, site, user, pass });
                    return;
                }
                // already saved: offered again only if the password changed
                // (the keyring is asked on a thread: it may wait to be unlocked)
                let (s, u) = (site.clone(), user.clone());
                self.off_thread(
                    move || keyring::lookup(&profile, &s, &u),
                    move |app, saved| {
                        if saved.as_deref() != Some(pass.as_str()) && app.tabs.iter().any(|t| t.id == tab) {
                            app.pw_prompt = Some(PwPrompt::Save { tab, site, user, pass });
                        }
                    },
                );
            }
            Some("pw:get") if parts.len() >= 2 => {
                // sign-in fields appeared: Sanaga offers its saved accounts in
                // its own bar; nothing reaches the page before the person
                // picks one there
                let site = parts[1].clone();
                let users: Vec<String> = self.store.logins.iter().filter(|(s, _)| *s == site).map(|(_, u)| u.clone()).collect();
                if users.is_empty() || self.pw_fill_dismissed.as_ref().is_some_and(|(t, s)| *t == tab && *s == site) {
                    return;
                }
                if !matches!(self.pw_prompt, Some(PwPrompt::Save { .. })) {
                    self.pw_prompt = Some(PwPrompt::Fill { tab, site, users });
                }
            }
            _ => {}
        }
    }

    /// Put a saved login in the page's fields: only after the person picked
    /// it in Sanaga's bar, only if the tab still shows that site, and the
    /// page script checks the origin again when it fills (top frame only).
    fn fill_login(&mut self, tab: u64, site: &str, user: &str) {
        let shows_site = |app: &Sanaga| app.tabs.iter().find(|t| t.id == tab).and_then(|t| t.web.as_ref()).is_some_and(|w| w.uri().map(|u| origin_of(&u)).as_deref() == Some(site));
        if !shows_site(self) {
            return;
        }
        // (the keyring is asked on a thread: it may wait to be unlocked)
        let (profile, site, user) = (self.store.profile.clone(), site.to_string(), user.to_string());
        let (s, u) = (site.clone(), user.clone());
        self.off_thread(
            move || keyring::lookup(&profile, &s, &u),
            move |app, pass| {
                let Some(pass) = pass else {
                    app.say(tr("Mot de passe introuvable dans le trousseau."));
                    return;
                };
                // the tab may have moved on while the keyring answered: the
                // password only ever goes to the site it was saved for
                let Some(w) = app.tabs.iter().find(|t| t.id == tab).and_then(|t| t.web.as_ref()) else { return };
                if w.uri().map(|u| origin_of(&u)).as_deref() != Some(site.as_str()) {
                    return;
                }
                let _ = w.evaluate_own(&format!("window.__sanagaFill && window.__sanagaFill({}, {}, {});", keyring::js_str(&user), keyring::js_str(&pass), keyring::js_str(&site)));
            },
        );
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
            let _ = w.evaluate_own(&js);
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
                            let (profile, site, user, pass) = (self.store.profile.clone(), site.clone(), user.clone(), pass.clone());
                            let (s, u) = (site.clone(), user.clone());
                            self.off_thread(
                                move || keyring::store(&profile, &s, &u, &pass),
                                move |app, ok| {
                                    if ok {
                                        if !app.store.logins.iter().any(|(s, u)| *s == site && *u == user) {
                                            app.store.logins.push((site, user));
                                            app.store.save_logins();
                                        }
                                        app.say(tr("Mot de passe enregistré dans le trousseau."));
                                    } else {
                                        app.say(tr("Le trousseau du système n'est pas disponible."));
                                    }
                                },
                            );
                        }
                    });
                }
                PwPrompt::Fill { tab, site, users } => {
                    ui.label(RichText::new(tf("Mot de passe enregistré pour {}", &[&host_of(site)])).size(13.0).color(th.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        if ui.button(tr("Ignorer")).clicked() {
                            keep = false;
                            // not offered again on this site in this tab
                            self.pw_fill_dismissed = Some((*tab, site.clone()));
                        }
                        for u in users.iter().take(3) {
                            let label = if u.is_empty() { tr("Remplir").to_string() } else { tf("Remplir : {}", &[&u]) };
                            let b = egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(th.accent);
                            if ui.add(b).clicked() {
                                keep = false;
                                let (tab, site, u) = (*tab, site.clone(), u.clone());
                                self.fill_login(tab, &site, &u);
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
        // one round at a time, on its own thread (the folder may be slow)
        let Some(local_dir) = self.store.data_dir().cloned() else { return };
        if self.sync_running {
            return;
        }
        self.sync_running = true;
        let job = crate::sync::SyncJob { folder: self.store.settings.sync_dir.clone(), local_dir, profile: self.store.profile.clone(), open_tabs: tabs };
        let mut status = self.sync_status.clone();
        // the lists as they are now: what the person changes during the
        // round is joined with the round's result, not lost (see `rejoin`)
        let pairs = |list: &[crate::store::Bookmark]| -> Vec<crate::sync::Item> { list.iter().map(|b| (b.url.clone(), b.title.clone())).collect() };
        let (bookmarks_before, reading_before) = (pairs(&self.store.bookmarks), pairs(&self.store.reading));
        self.off_thread(
            move || {
                let synced = crate::sync::sync(&job, &mut status);
                (synced, status)
            },
            move |app, (synced, status)| {
                app.sync_running = false;
                app.sync_status = status;
                let settle = |before: &[crate::sync::Item], now: &[crate::store::Bookmark], synced: Vec<crate::sync::Item>| -> Vec<crate::store::Bookmark> {
                    let now = pairs(now);
                    let list = if now == before { synced } else { crate::sync::rejoin(before, &now, &synced) };
                    list.into_iter().map(|(url, title)| crate::store::Bookmark { url, title }).collect()
                };
                if let Some(list) = synced.bookmarks {
                    app.store.bookmarks = settle(&bookmarks_before, &app.store.bookmarks, list);
                    app.store.save_bookmarks();
                }
                if let Some(list) = synced.reading {
                    app.store.reading = settle(&reading_before, &app.store.reading, list);
                    app.store.save_reading();
                }
            },
        );
    }

    /// Copy a saved password (read from the keyring on a thread).
    pub(super) fn copy_password(&mut self, site: String, user: String) {
        let profile = self.store.profile.clone();
        self.off_thread(
            move || keyring::lookup(&profile, &site, &user),
            |app, pass| {
                if let Some(p) = pass {
                    app.ctx.copy_text(p);
                    app.say(tr("Mot de passe copié."));
                }
            },
        );
    }

    pub(super) fn page_passwords(&mut self, ui: &mut Ui, th: &Theme) {
        // read the keyring again each time the page is opened
        let seen = ui.ctx().data_mut(|d| std::mem::replace(d.get_temp_mut_or_default::<u64>(egui::Id::new("pw-page-frame")), self.frame));
        if seen + 1 != self.frame {
            self.refresh_logins();
        }
        let mut remove: Option<usize> = None;
        let mut reveal: Option<usize> = None;
        let mut copy: Option<usize> = None;
        let filter = self.pw_filter.to_lowercase();
        let keyring_ok = self.keyring_ok();
        super::pages::centered_column(ui, |ui| {
            super::pages::page_title(ui, th, tr("Mots de passe"));
            ui.label(RichText::new(tr("Les mots de passe sont gardés dans le trousseau de votre système, pas dans les fichiers de Sanaga. Sanaga propose de les enregistrer quand vous vous connectez à un site, et de les remplir la fois suivante.")).size(13.0).color(th.sub));
            if !keyring_ok {
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
        // (the keyring is asked on a thread: it may wait to be unlocked)
        if let Some((s, u)) = reveal.and_then(|i| self.store.logins.get(i).cloned()) {
            let p = profile.clone();
            let (s2, u2) = (s.clone(), u.clone());
            self.off_thread(
                move || keyring::lookup(&p, &s2, &u2),
                // (shown on the account's line, wherever it is by now)
                move |app, pass| app.pw_shown = app.store.logins.iter().position(|(a, b)| *a == s && *b == u).zip(pass),
            );
        }
        if let Some((s, u)) = copy.and_then(|i| self.store.logins.get(i).cloned()) {
            self.copy_password(s, u);
        }
        if let Some(i) = remove.filter(|i| *i < self.store.logins.len()) {
            let (s, u) = self.store.logins.remove(i);
            self.off_thread(move || keyring::delete(&profile, &s, &u), |_, ()| {});
            self.store.save_logins();
            self.pw_shown = None;
        }
    }

    pub(super) fn page_extensions(&mut self, ui: &mut Ui, th: &Theme) {
        let list = crate::extensions::all();
        let mut toggle: Option<String> = None;
        let mut delete: Option<String> = None;
        let mut toggle_mobile = false;
        let mut open_folder = false;
        let mut install = false;
        let mut review_done = false;
        let on = self.store.extensions_on.clone();
        // an extension just installed (switched off): what it would do, first
        let review = self.ext_review.as_ref().and_then(|id| list.iter().find(|e| e.id == *id)).cloned();
        super::pages::centered_column(ui, |ui| {
            super::pages::page_title(ui, th, tr("Extensions"));
            ui.label(RichText::new(tr("Les extensions ajoutent des fonctions aux pages web. Les changements s'appliquent tout de suite aux pages ouvertes.")).size(13.0).color(th.sub));
            ui.add_space(12.0);
            if let Some(e) = &review {
                Frame::new().fill(th.accent.gamma_multiply(0.12)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(tf("« {} » est installée mais désactivée.", &[&e.name])).size(14.0).family(bold()).color(th.text));
                    let where_ = if e.matches.is_empty() { tr("Tous les sites").to_string() } else { e.matches.join(", ") };
                    ui.label(RichText::new(tf("Où elle agirait : {}", &[&where_])).color(th.text));
                    ui.label(RichText::new(tr("Sur ces sites, elle pourrait lire et modifier tout ce que la page affiche, y compris ce que vous y tapez.")).size(12.5).color(th.sub));
                    egui::CollapsingHeader::new(tr("Voir le code")).default_open(true).show(ui, |ui| {
                        let mut code = if e.js.is_empty() { e.css.clone() } else { e.js.clone() };
                        egui::ScrollArea::vertical().id_salt("ext-review-code").max_height(320.0).show(ui, |ui| {
                            ui.add(TextEdit::multiline(&mut code).code_editor().desired_width(f32::INFINITY).interactive(false));
                        });
                    });
                    ui.horizontal(|ui| {
                        let b = egui::Button::new(RichText::new(tr("Activer l'extension")).color(Color32::WHITE)).fill(th.accent);
                        if ui.add(b).clicked() {
                            toggle = Some(e.id.clone());
                            review_done = true;
                        }
                        if ui.button(tr("Supprimer")).clicked() {
                            delete = Some(e.id.clone());
                            review_done = true;
                        }
                        if ui.button(tr("Plus tard")).clicked() {
                            review_done = true;
                        }
                    });
                });
                ui.add_space(12.0);
            }
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
                            if !e.builtin && ui.button(tr("Supprimer")).clicked() {
                                delete = Some(e.id.clone());
                            }
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
            ui.horizontal(|ui| {
                if ui.button(tr("Installer une extension…")).clicked() {
                    install = true;
                }
                if ui.button(tr("Ouvrir le dossier des extensions")).clicked() {
                    open_folder = true;
                }
            });
            ui.label(RichText::new(tr("Chrome (.crx, .zip, manifest.json), Firefox (.xpi), scripts utilisateur (.user.js)")).size(11.5).color(th.sub));
        });
        if review_done || review.is_none() {
            self.ext_review = None;
        }
        if install {
            self.install_extension_dialog();
        }
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
            self.toggle_extension(&id);
        }
        if let Some(id) = delete {
            self.delete_extension(&id);
        }
        if open_folder {
            let dir = crate::extensions::prepare_folder();
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
    }
}
