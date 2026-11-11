//! Profiles, as in Chrome: every window shows its profile's picture (a
//! photo, an emoji or the initial on the profile's colour) at the right of
//! the toolbar; a click opens the profile menu (this profile, the others,
//! add one, a guest window, manage), and `sanaga://profiles` is the "Who's
//! using Sanaga?" page, where the window's profile is also customised.
//! Each profile keeps its own favourites, history, passwords and cookies,
//! and opens in its own windows.

use super::chrome::Fit as _;
use super::*;
use crate::icons::{self, Icon};
use crate::store::{ProfileInfo, Store};
use eframe::egui::{Align2, Color32, Margin, RichText, Sense, TextEdit};

/// The guest window's profile key (never a folder name).
const GUEST: &str = "\u{1}guest";

/// Signs offered as a profile picture (only those Sanaga's font can draw).
const SIGNS: [&str; 24] = ["★", "☀", "☂", "☕", "♥", "♣", "♠", "♦", "♪", "⚡", "⚽", "✈", "✿", "❄", "☘", "⚓", "☯", "✉", "⌂", "♞", "☎", "✂", "⛰", "☾"];

/// A profile's colour: the one chosen, or one picked from its name.
pub(super) fn profile_color(info: &ProfileInfo) -> Color32 {
    match info.color {
        1..=8 => crate::theme::PALETTE[(info.color - 1) as usize],
        _ if info.key == GUEST => Color32::from_rgb(0x6E, 0x6E, 0x73),
        _ => crate::theme::badge_color(&info.name),
    }
}

/// Start a window of another profile (or bring it to the front if it runs).
fn open_profile(key: &str) {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = std::process::Command::new(exe);
        if key == GUEST {
            cmd.arg("--guest");
        } else if !key.is_empty() {
            cmd.arg("--profile").arg(key);
        }
        let _ = cmd.spawn();
    }
}

/// A folder name for a new profile (no path characters, not taken).
fn new_key(name: &str) -> Option<String> {
    let clean: String = name.trim().chars().filter(|c| !matches!(c, '/' | '\\' | '\t' | '\n' | '\u{1}') && !c.is_control()).collect();
    let clean = clean.trim().trim_start_matches('.').to_string();
    if clean.is_empty() {
        return None;
    }
    let taken = Store::profiles();
    let mut key = clean.clone();
    let mut n = 2;
    while taken.contains(&key) {
        key = format!("{clean} {n}");
        n += 1;
    }
    Some(key)
}

impl Sanaga {
    /// This window's profile, as it looks now.
    pub(super) fn me_profile(&self) -> ProfileInfo {
        if self.guest {
            return ProfileInfo { key: GUEST.into(), name: tr("Invité").into(), color: 0, emoji: String::new(), photo: None };
        }
        let st = &self.store.settings;
        let key = self.store.profile.clone();
        let name = if !st.profile_name.trim().is_empty() {
            st.profile_name.trim().to_string()
        } else if key.is_empty() {
            tr("Personnel").to_string()
        } else {
            key.clone()
        };
        let photo = Store::profile_dir(&key).join("avatar.png");
        ProfileInfo { key, name, color: st.profile_color, emoji: st.profile_emoji.trim().to_string(), photo: photo.exists().then_some(photo) }
    }

    /// A profile's photo, loaded once (again when the file changes).
    fn avatar_texture(&mut self, path: &std::path::Path) -> Option<egui::TextureId> {
        let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
        if let Some((t, tex)) = self.avatar_tex.get(path) {
            if *t == stamp {
                return Some(tex.id());
            }
        }
        let img = image::open(path).ok()?.to_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        let tex = self.ctx.load_texture(format!("avatar-{}", path.display()), color, egui::TextureOptions::LINEAR);
        let id = tex.id();
        self.avatar_tex.insert(path.to_path_buf(), (stamp, tex));
        Some(id)
    }

