//! The Settings page, built like macOS System Settings: a sidebar with a
//! search field and the categories (each with its coloured icon), back /
//! forward arrows and the category's title above the content, and the
//! settings in rounded groups of rows — the label on the left, the control on
//! the right (switches, pop-up menus, segmented controls), a hairline between
//! rows and a note under the group. ↑ / ↓ change category, Escape clears the
//! search.

use super::*;
use crate::icons::{self, Icon};
use crate::store;
use eframe::egui::{Align, Align2, Color32, CursorIcon, Layout, Margin, Pos2, RichText, ScrollArea, Sense, TextEdit, UiBuilder};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Section {
    General,
    Appearance,
    Privacy,
    Passwords,
    Performance,
    Sync,
    Extensions,
    Intelligence,
}

impl Section {
    const ALL: [Section; 8] = [Section::General, Section::Appearance, Section::Intelligence, Section::Privacy, Section::Passwords, Section::Performance, Section::Sync, Section::Extensions];

    fn label(self) -> &'static str {
        match self {
            Section::General => tr("Général"),
            Section::Appearance => tr("Apparence"),
            Section::Privacy => tr("Confidentialité et sécurité"),
            Section::Passwords => tr("Mots de passe"),
            Section::Performance => tr("Performances"),
            Section::Sync => tr("Synchronisation et profils"),
            Section::Extensions => tr("Extensions"),
            Section::Intelligence => tr("Intelligence"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            Section::General => Icon::Home,
            Section::Appearance => Icon::Palette,
            Section::Privacy => Icon::Shield,
            Section::Passwords => Icon::Lock,
            Section::Performance => Icon::Leaf,
            Section::Sync => Icon::Rotate,
            Section::Extensions => Icon::Puzzle,
            Section::Intelligence => Icon::Sparkle,
        }
    }

    /// The colour of the category's icon tile (as in macOS System Settings).
    fn tile(self) -> Color32 {
        match self {
            Section::General => Color32::from_rgb(0x8F, 0x8D, 0x88),
            Section::Appearance => Color32::from_rgb(0x1F, 0x5F, 0xCC),
            Section::Intelligence => Color32::from_rgb(0x7A, 0x3F, 0xC4),
            Section::Privacy => Color32::from_rgb(0x2A, 0x6C, 0xDF),
            Section::Passwords => Color32::from_rgb(0x5C, 0x61, 0x6B),
            Section::Performance => Color32::from_rgb(0xE0, 0x6A, 0x12),
            Section::Sync => Color32::from_rgb(0x2E, 0x80, 0x49),
            Section::Extensions => Color32::from_rgb(0x12, 0x8F, 0x86),
        }
    }
}

/// Is a group shown? Its category is the chosen one, or, while searching, its
/// title or one of its labels contains the search.
struct Shown<'a> {
    section: Section,
    query: &'a str,
}

impl Shown<'_> {
    fn shows(&self, sec: Section, title: &str, labels: &[&str]) -> bool {
        if self.query.is_empty() {
            return sec == self.section;
        }
        let q = self.query;
        title.to_lowercase().contains(q) || sec.label().to_lowercase().contains(q) || labels.iter().any(|l| l.to_lowercase().contains(q))
    }
}

// ------------------------------------------------------------------ controls

/// Rows of one group: counts them to draw the hairline between two.
struct Rows<'t> {
    th: &'t Theme,
    n: usize,
}

const ROW_PAD: f32 = 14.0;

/// A rounded group of rows, with an optional title above it.
fn group(ui: &mut Ui, th: &Theme, title: &str, add: impl FnOnce(&mut Ui, &mut Rows)) {
    if !title.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(title).size(13.0).family(bold()).color(th.text));
        ui.add_space(6.0);
    }
    Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::symmetric(0, 2)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut rows = Rows { th, n: 0 };
        add(ui, &mut rows);
    });
    ui.add_space(6.0);
}

/// A note under a group (secondary text).
fn footnote(ui: &mut Ui, th: &Theme, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(ROW_PAD);
        ui.add(egui::Label::new(RichText::new(text).size(11.5).color(th.sub)).wrap());
    });
    ui.add_space(14.0);
}

impl Rows<'_> {
    /// One row: label (and a line of explanation) on the left, `control` on
    /// the right. `click`: the whole row is clickable (returns the response).
    fn row(&mut self, ui: &mut Ui, label: &str, sub: &str, click: bool, control: impl FnOnce(&mut Ui)) -> egui::Response {
        let th = self.th;
        let w = ui.available_width();
        let h = if sub.is_empty() { 40.0 } else { 54.0 };
        let (r, resp) = ui.allocate_exact_size(vec2(w, h), if click { Sense::click() } else { Sense::hover() });
        if self.n > 0 {
            ui.painter().line_segment([pos2(r.left() + ROW_PAD, r.top()), pos2(r.right() - ROW_PAD, r.top())], Stroke::new(1.0, th.hairline.gamma_multiply(0.6)));
        }
        self.n += 1;
        if click && resp.hovered() {
            ui.painter().rect_filled(r.shrink2(vec2(4.0, 2.0)), CornerRadius::same(8), th.track.gamma_multiply(0.6));
        }
        let label_w = (w * 0.55).max(160.0);
        let clip = Rect::from_min_max(pos2(r.left() + ROW_PAD, r.top()), pos2(r.left() + label_w, r.bottom()));
        let p = ui.painter().with_clip_rect(clip);
        if sub.is_empty() {
            p.text(pos2(clip.left(), r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.5), th.text);
        } else {
            p.text(pos2(clip.left(), r.top() + 18.0), Align2::LEFT_CENTER, label, FontId::proportional(13.5), th.text);
            p.text(pos2(clip.left(), r.top() + 37.0), Align2::LEFT_CENTER, sub, FontId::proportional(11.5), th.sub);
        }
        let ctl = Rect::from_min_max(pos2(r.left() + label_w * 0.6, r.top()), pos2(r.right() - ROW_PAD, r.bottom()));
        let mut child = ui.new_child(UiBuilder::new().max_rect(ctl).layout(Layout::right_to_left(Align::Center)));
        control(&mut child);
        if click { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp }
    }

    /// A row with a switch on the right; true when it changed.
    fn switch(&mut self, ui: &mut Ui, label: &str, sub: &str, on: &mut bool) -> bool {
        let th = self.th;
        let mut changed = false;
        let resp = self.row(ui, label, sub, true, |ui| {
            changed |= switch(ui, th, on);
        });
        if resp.clicked() && !changed {
            *on = !*on;
            changed = true;
        }
        changed
    }

    /// A row opening another page or panel (chevron on the right).
    fn link(&mut self, ui: &mut Ui, label: &str, sub: &str, value: &str) -> bool {
        let th = self.th;
        self.row(ui, label, sub, true, |ui| {
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 12.0), Sense::hover());
            chevron(ui.painter(), r.center(), th.sub);
            if !value.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(value).size(13.0).color(th.sub));
            }
        })
        .clicked()
    }

    /// A row of a list where one item is chosen (a check mark on the right).
    fn choice(&mut self, ui: &mut Ui, label: &str, sub: &str, chosen: bool) -> bool {
        let th = self.th;
        self.row(ui, label, sub, true, |ui| {
            if chosen {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                check(ui.painter(), r.center(), th.accent);
            }
        })
        .clicked()
    }
}

