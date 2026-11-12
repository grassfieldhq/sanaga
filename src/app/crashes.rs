//! After a crash: the bar shown at the next start (the tabs are back, or one
//! click brings them back), and the page `sanaga://crashes` where the reports
//! kept on this computer are read, copied, sent by the person, or deleted.

use super::pages::centered_column;
use super::*;
use crate::crash::{self, Kind};
use crate::icons::{self, Icon};
use eframe::egui::{Align, Color32, Label, Layout, Margin, RichText, Sense};

/// What the bar after an abrupt end offers.
pub(super) struct CrashNotice {
    /// The tabs of the last run, when they were not reopened by themselves
    /// (session lines, as saved).
    pub(super) lost_tabs: Vec<String>,
}

impl Sanaga {
    /// The last run of this profile ended without closing (`since`: when it
    /// had started). Keep a word of it when no report says more, and tell
    /// the person at the top of the window.
    pub(super) fn note_abrupt_end(&mut self, since: std::time::SystemTime, lost_tabs: Vec<String>) {
        let root = Store::root_dir();
        let explained = crash::list(&root).iter().any(|r| r.kind == Kind::Interface && std::fs::metadata(&r.path).and_then(|m| m.modified()).is_ok_and(|t| t >= since));
        if !explained {
            crash::write(&root, Kind::Abrupt, &crate::page::engine_version(), "");
        }
        self.crash_notice = Some(CrashNotice { lost_tabs });
    }

    /// A page's engine stopped: keep a report (never the page's address).
    pub(super) fn report_page_crash(&mut self, memory: bool) {
        // (a guest window, a screenshot: nothing is kept)
        if self.store.data_dir().is_none() {
            return;
        }
        crash::write(&Store::root_dir(), if memory { Kind::PageMemory } else { Kind::Page }, &crate::page::engine_version(), "");
    }

    pub(super) fn crash_bar_visible(&self) -> bool {
        self.crash_notice.is_some() && !self.focus_mode
    }

    /// "Sanaga stopped unexpectedly": reopen the tabs, read the report, or dismiss.
    pub(super) fn crash_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let lost = self.crash_notice.as_ref().map(|n| n.lost_tabs.len()).unwrap_or(0);
        let mut act: Option<&str> = None;
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            icons::paint(ui.painter(), r, Icon::Info, th.accent);
            ui.add_space(6.0);
            let text = if lost > 0 {
                tf("Sanaga s'est arrêté de façon inattendue. {} onglet(s) peuvent être rouverts.", &[&lost])
            } else {
                tr("Sanaga s'est arrêté de façon inattendue. Vos onglets sont revenus.").to_string()
            };
            ui.label(RichText::new(text).size(13.0).color(th.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.button(tr("Ignorer")).clicked() {
                    act = Some("dismiss");
                }
                if ui.button(tr("Voir le rapport")).clicked() {
                    act = Some("report");
                }
                if lost > 0 && ui.add(egui::Button::new(RichText::new(tr("Rouvrir les onglets")).color(Color32::WHITE)).fill(th.accent).corner_radius(CornerRadius::same(8))).clicked() {
                    act = Some("reopen");
                }
            });
        });
        match act {
            Some("dismiss") => self.crash_notice = None,
            Some("report") => {
                self.crash_notice = None;
                self.open_tab("sanaga://crashes", false);
            }
            Some("reopen") => {
                let lines = self.crash_notice.take().map(|n| n.lost_tabs).unwrap_or_default();
                // (they load when first shown, like any restored tab)
                self.restoring = true;
                let keep = self.active;
                for line in lines {
                    let u = line.split('\t').next().unwrap_or("");
                    let u = u.strip_prefix("lecteur:").unwrap_or(u);
                    if !u.is_empty() {
                        let u = input_to_url(u, self.store.settings.engine, &self.store.settings.home);
                        self.open_tab(&u, false);
                    }
                }
                self.restoring = false;
                self.active = keep.min(self.tabs.len().saturating_sub(1));
                self.save_session();
            }
            _ => {}
        }
    }

    /// `sanaga://crashes`: the reports kept on this computer.
    pub(super) fn page_crashes(&mut self, ui: &mut Ui, th: &Theme) {
        let root = Store::root_dir();
        let reports = crash::list(&root);
        let now = ui.input(|i| i.time);
        let mut remove: Option<std::path::PathBuf> = None;
        let mut remove_all = false;
        let mut send: Option<String> = None;
        let mut copy: Option<(std::path::PathBuf, String)> = None;
        centered_column(ui, |ui| {
            ui.add_space(26.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Rapports de plantage")).size(26.0).family(bold()).color(th.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !reports.is_empty() && ui.button(tr("Tout supprimer")).clicked() {
                        remove_all = true;
                    }
                });
            });
            ui.add(Label::new(RichText::new(tr("Quand Sanaga ou une page s'arrête sans prévenir, un court rapport est gardé ici, sur cet ordinateur. Rien n'est envoyé tout seul : vous le lisez, puis vous choisissez de l'envoyer ou non. Il ne contient ni l'adresse des pages, ni votre nom.")).color(th.sub)).wrap());
            ui.add_space(16.0);
            if reports.is_empty() {
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(16)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(tr("Aucun rapport : rien ne s'est arrêté.")).color(th.sub));
                });
            }
            for r in &reports {
                let what = match r.kind {
                    Kind::Interface => tr("Sanaga s'est arrêté"),
                    Kind::Page => tr("Une page a cessé de fonctionner"),
                    Kind::PageMemory => tr("Une page utilisait trop de mémoire"),
                    Kind::Abrupt => tr("Sanaga a été arrêté sans se fermer"),
                };
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(16)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(what).family(bold()).color(th.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(RichText::new(format!("{} UTC", r.when)).size(12.0).color(th.sub)));
                    });
                    ui.add_space(6.0);
                    // the report as it would be sent, to the letter
                    ui.add(Label::new(RichText::new(r.text.trim_end()).monospace().size(12.0).color(th.sub)).wrap().selectable(true));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new(tr("Envoyer sur GitHub…")).color(Color32::WHITE)).fill(th.accent).corner_radius(CornerRadius::same(8))).clicked() {
                            send = Some(crash::issue_url(r));
                        }
                        let copied = self.crash_copied.as_ref().is_some_and(|(p, at)| *p == r.path && now - at < 2.0);
                        if ui.button(if copied { tr("Copié") } else { tr("Copier") }).clicked() {
                            copy = Some((r.path.clone(), r.text.clone()));
                        }
                        if ui.button(tr("Supprimer")).clicked() {
                            remove = Some(r.path.clone());
                        }
                    });
                });
                ui.add_space(8.0);
            }
            ui.add_space(4.0);
            ui.add(Label::new(RichText::new(tr("« Envoyer sur GitHub » ouvre un nouveau signalement déjà rempli avec ce texte : vous pouvez encore le modifier, et c'est vous qui l'envoyez.")).size(12.0).color(th.sub)).wrap());
            ui.add_space(30.0);
        });
        if let Some((path, text)) = copy {
            self.ctx.copy_text(text);
            self.crash_copied = Some((path, now));
        }
        if let Some(p) = remove {
            crash::remove(&root, &p);
        }
        if remove_all {
            crash::remove_all(&root);
        }
        if let Some(u) = send {
            self.open_tab(&u, false);
        }
    }
}