    /// Paint a profile's round picture in `rect`.
    pub(super) fn paint_avatar(&mut self, p: &egui::Painter, rect: Rect, info: &ProfileInfo) {
        let c = rect.center();
        let r = rect.width().min(rect.height()) / 2.0;
        if let Some(id) = info.photo.as_deref().and_then(|ph| self.avatar_texture(ph)) {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            p.add(egui::epaint::RectShape::filled(Rect::from_center_size(c, vec2(r * 2.0, r * 2.0)), CornerRadius::same(r.round().min(255.0) as u8), Color32::WHITE).with_texture(id, uv));
            return;
        }
        let col = profile_color(info);
        // a soft gradient, as Chrome's coloured avatars
        p.circle_filled(c, r, col);
        p.circle_filled(pos2(c.x - r * 0.25, c.y - r * 0.3), r * 0.62, Color32::from_white_alpha(18));
        if info.key == GUEST {
            icons::paint(p, Rect::from_center_size(c, vec2(r * 1.1, r * 1.1)), Icon::Private, Color32::WHITE);
            return;
        }
        let sign = info.emoji.trim();
        let font = FontId::new(r * 1.05, bold());
        let drawable = !sign.is_empty() && p.fonts_mut(|f| f.has_glyphs(&font, sign));
        let text = if drawable { sign.to_string() } else { info.name.chars().next().unwrap_or('P').to_uppercase().to_string() };
        p.text(c, Align2::CENTER_CENTER, text, font, Color32::WHITE);
    }

    /// The toolbar's profile button (right side, before the ⋮ menu): the
    /// picture alone for the main profile, with the name for the others
    /// (Chrome's "Work" chip). Returns the width used.
    pub(super) fn profile_button(&mut self, ui: &mut Ui, right: f32, cy: f32, th: &Theme) -> Rect {
        let info = self.me_profile();
        let named = !info.key.is_empty();
        let label = if named { Some(ui.painter().layout_no_wrap(info.name.clone(), FontId::new(12.5, bold()), th.text)) } else { None };
        let w = 32.0 + label.as_ref().map(|g| g.size().x.min(120.0) + 12.0).unwrap_or(0.0);
        let r = Rect::from_min_size(pos2(right - w, cy - 16.0), vec2(w, 32.0));
        let resp = ui.interact(r, egui::Id::new("profile-button"), Sense::click());
        let col = profile_color(&info);
        if named {
            ui.painter().rect_filled(r.shrink2(vec2(0.0, 2.0)), CornerRadius::same(14), col.gamma_multiply(if resp.hovered() || self.profile_open { 0.30 } else { 0.18 }));
        } else if resp.hovered() || self.profile_open {
            ui.painter().rect_filled(r.shrink(2.0), CornerRadius::same(10), th.omnibox_hover);
        }
        let av = Rect::from_center_size(pos2(r.left() + 16.0, cy), vec2(22.0, 22.0));
        let painter = ui.painter().clone();
        self.paint_avatar(&painter, av, &info);
        if let Some(g) = label {
            let clip = Rect::from_min_max(pos2(av.right() + 6.0, r.top()), pos2(r.right() - 8.0, r.bottom()));
            ui.painter().with_clip_rect(clip).galley(pos2(clip.left(), cy - g.size().y / 2.0), g, th.text);
        }
        if resp.tip(tf("{} — profils", &[&info.name])).clicked() {
            self.profile_open = !self.profile_open;
            self.menu_open = false;
            self.overflow_open = false;
        }
        self.profile_rect = r;
        r
    }

