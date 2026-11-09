//! Tab groups, as in Chrome: every tab stays in the tab bar; a group is a
//! coloured label in front of its tabs (a line of its colour runs under
//! them); a click on the label folds or unfolds the group; a new group opens
//! its editor at once — name, colour, and what to do with it. Dragging a tab
//! among a group's tabs puts it in the group; dragging it out takes it out.

use super::tabs::GROUP_COLORS;
use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align2, Color32, Margin, Sense};

/// What the tab bar shows, in order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum StripItem {
    /// A group's label (before its tabs).
    Label(usize),
    Tab(usize),
}

impl Sanaga {
    /// The tab bar's items: pinned tabs, then tabs and group labels in order;
    /// a folded group shows only its label.
    pub(super) fn strip_items(&self) -> Vec<StripItem> {
        let mut out = Vec::new();
        let mut last = usize::MAX;
        for i in self.visible_tabs() {
            let t = &self.tabs[i];
            if t.pinned {
                out.push(StripItem::Tab(i));
                continue;
            }
            if t.group > 0 && t.group != last {
                out.push(StripItem::Label(t.group));
            }
            last = t.group;
            if !(t.group > 0 && self.group_folded(t.group)) {
                out.push(StripItem::Tab(i));
            }
        }
        out
    }

    /// Width of a group's label in the tab bar.
    pub(super) fn label_width(&self, ui: &Ui, g: usize) -> f32 {
        let name = self.groups.get(g).map(|n| n.trim().to_string()).unwrap_or_default();
        let folded = self.group_folded(g);
        let count = if folded { format!("  {}", self.tabs.iter().filter(|t| t.group == g).count()) } else { String::new() };
        if name.is_empty() && count.is_empty() {
            return 26.0;
        }
        let g = ui.painter().layout_no_wrap(format!("{name}{count}"), FontId::new(12.0, bold()), Color32::WHITE);
        (g.size().x + 22.0).clamp(26.0, 170.0)
    }

