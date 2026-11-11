//! "Import from another browser" page: favourites, history and passwords from
//! Chrome-family browsers and Firefox found on this computer, and passwords
//! from a CSV export. The work runs in the background; passwords go straight
//! into the system keyring.

use super::*;
use crate::migrate::{self, Source};
use eframe::egui::{Align, Color32, Layout, Margin, RichText};

pub(super) struct ImportDone {
    pub source: String,
    pub bookmarks: Vec<(String, String)>,
    pub history: Vec<(u64, String, String)>,
    /// logins now in the keyring: (site, user)
    pub logins: Vec<(String, String)>,
    pub keyring_failed: usize,
    pub problems: Vec<String>,
}

fn store_logins(profile: &str, logins: Vec<(String, String, String)>) -> (Vec<(String, String)>, usize) {
    let mut saved = Vec::new();
    let mut failed = 0;
    for (site, user, pass) in logins {
        if crate::keyring::store(profile, &site, &user, &pass) {
            saved.push((site, user));
        } else {
            failed += 1;
        }
    }
    (saved, failed)
}

impl Sanaga {
    fn start_import(&mut self, src: Source) {
        let (bm, hist, pw) = (self.import_bookmarks, self.import_history, self.import_passwords && src.has_passwords());
        let profile = self.store.profile.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = self.ctx.clone();
        self.import_rx = Some(rx);
        self.import_status = tf("Import depuis {} en cours…", &[&src.name]);
        std::thread::Builder::new()
            .name("sanaga-import".into())
            .spawn(move || {
                let got = migrate::import(&src, bm, hist, pw);
                let (logins, keyring_failed) = store_logins(&profile, got.logins);
                let _ = tx.send(ImportDone { source: src.name, bookmarks: got.bookmarks, history: got.history, logins, keyring_failed, problems: got.problems });
                ctx.request_repaint();
            })
            .ok();
    }