    /// The profile menu (Chrome's): this profile, the others, add, guest, manage.
    pub(super) fn profile_menu(&mut self, ctx: &egui::Context, th: &Theme) {
        if !self.profile_open {
            return;
        }
        let w = 320.0;
        let me = self.me_profile();
        let others: Vec<ProfileInfo> = Store::all_profiles().into_iter().filter(|p| p.key != me.key).collect();
        let (pos, pivot, _) = super::layout::popover_place(ctx, self.profile_rect, (self.profile_rect.right() - w).max(8.0), 6.0);
        let dark = self.store.settings.dark;
        let col = profile_color(&me);
        let mut act: Option<(&'static str, String)> = None;
        let area = egui::Area::new(egui::Id::new("profile-menu")).order(egui::Order::Foreground).pivot(pivot).fixed_pos(pos).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.hairline.gamma_multiply(0.6)))
                .corner_radius(CornerRadius::same(16))
                .shadow(egui::Shadow { offset: [0, 10], blur: 32, spread: 0, color: Color32::from_black_alpha(if dark { 120 } else { 50 }) })
                .inner_margin(Margin::ZERO)
                .show(ui, |ui| {
                    ui.set_width(w);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    // banner in the profile's colour, the picture over its edge
                    let (banner, _) = ui.allocate_exact_size(vec2(w, 64.0), Sense::hover());
                    let top_round = CornerRadius { nw: 16, ne: 16, sw: 0, se: 0 };
                    ui.painter().rect_filled(banner, top_round, col.gamma_multiply(0.85));
                    // light from the top right, as on a card
                    let clip = ui.painter().with_clip_rect(banner);
                    clip.circle_filled(pos2(banner.right() - 40.0, banner.top() - 10.0), 90.0, Color32::from_white_alpha(28));
                    clip.circle_filled(pos2(banner.left() + 30.0, banner.bottom() + 30.0), 70.0, Color32::from_black_alpha(18));
                    let (head, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
                    let ac = pos2(head.center().x, banner.bottom());
                    ui.painter().circle_filled(ac, 36.0, th.page);
                    let p = ui.painter().clone();
                    self.paint_avatar(&p, Rect::from_center_size(ac, vec2(64.0, 64.0)), &me);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(&me.name).size(16.5).family(bold()).color(th.text));
                        ui.add_space(2.0);
                        let sub = if me.key == GUEST { tr("Rien n'est gardé après la fermeture de la fenêtre") } else { tr("Favoris, historique, mots de passe et cookies à part") };
                        ui.label(RichText::new(sub).size(12.0).color(th.sub));
                        ui.add_space(10.0);
                        if me.key != GUEST {
                            let b = egui::Button::new(RichText::new(tr("Personnaliser le profil")).size(13.0).color(th.accent)).fill(th.accent.gamma_multiply(0.12)).corner_radius(CornerRadius::same(14)).min_size(vec2(200.0, 30.0));
                            if ui.add(b).clicked() {
                                act = Some(("customize", String::new()));
                            }
                        }
                    });
                    ui.add_space(12.0);
                    let (sep, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
                    ui.painter().rect_filled(sep, 0.0, th.hairline.gamma_multiply(0.6));
                    ui.add_space(6.0);
                    let row = |ui: &mut Ui, this: &mut Sanaga, avatar: Option<&ProfileInfo>, icon: Icon, label: &str| -> bool {
                        let (r, resp) = ui.allocate_exact_size(vec2(w, 38.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r.shrink2(vec2(6.0, 1.0)), CornerRadius::same(8), th.omnibox_hover);
                        }
                        let ic = Rect::from_center_size(pos2(r.left() + 30.0, r.center().y), vec2(26.0, 26.0));
                        match avatar {
                            Some(info) => {
                                let p = ui.painter().clone();
                                this.paint_avatar(&p, ic, info);
                            }
                            None => {
                                ui.painter().circle_filled(ic.center(), 13.0, th.omnibox);
                                icons::paint(ui.painter(), ic.shrink(6.0), icon, th.text);
                            }
                        }
                        ui.painter().text(pos2(ic.right() + 12.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.5), th.text);
                        resp.clicked()
                    };
                    if !others.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add_space(18.0);
                            ui.label(RichText::new(tr("Autres profils")).size(11.5).family(bold()).color(th.sub));
                        });
                        ui.add_space(4.0);
                        for o in &others {
                            if row(ui, self, Some(o), Icon::Plus, &o.name) {
                                act = Some(("open", o.key.clone()));
                            }
                        }
                        ui.add_space(4.0);
                        let (sep, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
                        ui.painter().rect_filled(sep.shrink2(vec2(14.0, 0.0)), 0.0, th.hairline.gamma_multiply(0.5));
                        ui.add_space(4.0);
                    }
                    if row(ui, self, None, Icon::Plus, tr("Ajouter un profil")) {
                        act = Some(("add", String::new()));
                    }
                    if me.key != GUEST && row(ui, self, None, Icon::Private, tr("Ouvrir une fenêtre Invité")) {
                        act = Some(("open", GUEST.into()));
                    }
                    if row(ui, self, None, Icon::Grid, tr("Gérer les profils")) {
                        act = Some(("manage", String::new()));
                    }
                    if row(ui, self, None, Icon::Close, &tf("Fermer ce profil ({})", &[&me.name])) {
                        act = Some(("close", String::new()));
                    }
                    ui.add_space(6.0);
                });
        });
        let anchor = self.profile_rect;
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if esc || (ctx.input(|i| i.pointer.any_pressed()) && !ctx.pointer_interact_pos().is_some_and(|p| area.response.rect.contains(p) || anchor.contains(p))) {
            self.profile_open = false;
        }
        let Some((what, key)) = act else { return };
        self.profile_open = false;
        let a = self.active;
        match what {
            "open" => open_profile(&key),
            "customize" => {
                self.profile_section = 1;
                self.navigate(a, "sanaga://profiles".into(), None, true);
            }
            "add" => {
                self.profile_section = 2;
                self.navigate(a, "sanaga://profiles".into(), None, true);
            }
            "manage" => {
                self.profile_section = 0;
                self.navigate(a, "sanaga://profiles".into(), None, true);
            }
            "close" => self.run_shortcut("quit"),
            _ => {}
        }
    }

    /// `sanaga://profiles`: "Who's using Sanaga?" — every profile as a card
    /// (click: its window), adding one, the guest window, and this window's
    /// profile customised (name, colour, picture).
    pub(super) fn page_profiles(&mut self, ui: &mut Ui, th: &Theme) {
        let me = self.me_profile();
        let all = Store::all_profiles();
        let mut open: Option<String> = None;
        let mut delete: Option<String> = None;
        super::pages::centered_column(ui, |ui| {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(tr("Qui utilise Sanaga ?")).size(28.0).family(bold()).color(th.text));
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Chaque profil a ses favoris, son historique, ses mots de passe et ses cookies, et s'ouvre dans ses propres fenêtres.")).size(13.0).color(th.sub));
            });
            ui.add_space(24.0);
            // the cards, centred, as many per row as fit
            let card = vec2(150.0, 172.0);
            let gap = 16.0;
            let n = all.len() + 1;
            let per_row = (((ui.available_width() + gap) / (card.x + gap)).floor() as usize).clamp(1, 5);
            let rows = n.div_ceil(per_row);
            for row in 0..rows {
                let count = (n - row * per_row).min(per_row);
                let total = count as f32 * card.x + (count as f32 - 1.0) * gap;
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
                    ui.spacing_mut().item_spacing.x = gap;
                    for k in row * per_row..row * per_row + count {
                        let (r, resp) = ui.allocate_exact_size(card, Sense::click());
                        let hover = resp.hovered();
                        if k < all.len() {
                            let info = &all[k];
                            let mine = info.key == me.key;
                            ui.painter().rect_filled(r, CornerRadius::same(18), if hover { th.omnibox_hover } else { th.card });
                            if mine {
                                ui.painter().rect_stroke(r, CornerRadius::same(18), Stroke::new(2.0, th.accent), egui::StrokeKind::Inside);
                            }
                            let shown = if mine { me.clone() } else { info.clone() };
                            let p = ui.painter().clone();
                            self.paint_avatar(&p, Rect::from_center_size(pos2(r.center().x, r.top() + 64.0), vec2(84.0, 84.0)), &shown);
                            let clip = r.shrink2(vec2(10.0, 0.0));
                            ui.painter().with_clip_rect(clip).fit(pos2(r.center().x, r.top() + 124.0), Align2::CENTER_CENTER, &shown.name, FontId::new(14.5, bold()), th.text);
                            let sub = if mine { tr("Cette fenêtre") } else { tr("Ouvrir") };
                            ui.painter().text(pos2(r.center().x, r.top() + 146.0), Align2::CENTER_CENTER, sub, FontId::proportional(12.0), if mine { th.accent } else { th.sub });
                            // delete (not the main profile, not this window's)
                            if !mine && !info.key.is_empty() && hover {
                                let x = Rect::from_center_size(pos2(r.right() - 18.0, r.top() + 18.0), vec2(22.0, 22.0));
                                let xr = ui.interact(x, egui::Id::new(("profile-del", k)), Sense::click());
                                ui.painter().circle_filled(x.center(), 11.0, if xr.hovered() { Color32::from_rgb(0xD7, 0x3A, 0x31) } else { th.omnibox });
                                icons::paint(ui.painter(), x.shrink(6.0), Icon::Close, if xr.hovered() { Color32::WHITE } else { th.sub });
                                if xr.on_hover_text(tr("Supprimer ce profil")).clicked() {
                                    delete = Some(info.key.clone());
                                    continue;
                                }
                            }
                            if resp.clicked() {
                                if mine {
                                    self.profile_section = 1;
                                } else {
                                    open = Some(info.key.clone());
                                }
                            }
                        } else {
                            // add a profile
                            ui.painter().rect_stroke(r.shrink(1.0), CornerRadius::same(18), Stroke::new(1.5, if hover { th.accent } else { th.border }), egui::StrokeKind::Inside);
                            let c = pos2(r.center().x, r.top() + 64.0);
                            ui.painter().circle_filled(c, 42.0, if hover { th.accent.gamma_multiply(0.15) } else { th.omnibox });
                            icons::paint(ui.painter(), Rect::from_center_size(c, vec2(30.0, 30.0)), Icon::Plus, if hover { th.accent } else { th.sub });
                            ui.painter().text(pos2(r.center().x, r.top() + 124.0), Align2::CENTER_CENTER, tr("Ajouter"), FontId::new(14.5, bold()), th.text);
                            if resp.clicked() {
                                self.profile_section = 2;
                            }
                        }
                    }
                });
                ui.add_space(gap);
            }
            ui.add_space(4.0);
            ui.vertical_centered(|ui| {
                if !self.guest && ui.add(egui::Button::new(RichText::new(tr("Ouvrir une fenêtre Invité")).size(13.0).color(th.accent)).frame(false)).clicked() {
                    open = Some(GUEST.into());
                }
            });
            ui.add_space(18.0);

            // confirming a deletion
            if let Some(key) = self.profile_delete.clone() {
                let name = Store::profile_info(&key).name;
                section(ui, th, |ui| {
                    ui.label(RichText::new(tf("Supprimer le profil « {} » ?", &[&name])).size(15.0).family(bold()).color(th.text));
                    ui.add_space(4.0);
                    ui.label(RichText::new(tr("Ses favoris, son historique, ses mots de passe enregistrés dans Sanaga et ses cookies seront effacés de cet ordinateur. Fermez d'abord ses fenêtres.")).size(12.5).color(th.sub));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let b = egui::Button::new(RichText::new(tr("Supprimer")).color(Color32::WHITE)).fill(Color32::from_rgb(0xD7, 0x3A, 0x31));
                        if ui.add(b).clicked() {
                            let dir = Store::profile_dir(&key);
                            // (only a folder of the profiles directory)
                            if !key.is_empty() && dir.parent() == Some(Store::root_dir().join("profils").as_path()) {
                                let _ = std::fs::remove_dir_all(&dir);
                            }
                            self.profile_delete = None;
                        }
                        if ui.button(tr("Annuler")).clicked() {
                            self.profile_delete = None;
                        }
                    });
                });
                ui.add_space(14.0);
            }

            // adding a profile
            if self.profile_section == 2 {
                let mut create = false;
                section(ui, th, |ui| {
                    ui.label(RichText::new(tr("Nouveau profil")).size(15.0).family(bold()).color(th.text));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let resp = ui.add(TextEdit::singleline(&mut self.new_profile_name).hint_text(tr("Nom (ex. Travail, École)")).desired_width(260.0));
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            create = true;
                        }
                        if !resp.has_focus() && self.new_profile_name.is_empty() {
                            resp.request_focus();
                        }
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("Couleur")).size(12.5).color(th.sub));
                        for c in 1..=8u8 {
                            let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                            ui.painter().circle_filled(r.center(), 10.0, crate::theme::PALETTE[(c - 1) as usize]);
                            if self.new_profile_color == c {
                                ui.painter().circle_stroke(r.center(), 12.0, Stroke::new(2.0, th.text));
                            }
                            if resp.clicked() {
                                self.new_profile_color = c;
                            }
                        }
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let b = egui::Button::new(RichText::new(tr("Créer et ouvrir")).color(Color32::WHITE)).fill(th.accent);
                        if ui.add(b).clicked() {
                            create = true;
                        }
                        if ui.button(tr("Annuler")).clicked() {
                            self.profile_section = 0;
                            self.new_profile_name.clear();
                        }
                    });
                });
                if create {
                    if let Some(key) = new_key(&self.new_profile_name) {
                        let dir = Store::profile_dir(&key);
                        // (the person's data: closed to other users)
                        crate::store::ensure_private_dir(&dir);
                        // its name, colour and this window's language and look
                        let st = &self.store.settings;
                        let first = format!(
                            "language={}\ntheme={}\naccent={}\nprofile_name={}\nprofile_color={}\n",
                            st.language,
                            st.theme,
                            st.accent,
                            self.new_profile_name.trim().replace('\n', " "),
                            if self.new_profile_color == 0 { 1 } else { self.new_profile_color }
                        );
                        if !dir.join("settings.txt").exists() {
                            let _ = crate::store::write_atomic(&dir.join("settings.txt"), first);
                        }
                        self.new_profile_name.clear();
                        self.profile_section = 0;
                        open = Some(key);
                    }
                }
                ui.add_space(14.0);
            }

            // this window's profile
            if me.key != GUEST {
                self.customize_section(ui, th, &me);
            }
        });
        if let Some(k) = delete {
            self.profile_delete = Some(k);
        }
        if let Some(k) = open {
            open_profile(&k);
        }
    }

    /// Name, colour and picture of this window's profile.
    fn customize_section(&mut self, ui: &mut Ui, th: &Theme, me: &ProfileInfo) {
        let mut changed = false;
        let mut pick_photo = false;
        let mut drop_photo = false;
        let highlight = self.profile_section == 1;
        section(ui, th, |ui| {
            if highlight {
                ui.painter().rect_stroke(ui.max_rect().expand(16.0), CornerRadius::same(16), Stroke::new(2.0, th.accent), egui::StrokeKind::Inside);
            }
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
                let p = ui.painter().clone();
                self.paint_avatar(&p, r, me);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr("Personnaliser ce profil")).size(15.0).family(bold()).color(th.text));
                    ui.add_space(6.0);
                    let hint = if me.key.is_empty() { tr("Personnel") } else { me.key.as_str() };
                    if ui.add(TextEdit::singleline(&mut self.store.settings.profile_name).hint_text(hint).desired_width(240.0).char_limit(40)).changed() {
                        changed = true;
                    }
                });
            });
            ui.add_space(12.0);
            ui.label(RichText::new(tr("Couleur")).size(12.5).color(th.sub));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // automatic (from the name), then the palette
                let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                ui.painter().circle_stroke(r.center(), 11.0, Stroke::new(1.5, th.sub));
                ui.painter().text(r.center(), Align2::CENTER_CENTER, "A", FontId::new(11.0, bold()), th.sub);
                if self.store.settings.profile_color == 0 {
                    ui.painter().circle_stroke(r.center(), 13.5, Stroke::new(2.0, th.text));
                }
                if resp.on_hover_text(tr("Automatique")).clicked() {
                    self.store.settings.profile_color = 0;
                    changed = true;
                }
                for c in 1..=8u8 {
                    let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                    ui.painter().circle_filled(r.center(), 11.0, crate::theme::PALETTE[(c - 1) as usize]);
                    if self.store.settings.profile_color == c {
                        ui.painter().circle_stroke(r.center(), 13.5, Stroke::new(2.0, th.text));
                    }
                    if resp.clicked() {
                        self.store.settings.profile_color = c;
                        changed = true;
                    }
                }
            });
            ui.add_space(12.0);
            ui.label(RichText::new(tr("Image du profil")).size(12.5).color(th.sub));
            ui.add_space(4.0);
            let font = FontId::new(15.0, bold());
            let signs: Vec<&str> = SIGNS.iter().copied().filter(|s| ui.painter().fonts_mut(|f| f.has_glyphs(&font, s))).collect();
            ui.horizontal_wrapped(|ui| {
                // the initial
                let (r, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                ui.painter().circle_filled(r.center(), 15.0, profile_color(me));
                ui.painter().text(r.center(), Align2::CENTER_CENTER, me.name.chars().next().unwrap_or('P').to_uppercase().to_string(), font.clone(), Color32::WHITE);
                if self.store.settings.profile_emoji.trim().is_empty() && me.photo.is_none() {
                    ui.painter().circle_stroke(r.center(), 16.5, Stroke::new(2.0, th.text));
                }
                if resp.on_hover_text(tr("Initiale")).clicked() {
                    self.store.settings.profile_emoji.clear();
                    drop_photo = me.photo.is_some();
                    changed = true;
                }
                for s in &signs {
                    let (r, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                    let on = self.store.settings.profile_emoji.trim() == *s && me.photo.is_none();
                    ui.painter().circle_filled(r.center(), 15.0, if on { profile_color(me) } else { th.omnibox });
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, *s, font.clone(), if on { Color32::WHITE } else { th.text });
                    if resp.clicked() {
                        self.store.settings.profile_emoji = s.to_string();
                        drop_photo = me.photo.is_some();
                        changed = true;
                    }
                }
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Choisir une photo…")).clicked() {
                    pick_photo = true;
                }
                if me.photo.is_some() && ui.button(tr("Retirer la photo")).clicked() {
                    drop_photo = true;
                }
            });
        });
        let dir = Store::profile_dir(&me.key);
        if drop_photo {
            let _ = std::fs::remove_file(dir.join("avatar.png"));
            self.avatar_tex.clear();
        }
        if pick_photo {
            if let Some(path) = crate::shell::choose_file(tr("Photo du profil"), &["*.png", "*.jpg", "*.jpeg", "*.webp", "*.gif", "*.bmp"], tr("Images"), None) {
                match image::open(&path) {
                    Ok(img) => {
                        // a centred square, 256 pixels
                        let s = img.width().min(img.height());
                        let sq = img.crop_imm((img.width() - s) / 2, (img.height() - s) / 2, s, s).resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
                        // (kept like the profile's other files: 0600)
                        let mut png = std::io::Cursor::new(Vec::new());
                        if sq.to_rgba8().write_to(&mut png, image::ImageFormat::Png).is_ok() && crate::store::write_atomic(&dir.join("avatar.png"), png.into_inner()).is_ok() {
                            self.avatar_tex.clear();
                        }
                    }
                    Err(_) => self.notice = Some((tr("Cette image ne peut pas être lue.").into(), self.ctx.input(|i| i.time))),
                }
            }
        }
        if changed {
            self.store.save_settings();
        }
    }
}

/// A rounded panel of the profiles page.
fn section(ui: &mut Ui, th: &Theme, add: impl FnOnce(&mut Ui)) {
    Frame::new().fill(th.card).corner_radius(CornerRadius::same(16)).inner_margin(Margin::same(18)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn profile_names_are_safe_folders() {
        assert_eq!(super::new_key("  ../Zq-sanaga-test/ "), Some("Zq-sanaga-test".to_string()));
        assert_eq!(super::new_key("zq/b\\c"), Some("zqbc".to_string()));
        assert_eq!(super::new_key("   "), None);
    }
}
