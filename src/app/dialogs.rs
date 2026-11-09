//! What pages ask — alert(), confirm(), prompt(), "leave this page?", a
//! site's login — shown as Sanaga's own sheets: the site's name on top (so a
//! page can't pretend to be Sanaga), clear buttons, Enter / Escape, and
//! "don't let this site show more alerts" when a page insists.

use super::*;
use crate::page::DialogKind;
use eframe::egui::{Align, Color32, Layout, Margin, RichText, TextEdit};

/// The dialog being answered: its key, the text typed, the password.
#[derive(Default)]
pub(super) struct DialogState {
    key: u64,
    text: String,
    password: String,
    fresh: bool,
    /// How many alerts this site showed in a row (to offer silencing it).
    count: u32,
    /// "Don't let this site show more alerts" ticked.
    silence: bool,
}

impl Sanaga {
    /// A page asks something: it waits in line on its own tab (shown when
    /// that tab is; a background tab never takes the front).
    pub(super) fn page_dialog(&mut self, tab: u64, key: u64) {
        let Some(info) = crate::page::dialog(key) else { return };
        // a site silenced by the person: answered without asking
        if self.silenced_sites.contains(&info.host) && matches!(info.kind, DialogKind::Alert | DialogKind::Confirm | DialogKind::Prompt(_)) {
            crate::page::answer_dialog(key, matches!(info.kind, DialogKind::Alert), "", "");
            return;
        }
        self.dialogs.push((tab, key));
    }

    /// The question of the tab shown, if it has one waiting (tab, key).
    pub(super) fn shown_dialog(&self) -> Option<(u64, u64)> {
        let active = self.tabs.get(self.active).map(|t| t.id)?;
        self.dialogs.iter().copied().find(|(t, _)| *t == active)
    }

