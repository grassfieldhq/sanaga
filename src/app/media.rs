//! Video and music: the codecs YouTube & co. need (with a one-click
//! install), and sites protected by a DRM (Netflix, Disney+, Spotify…)
//! opened by themselves in a browser that can play them.

use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align, Color32, Layout, RichText, Sense};
use std::sync::atomic::Ordering as AtomicOrdering;

/// Install progress: nothing yet, running, done, failed.
const IDLE: u8 = 0;
const RUNNING: u8 = 1;
const DONE: u8 = 2;
const FAILED: u8 = 3;

impl Sanaga {
    /// The bar "codecs are missing" over the page shown?
    pub(super) fn codec_bar_visible(&self) -> bool {
        if !self.web_ok || self.codec_dismissed || self.focus_mode {
            return false;
        }
        let Some(t) = self.tabs.get(self.active) else { return false };
        if !matches!(t.view, View::Web) {
            return false;
        }
        let installing = self.codec_install.load(AtomicOrdering::Relaxed);
        if installing == RUNNING || installing == DONE {
            return true;
        }
        let wanted = crate::codecs::is_video_site(&host_of(t.url())) || self.codec_flag == Some(t.id);
        wanted && crate::codecs::status().is_some_and(|s| !s.complete())
    }

    pub(super) fn codec_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let state = self.codec_install.load(AtomicOrdering::Relaxed);
        let missing = crate::codecs::status().map(|s| s.missing().join(", ")).unwrap_or_default();
        let install = crate::codecs::install();
        let mut act: Option<&str> = None;
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            icons::paint(ui.painter(), r, Icon::Info, th.accent);
            ui.add_space(6.0);
            let text = match state {
                RUNNING => tr("Installation dans le terminal : confirmez-la et tapez votre mot de passe là-bas.").to_string(),
                DONE => tr("Codecs installés. Rechargez la page pour lire la vidéo.").to_string(),
                FAILED => tr("L'installation n'a pas abouti. Vous pouvez copier la commande et la lancer dans un terminal.").to_string(),
                // the distribution ships them: Sanaga installs nothing itself
                _ if crate::codecs::distribution().is_some() => tf("Il manque des codecs pour lire les vidéos de ce site ({}). Ils viennent avec votre système : mettez-le à jour.", &[&missing]),
                _ => match &install {
                    Some(i) => tf("Il manque des codecs pour lire les vidéos de ce site ({}). Commande : {}", &[&missing, &i.command]),
                    None => tf("Il manque des codecs pour lire les vidéos de ce site ({}). Ils s'installent une fois pour toutes.", &[&missing]),
                },
            };
            ui.label(RichText::new(text).size(13.0).color(th.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if state != RUNNING && ui.button(tr("Ignorer")).clicked() {
                    act = Some("dismiss");
                }
                match state {
                    DONE => {
                        if ui.add(egui::Button::new(RichText::new(tr("Recharger")).color(Color32::WHITE)).fill(th.accent).corner_radius(CornerRadius::same(8))).clicked() {
                            act = Some("reload");
                        }
                    }
                    RUNNING => {}
                    _ => {
                        if install.is_some() {
                            if ui.button(tr("Copier la commande")).clicked() {
                                act = Some("copy");
                            }
                            // (only this button, in Sanaga's own bar, starts it:
                            // never a page)
                            if crate::codecs::terminal().is_some()
                                && ui.add(egui::Button::new(RichText::new(tr("Installer dans un terminal")).color(Color32::WHITE)).fill(th.accent).corner_radius(CornerRadius::same(8))).clicked()
                            {
                                act = Some("install");
                            }
                        }
                    }
                }
            });
        });
        match act {
            Some("dismiss") => {
                self.codec_dismissed = true;
                self.codec_install.store(IDLE, AtomicOrdering::Relaxed);
            }
            Some("reload") => {
                self.codec_install.store(IDLE, AtomicOrdering::Relaxed);
                self.codec_flag = None;
                let a = self.active;
                self.reload(a);
            }
            Some("copy") => self.copy_codec_command(),
            Some("install") => self.install_codecs(),
            _ => {}
        }
    }

    pub(super) fn copy_codec_command(&mut self) {
        if let Some(i) = crate::codecs::install() {
            self.ctx.copy_text(i.command.clone());
            self.notice = Some((tf("Commande copiée : {}", &[&i.command]), self.ctx.input(|i| i.time)));
        }
    }

    /// Open a terminal running the exact install command shown (the package
    /// manager asks for confirmation and the password there), then watch
    /// for the decoders to appear. Only Sanaga's own buttons call this.
    pub(super) fn install_codecs(&mut self) {
        let (Some(i), Some(term)) = (crate::codecs::install(), crate::codecs::terminal()) else { return };
        if self.codec_install.load(AtomicOrdering::Relaxed) == RUNNING {
            return;
        }
        self.codec_install.store(RUNNING, AtomicOrdering::Relaxed);
        let (state, ctx) = (self.codec_install.clone(), self.ctx.clone());
        let close = tr("Appuyez sur Entrée pour fermer cette fenêtre.").to_string();
        std::thread::Builder::new()
            .name("sanaga-codec-install".into())
            .spawn(move || {
                // exactly the command shown (its words passed as arguments,
                // never re-read by a shell), then a pause so its result can
                // be read
                let script = "msg=$1; shift; \"$@\"; echo; echo \"$msg\"; read _";
                let started = std::process::Command::new(&term[0]).args(&term[1..]).args(["sh", "-c", script, "sh", close.as_str()]).args(&i.argv).status();
                // (a terminal may hand the command to a server and return at
                // once: the decoders are looked for until they are there)
                let mut ok = started.is_ok() && crate::codecs::check_now().complete();
                let until = std::time::Instant::now() + std::time::Duration::from_secs(600);
                while started.is_ok() && !ok && std::time::Instant::now() < until {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    ok = crate::codecs::check_now().complete();
                }
                state.store(if ok { DONE } else { FAILED }, AtomicOrdering::Relaxed);
                ctx.request_repaint();
            })
            .ok();
    }

    /// Each frame: protected sites opened by themselves elsewhere, when the
    /// person chose so.
    pub(super) fn media_step(&mut self) {
        if !self.store.settings.drm_auto {
            return;
        }
        let Some(service) = self.drm_bar_service() else { return };
        let url = self.tabs[self.active].url().to_string();
        // (not a site the person chose to open in Sanaga anyway)
        if crate::identity::protected_url(&url).is_none() {
            return;
        }
        let Some(way) = crate::identity::handoffs(service).into_iter().next() else { return };
        crate::identity::open_with(&way, &url);
        self.drm_dismissed.insert(host_of(&url));
        self.notice = Some((format!("{service} — {}…", way.label), self.ctx.input(|i| i.time)));
        // this tab goes back where it was (or closes if it was opened for it)
        let a = self.active;
        if self.tabs[a].idx > 0 || self.tabs[a].web.as_ref().is_some_and(|w| w.can_go_back().unwrap_or(false)) {
            self.go_history(a, -1);
        } else if self.tabs.len() > 1 {
            self.close_tab(a);
        } else {
            self.go_home(a);
        }
    }

    /// Settings: what the system can play, and protected sites.
    pub(super) fn media_settings(&mut self, ui: &mut Ui, th: &Theme) -> bool {
        let mut changed = false;
        let status = crate::codecs::status();
        let install = crate::codecs::install();
        let state = self.codec_install.load(AtomicOrdering::Relaxed);
        let mut act: Option<&str> = None;
        // (the words take the card's whole width; the buttons come under
        // them: side by side they ran into the text)
        ui.label(RichText::new(tr("Codecs vidéo et audio")).size(13.5).color(th.text));
        let sub = match &status {
            None => tr("Vérification…").to_string(),
            Some(s) if s.complete() => {
                if s.hardware {
                    tr("Tout est là : H.264, AV1, VP9, AAC, Opus, avec le décodage par la carte graphique.").to_string()
                } else {
                    tr("Tout est là : H.264, AV1, VP9, AAC, Opus.").to_string()
                }
            }
            Some(s) => tf("Il manque : {} — YouTube et d'autres sites ne lisent pas leurs vidéos sans eux.", &[&s.missing().join(", ")]),
        };
        ui.add(egui::Label::new(RichText::new(sub).size(11.5).color(th.sub)).wrap());
        if status.as_ref().is_some_and(|s| !s.complete()) {
            if let Some(i) = &install {
                // the exact command that would run
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(&i.command).size(11.0).family(egui::FontFamily::Monospace).color(th.sub)).wrap().selectable(true));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if state == RUNNING {
                        ui.spinner();
                        ui.label(RichText::new(tr("Installation…")).color(th.sub));
                    } else {
                        if crate::codecs::terminal().is_some() && ui.add(egui::Button::new(RichText::new(tr("Installer dans un terminal")).color(Color32::WHITE)).fill(th.accent)).clicked() {
                            act = Some("install");
                        }
                        if ui.button(tr("Copier la commande")).clicked() {
                            act = Some("copy");
                        }
                    }
                });
            }
        }
        ui.add_space(10.0);
        let line = ui.available_rect_before_wrap();
        ui.painter().line_segment([pos2(line.left(), line.top()), pos2(line.right(), line.top())], Stroke::new(1.0, th.hairline.gamma_multiply(0.6)));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 60.0);
                ui.label(RichText::new(tr("Ouvrir les sites protégés dans un navigateur compatible")).size(13.5).color(th.text));
                let ways = crate::identity::handoffs("Netflix");
                let sub = match ways.first() {
                    Some(w) => tf("Netflix, Disney+, Spotify… ont besoin de Widevine, absent de WebKit : ils s'ouvrent tout seuls ({}).", &[&w.label]),
                    None => tr("Netflix, Disney+, Spotify… ont besoin de Widevine, absent de WebKit. Installez Chromium, Chrome ou Firefox pour les regarder.").to_string(),
                };
                ui.label(RichText::new(sub).size(11.5).color(th.sub));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if super::settings::switch(ui, th, &mut self.store.settings.drm_auto) {
                    changed = true;
                }
            });
        });
        match act {
            Some("install") => self.install_codecs(),
            Some("copy") => self.copy_codec_command(),
            _ => {}
        }
        changed
    }
}