/// The macOS switch: a capsule, the accent colour when on, a white knob.
fn switch(ui: &mut Ui, th: &Theme, on: &mut bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(38.0, 22.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.14);
    let off = th.track.gamma_multiply(1.4);
    let col = lerp(off, th.accent, t);
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same(11), col);
    let x = r.left() + 11.0 + t * (r.width() - 22.0);
    p.circle_filled(pos2(x, r.center().y + 0.6), 9.4, Color32::from_black_alpha(40));
    p.circle_filled(pos2(x, r.center().y), 9.0, Color32::WHITE);
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    if resp.clicked() {
        *on = !*on;
        return true;
    }
    false
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

/// A segmented control (the chosen part a light pill); the index clicked.
fn segmented(ui: &mut Ui, th: &Theme, options: &[&str], chosen: usize) -> Option<usize> {
    let font = FontId::proportional(12.5);
    let widths: Vec<f32> = options.iter().map(|o| ui.painter().layout_no_wrap(o.to_string(), font.clone(), th.text).size().x + 26.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 4.0;
    let (r, _) = ui.allocate_exact_size(vec2(total, 26.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(13), th.track);
    let mut x = r.left() + 2.0;
    let mut out = None;
    for (k, (o, w)) in options.iter().zip(&widths).enumerate() {
        let cell = Rect::from_min_size(pos2(x, r.top() + 2.0), vec2(*w, r.height() - 4.0));
        let resp = ui.interact(cell, ui.id().with(("seg", o, k)), Sense::click());
        if k == chosen {
            ui.painter().add(egui::Shadow { offset: [0, 1], blur: 3, spread: 0, color: Color32::from_black_alpha(25) }.as_shape(cell, CornerRadius::same(11)));
            ui.painter().rect_filled(cell, CornerRadius::same(11), th.pill);
        } else if resp.hovered() {
            ui.painter().rect_filled(cell, CornerRadius::same(11), th.pill.gamma_multiply(0.4));
        }
        ui.painter().text(cell.center(), Align2::CENTER_CENTER, *o, font.clone(), th.text);
        if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() && k != chosen {
            out = Some(k);
        }
        x += w;
    }
    out
}

/// A pop-up menu button on the right of a row (macOS: the value and ⌃⌄).
fn popup<T: PartialEq + Copy>(ui: &mut Ui, id: &str, current: T, options: &[(T, &str)]) -> Option<T> {
    let label = options.iter().find(|o| o.0 == current).map(|o| o.1).unwrap_or("");
    let mut out = None;
    egui::ComboBox::from_id_salt(id).selected_text(label).width(220.0).show_ui(ui, |ui| {
        for (v, l) in options {
            if ui.selectable_label(*v == current, *l).clicked() && *v != current {
                out = Some(*v);
            }
        }
    });
    out
}

fn chevron(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    p.line_segment([pos2(c.x - 2.5, c.y - 5.0), pos2(c.x + 2.5, c.y)], s);
    p.line_segment([pos2(c.x + 2.5, c.y), pos2(c.x - 2.5, c.y + 5.0)], s);
}

fn check(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(2.0, col);
    p.line_segment([pos2(c.x - 5.0, c.y), pos2(c.x - 1.5, c.y + 4.0)], s);
    p.line_segment([pos2(c.x - 1.5, c.y + 4.0), pos2(c.x + 5.5, c.y - 4.5)], s);
}

/// A field on the right of a row.
fn field(ui: &mut Ui, value: &mut String, hint: &str, width: f32, password: bool) -> bool {
    ui.add(TextEdit::singleline(value).hint_text(hint).desired_width(width).password(password)).changed()
}

/// A choice drawn as a small picture of the window (where the tabs and the
/// address bar go). `tabs`: 0 top, 1 bottom, 2 left, 3 right, 9 not shown;
/// `bar`: 0 top, 1 bottom, 2 floating, 9 not shown.
fn layout_choice(ui: &mut Ui, th: &Theme, label: &str, tabs: u8, bar: u8, selected: bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(100.0, 88.0), Sense::click());
    let pic = Rect::from_min_size(r.min + vec2(4.0, 4.0), vec2(92.0, 58.0));
    let p = ui.painter();
    p.rect_filled(pic, CornerRadius::same(8), th.page);
    p.rect_stroke(pic, CornerRadius::same(8), Stroke::new(if selected { 2.5 } else { 1.0 }, if selected { th.accent } else if resp.hovered() { th.sub } else { th.border }), egui::StrokeKind::Outside);
    let tabs_col = th.accent.gamma_multiply(0.55);
    let bar_col = th.sub.gamma_multiply(0.55);
    let mut area = pic.shrink(5.0);
    match tabs {
        0 => {
            p.rect_filled(Rect::from_min_size(area.min, vec2(area.width(), 6.0)), 3.0, tabs_col);
            area.min.y += 8.0;
        }
        1 => {
            p.rect_filled(Rect::from_min_max(pos2(area.left(), area.bottom() - 6.0), area.max), 3.0, tabs_col);
            area.max.y -= 8.0;
        }
        2 => {
            p.rect_filled(Rect::from_min_size(area.min, vec2(20.0, area.height())), 3.0, tabs_col);
            area.min.x += 23.0;
        }
        3 => {
            p.rect_filled(Rect::from_min_max(pos2(area.right() - 20.0, area.top()), area.max), 3.0, tabs_col);
            area.max.x -= 23.0;
        }
        _ => {}
    }
    match bar {
        0 => {
            p.rect_filled(Rect::from_min_size(area.min, vec2(area.width(), 7.0)), 3.5, bar_col);
        }
        1 => {
            p.rect_filled(Rect::from_min_max(pos2(area.left(), area.bottom() - 7.0), area.max), 3.5, bar_col);
        }
        2 => {
            p.rect_filled(Rect::from_center_size(pos2(area.center().x, area.bottom() - 6.0), vec2(area.width() * 0.72, 8.0)), 4.0, bar_col);
        }
        _ => {}
    }
    p.text(pos2(r.center().x, r.bottom() - 10.0), Align2::CENTER_CENTER, label, FontId::proportional(12.0), if selected { th.accent } else { th.text });
    resp.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// Is Sanaga the system's default web browser? (`xdg-settings`)
fn default_browser_is_sanaga() -> bool {
    std::process::Command::new("xdg-settings")
        .args(["get", "default-web-browser"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "sanaga.desktop")
        .unwrap_or(false)
}

fn launcher_installed() -> bool {
    let data = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".local/share")));
    let mut dirs: Vec<std::path::PathBuf> = data.into_iter().collect();
    dirs.extend(std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into()).split(':').map(std::path::PathBuf::from));
    dirs.iter().any(|d| d.join("applications/sanaga.desktop").is_file())
}

