//! The person's own toolbar buttons (an address, one of Sanaga's commands, or
//! a script run in the page), and the » menu where toolbar buttons can be
//! tucked away to keep the toolbar clean.

use super::*;
use crate::store::CustomButton;
use eframe::egui::{Align2, Color32, Margin, RichText, Sense};

/// The form adding a button (Settings › Appearance).
#[derive(Default)]
pub(super) struct ButtonForm {
    pub label: String,
    pub symbol: String,
    pub kind: u8,
    pub value: String,
}

/// Sanaga's commands a button can run: name, label.
pub(super) fn commands() -> Vec<(&'static str, &'static str)> {
    vec![
        ("new", tr("Nouvel onglet")),
        ("window", tr("Nouvelle fenêtre")),
        ("private", tr("Onglet privé")),
        ("container", tr("Nouvel onglet dans un conteneur…")),
        ("reopen", tr("Rouvrir l'onglet fermé")),
        ("history", tr("Historique")),
        ("downloads", tr("Téléchargements")),
        ("settings", tr("Paramètres")),
        ("reader", tr("Lecteur")),
        ("summary", tr("Résumer la page")),
        ("translate", tr("Traduire la page")),
        ("overview", tr("Aperçu des onglets")),
        ("tabsearch", tr("Rechercher dans les onglets")),
        ("sidebar", tr("Barre latérale")),
        ("focus", tr("Mode focus")),
        ("ontop", tr("Garder la fenêtre au premier plan")),
        ("pip", tr("Vidéo flottante")),
        ("zap", tr("Supprimer un élément de la page…")),
        ("edittext", tr("Modifier le texte de la page")),
        ("unlock", tr("Débloquer le copier-coller")),
        ("archive", tr("Version archivée de la page")),
        ("site", tr("Réglages de ce site…")),
        ("devices", tr("Simulateur mobile")),
        ("inspector", tr("Inspecteur web")),
        ("source", tr("Afficher le code source")),
        ("print", tr("Imprimer…")),
        ("savepage", tr("Enregistrer la page")),
        ("readinglist", tr("Ajouter à la liste de lecture")),
        ("fullscreen", tr("Plein écran")),
        ("cleardata", tr("Effacer les données de navigation…")),
        ("activity", tr("Activité : mémoire et processeur")),
        ("crashes", tr("Rapports de plantage")),
        ("split", tr("Vue partagée : deux onglets côte à côte")),
        ("mobileside", tr("Aperçu mobile à côté de la page")),
    ]
}

fn command_label(name: &str) -> String {
    commands().into_iter().find(|c| c.0 == name).map(|c| c.1.to_string()).unwrap_or_else(|| name.to_string())
}

/// What a button does, in words (its tooltip).
pub(super) fn button_tip(b: &CustomButton) -> String {
    match b.kind {
        0 => format!("{} — {}", b.label, b.value),
        1 => format!("{} — {}", b.label, command_label(&b.value)),
        _ => format!("{} — {}", b.label, tr("script dans la page")),
    }
}

/// A button's face: its emoji / letters, centred.
pub(super) fn paint_symbol(p: &egui::Painter, r: Rect, b: &CustomButton, col: Color32) {
    let letters = || b.label.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase();
    let sym = if b.symbol.trim().is_empty() { letters() } else { b.symbol.trim().chars().take(3).collect() };
    // (colour emoji can't be drawn by the interface's fonts: the label's initials then)
    let sym = if p.fonts_mut(|f| f.has_glyphs(&FontId::proportional(15.0), &sym)) { sym } else { letters() };
    let size = if sym.chars().count() > 1 && sym.is_ascii() { 11.5 } else { 15.0 };
    p.text(r.center(), Align2::CENTER_CENTER, sym, FontId::proportional(size), col);
}

impl Sanaga {
    /// Is this toolbar button tucked away in the » menu?
    pub(super) fn tb_overflow(&self, key: &str) -> bool {
        self.store.settings.toolbar.split(',').any(|k| k.strip_prefix('~') == Some(key))
    }

    /// Something to show in the » menu?
    pub(super) fn has_overflow(&self) -> bool {
        self.store.settings.toolbar.split(',').any(|k| k.starts_with('~'))
    }

    /// Run the person's button `k`.
    pub(super) fn run_button(&mut self, k: usize) {
        let Some(b) = self.store.buttons.get(k).cloned() else { return };
        match b.kind {
            0 => {
                let u = input_to_url(&b.value, self.store.settings.engine, &self.store.settings.home);
                self.open_tab(&u, self.tabs[self.active].private);
            }
            1 => self.run_shortcut(&b.value),
            _ => {
                match self.tabs.get(self.active).and_then(|t| t.web.as_ref()) {
                    Some(w) => {
                        let _ = w.evaluate_script(&format!("try{{{}\n}}catch(e){{console.warn('bouton Sanaga',e)}}", b.value));
                    }
                    None => self.notice = Some((tr("Ce bouton agit sur une page web.").into(), self.ctx.input(|i| i.time))),
                }
            }
        }
    }

    /// What a toolbar button does (from the » menu).
    pub(super) fn toolbar_key_action(&mut self, key: &str) {
        let a = self.active;
        match key {
            "sidebar" => self.sidebar_open = !self.sidebar_open,
            "back" => self.go_history(a, -1),
            "forward" => self.go_history(a, 1),
            "reload_left" | "reload" => self.reload(a),
            "home" => self.go_home(a),
            "star" => self.run_shortcut("bookmark"),
            "reader" => self.toggle_reader(a),
            "newtab" => {
                self.open_tab("sanaga://newtab", false);
            }
            "bookmarks" => {
                self.open_tab("sanaga://bookmarks", false);
            }
            "reading" => self.sidebar_open = true,
            "print" => self.print_page(),
            "downloads" => {
                self.open_tab("sanaga://downloads", false);
            }
            "share" => self.share_open = true,
            "overview" => self.toggle_overview(),
            "mobile" => self.toggle_devices(),
            "summary" => self.summarize_page(),
            _ => {}
        }
    }