    fn start_csv_import(&mut self, path: std::path::PathBuf) {
        let profile = self.store.profile.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = self.ctx.clone();
        self.import_rx = Some(rx);
        self.import_status = tr("Import des mots de passe en cours…").to_string();
        std::thread::Builder::new()
            .name("sanaga-import".into())
            .spawn(move || {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let (logins, keyring_failed, problems) = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| migrate::parse_password_csv(&t).map_err(|e| e.to_string())) {
                    Ok(list) => {
                        let (l, f) = store_logins(&profile, list);
                        (l, f, Vec::new())
                    }
                    Err(e) => (Vec::new(), 0, vec![e]),
                };
                let _ = tx.send(ImportDone { source: name, bookmarks: Vec::new(), history: Vec::new(), logins, keyring_failed, problems });
                ctx.request_repaint();
            })
            .ok();
    }

    /// Put what the background import found into Sanaga.
    pub(super) fn poll_import(&mut self) {
        let Some(rx) = &self.import_rx else { return };
        let Ok(done) = rx.try_recv() else { return };
        self.import_rx = None;
        let mut n_bm = 0;
        for (url, title) in done.bookmarks {
            if !self.store.is_bookmarked(&url) {
                let title = if title.trim().is_empty() { url.clone() } else { title };
                self.store.bookmarks.push(crate::store::Bookmark { url, title });
                n_bm += 1;
            }
        }
        if n_bm > 0 {
            self.store.save_bookmarks();
        }
        let n_hist = done.history.len();
        if n_hist > 0 {
            let mut seen: HashSet<(u64, String)> = self.store.history.iter().map(|v| (v.time, v.url.clone())).collect();
            for (time, url, title) in done.history {
                if seen.insert((time, url.clone())) {
                    self.visited.insert(url.clone());
                    self.store.history.push(crate::store::Visit { time, url, title });
                }
            }
            self.store.history.sort_by_key(|v| v.time);
            let extra = self.store.history.len().saturating_sub(5000);
            self.store.history.drain(..extra);
            self.store.rewrite_history();
        }
        let n_pw = done.logins.len();
        for l in done.logins {
            if !self.store.logins.contains(&l) {
                self.store.logins.push(l);
            }
        }
        if n_pw > 0 {
            self.store.save_logins();
        }
        let mut msg = tf("Importé depuis {} : {} favoris, {} pages d'historique, {} mots de passe.", &[&done.source, &n_bm, &n_hist, &n_pw]);
        if done.keyring_failed > 0 {
            msg.push(' ');
            msg.push_str(&tf("{} mots de passe n'ont pas pu aller dans le trousseau.", &[&done.keyring_failed]));
        }
        for p in done.problems {
            msg.push_str("\n• ");
            msg.push_str(&p);
        }
        self.import_status = msg;
    }

    pub(super) fn page_import(&mut self, ui: &mut Ui, th: &Theme) {
        let sources = migrate::detect();
        let busy = self.import_rx.is_some();
        let mut go: Option<Source> = None;
        let mut csv = false;
        let mut html = false;
        super::pages::centered_column(ui, |ui| {
            super::pages::page_title(ui, th, tr("Importer depuis un autre navigateur"));
            ui.label(RichText::new(tr("Reprenez vos favoris, votre historique et vos mots de passe. Rien n'est modifié dans l'autre navigateur ; les mots de passe vont dans le trousseau de votre système.")).size(13.0).color(th.sub));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.import_bookmarks, tr("Favoris"));
                ui.checkbox(&mut self.import_history, tr("Historique"));
                ui.checkbox(&mut self.import_passwords, tr("Mots de passe"));
            });
            ui.add_space(10.0);
            if sources.is_empty() {
                ui.label(RichText::new(tr("Aucun autre navigateur trouvé sur cet ordinateur.")).color(th.sub));
            }
            for s in &sources {
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&s.name).size(14.0).family(bold()).color(th.text));
                            let what = if s.has_passwords() {
                                tr("Favoris, historique et mots de passe")
                            } else {
                                tr("Favoris et historique (mots de passe : exportez-les en CSV depuis Firefox, puis importez le fichier ci-dessous)")
                            };
                            ui.label(RichText::new(what).size(12.0).color(th.sub));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let b = egui::Button::new(RichText::new(tr("Importer")).color(Color32::WHITE)).fill(th.accent);
                            if ui.add_enabled(!busy, b).clicked() {
                                go = Some(s.clone());
                            }
                        });
                    });
                });
                ui.add_space(6.0);
            }
            ui.add_space(10.0);
            ui.label(RichText::new(tr("Depuis un fichier")).size(15.0).family(bold()).color(th.text));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!busy, egui::Button::new(tr("Mots de passe (fichier CSV)…"))).clicked() {
                    csv = true;
                }
                if ui.add_enabled(!busy, egui::Button::new(tr("Favoris (fichier HTML)…"))).clicked() {
                    html = true;
                }
            });
            ui.label(RichText::new(tr("Chrome : Paramètres › Mots de passe › Exporter. Firefox : about:logins › menu « … » › Exporter. Pensez à supprimer le fichier exporté après l'import : il contient vos mots de passe en clair.")).size(12.0).color(th.sub));
            if !self.import_status.is_empty() {
                ui.add_space(14.0);
                Frame::new().fill(th.accent.gamma_multiply(0.10)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if busy {
                            ui.add(egui::Spinner::new().size(14.0).color(th.accent));
                        }
                        ui.label(RichText::new(&self.import_status).size(13.0).color(th.text));
                    });
                });
            }
        });
        if let Some(s) = go {
            self.start_import(s);
        }
        if csv {
            if let Some(p) = self.choose_file(tr("Importer des mots de passe (CSV)"), &["*.csv"], tr("Mots de passe (CSV)")) {
                self.start_csv_import(p);
            }
        }
        if html {
            self.navigate(self.active, "sanaga://bookmarks".into(), None, true);
        }
    }
}