/// Something a row asks for, done once the drawing is over.
enum Act {
    Go(&'static str),
    MakeDefault,
    ClearHistory,
    SyncNow,
    SaveKey,
    ForgetKey,
    FindModels,
}

const SIDEBAR_W: f32 = 236.0;

impl Sanaga {
    /// Change category, remembering the way back (the ‹ › arrows).
    fn settings_go(&mut self, sec: Section) {
        if sec != self.settings_section {
            self.settings_back.push(self.settings_section);
            self.settings_fwd.clear();
            self.settings_section = sec;
        }
        self.settings_filter.clear();
    }

    pub(super) fn page_settings(&mut self, ui: &mut Ui, th: &Theme) {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, th.page);

        // keyboard: ↑ / ↓ change category (when no field has the keys), Escape clears the search
        let typing = ui.ctx().memory(|m| m.focused().is_some());
        let (up, down, esc) = ui.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Escape)));
        if !typing && (up || down) {
            let k = Section::ALL.iter().position(|s| *s == self.settings_section).unwrap_or(0);
            let n = Section::ALL.len();
            let next = if down { (k + 1) % n } else { (k + n - 1) % n };
            self.settings_go(Section::ALL[next]);
        }
        if esc && !self.settings_filter.is_empty() {
            self.settings_filter.clear();
        }

        // ---------------- sidebar ----------------
        let wide = full.width() >= 760.0;
        let side = Rect::from_min_size(full.min, vec2(if wide { SIDEBAR_W } else { 0.0 }, full.height()));
        let mut picked: Option<Section> = None;
        let mut about = false;
        if wide {
            ui.painter().rect_filled(side, 0.0, th.sidebar);
            ui.painter().line_segment([side.right_top(), side.right_bottom()], Stroke::new(1.0, th.hairline.gamma_multiply(0.5)));
            let mut s = ui.new_child(UiBuilder::new().max_rect(side.shrink2(vec2(10.0, 12.0))).layout(Layout::top_down(Align::Min)));
            self.settings_search(&mut s, th);
            s.add_space(10.0);
            let searching = !self.settings_filter.trim().is_empty();
            for sec in Section::ALL {
                let sel = !searching && sec == self.settings_section;
                let (r, resp) = s.allocate_exact_size(vec2(s.available_width(), 32.0), Sense::click());
                if sel {
                    s.painter().rect_filled(r, CornerRadius::same(8), th.accent);
                } else if resp.hovered() {
                    s.painter().rect_filled(r, CornerRadius::same(8), th.track.gamma_multiply(0.7));
                }
                let tile = Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), vec2(22.0, 22.0));
                s.painter().rect_filled(tile, CornerRadius::same(6), sec.tile());
                icons::paint(s.painter(), tile.shrink(5.0), sec.icon(), Color32::WHITE);
                let col = if sel { Color32::WHITE } else { th.text };
                let clip = Rect::from_min_max(pos2(r.left() + 36.0, r.top()), pos2(r.right() - 6.0, r.bottom()));
                s.painter().with_clip_rect(clip).text(pos2(clip.left(), r.center().y), Align2::LEFT_CENTER, sec.label(), FontId::proportional(13.0), col);
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    picked = Some(sec);
                }
            }
            // at the bottom: about Sanaga
            let ar = Rect::from_min_size(pos2(side.left() + 10.0, side.bottom() - 44.0), vec2(side.width() - 20.0, 32.0));
            let aresp = ui.interact(ar, egui::Id::new("settings-about"), Sense::click());
            if aresp.hovered() {
                ui.painter().rect_filled(ar, CornerRadius::same(8), th.track.gamma_multiply(0.7));
            }
            icons::logo(ui.painter(), pos2(ar.left() + 18.0, ar.center().y), 11.0);
            ui.painter().text(pos2(ar.left() + 36.0, ar.center().y), Align2::LEFT_CENTER, tr("À propos de Sanaga"), FontId::proportional(13.0), th.text);
            if aresp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                about = true;
            }
        }
        if let Some(sec) = picked {
            self.settings_go(sec);
        }

        // ---------------- header: ‹ › and the title ----------------
        let content = Rect::from_min_max(pos2(side.right(), full.top()), full.max);
        let head = Rect::from_min_size(content.min, vec2(content.width(), 52.0));
        let mut back = false;
        let mut fwd = false;
        {
            let cy = head.center().y;
            let cap = Rect::from_min_size(pos2(head.left() + 16.0, cy - 15.0), vec2(60.0, 30.0));
            ui.painter().add(egui::Shadow { offset: [0, 1], blur: 6, spread: 0, color: th.capsule_shadow }.as_shape(cap, CornerRadius::same(15)));
            ui.painter().rect_filled(cap, CornerRadius::same(15), th.capsule);
            for (k, enabled) in [(0, !self.settings_back.is_empty()), (1, !self.settings_fwd.is_empty())] {
                let r = Rect::from_min_size(pos2(cap.left() + k as f32 * 30.0, cap.top()), vec2(30.0, 30.0));
                let resp = ui.interact(r, egui::Id::new(("settings-nav", k)), if enabled { Sense::click() } else { Sense::hover() });
                if enabled && resp.hovered() {
                    ui.painter().circle_filled(r.center(), 13.0, th.track);
                }
                icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(13.0, 13.0)), if k == 0 { Icon::Back } else { Icon::Forward }, if enabled { th.text } else { th.sub.gamma_multiply(0.45) });
                if resp.clicked() {
                    if k == 0 {
                        back = true;
                    } else {
                        fwd = true;
                    }
                }
            }
            ui.painter().line_segment([pos2(cap.center().x, cy - 9.0), pos2(cap.center().x, cy + 9.0)], Stroke::new(1.0, th.hairline));
            let searching = !self.settings_filter.trim().is_empty();
            let title = if searching { tr("Résultats de la recherche").to_string() } else { self.settings_section.label().to_string() };
            ui.painter().text(pos2(cap.right() + 14.0, cy), Align2::LEFT_CENTER, title, FontId::new(15.0, bold()), th.text);
        }
        if back {
            if let Some(s) = self.settings_back.pop() {
                self.settings_fwd.push(self.settings_section);
                self.settings_section = s;
            }
        } else if fwd {
            if let Some(s) = self.settings_fwd.pop() {
                self.settings_back.push(self.settings_section);
                self.settings_section = s;
            }
        }

        // ---------------- content ----------------
        let body = Rect::from_min_max(pos2(content.left(), head.bottom()), content.max);
        let mut b = ui.new_child(UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
        let mut acts: Vec<Act> = Vec::new();
        ScrollArea::vertical().id_salt(("settings-body", self.settings_section as u8)).auto_shrink(false).show(&mut b, |ui| {
            let w = ui.available_width();
            let cw = (w - 48.0).min(640.0);
            ui.horizontal(|ui| {
                ui.add_space(((w - cw) / 2.0).max(16.0));
                ui.vertical(|ui| {
                    ui.set_width(cw);
                    if !wide {
                        self.settings_search(ui, th);
                        ui.add_space(8.0);
                        let current = self.settings_section;
                        let opts: Vec<(Section, &str)> = Section::ALL.iter().map(|s| (*s, s.label())).collect();
                        if let Some(s) = popup(ui, "settings-section", current, &opts) {
                            self.settings_go(s);
                        }
                        ui.add_space(12.0);
                    }
                    ui.add_space(4.0);
                    self.settings_content(ui, th, &mut acts);
                    ui.add_space(30.0);
                });
            });
        });
        if about {
            acts.push(Act::Go("sanaga://about"));
        }
        self.settings_actions(acts);
    }

    fn settings_search(&mut self, ui: &mut Ui, th: &Theme) {
        Frame::new().fill(th.track).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(8, 0)).show(ui, |ui| {
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 28.0), Layout::left_to_right(Align::Center), |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                icons::paint(ui.painter(), r, Icon::Search, th.sub);
                ui.add_space(2.0);
                ui.add(
                    TextEdit::singleline(&mut self.settings_filter)
                        .id(egui::Id::new("settings-search"))
                        .frame(Frame::NONE)
                        .font(FontId::proportional(13.0))
                        .vertical_align(Align::Center)
                        .hint_text(RichText::new(tr("Rechercher")).color(th.sub))
                        .desired_width(ui.available_width()),
                );
            });
        });
    }

    fn settings_content(&mut self, ui: &mut Ui, th: &Theme, acts: &mut Vec<Act>) {
        let mut changed = false;
        let web_ok = self.web_ok;
        let sync = self.sync_status.clone();
        let profile = if self.store.profile.is_empty() { tr("Personnel").to_string() } else { self.store.profile.clone() };
        if self.is_default_browser.is_none() {
            self.is_default_browser = Some(default_browser_is_sanaga());
        }
        let is_default = self.is_default_browser.unwrap_or(false);
        let section = self.settings_section;
        let query = self.settings_filter.trim().to_lowercase();
        if section == Section::Intelligence && self.store.settings.ai == "claude" && self.ai_key_known.is_none() {
            self.ai_key_known = Some(crate::keyring::lookup(&self.store.profile, crate::ai::KEY_SITE, "claude").is_some_and(|k| !k.is_empty()));
        }
        if section == Section::Intelligence && self.store.settings.ai == "ollama" && self.ai_models.is_none() {
            self.refresh_ollama_models();
        }
        let key_known = self.ai_key_known.unwrap_or(false);
        let models = self.ai_models.clone();
        let logins = self.store.logins.len();
        let keyring_ok = crate::keyring::available();
        let now = ui.input(|i| i.time);
        let confirm_clear = self.settings_confirm.is_some_and(|t| now - t < 4.0);
        let key_input = &mut self.ai_key_input;
        let st = &mut self.store.settings;
        let show = Shown { section, query: &query };
        let mut any = false;
        let mut ask_confirm = false;

        // ---------------- General ----------------
        if show.shows(Section::General, tr("Navigateur par défaut"), &[]) {
            any = true;
            group(ui, th, "", |ui, rows| {
                let label = if is_default { tr("Sanaga est votre navigateur par défaut.") } else { tr("Sanaga n'est pas votre navigateur par défaut.") };
                rows.row(ui, tr("Navigateur par défaut"), label, false, |ui| {
                    if !is_default && ui.add(egui::Button::new(RichText::new(tr("Définir par défaut")).color(Color32::WHITE)).fill(th.accent).corner_radius(CornerRadius::same(7))).clicked() {
                        acts.push(Act::MakeDefault);
                    }
                    if is_default {
                        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                        check(ui.painter(), r.center(), th.accent);
                    }
                });
            });
            footnote(ui, th, tr("Les liens ouverts depuis vos autres applications s'ouvriront dans Sanaga."));
        }
        if show.shows(Section::General, tr("Langue"), &["Language", "Idioma", "Sprache"]) {
            any = true;
            group(ui, th, "", |ui, rows| {
                rows.row(ui, tr("Langue"), "Language · Idioma · Sprache", false, |ui| {
                    let opts: Vec<(&str, &str)> = crate::i18n::LANGUAGES.iter().map(|(c, n)| (*c, *n)).collect();
                    let cur = opts.iter().find(|o| o.0 == st.language).map(|o| o.0).unwrap_or("en");
                    if let Some(code) = popup(ui, "language", cur, &opts) {
                        st.language = code.to_string();
                        changed = true;
                    }
                });
            });
            footnote(ui, th, tr("Toute l'interface de Sanaga change aussitôt ; les sites sont aussi demandés dans cette langue (pour les nouveaux onglets)."));
        }
        if show.shows(Section::General, tr("Au démarrage"), &[tr("Page d'accueil"), tr("Rouvrir les onglets de la dernière session"), tr("Moteur de recherche")]) {
            any = true;
            group(ui, th, "", |ui, rows| {
                rows.row(ui, tr("Au démarrage"), "", false, |ui| {
                    if let Some(v) = popup(ui, "startup", st.restore_tabs, &[(true, tr("Rouvrir les onglets de la dernière session")), (false, tr("Ouvrir la page d'accueil"))]) {
                        st.restore_tabs = v;
                        changed = true;
                    }
                });
                rows.row(ui, tr("Page d'accueil"), "", false, |ui| {
                    changed |= field(ui, &mut st.home, "sanaga://newtab", 240.0, false);
                });
                rows.row(ui, tr("Moteur de recherche"), "", false, |ui| {
                    let opts: Vec<(Engine, &str)> = Engine::ALL.iter().map(|e| (*e, e.label())).collect();
                    if let Some(e) = popup(ui, "engine", st.engine, &opts) {
                        st.engine = e;
                        changed = true;
                    }
                });
            });
            footnote(ui, th, tr("La version légère de DuckDuckGo fonctionne le mieux avec Sanaga."));
        }

        // ---------------- Appearance ----------------
        if show.shows(Section::Appearance, tr("Thème"), &[tr("Couleur d'accent"), tr("Clair"), tr("Sombre"), "Grassfield"]) {
            any = true;
            group(ui, th, "", |ui, rows| {
                rows.row(ui, tr("Apparence"), "", false, |ui| {
                    if let Some(k) = segmented(ui, th, &[tr("Système"), tr("Clair"), tr("Sombre")], st.theme as usize) {
                        st.theme = k as u8;
                        if k > 0 {
                            st.dark = k == 2;
                        }
                        changed = true;
                    }
                });
                rows.row(ui, tr("Couleur d'accent"), "", false, |ui| {
                    // right to left: the last colour first
                    for (k, (name, _l, _d, swatch)) in crate::theme::ACCENTS.iter().enumerate().rev() {
                        let col = if k == 0 { crate::theme::ACCENTS[crate::theme::accent_in_use_of_system()].3 } else { *swatch };
                        let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                        let chosen = st.accent as usize == k;
                        if k == 0 {
                            // "System": half of the system colour, half white, as macOS's multicolour
                            ui.painter().circle_filled(r.center(), 8.0, col);
                            ui.painter().circle_stroke(r.center(), 8.0, Stroke::new(2.0, Color32::WHITE.gamma_multiply(0.7)));
                        } else {
                            ui.painter().circle_filled(r.center(), 8.0, col);
                        }
                        if chosen {
                            ui.painter().circle_stroke(r.center(), 10.5, Stroke::new(1.5, col));
                            ui.painter().circle_filled(r.center(), 2.5, Color32::WHITE);
                        }
                        if resp.on_hover_cursor(CursorIcon::PointingHand).tip(tr(name)).clicked() && !chosen {
                            st.accent = k as u8;
                            changed = true;
                        }
                    }
                });
            });
            footnote(ui, th, tr("« Système » suit l'apparence et la couleur d'accent choisies pour tout l'ordinateur (Grassfield › Réglages › Apparence)."));
        }
        if show.shows(Section::Appearance, tr("Disposition"), &[tr("Onglets"), tr("Barre d'adresse"), tr("Onglets verticaux")]) {
            any = true;
            group(ui, th, tr("Disposition"), |ui, _rows| {
                Frame::new().inner_margin(Margin::symmetric(10, 10)).show(ui, |ui| {
                    ui.label(RichText::new(tr("Onglets")).size(12.5).color(th.sub));
                    ui.horizontal_wrapped(|ui| {
                        for (v, label) in [(0u8, tr("En haut")), (1, tr("En bas")), (2, tr("À gauche")), (3, tr("À droite"))] {
                            if layout_choice(ui, th, label, v, 9, st.tabs_pos == v) && st.tabs_pos != v {
                                st.tabs_pos = v;
                                changed = true;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new(tr("Barre d'adresse")).size(12.5).color(th.sub));
                    ui.horizontal_wrapped(|ui| {
                        for (v, label) in [(0u8, tr("En haut")), (1, tr("En bas")), (2, tr("Flottante"))] {
                            if layout_choice(ui, th, label, 9, v, st.bar_pos == v) && st.bar_pos != v {
                                st.bar_pos = v;
                                changed = true;
                            }
                        }
                    });
                });
            });
            footnote(ui, th, tr("Où mettre les onglets — sur le côté, ils restent lisibles même très nombreux."));
        }
        if show.shows(Section::Appearance, tr("Barre d'outils"), &[tr("Afficher la barre de favoris"), tr("Zoom par défaut")]) {
            any = true;
            group(ui, th, tr("Barre d'outils"), |ui, rows| {
                changed |= rows.switch(ui, tr("Afficher la barre de favoris"), "", &mut st.show_bookmarks_bar);
                rows.row(ui, tr("Zoom par défaut"), "", false, |ui| {
                    let steps = [0.5f32, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];
                    let cur = steps.iter().copied().min_by(|a, b| (a - st.zoom).abs().total_cmp(&(b - st.zoom).abs())).unwrap_or(1.0);
                    let labels: Vec<String> = steps.iter().map(|s| format!("{:.0} %", s * 100.0)).collect();
                    let opts: Vec<(u32, &str)> = steps.iter().zip(&labels).map(|(s, l)| ((s * 100.0) as u32, l.as_str())).collect();
                    if let Some(v) = popup(ui, "zoom", (cur * 100.0) as u32, &opts) {
                        st.zoom = v as f32 / 100.0;
                        changed = true;
                    }
                });
            });
            footnote(ui, th, tr("Ctrl + molette ou Ctrl +/− change le zoom d'un site ; Sanaga s'en souvient."));
            group(ui, th, tr("Boutons de la barre d'outils"), |ui, rows| {
                for (key, _) in crate::store::TOOLBAR_BUTTONS {
                    let mut on = st.toolbar.split(',').any(|k| k == key);
                    if rows.switch(ui, super::toolbar::toolbar_button_label(key), "", &mut on) {
                        let off = format!("-{key}");
                        let mut keys: Vec<String> = st.toolbar.split(',').filter(|k| !k.is_empty() && *k != key && *k != off).map(|k| k.to_string()).collect();
                        keys.push(if on { key.to_string() } else { off });
                        st.toolbar = keys.join(",");
                        changed = true;
                    }
                }
                if rows.link(ui, tr("Rétablir les boutons d'origine"), "", "") {
                    st.toolbar = crate::store::default_toolbar();
                    changed = true;
                }
            });
            footnote(ui, th, tr("Aussi : clic droit sur la barre d'outils."));
        }

        // ---------------- Intelligence ----------------
        if show.shows(Section::Intelligence, tr("Résumés et extensions sur mesure"), &[tr("Résumer la page"), tr("Créer une extension"), "Claude", "Ollama", "Codex"]) {
            any = true;
            let found = crate::ai_cli::installed();
            group(ui, th, tr("Source de l'intelligence"), |ui, rows| {
                let mut pick = |rows: &mut Rows, ui: &mut Ui, key: &str, label: &str, sub: &str| {
                    if rows.choice(ui, label, sub, st.ai == key) && st.ai != key {
                        st.ai = key.to_string();
                        st.ai_model.clear();
                        changed = true;
                    }
                };
                for t in &found {
                    pick(rows, ui, t.key(), &tf("{} (installé)", &[&t.name()]), tr("Votre propre compte, sans clé — appelé verrouillé"));
                }
                pick(rows, ui, "claude", "Claude (Anthropic)", tr("Avec votre clé d'API"));
                pick(rows, ui, "ollama", "Ollama", tr("Un modèle sur cet ordinateur : gratuit et privé"));
                pick(rows, ui, "", tr("Sanaga seul"), tr("Résumé simple, sans IA ; rien ne quitte l'ordinateur"));
            });
            footnote(ui, th, tr("Sanaga résume les pages (bouton ✦ ou Ctrl+Maj+Y) et écrit des extensions à partir de ce que vous décrivez."));
            match st.ai.as_str() {
                "ollama" => {
                    group(ui, th, "", |ui, rows| {
                        rows.row(ui, tr("Modèle"), "", false, |ui| match &models {
                            Some(m) if !m.is_empty() => {
                                let cur = if st.ai_model.is_empty() { m[0].clone() } else { st.ai_model.clone() };
                                let opts: Vec<(usize, &str)> = m.iter().enumerate().map(|(i, n)| (i, n.as_str())).collect();
                                let k = m.iter().position(|n| *n == cur).unwrap_or(0);
                                if let Some(i) = popup(ui, "ollama-model", k, &opts) {
                                    st.ai_model = m[i].clone();
                                    changed = true;
                                }
                            }
                            _ => {
                                ui.label(RichText::new(tr("Ollama ne répond pas")).color(th.sub));
                            }
                        });
                        if rows.link(ui, tr("Chercher les modèles"), "", "") {
                            acts.push(Act::FindModels);
                        }
                    });
                    footnote(ui, th, tr("Ollama ne répond pas : installez-le (ollama.com), puis dans un terminal : ollama pull llama3.2"));
                }
                "claude" => {
                    group(ui, th, "", |ui, rows| {
                        if key_known {
                            rows.row(ui, tr("Clé d'API"), tr("Clé enregistrée dans le trousseau du système."), false, |ui| {
                                if ui.button(tr("Oublier la clé")).clicked() {
                                    acts.push(Act::ForgetKey);
                                }
                            });
                        } else {
                            rows.row(ui, tr("Clé d'API"), "", false, |ui| {
                                if ui.add_enabled(!key_input.trim().is_empty(), egui::Button::new(tr("Enregistrer"))).clicked() {
                                    acts.push(Act::SaveKey);
                                }
                                field(ui, key_input, "sk-ant-…", 200.0, true);
                            });
                        }
                        rows.row(ui, tr("Modèle"), "", false, |ui| {
                            let mut m = if st.ai_model.is_empty() { crate::ai::CLAUDE_MODEL.to_string() } else { st.ai_model.clone() };
                            if field(ui, &mut m, "", 200.0, false) {
                                st.ai_model = m;
                                changed = true;
                            }
                        });
                    });
                    footnote(ui, th, tr("Avec Claude, le texte de la page résumée est envoyé à Anthropic."));
                }
                k if crate::ai_cli::CliTool::from_key(k).is_some() => {
                    let tool = crate::ai_cli::CliTool::from_key(k).unwrap_or(crate::ai_cli::CliTool::ClaudeCode);
                    group(ui, th, "", |ui, rows| {
                        rows.row(ui, tr("Modèle"), "", false, |ui| {
                            changed |= field(ui, &mut st.ai_model, tr("celui par défaut"), 200.0, false);
                        });
                    });
                    footnote(ui, th, &tf("{} est appelé verrouillé : dans un dossier vide, sans outils ni accès à vos fichiers ; Sanaga lit seulement sa réponse.", &[&tool.name()]));
                }
                _ => {}
            }
            group(ui, th, "", |ui, rows| {
                if rows.link(ui, tr("Créer une extension"), tr("Décrivez ce que vous voulez : Sanaga l'écrit pour vous"), "") {
                    acts.push(Act::Go("sanaga://create-extension"));
                }
            });
            ui.add_space(12.0);
        }

        // ---------------- Privacy ----------------
        if show.shows(Section::Privacy, tr("Suivi"), &[tr("Empêcher le suivi intersites"), tr("Effacer les données"), tr("Paramètres des sites"), tr("Sécurité")]) {
            any = true;
            group(ui, th, "", |ui, rows| {
                changed |= rows.switch(ui, tr("Empêcher le suivi intersites"), "", &mut st.itp);
            });
            footnote(ui, th, tr("Les traceurs publicitaires ne peuvent plus vous suivre d'un site à l'autre (comme dans Safari). Le Rapport de confidentialité de la page d'accueil montre ceux qui ont été tenus à l'écart. S'applique aux nouveaux onglets."));
            group(ui, th, tr("Données de navigation"), |ui, rows| {
                if rows.link(ui, tr("Effacer les données"), tr("Historique, cookies, cache…"), "") {
                    acts.push(Act::Go("sanaga://clear"));
                }
                if rows.link(ui, tr("Paramètres des sites"), tr("Caméra, micro, notifications, zoom… site par site"), "") {
                    acts.push(Act::Go("sanaga://sites"));
                }
                rows.row(ui, tr("Historique de navigation"), "", false, |ui| {
                    let (text, col) = if confirm_clear { (tr("Confirmer l'effacement"), Color32::from_rgb(0xC0, 0x30, 0x2A)) } else { (tr("Effacer…"), th.text) };
                    if ui.add(egui::Button::new(RichText::new(text).color(if confirm_clear { Color32::WHITE } else { col })).fill(if confirm_clear { col } else { th.track })).clicked() {
                        if confirm_clear {
                            acts.push(Act::ClearHistory);
                        } else {
                            ask_confirm = true;
                        }
                    }
                });
            });
            ui.add_space(12.0);
            group(ui, th, tr("Sécurité"), |ui, rows| {
                rows.row(ui, tr("Pages isolées (bac à sable)"), tr("Chaque page s'exécute isolée du reste de l'ordinateur."), false, |ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    check(ui.painter(), r.center(), th.accent);
                });
                rows.row(ui, tr("Mots de passe"), tr("Dans le trousseau du système"), false, |ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    check(ui.painter(), r.center(), if keyring_ok { th.accent } else { th.sub });
                });
            });
            footnote(ui, th, tr("Avec le moteur Sanaga Léger, aucun script n'est exécuté : pas de publicités, pas de traceurs."));
        }

        // ---------------- Passwords ----------------
        if show.shows(Section::Passwords, tr("Mots de passe"), &[tr("Proposer d'enregistrer et de remplir les mots de passe"), tr("Gérer les mots de passe enregistrés"), tr("Coordonnées")]) {
            any = true;
            group(ui, th, "", |ui, rows| {
                changed |= rows.switch(ui, tr("Proposer d'enregistrer et de remplir les mots de passe"), "", &mut st.passwords);
                let count = if logins == 1 { tr("1 compte").to_string() } else { tf("{} comptes", &[&logins]) };
                if rows.link(ui, tr("Gérer les mots de passe enregistrés"), "", &count) {
                    acts.push(Act::Go("sanaga://passwords"));
                }
                if rows.link(ui, tr("Importer depuis un autre navigateur (Chrome, Firefox…)"), "", "") {
                    acts.push(Act::Go("sanaga://import"));
                }
            });
            footnote(
                ui,
                th,
                if keyring_ok {
                    tr("Les mots de passe sont enregistrés dans le trousseau du système (Secret Service), jamais dans les fichiers de Sanaga. Un seul compte pour un site : Sanaga le remplit tout seul.")
                } else {
                    tr("Le trousseau du système (Secret Service) est introuvable : les mots de passe ne peuvent pas être enregistrés.")
                },
            );
            group(ui, th, tr("Coordonnées"), |ui, rows| {
                for (label, value) in [(tr("Nom"), &mut st.fill_name), (tr("E-mail"), &mut st.fill_email), (tr("Téléphone"), &mut st.fill_phone), (tr("Adresse"), &mut st.fill_address)] {
                    rows.row(ui, label, "", false, |ui| {
                        changed |= field(ui, value, "", 260.0, false);
                    });
                }
            });
            footnote(ui, th, tr("Pour remplir les formulaires : menu ⋮ › Remplir avec mes coordonnées, ou Ctrl+Maj+F."));
        }

        // ---------------- Performance ----------------
        if show.shows(Section::Performance, tr("Économiseur de mémoire"), &[tr("Moteur d'affichage des pages"), tr("Charger les images")]) {
            any = true;
            group(ui, th, "", |ui, rows| {
                rows.row(ui, tr("Économiseur de mémoire"), "", false, |ui| {
                    if let Some(v) = popup(ui, "memory", st.memory_saver, &[(0u8, tr("Désactivé")), (1, tr("Modéré")), (2, tr("Équilibré")), (3, tr("Maximal"))]) {
                        st.memory_saver = v;
                        changed = true;
                    }
                });
                changed |= rows.switch(ui, tr("Charger les images"), "", &mut st.images);
            });
            footnote(ui, th, tr("Les onglets que vous n'avez pas regardés depuis un moment sont mis en veille et se rechargent quand vous y revenez (Équilibré : après 20 minutes). Les onglets qui jouent du son ne dorment jamais."));
            group(ui, th, tr("Moteur d'affichage des pages"), |ui, rows| {
                if rows.choice(ui, "WebKit", tr("Sites complets avec JavaScript, vidéos et applications (recommandé)"), st.webkit) && !st.webkit {
                    st.webkit = true;
                    changed = true;
                }
                if rows.choice(ui, tr("Sanaga Léger"), tr("Pages simplifiées, très rapide et très économe en mémoire"), !st.webkit) && st.webkit {
                    st.webkit = false;
                    changed = true;
                }
            });
            footnote(ui, th, if web_ok { tr("Le changement s'applique aux prochaines pages ouvertes.") } else { tr("WebKit n'est pas disponible ici : Sanaga utilise le moteur léger.") });
        }

        // ---------------- Sync and profiles ----------------
        if show.shows(Section::Sync, tr("Synchronisation"), &[tr("Dossier"), tr("Profils")]) {
            any = true;
            let state = if !sync.error.is_empty() {
                sync.error.clone()
            } else if let Some(t) = sync.last {
                tf("Dernière synchronisation {} · autres appareils : {} · cet ordinateur : {}", &[&store::relative_time(t), &sync.devices.len(), &crate::sync::device_name()])
            } else {
                tr("Désactivée").to_string()
            };
            group(ui, th, "", |ui, rows| {
                rows.row(ui, tr("Dossier partagé"), "", false, |ui| {
                    changed |= field(ui, &mut st.sync_dir, "/home/vous/Nextcloud", 260.0, false);
                });
                if rows.link(ui, tr("Synchroniser maintenant"), &state, "") {
                    acts.push(Act::SyncNow);
                }
            });
            footnote(ui, th, tr("Choisissez un dossier partagé entre vos ordinateurs (Syncthing, Nextcloud, Dropbox, clé USB…). Sanaga y garde vos favoris, votre liste de lecture et vos onglets ouverts, et montre sur la page d'accueil les onglets de vos autres appareils."));
            group(ui, th, tr("Profils"), |ui, rows| {
                if rows.link(ui, tr("Gérer les profils"), "", &profile) {
                    acts.push(Act::Go("sanaga://profiles"));
                }
            });
        }

        // ---------------- Extensions ----------------
        if show.shows(Section::Extensions, tr("Extensions"), &[tr("Créer une extension")]) {
            any = true;
            group(ui, th, "", |ui, rows| {
                if rows.link(ui, tr("Gérer les extensions"), tr("Bloqueur de publicités, mode sombre, les vôtres…"), "") {
                    acts.push(Act::Go("sanaga://extensions"));
                }
                if rows.link(ui, tr("Créer une extension"), tr("Décrivez ce que vous voulez : Sanaga l'écrit pour vous"), "") {
                    acts.push(Act::Go("sanaga://create-extension"));
                }
            });
        }

        if !any {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(tr("Aucun paramètre ne correspond à votre recherche.")).color(th.sub));
            });
        }
        if ask_confirm {
            self.settings_confirm = Some(now);
        }
        if changed {
            crate::i18n::set_language(&self.store.settings.language);
            self.store.save_settings();
            if !self.store.settings.sync_dir.trim().is_empty() {
                acts.push(Act::SyncNow);
            }
        }
    }

    fn settings_actions(&mut self, acts: Vec<Act>) {
        for a in acts {
            match a {
                Act::Go(p) => self.navigate(self.active, p.to_string(), None, true),
                Act::MakeDefault => self.make_default_browser(),
                Act::ClearHistory => {
                    self.store.clear_history();
                    self.visited.clear();
                    self.settings_confirm = None;
                    self.notice = Some((tr("Historique effacé.").into(), self.ctx.input(|i| i.time)));
                }
                Act::SyncNow => self.sync_now(),
                Act::SaveKey => {
                    let key = self.ai_key_input.trim().to_string();
                    let ok = crate::keyring::store(&self.store.profile, crate::ai::KEY_SITE, "claude", &key);
                    self.ai_key_input.clear();
                    self.ai_key_known = Some(ok);
                    let msg = if ok { tr("Clé enregistrée dans le trousseau du système.") } else { tr("Le trousseau du système n'est pas disponible.") };
                    self.notice = Some((msg.into(), self.ctx.input(|i| i.time)));
                }
                Act::ForgetKey => {
                    crate::keyring::delete(&self.store.profile, crate::ai::KEY_SITE, "claude");
                    self.ai_key_known = Some(false);
                }
                Act::FindModels => self.refresh_ollama_models(),
            }
        }
    }

    /// Make Sanaga the default browser (web links and HTML files).
    fn make_default_browser(&mut self) {
        let now = self.ctx.input(|i| i.time);
        if !launcher_installed() {
            self.notice = Some((tr("Installez d'abord Sanaga (packaging/install.sh) pour qu'il apparaisse dans vos applications.").into(), now));
            return;
        }
        let ok = std::process::Command::new("xdg-settings").args(["set", "default-web-browser", "sanaga.desktop"]).status().is_ok_and(|s| s.success());
        // HTML files too
        let _ = std::process::Command::new("xdg-mime").args(["default", "sanaga.desktop", "text/html", "application/xhtml+xml"]).status();
        self.is_default_browser = Some(default_browser_is_sanaga());
        self.notice = Some((if ok { tr("Sanaga est maintenant votre navigateur par défaut.") } else { tr("Impossible de changer le navigateur par défaut.") }.into(), now));
    }
}