    /// Draw a group's label; true when clicked (fold / unfold).
    pub(super) fn draw_label(&mut self, ui: &mut Ui, g: usize, r: Rect) {
        let col = self.group_color(g);
        let resp = ui.interact(r, egui::Id::new(("group-label", g)), Sense::click());
        let chip = r.shrink2(vec2(3.0, 4.0));
        let folded = self.group_folded(g);
        let name = self.groups.get(g).map(|n| n.trim().to_string()).unwrap_or_default();
        let count = self.tabs.iter().filter(|t| t.group == g).count();
        if name.is_empty() && !folded {
            ui.painter().circle_filled(chip.center(), 6.5, col);
        } else {
            let fill = if resp.hovered() { col.gamma_multiply(0.85) } else { col };
            ui.painter().rect_filled(chip, CornerRadius::same(7), fill);
            let text = if folded { if name.is_empty() { count.to_string() } else { format!("{name}  {count}") } } else { name.clone() };
            ui.painter().with_clip_rect(chip.shrink2(vec2(6.0, 0.0))).text(chip.center(), Align2::CENTER_CENTER, text, FontId::new(12.0, bold()), Color32::WHITE);
        }
        self.group_label_rects.push((g, r));
        let tip = if folded { tr("Déplier le groupe — clic droit : nom, couleur…") } else { tr("Replier le groupe — clic droit : nom, couleur…") };
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).tip(tip);
        if resp.secondary_clicked() || ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary)) && resp.hovered() {
            self.group_editor = Some(g);
        } else if resp.clicked() {
            self.toggle_group_fold(g);
        }
    }

    /// Put the shown tab in a new group and open its editor (Chrome).
    pub(super) fn group_shown_tab(&mut self) {
        let a = self.active;
        if self.tabs[a].pinned {
            self.tabs[a].pinned = false;
        }
        self.tab_to_new_group(a);
    }

    /// The editor of a group: under its label, at once when it is new.
    pub(super) fn group_editor_popup(&mut self, ctx: &egui::Context, th: &Theme) {
        let Some(g) = self.group_editor else { return };
        if g == 0 || g >= self.groups.len() {
            self.group_editor = None;
            return;
        }
        self.sync_group_meta();
        let anchor = self.group_label_rects.iter().find(|(k, _)| *k == g).map(|(_, r)| *r);
        let screen = ctx.content_rect();
        let w = 280.0;
        let pos = match anchor {
            Some(r) => pos2((r.left()).clamp(8.0, screen.right() - w - 8.0), r.bottom() + 6.0),
            None => pos2(screen.center().x - w / 2.0, screen.top() + 110.0),
        };
        let mut act: Option<&str> = None;
        let mut color: Option<u8> = None;
        let dark = self.store.settings.dark;
        let first = self.group_editing == Some(g);
        let area = egui::Area::new(egui::Id::new("group-editor")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.hairline.gamma_multiply(0.6)))
                .corner_radius(CornerRadius::same(14))
                .shadow(egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if dark { 110 } else { 45 }) })
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(w - 24.0);
                    let resp = ui.add(egui::TextEdit::singleline(&mut self.groups[g]).hint_text(tr("Nommer ce groupe")).desired_width(f32::INFINITY).margin(Margin::symmetric(9, 6)));
                    if first {
                        resp.request_focus();
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        act = Some("done");
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (k, (name, col)) in GROUP_COLORS.iter().enumerate() {
                            let (r, cresp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                            let chosen = self.group_colors.get(g).copied() == Some(k as u8);
                            ui.painter().circle_filled(r.center(), 8.5, *col);
                            if chosen {
                                ui.painter().circle_stroke(r.center(), 10.5, Stroke::new(2.0, *col));
                                ui.painter().circle_filled(r.center(), 3.0, Color32::WHITE);
                            }
                            if cresp.on_hover_cursor(egui::CursorIcon::PointingHand).tip(tr(name)).clicked() {
                                color = Some(k as u8);
                            }
                        }
                    });
                    ui.add_space(8.0);
                    ui.separator();
                    let row = |ui: &mut Ui, icon: Icon, label: &str| -> bool {
                        let (r, resp) = ui.allocate_exact_size(vec2(w - 24.0, 30.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(7), th.accent.gamma_multiply(0.14));
                        }
                        icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 14.0, r.center().y), vec2(14.0, 14.0)), icon, th.text.gamma_multiply(0.8));
                        ui.painter().text(pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.5), th.text);
                        resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                    };
                    if row(ui, Icon::Plus, tr("Nouvel onglet dans le groupe")) {
                        act = Some("newtab");
                    }
                    if row(ui, Icon::Close, tr("Dissocier")) {
                        act = Some("ungroup");
                    }
                    if row(ui, Icon::Grid, tr("Déplacer le groupe dans une nouvelle fenêtre")) {
                        act = Some("window");
                    }
                    if row(ui, Icon::Close, tr("Fermer le groupe")) {
                        act = Some("close");
                    }
                });
        });
        self.group_editing = None;
        if let Some(k) = color {
            self.group_colors[g] = k;
            self.save_session();
        }
        let outside = ctx.input(|i| i.pointer.any_pressed()) && !ctx.pointer_interact_pos().map(|p| area.response.rect.contains(p) || anchor.is_some_and(|r| r.contains(p))).unwrap_or(false);
        if outside || ctx.input(|i| i.key_pressed(Key::Escape)) {
            act = act.or(Some("done"));
        }
        match act {
            Some("done") => {
                self.group_editor = None;
                self.save_session();
            }
            Some("newtab") => {
                self.group_editor = None;
                if let Some(last) = self.visible_tabs().into_iter().rfind(|&i| self.tabs[i].group == g) {
                    self.group_collapsed[g] = false;
                    self.new_tab_right(last);
                }
            }
            Some("ungroup") => {
                self.group_editor = None;
                self.ungroup(g);
            }
            Some("window") => {
                self.group_editor = None;
                let urls: Vec<String> = self.tabs.iter().filter(|t| t.group == g && !t.private && !t.entries.is_empty()).map(|t| t.url().to_string()).collect();
                if !urls.is_empty() && urls.len() < self.tabs.len() {
                    self.new_window(urls);
                    self.close_group(g);
                }
            }
            Some("close") => {
                self.group_editor = None;
                self.close_group(g);
            }
            _ => {}
        }
    }
}