    pub(super) fn dialog_sheet(&mut self, ctx: &egui::Context, th: &Theme) {
        // answered or gone (tab closed): dropped
        let tabs = &self.tabs;
        self.dialogs.retain(|&(tab, key)| {
            let tab_open = tabs.iter().any(|t| t.id == tab);
            if !tab_open {
                crate::page::answer_dialog(key, false, "", "");
            }
            tab_open && crate::page::dialog(key).is_some()
        });
        // only the question of the tab shown: a background tab's waits there
        let Some((_, key)) = self.shown_dialog() else { return };
        let Some(info) = crate::page::dialog(key) else { return };
        if self.dialog_state.key != key {
            let same_site = self.last_dialog_host == info.host;
            self.dialog_state = DialogState { key, text: if let DialogKind::Prompt(d) = &info.kind { d.clone() } else { String::new() }, password: String::new(), fresh: true, count: if same_site { self.dialog_state.count + 1 } else { 1 }, silence: false };
            self.last_dialog_host = info.host.clone();
        }
        let screen = ctx.content_rect();
        // the page under it, dimmed
        ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("dialog-dim"))).rect_filled(screen, 0.0, Color32::from_black_alpha(if self.store.settings.dark { 110 } else { 70 }));
        let w = 440.0;
        let (mut ok, mut cancel) = (false, false);
        let site = if info.host.is_empty() { tr("Cette page").to_string() } else { info.host.clone() };
        let st = &mut self.dialog_state;
        egui::Area::new(egui::Id::new("page-dialog")).order(egui::Order::Foreground).fixed_pos(pos2(screen.center().x - w / 2.0, screen.top() + 96.0)).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.hairline.gamma_multiply(0.7)))
                .corner_radius(CornerRadius::same(16))
                .shadow(egui::Shadow { offset: [0, 14], blur: 40, spread: 0, color: Color32::from_black_alpha(80) })
                .inner_margin(Margin::same(20))
                .show(ui, |ui| {
                    ui.set_width(w - 40.0);
                    // who is asking: the site, never Sanaga
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
                        super::chrome::site_badge(ui.painter(), r.center(), 10.0, &format!("https://{}/", info.host), &site);
                        ui.add_space(4.0);
                        let head = match &info.kind {
                            DialogKind::Login(_) => tf("{} demande de vous connecter", &[&site]),
                            DialogKind::Leave => tf("Quitter {} ?", &[&site]),
                            // (it may come from a frame of another site
                            // inside the page: WebKit doesn't say which)
                            _ => tf("Une page sur {} dit :", &[&site]),
                        };
                        ui.label(RichText::new(head).size(14.5).family(bold()).color(th.text));
                    });
                    ui.add_space(10.0);
                    let message = match &info.kind {
                        DialogKind::Leave => tr("Ce que vous avez saisi sur cette page pourrait ne pas être enregistré.").to_string(),
                        DialogKind::Login(realm) if !realm.is_empty() => tf("Espace : « {} ». Votre identifiant et votre mot de passe sont envoyés à ce site.", &[realm]),
                        DialogKind::Login(_) => tr("Votre identifiant et votre mot de passe sont envoyés à ce site.").to_string(),
                        _ => info.message.chars().take(2000).collect(),
                    };
                    if !message.trim().is_empty() {
                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                            ui.add(egui::Label::new(RichText::new(message).size(14.0).color(th.text)).wrap());
                        });
                        ui.add_space(12.0);
                    }
                    match &info.kind {
                        DialogKind::Prompt(_) => {
                            let r = ui.add(TextEdit::singleline(&mut st.text).desired_width(f32::INFINITY).margin(Margin::symmetric(10, 7)));
                            if st.fresh {
                                r.request_focus();
                            }
                            ui.add_space(12.0);
                        }
                        DialogKind::Login(_) => {
                            let r = ui.add(TextEdit::singleline(&mut st.text).hint_text(tr("Identifiant")).desired_width(f32::INFINITY).margin(Margin::symmetric(10, 7)));
                            if st.fresh {
                                r.request_focus();
                            }
                            ui.add_space(6.0);
                            ui.add(TextEdit::singleline(&mut st.password).hint_text(tr("Mot de passe")).password(true).desired_width(f32::INFINITY).margin(Margin::symmetric(10, 7)));
                            ui.add_space(12.0);
                        }
                        _ => {}
                    }
                    st.fresh = false;
                    if st.count >= 2 && matches!(info.kind, DialogKind::Alert | DialogKind::Confirm | DialogKind::Prompt(_)) {
                        ui.checkbox(&mut st.silence, RichText::new(tr("Ne plus laisser ce site afficher d'alertes")).size(12.5).color(th.sub));
                        ui.add_space(10.0);
                    }
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let (ok_label, cancel_label) = match &info.kind {
                                DialogKind::Alert => (tr("OK"), ""),
                                DialogKind::Leave => (tr("Quitter"), tr("Rester")),
                                DialogKind::Login(_) => (tr("Se connecter"), tr("Annuler")),
                                _ => (tr("OK"), tr("Annuler")),
                            };
                            let b = egui::Button::new(RichText::new(ok_label).color(Color32::WHITE).size(13.5)).fill(th.accent).corner_radius(CornerRadius::same(10)).min_size(vec2(84.0, 32.0));
                            if ui.add(b).clicked() {
                                ok = true;
                            }
                            if !cancel_label.is_empty() && ui.add(egui::Button::new(RichText::new(cancel_label).size(13.5)).corner_radius(CornerRadius::same(10)).min_size(vec2(84.0, 32.0))).clicked() {
                                cancel = true;
                            }
                        });
                    });
                });
        });
        let (enter, esc) = ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        if enter {
            ok = true;
        }
        if esc {
            cancel = true;
        }
        if ok || cancel {
            if self.dialog_state.silence {
                self.silenced_sites.insert(info.host.clone());
            }
            let (text, password) = (std::mem::take(&mut self.dialog_state.text), std::mem::take(&mut self.dialog_state.password));
            crate::page::answer_dialog(key, ok, &text, &password);
            self.dialogs.retain(|&(_, k)| k != key);
            self.switcher_refocus = true;
        }
    }
}