    /// The » menu: the toolbar buttons tucked away.
    pub(super) fn overflow_menu(&mut self, ctx: &egui::Context, th: &Theme) {
        if !self.overflow_open {
            return;
        }
        let keys: Vec<&'static str> = crate::store::TOOLBAR_BUTTONS.iter().map(|b| b.0).filter(|k| self.tb_overflow(k) && *k != "memory").collect();
        let w = 250.0;
        let (pos, pivot, _) = super::layout::popover_place(ctx, self.overflow_rect, (self.overflow_rect.right() - w).max(8.0), 6.0);
        let mut pick: Option<&'static str> = None;
        let dark = self.store.settings.dark;
        let area = egui::Area::new(egui::Id::new("overflow-menu")).order(egui::Order::Foreground).pivot(pivot).fixed_pos(pos).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.hairline.gamma_multiply(0.6)))
                .corner_radius(CornerRadius::same(12))
                .shadow(egui::Shadow { offset: [0, 8], blur: 26, spread: 0, color: Color32::from_black_alpha(if dark { 110 } else { 45 }) })
                .inner_margin(Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(w - 12.0);
                    for k in &keys {
                        let (r, resp) = ui.allocate_exact_size(vec2(w - 12.0, 30.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(7), th.accent);
                        }
                        let col = if resp.hovered() { Color32::WHITE } else { th.text };
                        let label = super::toolbar::toolbar_button_label(k);
                        let short = label.split(',').next().unwrap_or(label);
                        ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, short, FontId::proportional(13.5), col);
                        if resp.clicked() {
                            pick = Some(k);
                        }
                    }
                    if keys.is_empty() {
                        ui.label(RichText::new(tr("Rien ici pour l'instant.")).color(th.sub));
                    }
                    ui.separator();
                    if ui.small_button(tr("Choisir les boutons…")).clicked() {
                        pick = Some("__settings");
                    }
                });
        });
        let anchor = self.overflow_rect;
        let outside = ctx.input(|i| i.pointer.any_pressed()) && !ctx.pointer_interact_pos().map(|p| area.response.rect.contains(p) || anchor.contains(p)).unwrap_or(false);
        if outside || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.overflow_open = false;
        }
        if let Some(k) = pick {
            self.overflow_open = false;
            if k == "__settings" {
                self.open_settings(&super::settings::Section::Appearance.address());
            } else {
                self.toolbar_key_action(k);
            }
        }
    }

    /// Settings › Appearance: the person's buttons, and the form adding one.
    /// True when something changed.
    pub(super) fn buttons_settings(&mut self, ui: &mut Ui, th: &Theme) -> bool {
        let mut changed = false;
        let mut remove: Option<usize> = None;
        for (k, b) in self.store.buttons.iter().enumerate() {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(8), th.track);
                paint_symbol(ui.painter(), r, b, th.text);
                ui.label(RichText::new(button_tip(b)).size(13.0).color(th.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(tr("Supprimer")).clicked() {
                        remove = Some(k);
                    }
                });
            });
        }
        if let Some(k) = remove {
            self.store.buttons.remove(k);
            self.store.save_buttons();
            changed = true;
        }
        ui.add_space(6.0);
        let f = &mut self.button_form;
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut f.symbol).hint_text("⭐").desired_width(44.0));
            ui.add(egui::TextEdit::singleline(&mut f.label).hint_text(tr("Nom du bouton")).desired_width(170.0));
            let kinds = [tr("Adresse"), tr("Commande de Sanaga"), tr("Script dans la page")];
            egui::ComboBox::from_id_salt("button-kind").selected_text(kinds[f.kind.min(2) as usize]).width(170.0).show_ui(ui, |ui| {
                for (k, l) in kinds.iter().enumerate() {
                    if ui.selectable_label(f.kind == k as u8, *l).clicked() {
                        f.kind = k as u8;
                        f.value.clear();
                    }
                }
            });
        });
        match f.kind {
            0 => {
                ui.add(egui::TextEdit::singleline(&mut f.value).hint_text("https://…").desired_width(f32::INFINITY));
            }
            1 => {
                let cmds = commands();
                let current = cmds.iter().find(|c| c.0 == f.value).map(|c| c.1).unwrap_or(tr("Choisir une commande"));
                egui::ComboBox::from_id_salt("button-command").selected_text(current).width(300.0).show_ui(ui, |ui| {
                    for (name, label) in &cmds {
                        if ui.selectable_label(f.value == *name, *label).clicked() {
                            f.value = name.to_string();
                        }
                    }
                });
            }
            _ => {
                ui.add(egui::TextEdit::multiline(&mut f.value).hint_text("document.body.style.zoom = '1.2';").code_editor().desired_rows(3).desired_width(f32::INFINITY));
            }
        }
        let ready = !f.label.trim().is_empty() && !f.value.trim().is_empty();
        if ui.add_enabled(ready, egui::Button::new(RichText::new(tr("Ajouter le bouton")).color(if ready { Color32::WHITE } else { th.sub })).fill(if ready { th.accent } else { th.track })).clicked() {
            let b = CustomButton { label: f.label.trim().to_string(), symbol: f.symbol.trim().to_string(), kind: f.kind, value: f.value.trim().to_string() };
            *f = ButtonForm::default();
            self.store.buttons.push(b);
            self.store.save_buttons();
            changed = true;
        }
        changed
    }
}
