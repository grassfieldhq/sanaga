//! Containers: tabs with their own cookies, logins and site data, apart from
//! the rest of the profile — two accounts of the same site side by side, or
//! shopping kept away from banking, without another browser or profile.

use super::tabs::GROUP_COLORS;
use super::*;
use crate::store::Container;
use eframe::egui::{Align2, Color32, Margin, RichText, Sense};

/// Containers offered when there are none yet.
const SUGGESTED: [(&str, u8); 4] = [("Personnel", 1), ("Travail", 7), ("Achats", 5), ("Banque", 4)];

impl Sanaga {
    /// The colour of a container ("" = none).
    pub(super) fn container_color(&self, name: &str) -> Option<Color32> {
        if name.is_empty() {
            return None;
        }
        self.store.containers.iter().find(|c| c.name == name).map(|c| GROUP_COLORS.get(c.color as usize).map(|g| g.1).unwrap_or(GROUP_COLORS[1].1))
    }

    /// Open `url` (a new tab when empty) in a tab of container `name`.
    pub(super) fn open_in_container(&mut self, url: &str, name: &str) {
        let u = if url.is_empty() { "sanaga://newtab" } else { url };
        // the tab's page is made with the container's engine: set it first
        let i = self.open_tab("sanaga://newtab", false);
        self.tabs[i].container = name.to_string();
        if u != "sanaga://newtab" {
            self.navigate(i, u.to_string(), None, true);
            self.focus_address = false;
        }
        self.save_session();
    }

    /// A new container; false when the name is empty or taken.
    pub(super) fn add_container(&mut self, name: &str) -> bool {
        let name = name.trim().replace(['\t', '\n'], " ");
        if name.is_empty() || self.store.containers.iter().any(|c| c.name.eq_ignore_ascii_case(&name)) {
            return false;
        }
        let color = SUGGESTED.iter().find(|s| tr(s.0) == name).map(|s| s.1).unwrap_or(((self.store.containers.len() * 3 + 1) % GROUP_COLORS.len()) as u8);
        self.store.containers.push(Container { name, color });
        self.store.save_containers();
        true
    }

    /// Remove a container and everything it kept (cookies, logins, site data).
    pub(super) fn remove_container(&mut self, name: &str) {
        self.store.containers.retain(|c| c.name != name);
        self.store.save_containers();
        for t in &mut self.tabs {
            if t.container == name {
                t.container.clear();
            }
        }
        if let Some(base) = &self.web_dir {
            let safe: String = name.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
            let dir = base.join("containers").join(safe);
            if dir.starts_with(base) && dir != *base {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
        self.notice = Some((tf("Conteneur « {} » supprimé avec ses données.", &[&name]), self.ctx.input(|i| i.time)));
    }

    /// Items "open in this container" for a menu; true when one was chosen.
    pub(super) fn container_items(&mut self, ui: &mut Ui, url: &str) -> bool {
        let list = self.store.containers.clone();
        for c in &list {
            let col = GROUP_COLORS.get(c.color as usize).map(|g| g.1).unwrap_or(Color32::GRAY);
            if super::chrome::menu_item_dot(ui, col, &c.name) {
                self.open_in_container(url, &c.name);
                return true;
            }
        }
        if !list.is_empty() {
            ui.separator();
        }
        if super::chrome::menu_item(ui, crate::icons::Icon::Plus, tr("Nouveau conteneur…")) {
            self.container_ask = Some(url.to_string());
            return true;
        }
        false
    }

    /// The chooser: which container to open the page in (or a new one).
    pub(super) fn container_chooser(&mut self, ctx: &egui::Context, th: &Theme) {
        let Some(url) = self.container_ask.clone() else { return };
        let mut pick: Option<String> = None;
        let mut close = false;
        let screen = ctx.content_rect();
        let list = self.store.containers.clone();
        egui::Area::new(egui::Id::new("container-ask")).order(egui::Order::Foreground).fixed_pos(pos2(screen.center().x - 190.0, screen.top() + 110.0)).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.border))
                .corner_radius(CornerRadius::same(14))
                .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(60) })
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(348.0);
                    ui.label(RichText::new(tr("Ouvrir dans un conteneur")).size(15.0).family(bold()).color(th.text));
                    ui.add_space(2.0);
                    ui.label(RichText::new(tr("Chaque conteneur garde ses propres cookies et connexions : deux comptes d'un même site côte à côte.")).size(12.0).color(th.sub));
                    if !url.is_empty() {
                        ui.add_space(4.0);
                        ui.label(RichText::new(host_of(&url)).size(12.0).color(th.accent));
                    }
                    ui.add_space(10.0);
                    for c in &list {
                        let col = GROUP_COLORS.get(c.color as usize).map(|g| g.1).unwrap_or(Color32::GRAY);
                        let (r, resp) = ui.allocate_exact_size(vec2(348.0, 34.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(8), th.omnibox_hover);
                        }
                        ui.painter().circle_filled(pos2(r.left() + 16.0, r.center().y), 6.0, col);
                        ui.painter().text(pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, &c.name, FontId::proportional(13.5), th.text);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            pick = Some(c.name.clone());
                        }
                    }
                    if list.is_empty() {
                        ui.label(RichText::new(tr("Créez votre premier conteneur :")).size(12.5).color(th.sub));
                        ui.horizontal_wrapped(|ui| {
                            for (name, color) in SUGGESTED {
                                let col = GROUP_COLORS[color as usize].1;
                                if ui.add(egui::Button::new(RichText::new(format!("● {}", tr(name))).color(col)).corner_radius(CornerRadius::same(12))).clicked() {
                                    pick = Some(tr(name).to_string());
                                }
                            }
                        });
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let resp = ui.add(egui::TextEdit::singleline(&mut self.new_container_name).hint_text(tr("Nouveau conteneur")).desired_width(220.0));
                        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        if (ui.button(tr("Créer")).clicked() || enter) && !self.new_container_name.trim().is_empty() {
                            pick = Some(self.new_container_name.trim().to_string());
                        }
                    });
                    ui.add_space(8.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr("Annuler")).clicked() {
                            close = true;
                        }
                    });
                });
        });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        if let Some(name) = pick {
            if !self.store.containers.iter().any(|c| c.name == name) {
                self.add_container(&name);
            }
            self.new_container_name.clear();
            self.container_ask = None;
            self.open_in_container(&url, &name);
            return;
        }
        if close {
            self.container_ask = None;
        }
    }
}
