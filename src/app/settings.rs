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

    /// The category at this place in the list (test aid).
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(super) fn by_place(n: usize) -> Option<Section> {
        Section::ALL.get(n).copied()
    }

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

    /// The category's name in its address: `sanaga://settings/privacy`.
    fn slug(self) -> &'static str {
        match self {
            Section::General => "general",
            Section::Appearance => "appearance",
            Section::Privacy => "privacy",
            Section::Passwords => "passwords",
            Section::Performance => "performance",
            Section::Sync => "sync",
            Section::Extensions => "extensions",
            Section::Intelligence => "intelligence",
        }
    }

    /// The address of a category.
    pub(super) fn address(self) -> String {
        format!("sanaga://settings/{}", self.slug())
    }

    /// The category an address names (None: Settings with no category).
    pub(super) fn of_address(url: &str) -> Option<Section> {
        let name = url.trim_end_matches('/').strip_prefix("sanaga://settings/")?;
        Section::ALL.into_iter().find(|s| s.slug() == name)
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

/// A text as it is searched: small letters, accents left out ("général"
/// is found by typing "general"; "ß" by "ss").
pub(super) fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(|c| c.to_lowercase()) {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => out.push('a'),
            'ç' => out.push('c'),
            'è' | 'é' | 'ê' | 'ë' => out.push('e'),
            'ì' | 'í' | 'î' | 'ï' => out.push('i'),
            'ñ' => out.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => out.push('o'),
            'ù' | 'ú' | 'û' | 'ü' => out.push('u'),
            'ý' | 'ÿ' => out.push('y'),
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            'ß' => out.push_str("ss"),
            // (a combining accent typed apart from its letter)
            '\u{300}'..='\u{36f}' => {}
            c => out.push(c),
        }
    }
    out
}

/// The search of the settings. (Kept in one place for the frame being
/// drawn, rather than handed to every group and row.)
///
/// What can be found is what the settings really show: before the first
/// results, every category is drawn once out of sight, and each group notes
/// the words of its title, rows and note (`index`). A group is a result
/// when one of them holds what is searched; a setting added to a category
/// is found without being declared anywhere.
#[derive(Default)]
struct Search {
    /// What is searched (folded); empty: not searching.
    query: String,
    /// The categories are being drawn out of sight to note their words.
    indexing: bool,
    /// The words of each group: (block, place of the group in it) → folded.
    index: HashMap<(String, usize), String>,
    indexed: bool,
    /// The block being drawn (category and title), its category, whether
    /// its own name is what was found (then all its groups are shown), and
    /// the place of the next group in it.
    block: String,
    section: Option<Section>,
    block_found: bool,
    next_group: usize,
    /// The group being drawn, and whether it is shown (its note follows it).
    group: usize,
    group_shown: bool,
    /// A category's name clicked in the results: go there.
    go: Option<Section>,
}

thread_local! {
    static SEARCH: std::cell::RefCell<Search> = std::cell::RefCell::new(Search::default());
}

/// Note words of the group being drawn (while the index is made).
fn index_words(words: &[&str]) {
    SEARCH.with(|s| {
        let mut s = s.borrow_mut();
        if s.indexing {
            let key = (s.block.clone(), s.group);
            let entry = s.index.entry(key).or_default();
            for w in words {
                entry.push_str(&fold(w));
                entry.push('\n');
            }
        }
    });
}

/// Is a block of groups shown? Its category is the chosen one, or, while
/// searching, one of its groups holds what is searched.
struct Shown<'a> {
    section: Section,
    query: &'a str,
}

impl Shown<'_> {
    fn shows(&self, sec: Section, title: &str, labels: &[&str]) -> bool {
        SEARCH.with(|s| {
            let mut s = s.borrow_mut();
            s.block = format!("{}/{title}", sec.slug());
            s.section = Some(sec);
            s.next_group = 0;
            s.block_found = false;
            if s.indexing {
                return true;
            }
            if self.query.is_empty() {
                return sec == self.section;
            }
            let q = self.query;
            // the block's own name, its category's, or words given for it
            // (other languages, brand names): all its groups are results
            s.block_found = fold(title).contains(q) || fold(sec.label()).contains(q) || labels.iter().any(|l| fold(l).contains(q));
            s.block_found || s.index.iter().any(|((block, _), words)| *block == s.block && words.contains(q))
        })
    }
}

// ------------------------------------------------------------------ controls

/// Rows of one group: counts them to draw the hairline between two.
struct Rows<'t> {
    th: &'t Theme,
    n: usize,
    /// The next row is a switch, on or off (said to screen readers).
    checked: Option<bool>,
}

const ROW_PAD: f32 = 14.0;

/// A rounded group of rows, with an optional title above it.
fn group(ui: &mut Ui, th: &Theme, title: &str, add: impl FnOnce(&mut Ui, &mut Rows)) {
    // its place in its block; while searching: is it a result, and of which category
    let (shown, found_in) = SEARCH.with(|s| {
        let mut s = s.borrow_mut();
        s.group = s.next_group;
        s.next_group += 1;
        let searching = !s.query.is_empty() && !s.indexing;
        let shown = !searching || s.block_found || s.index.get(&(s.block.clone(), s.group)).is_some_and(|words| words.contains(s.query.as_str()));
        s.group_shown = shown;
        (shown, if searching { s.section } else { None })
    });
    index_words(&[title]);
    if !shown {
        return;
    }
    if let Some(sec) = found_in {
        // a search result: the category it is found in, to go there in one click
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(4), sec.tile());
            icons::paint(ui.painter(), r.shrink(3.5), sec.icon(), Color32::WHITE);
            let name = egui::Label::new(RichText::new(sec.label()).size(12.0).color(th.sub)).sense(Sense::click());
            let resp = ui.add(name).on_hover_cursor(CursorIcon::PointingHand);
            if resp.hovered() {
                ui.painter().line_segment([resp.rect.left_bottom(), resp.rect.right_bottom()], Stroke::new(1.0, th.sub));
            }
            if resp.clicked() {
                SEARCH.with(|s| s.borrow_mut().go = Some(sec));
            }
            if !title.is_empty() {
                ui.label(RichText::new("›").size(12.0).color(th.sub));
                ui.label(RichText::new(title).size(12.0).family(bold()).color(th.text));
            }
        });
        ui.add_space(4.0);
    } else if !title.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(title).size(13.0).family(bold()).color(th.text));
        ui.add_space(6.0);
    }
    Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::symmetric(0, 2)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut rows = Rows { th, n: 0, checked: None };
        add(ui, &mut rows);
    });
    ui.add_space(6.0);
}

/// A note under a group (secondary text).
fn footnote(ui: &mut Ui, th: &Theme, text: &str) {
    index_words(&[text]);
    // (the note of a group that is not among the results goes with it)
    if !SEARCH.with(|s| s.borrow().group_shown) {
        return;
    }
    ui.horizontal(|ui| {
        ui.add_space(ROW_PAD);
        ui.add(egui::Label::new(RichText::new(text).size(11.5).color(th.sub)).wrap());
    });
    ui.add_space(14.0);
}

impl Rows<'_> {
    /// One row: label (and its explanation) on the left, `control` on the
    /// right. `click`: the whole row is clickable (returns the response).
    ///
    /// The text is never cut: it takes the room the control leaves (measured
    /// at the frame before), goes on to further lines when it must, and the
    /// row grows with it — in every language and at every window width.
    fn row(&mut self, ui: &mut Ui, label: &str, sub: &str, click: bool, control: impl FnOnce(&mut Ui)) -> egui::Response {
        let th = self.th;
        let w = ui.available_width();
        let id = ui.id().with(("settings-row", label));
        let known: Option<f32> = ui.data(|d| d.get_temp(id));
        let ctl_w = known.unwrap_or(w * 0.42).min(w * 0.62);
        let text_w = (w - 2.0 * ROW_PAD - ctl_w - if ctl_w > 0.0 { 14.0 } else { 0.0 }).max(110.0);
        let title = ui.painter().layout(label.to_string(), FontId::proportional(13.5), th.text, text_w);
        let note = (!sub.is_empty()).then(|| ui.painter().layout(sub.to_string(), FontId::proportional(11.5), th.sub, text_w));
        let text_h = title.size().y + note.as_ref().map_or(0.0, |n| 3.0 + n.size().y);
        let h = (text_h + 20.0).max(if sub.is_empty() { 40.0 } else { 54.0 }).round();
        let (r, resp) = ui.allocate_exact_size(vec2(w, h), if click { Sense::click() } else { Sense::hover() });
        // screen readers: the row's words, and what it is
        let checked = self.checked.take();
        let said = if sub.is_empty() { label.to_string() } else { format!("{label}. {sub}") };
        resp.widget_info(|| match checked {
            Some(on) => egui::WidgetInfo::selected(egui::WidgetType::CheckBox, true, on, &said),
            None => egui::WidgetInfo::labeled(if click { egui::WidgetType::Button } else { egui::WidgetType::Label }, true, &said),
        });
        if self.n > 0 {
            ui.painter().line_segment([pos2(r.left() + ROW_PAD, r.top()), pos2(r.right() - ROW_PAD, r.top())], Stroke::new(1.0, th.hairline.gamma_multiply(0.6)));
        }
        self.n += 1;
        // a search result: the rows that hold what is searched stand out
        index_words(&[label, sub]);
        let searched = SEARCH.with(|s| {
            let s = s.borrow();
            !s.query.is_empty() && !s.indexing && (fold(label).contains(s.query.as_str()) || fold(sub).contains(s.query.as_str()))
        });
        if searched {
            ui.painter().rect_filled(r.shrink2(vec2(4.0, 2.0)), CornerRadius::same(8), th.accent.gamma_multiply(0.13));
        }
        if click && resp.hovered() {
            ui.painter().rect_filled(r.shrink2(vec2(4.0, 2.0)), CornerRadius::same(8), th.track.gamma_multiply(0.6));
        }
        let top = r.center().y - text_h / 2.0;
        ui.painter().galley(pos2(r.left() + ROW_PAD, top), title.clone(), th.text);
        if let Some(note) = note {
            ui.painter().galley(pos2(r.left() + ROW_PAD, top + title.size().y + 3.0), note, th.sub);
        }
        let ctl = Rect::from_min_max(pos2(r.left() + ROW_PAD + 110.0, r.top()), pos2(r.right() - ROW_PAD, r.bottom()));
        let mut child = ui.new_child(UiBuilder::new().max_rect(ctl).layout(Layout::right_to_left(Align::Center)));
        control(&mut child);
        // what the control really took, for the text at the next frame
        let used = child.min_rect().width().max(0.0);
        if known.is_none_or(|k| (k - used).abs() > 0.5) {
            ui.data_mut(|d| d.insert_temp(id, used));
            ui.ctx().request_repaint();
        }
        if click { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp }
    }

    /// A row that does something now (not a page to go to): a button on the
    /// right says what. True when it is pressed.
    fn action(&mut self, ui: &mut Ui, label: &str, sub: &str, button: &str) -> bool {
        let mut pressed = false;
        self.row(ui, label, sub, false, |ui| {
            pressed = ui.button(button).clicked();
        });
        pressed
    }

    /// A row with a switch on the right; true when it changed.
    fn switch(&mut self, ui: &mut Ui, label: &str, sub: &str, on: &mut bool) -> bool {
        let th = self.th;
        let mut changed = false;
        self.checked = Some(*on);
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
pub(super) fn switch(ui: &mut Ui, th: &Theme, on: &mut bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(38.0, 22.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.14);
    // (off: a grey that shows on the card in the light and the dark look —
    // the track's own colour is the card's in the dark, and left the knob
    // floating alone)
    let off = lerp(th.card, th.sub, 0.42);
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
pub(super) fn segmented(ui: &mut Ui, th: &Theme, options: &[&str], chosen: usize) -> Option<usize> {
    let font = FontId::proportional(12.5);
    index_words(options);
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
pub(super) fn popup<T: PartialEq + Copy>(ui: &mut Ui, id: &str, current: T, options: &[(T, &str)]) -> Option<T> {
    let label = options.iter().find(|o| o.0 == current).map(|o| o.1).unwrap_or("");
    // (its choices can be searched for: "battery", "DuckDuckGo"…)
    index_words(&options.iter().map(|o| o.1).collect::<Vec<_>>());
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

/// A small round button with a symbol (in a row's controls).
fn small_icon_button(ui: &mut Ui, th: &Theme, icon: Icon, tip: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    if resp.hovered() {
        ui.painter().circle_filled(r.center(), 13.0, th.track);
    }
    icons::paint(ui.painter(), r.shrink(7.5), icon, th.sub);
    resp.on_hover_cursor(CursorIcon::PointingHand).tip(tip).clicked()
}

/// A field on the right of a row.
fn field(ui: &mut Ui, value: &mut String, hint: &str, width: f32, password: bool) -> bool {
    ui.add(TextEdit::singleline(value).hint_text(hint).desired_width(width).password(password)).changed()
}

/// A choice drawn as a small picture of the window (where the tabs and the
/// address bar go). `tabs`: 0 top, 1 bottom, 2 left, 3 right, 9 not shown;
/// `bar`: 0 top, 1 bottom, 2 floating, 9 not shown.
pub(super) fn layout_choice(ui: &mut Ui, th: &Theme, label: &str, tabs: u8, bar: u8, selected: bool) -> bool {
    index_words(&[label]);
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
/// Desktops that send links to a routing entry of their own, which then
/// opens the browser chosen in the general list (Omarchy).
const ROUTERS: [&str; 1] = ["omarchy-browser.desktop"];

/// `xdg-settings get default-web-browser`; `general`: from the general list
/// only (not the desktop's own, which may hold a routing entry).
fn xdg_default_browser(general: bool) -> String {
    let mut cmd = std::process::Command::new("xdg-settings");
    cmd.args(["get", "default-web-browser"]);
    if general {
        cmd.env_remove("BROWSER").env("XDG_CURRENT_DESKTOP", "");
    }
    cmd.output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

fn default_browser_is_sanaga() -> bool {
    // Omarchy says itself which browser its routing entry opens
    if let Ok(o) = std::process::Command::new("omarchy-cmd-default-browser").output() {
        if o.status.success() {
            return String::from_utf8_lossy(&o.stdout).trim() == "sanaga.desktop";
        }
    }
    match xdg_default_browser(false) {
        b if b == "sanaga.desktop" => true,
        b if b.is_empty() || ROUTERS.contains(&b.as_str()) => xdg_default_browser(true) == "sanaga.desktop",
        _ => false,
    }
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
    /// Set the sync folder's secret phrase (true) or forget it here (false).
    SyncSecret(bool),
    SaveKey,
    ForgetKey,
    FindModels,
    ToggleExt(String),
    DeleteExt(String),
    ToggleMobile,
    OpenExtFolder,
    CopyPassword(usize),
    DeletePassword(usize),
    SetPermission(String, String, Option<bool>),
    SetIdentity(String, Option<crate::identity::Identity>),
    Onboarding,
    InstallExt,
    ProfileLauncher,
    ResetSite(String),
}

const SIDEBAR_W: f32 = 236.0;

impl Sanaga {
    /// Change category. Each category has its address
    /// (`sanaga://settings/privacy`): going to one is a step of the tab's
    /// own history, so the toolbar's arrows, Alt+← / Alt+→ and the mouse's
    /// side buttons go back and forth between categories like between
    /// pages, a category can be opened from anywhere, and the session
    /// brings it back.
    fn settings_go(&mut self, sec: Section) {
        self.settings_filter.clear();
        if sec != self.settings_section {
            let a = self.active;
            self.navigate(a, sec.address(), None, true);
            self.settings_section = sec;
        }
    }

    /// The category follows the tab's address (a step back or forward, a
    /// link, a restored tab); Settings opened without one shows the
    /// category seen last, and takes its address.
    fn settings_follow_address(&mut self) {
        let a = self.active;
        let Some(t) = self.tabs.get_mut(a) else { return };
        match Section::of_address(&t.entries[t.idx].url) {
            Some(sec) => self.settings_section = sec,
            None => {
                let address = self.settings_section.address();
                t.entries[t.idx].url = address.clone();
                t.address = address;
            }
        }
    }

    pub(super) fn page_settings(&mut self, ui: &mut Ui, th: &Theme) {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, th.page);
        self.settings_follow_address();

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
        // (as wide as its longest name asks, in every language)
        let longest = Section::ALL.iter().map(|s| ui.painter().layout_no_wrap(s.label().to_string(), FontId::proportional(13.0), th.text).size().x).fold(0.0, f32::max);
        let side_w = (longest + 36.0 + 6.0 + 20.0 + 8.0).clamp(SIDEBAR_W, 320.0);
        let side = Rect::from_min_size(full.min, vec2(if wide { side_w } else { 0.0 }, full.height()));
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
            // (the same steps as the toolbar's arrows: the tab's history)
            let (can_back, can_fwd) = self.tabs.get(self.active).map(|t| (t.idx > 0, t.idx + 1 < t.entries.len())).unwrap_or((false, false));
            for (k, enabled) in [(0, can_back), (1, can_fwd)] {
                let r = Rect::from_min_size(pos2(cap.left() + k as f32 * 30.0, cap.top()), vec2(30.0, 30.0));
                let resp = ui.interact(r, egui::Id::new(("settings-nav", k)), if enabled { Sense::click() } else { Sense::hover() });
                if enabled && resp.hovered() {
                    ui.painter().circle_filled(r.center(), 13.0, th.track);
                }
                icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(13.0, 13.0)), if k == 0 { Icon::Back } else { Icon::Forward }, if enabled { th.text } else { th.sub.gamma_multiply(0.45) });
                let resp = resp.tip(if k == 0 { tr("Précédent") } else { tr("Suivant") });
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
        if back || fwd {
            self.settings_filter.clear();
            let a = self.active;
            self.go_history(a, if back { -1 } else { 1 });
            return;
        }

        // ---------------- content ----------------
        let body = Rect::from_min_max(pos2(content.left(), head.bottom()), content.max);
        let mut b = ui.new_child(UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
        let mut acts: Vec<Act> = Vec::new();
        // The first frame of a search: every category is drawn once, out of
        // sight and out of reach, to note what each group says (see
        // `Search`); the results come at the next frame.
        let searching = !self.settings_filter.trim().is_empty();
        if searching && !SEARCH.with(|s| s.borrow().indexed) {
            SEARCH.with(|s| {
                let mut s = s.borrow_mut();
                s.index.clear();
                s.indexing = true;
            });
            let away = Rect::from_min_size(pos2(body.left(), body.bottom() + 20_000.0), vec2(640.0, 10.0));
            let mut hidden = ui.new_child(UiBuilder::new().max_rect(away).layout(Layout::top_down(Align::Min)).id_salt("settings-index"));
            hidden.set_invisible();
            hidden.set_width(640.0);
            // (nothing it holds can be clicked: what it asks for is dropped)
            self.settings_content(&mut hidden, th, &mut Vec::new());
            SEARCH.with(|s| {
                let mut s = s.borrow_mut();
                s.indexing = false;
                s.indexed = true;
            });
            ui.ctx().request_repaint();
            return;
        }
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
        // a category's name clicked in the search results
        let wanted = SEARCH.with(|s| s.borrow_mut().go.take());
        if let Some(sec) = wanted {
            self.settings_go(sec);
        }
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
        // (asked of the desktop's tools once, on a thread)
        if self.is_default_browser.is_none() && self.first_time("default-browser") {
            self.off_thread(default_browser_is_sanaga, |app, yes| app.is_default_browser = Some(yes));
        }
        let is_default = self.is_default_browser.unwrap_or(false);
        let section = self.settings_section;
        let query = fold(self.settings_filter.trim());
        SEARCH.with(|s| {
            let mut s = s.borrow_mut();
            s.query = query.clone();
            s.group_shown = true;
            // (the index is made again at the next search: extensions,
            // sites and the language may have changed)
            if query.is_empty() && !s.indexing {
                s.indexed = false;
            }
        });
        if section == Section::Intelligence && self.store.settings.ai == "claude" && self.ai_key_known.is_none() {
            self.check_ai_key();
        }
        if section == Section::Intelligence && self.store.settings.ai == "ollama" && self.ai_models.is_none() {
            self.refresh_ollama_models();
        }
        let key_known = self.ai_key_known.unwrap_or(false);
        let models = self.ai_models.clone();
        let logins = self.store.logins.len();
        let login_list = self.store.logins.clone();
        let permissions = self.store.permissions.clone();
        let identities = self.store.identities.clone();
        let extensions = crate::extensions::all();
        let ext_on = self.store.extensions_on.clone();
        let mobile_on = self.tb("mobile");
        let confirm_key = egui::Id::new("settings-confirm-delete");
        let confirm_del: Option<(String, f64)> = ui.ctx().data(|d| d.get_temp(confirm_key));
        let keyring_ok = self.keyring_ok();
        let now = ui.input(|i| i.time);
        let confirm_clear = self.settings_confirm.is_some_and(|t| now - t < 4.0);
        let key_input = &mut self.ai_key_input;
        let sync_phrase = &mut self.sync_phrase_input;
        let profile_key = self.store.profile.clone();
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
            group(ui, th, "", |ui, rows| {
                if rows.link(ui, tr("Revoir la présentation de Sanaga"), tr("Les premiers réglages, pas à pas"), "") {
                    acts.push(Act::Onboarding);
                }
            });
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
                    let state = super::toolbar::toolbar_state(&st.toolbar, key);
                    rows.row(ui, super::toolbar::toolbar_button_label(key), "", false, |ui| {
                        // (the memory figure has no menu entry: shown or hidden)
                        let opts: &[&str] = if key == "memory" { &[tr("Affiché"), tr("Masqué")] } else { &[tr("Affiché"), tr("Menu »"), tr("Masqué")] };
                        let cur = if key == "memory" { state.min(1) } else { state };
                        if let Some(v) = segmented(ui, th, opts, cur) {
                            let v = if key == "memory" && v == 1 { 2 } else { v };
                            super::toolbar::set_toolbar_state(&mut st.toolbar, key, v);
                            changed = true;
                        }
                    });
                }
                if rows.action(ui, tr("Rétablir les boutons d'origine"), "", tr("Rétablir")) {
                    st.toolbar = crate::store::default_toolbar();
                    changed = true;
                }
            });
            footnote(ui, th, tr("« Menu » » range le bouton dans le menu » de la barre d'outils, pour la garder épurée. Aussi : clic droit sur la barre d'outils."));
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
            footnote(ui, th, tr("Sanaga résume les pages (bouton Intelligence de la barre d'outils, ou Ctrl+Maj+Y) et écrit des extensions à partir de ce que vous décrivez."));
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
                        if rows.action(ui, tr("Chercher les modèles"), "", tr("Chercher")) {
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
                    if tool.tool_less() {
                        footnote(ui, th, &tf("{} est appelé verrouillé : dans un dossier vide, sans outils ni accès à vos fichiers ; Sanaga lit seulement sa réponse.", &[&tool.name()]));
                    } else {
                        footnote(ui, th, &tf("{} peut lire vos fichiers, même verrouillé : Sanaga ne lui envoie pas le texte des pages (résumé, traduction). Il peut écrire des extensions, que vous relisez avant de les activer.", &[&tool.name()]));
                    }
                }
                _ => {}
            }
            group(ui, th, tr("Traduction des pages"), |ui, rows| {
                for (key, label, sub) in [
                    ("ai", tr("Avec l'intelligence choisie ci-dessus"), tr("La page est traduite sur place ; seul son texte est envoyé")),
                    ("google", "Google Traduction", tr("L'adresse de la page est envoyée à Google")),
                    ("", tr("Me demander la prochaine fois"), ""),
                ] {
                    if rows.choice(ui, label, sub, st.translate_with == key) && st.translate_with != key {
                        st.translate_with = key.to_string();
                        changed = true;
                    }
                }
                rows.row(ui, tr("Traduire vers"), "", false, |ui| {
                    let label = |code: &str| if code.is_empty() { tf("Langue de Sanaga ({})", &[&super::translate::target("").2]) } else { super::translate::target(code).2.to_string() };
                    egui::ComboBox::from_id_salt("translate-to").selected_text(label(&st.translate_to)).width(220.0).show_ui(ui, |ui| {
                        if ui.selectable_label(st.translate_to.is_empty(), label("")).clicked() && !st.translate_to.is_empty() {
                            st.translate_to.clear();
                            changed = true;
                        }
                        for (code, _, name) in super::translate::LANGUAGES {
                            if ui.selectable_label(st.translate_to == code, name).clicked() && st.translate_to != code {
                                st.translate_to = code.to_string();
                                changed = true;
                            }
                        }
                    });
                });
            });
            ui.add_space(12.0);
            group(ui, th, "", |ui, rows| {
                if rows.link(ui, tr("Créer une extension"), tr("Décrivez ce que vous voulez : Sanaga l'écrit pour vous"), "") {
                    acts.push(Act::Go("sanaga://create-extension"));
                }
            });
            ui.add_space(12.0);
        }

        // ---------------- Privacy ----------------
        if show.shows(Section::Privacy, tr("Suivi"), &[tr("Bloquer les publicités et les traceurs"), tr("Empêcher le suivi intersites"), tr("Effacer les données"), tr("Paramètres des sites"), tr("Sécurité")]) {
            any = true;
            group(ui, th, "", |ui, rows| {
                changed |= rows.switch(ui, tr("Bloquer les publicités et les traceurs"), tr("Les demandes vers les régies publicitaires et les traceurs ne partent jamais"), &mut st.blocker);
                changed |= rows.switch(ui, tr("Listes de blocage complètes"), tr("EasyList et EasyPrivacy, récupérées une fois par semaine auprès de leurs auteurs (easylist.to)"), &mut st.block_lists);
                changed |= rows.switch(ui, tr("Empêcher le suivi intersites"), "", &mut st.itp);
                changed |= rows.switch(ui, tr("Laisser les sites demander à envoyer des notifications"), tr("Désactivé : leurs demandes sont refusées sans vous déranger"), &mut st.notif_prompts);
            });
            footnote(ui, th, tr("Les traceurs publicitaires ne peuvent plus vous suivre d'un site à l'autre (comme dans Safari). Le Rapport de confidentialité de la page d'accueil montre ceux qui ont été tenus à l'écart. S'applique aux nouveaux onglets."));
            if !permissions.is_empty() {
                group(ui, th, tr("Autorisations des sites"), |ui, rows| {
                    for (site, kind, allowed) in &permissions {
                        rows.row(ui, &host_of(site), super::plus::kind_label(kind), false, |ui| {
                            if small_icon_button(ui, th, Icon::Close, tr("Oublier ce choix")) {
                                acts.push(Act::SetPermission(site.clone(), kind.clone(), None));
                            }
                            if let Some(v) = segmented(ui, th, &[tr("Autorisé"), tr("Bloqué")], if *allowed { 0 } else { 1 }) {
                                acts.push(Act::SetPermission(site.clone(), kind.clone(), Some(v == 0)));
                            }
                        });
                    }
                });
                ui.add_space(12.0);
            }
            group(ui, th, tr("Identité du navigateur par site"), |ui, rows| {
                if identities.is_empty() {
                    rows.row(ui, tr("Aucun choix pour l'instant"), tr("Menu principal › Plus d'outils › Se présenter à ce site comme…"), false, |_| {});
                }
                for (host, i) in &identities {
                    rows.row(ui, host, "", false, |ui| {
                        if small_icon_button(ui, th, Icon::Close, tr("Oublier ce choix")) {
                            acts.push(Act::SetIdentity(host.clone(), None));
                        }
                        let all = crate::identity::Identity::ALL;
                        let labels: Vec<&str> = all.iter().map(|x| x.label()).collect();
                        let k = all.iter().position(|x| x == i).unwrap_or(0);
                        if let Some(n) = segmented(ui, th, &labels, k) {
                            acts.push(Act::SetIdentity(host.clone(), if n == 0 { None } else { Some(all[n]) }));
                        }
                    });
                }
            });
            footnote(ui, th, tr("Sanaga se présente comme Chrome aux quelques applications web qui refusent WebKit (WhatsApp Web, Teams, Zoom…). Vous pouvez choisir pour n'importe quel site."));
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
            if !login_list.is_empty() {
                let shown = login_list.len().min(60);
                group(ui, th, tr("Comptes enregistrés"), |ui, rows| {
                    for (i, (site, user)) in login_list.iter().take(shown).enumerate() {
                        let who = if user.is_empty() { tr("(sans identifiant)").to_string() } else { user.clone() };
                        rows.row(ui, &host_of(site), &who, false, |ui| {
                            if small_icon_button(ui, th, Icon::Close, tr("Supprimer")) {
                                acts.push(Act::DeletePassword(i));
                            }
                            if small_icon_button(ui, th, Icon::Copy, tr("Copier le mot de passe")) {
                                acts.push(Act::CopyPassword(i));
                            }
                        });
                    }
                    if login_list.len() > shown && rows.link(ui, &tf("Voir les {} comptes", &[&login_list.len()]), "", "") {
                        acts.push(Act::Go("sanaga://passwords"));
                    }
                });
                ui.add_space(12.0);
            }
            group(ui, th, tr("Coordonnées"), |ui, rows| {
                for (label, value) in [(tr("Nom"), &mut st.fill_name), (tr("E-mail"), &mut st.fill_email), (tr("Téléphone"), &mut st.fill_phone), (tr("Adresse"), &mut st.fill_address)] {
                    rows.row(ui, label, "", false, |ui| {
                        changed |= field(ui, value, "", 260.0, false);
                    });
                }
            });
            footnote(ui, th, tr("Pour remplir les formulaires : menu principal › Remplir avec mes coordonnées, ou Ctrl+Maj+F."));
        }

        // ---------------- Performance ----------------
        if show.shows(Section::Performance, tr("Économiseur de mémoire"), &[tr("Mode économie d'énergie"), tr("Moteur d'affichage des pages"), tr("Charger les images")]) {
            any = true;
            group(ui, th, "", |ui, rows| {
                rows.row(ui, tr("Économiseur de mémoire"), tr("Onglets inactifs mis en veille après 6 h (modéré), 4 h (équilibré) ou 2 h (maximal), comme dans Chrome"), false, |ui| {
                    if let Some(v) = popup(ui, "memory", st.memory_saver, &[(0u8, tr("Désactivé")), (1, tr("Modéré")), (2, tr("Équilibré")), (3, tr("Maximal"))]) {
                        st.memory_saver = v;
                        changed = true;
                    }
                });
                rows.row(ui, tr("Mode économie d'énergie"), tr("Onglets inactifs mis en veille plus tôt, moins de réveils"), false, |ui| {
                    if let Some(v) = popup(ui, "low-power", st.low_power, &[(0u8, tr("Désactivé")), (3, tr("Batterie à 20 % ou moins")), (1, tr("Sur batterie")), (2, tr("Toujours"))]) {
                        st.low_power = v;
                        changed = true;
                    }
                });
                changed |= rows.switch(ui, tr("Charger les images"), "", &mut st.images);
            });
            footnote(ui, th, tr("Les onglets que vous n'avez pas regardés depuis longtemps sont mis en veille et se rechargent quand vous y revenez. Les onglets qui jouent du son ne dorment jamais."));
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
        if show.shows(Section::Sync, tr("Synchronisation"), &[tr("Dossier"), tr("Profils"), tr("Emoji du profil"), tr("Ajouter ce profil à mes applications")]) {
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
                if rows.action(ui, tr("Synchroniser maintenant"), &state, tr("Synchroniser")) {
                    acts.push(Act::SyncNow);
                }
                // the secret phrase: what goes into the folder is sealed on this computer first
                match crate::sync::has_secret(&profile_key) {
                    Some(true) => {
                        if rows.action(ui, tr("Chiffrement activé"), tr("Le dossier ne contient que des fichiers scellés."), tr("Retirer la phrase")) {
                            acts.push(Act::SyncSecret(false));
                        }
                    }
                    Some(false) => {
                        rows.row(ui, tr("Phrase secrète"), tr("Chiffre le dossier : la même phrase sur chaque ordinateur"), false, |ui| {
                            if ui.add_enabled(sync_phrase.chars().count() >= 8, egui::Button::new(tr("Chiffrer"))).clicked() {
                                acts.push(Act::SyncSecret(true));
                            }
                            field(ui, sync_phrase, tr("8 caractères ou plus"), 200.0, true);
                        });
                    }
                    None => {}
                }
            });
            footnote(ui, th, tr("Choisissez un dossier partagé entre vos ordinateurs (Syncthing, Nextcloud, Dropbox, clé USB…). Sanaga y garde vos favoris, votre liste de lecture et vos onglets ouverts, et montre sur la page d'accueil les onglets de vos autres appareils."));
            group(ui, th, tr("Profils"), |ui, rows| {
                if rows.link(ui, tr("Gérer les profils"), "", &profile) {
                    acts.push(Act::Go("sanaga://profiles"));
                }
                rows.row(ui, tr("Emoji du profil"), tr("Sur son badge et l'icône de son lanceur"), false, |ui| {
                    if ui.add(TextEdit::singleline(&mut st.profile_emoji).hint_text("★").desired_width(60.0).char_limit(8)).changed() {
                        changed = true;
                    }
                });
                if rows.action(ui, tr("Ajouter ce profil à mes applications"), tr("Un lanceur à lui, avec son icône : vraiment séparé des autres"), tr("Ajouter")) {
                    acts.push(Act::ProfileLauncher);
                }
            });
        }

        // ---------------- Extensions ----------------
        if show.shows(Section::Extensions, tr("Extensions"), &[tr("Créer une extension"), tr("Simulateur mobile"), tr("Installer une extension…"), "Chrome", "Firefox", ".xpi", ".crx"]) {
            any = true;
            group(ui, th, tr("Extensions installées"), |ui, rows| {
                let mut m = mobile_on;
                if rows.switch(ui, tr("Simulateur mobile"), tr("Intégrée à Sanaga"), &mut m) {
                    acts.push(Act::ToggleMobile);
                }
                for e in &extensions {
                    let mut on = ext_on.contains(&e.id);
                    let sub = if e.builtin { tr("Intégrée à Sanaga").to_string() } else { e.description.clone() };
                    let id = e.id.clone();
                    let confirming = confirm_del.as_ref().is_some_and(|(cid, t)| *cid == id && now - t < 4.0);
                    let mut toggled = false;
                    let resp = rows.row(ui, &e.name, &sub, false, |ui| {
                        toggled = switch(ui, th, &mut on);
                        if !e.builtin {
                            ui.add_space(6.0);
                            if confirming {
                                if ui.add(egui::Button::new(RichText::new(tr("Confirmer")).color(Color32::WHITE)).fill(Color32::from_rgb(0xC0, 0x30, 0x2A))).clicked() {
                                    acts.push(Act::DeleteExt(id.clone()));
                                }
                            } else if small_icon_button(ui, th, Icon::Close, tr("Supprimer l'extension")) {
                                ui.ctx().data_mut(|d| d.insert_temp(confirm_key, (id.clone(), now)));
                            }
                        }
                    });
                    let _ = resp;
                    if toggled {
                        acts.push(Act::ToggleExt(e.id.clone()));
                    }
                }
            });
            footnote(ui, th, tr("Les changements s'appliquent tout de suite aux pages ouvertes."));
            group(ui, th, "", |ui, rows| {
                if rows.link(ui, tr("Créer une extension"), tr("Décrivez ce que vous voulez : Sanaga l'écrit pour vous"), "") {
                    acts.push(Act::Go("sanaga://create-extension"));
                }
                if rows.action(ui, tr("Installer une extension…"), tr("Chrome (.crx, .zip, manifest.json), Firefox (.xpi), scripts utilisateur (.user.js)"), tr("Choisir un fichier…")) {
                    acts.push(Act::InstallExt);
                }
                if rows.action(ui, tr("Ouvrir le dossier des extensions"), tr("Vos fichiers .js et .css"), tr("Ouvrir")) {
                    acts.push(Act::OpenExtFolder);
                }
            });
            footnote(ui, th, tr("Les extensions des autres navigateurs marchent quand elles agissent dans les pages (bloqueurs d'éléments, mises en forme, aides à la lecture…). Celles qui ont besoin d'une fenêtre à elles ou d'un service en arrière-plan ne sont pas prises en charge."));
        }

        // ---------------- window, toolbar buttons, containers, site settings ----------------
        let mut changed_more = false;
        if show.shows(Section::General, tr("Vidéo et musique"), &["YouTube", "Netflix", "Spotify", "codecs", tr("Codecs vidéo et audio")]) {
            any = true;
            ui.add_space(4.0);
            ui.label(RichText::new(tr("Vidéo et musique")).size(13.0).family(bold()).color(th.text));
            ui.add_space(6.0);
            Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::symmetric(14, 12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                changed_more |= self.media_settings(ui, th);
            });
            ui.add_space(6.0);
            footnote(ui, th, tr("WebKit lit les vidéos avec GStreamer : les codecs viennent des paquets du système. Les vidéos protégées par Widevine (Netflix, Disney+, Spotify web…) ne peuvent être lues par aucun navigateur WebKit."));
        }
        if show.shows(Section::Appearance, tr("Fenêtre"), &[tr("Bordure de la fenêtre"), tr("Mode compact"), tr("Onglets en arbre"), tr("Mode focus")]) {
            any = true;
            let st = &mut self.store.settings;
            group(ui, th, tr("Fenêtre"), |ui, rows| {
                rows.row(ui, tr("Bordure de la fenêtre"), tr("Une couleur autour de la fenêtre"), false, |ui| {
                    if let Some(v) = popup(ui, "border", st.border, &[(0u8, tr("Aucune")), (1, tr("Couleur d'accent")), (2, tr("Dégradé")), (3, tr("Couleurs animées"))]) {
                        st.border = v;
                        changed_more = true;
                    }
                });
                if st.border > 0 {
                    rows.row(ui, tr("Épaisseur de la bordure"), "", false, |ui| {
                        let mut v = st.border_size as f32;
                        if ui.add(egui::Slider::new(&mut v, 1.0..=16.0).suffix(" pt")).changed() {
                            st.border_size = v as u8;
                            changed_more = true;
                        }
                    });
                    rows.row(ui, tr("Opacité de la bordure"), "", false, |ui| {
                        let mut v = st.border_alpha as f32;
                        if ui.add(egui::Slider::new(&mut v, 10.0..=100.0).suffix(" %")).changed() {
                            st.border_alpha = v as u8;
                            changed_more = true;
                        }
                    });
                }
                changed_more |= rows.switch(ui, tr("Mode compact"), tr("Barre d'outils et onglets plus bas : plus de place pour la page"), &mut st.compact);
                changed_more |= rows.switch(ui, tr("Onglets en arbre"), tr("Onglets verticaux : un onglet ouvert depuis une page se range dessous"), &mut st.tree_tabs);
                rows.row(ui, tr("Bouton de fermeture des onglets"), "", false, |ui| {
                    if let Some(v) = segmented(ui, th, &[tr("À gauche"), tr("À droite")], if st.close_right { 1 } else { 0 }) {
                        st.close_right = v == 1;
                        changed_more = true;
                    }
                });
                changed_more |= rows.switch(ui, tr("Toujours afficher les URL en entier"), tr("Sinon la barre d'adresse omet https:// et www. et montre l'adresse décodée, comme Chrome"), &mut st.full_urls);
                changed_more |= rows.switch(ui, tr("Sélecteur d'onglets (Ctrl+Tab)"), tr("Les onglets utilisés en dernier, en images ; Ctrl+Page suivante suit la barre d'onglets"), &mut st.tab_switcher);
            });
            footnote(ui, th, tr("Mode focus (F9) : seule la page reste, sans rien autour."));
            let current = self.store.settings.app_icon as usize;
            let mut pick_icon: Option<u8> = None;
            group(ui, th, tr("Icône de l'application"), |ui, _rows| {
                Frame::new().inner_margin(Margin::symmetric(10, 10)).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (k, (name, top, bottom, _)) in crate::icons::ICON_STYLES.iter().enumerate() {
                            let (r, resp) = ui.allocate_exact_size(vec2(84.0, 84.0), Sense::click());
                            let tile = Rect::from_center_size(pos2(r.center().x, r.top() + 30.0), vec2(48.0, 48.0));
                            let mut mesh = egui::Mesh::default();
                            let (t, b) = (Color32::from_rgb(top[0], top[1], top[2]), Color32::from_rgb(bottom[0], bottom[1], bottom[2]));
                            mesh.colored_vertex(tile.left_top(), t);
                            mesh.colored_vertex(tile.right_top(), t);
                            mesh.colored_vertex(tile.right_bottom(), b);
                            mesh.colored_vertex(tile.left_bottom(), b);
                            mesh.add_triangle(0, 1, 2);
                            mesh.add_triangle(0, 2, 3);
                            ui.painter().rect_filled(tile, CornerRadius::same(11), b);
                            ui.painter().add(egui::Shape::mesh(mesh));
                            for dy in [-6.0f32, 6.0] {
                                ui.painter().line_segment([pos2(tile.left() + 9.0, tile.center().y + dy), pos2(tile.right() - 9.0, tile.center().y + dy - 3.0)], Stroke::new(4.0, Color32::WHITE));
                            }
                            if k == current {
                                ui.painter().rect_stroke(tile.expand(3.0), CornerRadius::same(13), Stroke::new(2.0, th.accent), egui::StrokeKind::Outside);
                            }
                            ui.painter().text(pos2(r.center().x, r.bottom() - 10.0), Align2::CENTER_CENTER, tr(name), FontId::proportional(11.5), if k == current { th.accent } else { th.text });
                            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() && k != current {
                                pick_icon = Some(k as u8);
                            }
                        }
                    });
                });
            });
            if let Some(k) = pick_icon {
                self.set_app_icon(k);
            }
        }
        if show.shows(Section::Appearance, tr("Vos boutons"), &[tr("Boutons programmables"), tr("Ajouter le bouton")]) {
            any = true;
            ui.add_space(4.0);
            ui.label(RichText::new(tr("Vos boutons")).size(13.0).family(bold()).color(th.text));
            ui.add_space(6.0);
            Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                changed_more |= self.buttons_settings(ui, th);
            });
            ui.add_space(6.0);
            footnote(ui, th, tr("Un bouton dans la barre d'outils qui ouvre une adresse, lance une commande de Sanaga (mode focus, vidéo flottante, conteneur…) ou exécute votre script dans la page."));
        }
        if show.shows(Section::Privacy, tr("Conteneurs"), &[tr("Conteneur"), tr("Comptes séparés")]) {
            any = true;
            let list = self.store.containers.clone();
            let mut remove: Option<String> = None;
            let mut add = false;
            group(ui, th, tr("Conteneurs"), |ui, rows| {
                for c in &list {
                    let col = super::tabs::GROUP_COLORS.get(c.color as usize).map(|g| g.1).unwrap_or(Color32::GRAY);
                    rows.row(ui, &format!("●  {}", c.name), "", false, |ui| {
                        if small_icon_button(ui, th, Icon::Close, tr("Supprimer le conteneur et ses données")) {
                            remove = Some(c.name.clone());
                        }
                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 5.0, col);
                    });
                }
                rows.row(ui, tr("Nouveau conteneur"), "", false, |ui| {
                    if ui.button(tr("Créer")).clicked() {
                        add = true;
                    }
                    ui.add(TextEdit::singleline(&mut self.new_container_name).hint_text(tr("Travail, Achats…")).desired_width(180.0));
                });
            });
            footnote(ui, th, tr("Les onglets d'un conteneur ont leurs propres cookies et connexions : deux comptes d'un même site côte à côte, ou les achats tenus à l'écart du reste. Clic droit sur un onglet ou un lien › Ouvrir dans un conteneur."));
            if add {
                let name = std::mem::take(&mut self.new_container_name);
                self.add_container(&name);
            }
            if let Some(n) = remove {
                self.remove_container(&n);
            }
        }
        if show.shows(Section::Privacy, tr("Réglages par site"), &["JavaScript", tr("Polices"), tr("Mode sombre")]) {
            any = true;
            let sites = crate::site_prefs::all();
            group(ui, th, tr("Réglages par site"), |ui, rows| {
                if sites.is_empty() {
                    rows.row(ui, tr("Aucun réglage pour l'instant"), tr("Menu principal › Plus d'outils › Réglages de ce site…"), false, |_| {});
                }
                for (host, p) in &sites {
                    let sub = super::sitepanel::summary(p);
                    rows.row(ui, host, &sub, false, |ui| {
                        if small_icon_button(ui, th, Icon::Close, tr("Remettre les réglages par défaut")) {
                            acts.push(Act::ResetSite(host.clone()));
                        }
                    });
                }
            });
            footnote(ui, th, tr("JavaScript, polices, bloqueur, mode sombre, police, éléments retirés : site par site. Clic droit dans une page › Réglages de ce site…"));
        }
        if changed_more {
            changed = true;
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
            crate::page::BLOCKER_ON.store(self.store.settings.blocker, Ordering::Relaxed);
            crate::page::LISTS_ON.store(self.store.settings.block_lists, Ordering::Relaxed);
            // (switched on: the lists are loaded, or fetched if there are none yet)
            crate::page::prepare_lists();
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
                Act::SyncSecret(set) => {
                    let phrase = self.sync_phrase_input.trim().to_string();
                    self.sync_phrase_input.clear();
                    let profile = self.store.profile.clone();
                    self.off_thread(
                        move || crate::sync::set_secret(&profile, set.then_some(phrase.as_str())),
                        move |app, ok| {
                            app.say(match (ok, set) {
                                (false, _) => tr("Le trousseau du système n'est pas disponible."),
                                (true, true) => tr("Phrase enregistrée dans le trousseau. Le dossier est maintenant chiffré."),
                                (true, false) => tr("Phrase retirée de cet ordinateur."),
                            });
                            if ok {
                                app.sync_now();
                            }
                        },
                    );
                }
                Act::SaveKey => {
                    let key = self.ai_key_input.trim().to_string();
                    self.ai_key_input.clear();
                    let profile = self.store.profile.clone();
                    self.off_thread(
                        move || crate::keyring::store(&profile, crate::ai::KEY_SITE, "claude", &key),
                        |app, ok| {
                            app.ai_key_known = Some(ok);
                            app.say(if ok { tr("Clé enregistrée dans le trousseau du système.") } else { tr("Le trousseau du système n'est pas disponible.") });
                        },
                    );
                }
                Act::ForgetKey => {
                    let profile = self.store.profile.clone();
                    self.off_thread(move || crate::keyring::delete(&profile, crate::ai::KEY_SITE, "claude"), |_, ()| {});
                    self.ai_key_known = Some(false);
                }
                Act::FindModels => self.refresh_ollama_models(),
                Act::ToggleExt(id) => self.toggle_extension(&id),
                Act::DeleteExt(id) => self.delete_extension(&id),
                Act::ToggleMobile => {
                    let on = self.tb("mobile");
                    let mut keys: Vec<String> = self.store.settings.toolbar.split(',').filter(|k| !k.is_empty() && *k != "mobile" && *k != "-mobile").map(|k| k.to_string()).collect();
                    keys.push(if on { "-mobile".into() } else { "mobile".into() });
                    self.store.settings.toolbar = keys.join(",");
                    self.store.save_settings();
                    if on {
                        self.device = None;
                    }
                }
                Act::OpenExtFolder => {
                    let dir = crate::extensions::prepare_folder();
                    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                }
                Act::CopyPassword(i) => {
                    if let Some((s, u)) = self.store.logins.get(i).cloned() {
                        self.copy_password(s, u);
                    }
                }
                Act::DeletePassword(i) => {
                    if i < self.store.logins.len() {
                        let (s, u) = self.store.logins.remove(i);
                        let profile = self.store.profile.clone();
                        self.off_thread(move || crate::keyring::delete(&profile, &s, &u), |_, ()| {});
                        self.store.save_logins();
                    }
                }
                Act::SetPermission(site, kind, allow) => self.store.set_permission(&site, &kind, allow),
                Act::SetIdentity(host, i) => self.set_identity(&host, i),
                Act::InstallExt => self.install_extension_dialog(),
                Act::ProfileLauncher => self.add_profile_launcher(),
                Act::ResetSite(host) => self.save_site(&host, crate::site_prefs::SitePrefs::default(), true),
                Act::Onboarding => {
                    self.onboarding_step = 0;
                    self.navigate(self.active, "sanaga://start".into(), None, true);
                }
            }
        }
    }

    /// Ask for an extension file (Chrome, Firefox, user script) and install it.
    pub(super) fn install_extension_dialog(&mut self) {
        let pats = ["*.xpi", "*.crx", "*.zip", "*.js", "*.css", "manifest.json"];
        let Some(path) = crate::shell::choose_file(tr("Installer une extension"), &pats, tr("Extensions"), None) else { return };
        let now = self.ctx.input(|i| i.time);
        match crate::webext::install(&path) {
            Ok((name, id)) => {
                // installed switched off: the person reads what it would do
                // (sites, code) on the Extensions page, then turns it on
                self.ext_review = Some(id);
                self.notice = Some((tf("« {} » est installée mais désactivée : lisez-la, puis activez-la.", &[&name]), now));
                let a = self.active;
                self.navigate(a, "sanaga://extensions".into(), None, true);
            }
            Err(e) => self.notice = Some((tf("Impossible d'installer l'extension : {}", &[&e]), now)),
        }
    }

    pub(super) fn make_default_browser_pub(&mut self) {
        self.make_default_browser();
    }

    /// Make Sanaga the default browser (web links and HTML files).
    fn make_default_browser(&mut self) {
        let now = self.ctx.input(|i| i.time);
        if !launcher_installed() {
            self.notice = Some((tr("Installez d'abord Sanaga (packaging/install.sh) pour qu'il apparaisse dans vos applications.").into(), now));
            return;
        }
        let _ = now;
        // (the desktop's tools are slow, and are waited for: on a thread)
        self.off_thread(
            || {
                // 1. the general list (~/.config/mimeapps.list): what every desktop
                //    reads, and where Omarchy's routing entry looks for the browser
                let general = |program: &str, args: &[&str]| {
                    std::process::Command::new(program).args(args).env_remove("BROWSER").env("XDG_CURRENT_DESKTOP", "").status().is_ok_and(|s| s.success())
                };
                general("xdg-settings", &["set", "default-web-browser", "sanaga.desktop"]);
                // web links and HTML files
                general("xdg-mime", &["default", "sanaga.desktop", "x-scheme-handler/http", "x-scheme-handler/https", "text/html", "application/xhtml+xml"]);
                // 2. the desktop's own setting (GNOME, KDE…), unless it routes links
                //    to the general choice by itself (Omarchy)
                if !ROUTERS.contains(&xdg_default_browser(false).as_str()) {
                    let _ = std::process::Command::new("xdg-settings").args(["set", "default-web-browser", "sanaga.desktop"]).status();
                }
                default_browser_is_sanaga()
            },
            |app, ok| {
                app.is_default_browser = Some(ok);
                app.say(if ok { tr("Sanaga est maintenant votre navigateur par défaut.") } else { tr("Impossible de changer le navigateur par défaut.") });
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searching_ignores_accents_and_capitals() {
        assert_eq!(fold("Général"), "general");
        assert_eq!(fold("Économiseur de MÉMOIRE"), "economiseur de memoire");
        assert_eq!(fold("Straße · Œuvre · Ñandú"), "strasse · oeuvre · nandu");
        // an accent typed apart from its letter
        assert_eq!(fold("e\u{301}te\u{301}"), "ete");
        assert!(fold("Confidentialité et sécurité").contains(&fold("SECUR")));
    }

    #[test]
    fn every_category_has_its_address() {
        for sec in Section::ALL {
            assert_eq!(Section::of_address(&sec.address()), Some(sec));
            assert_eq!(Section::of_address(&format!("{}/", sec.address())), Some(sec));
        }
        assert_eq!(Section::of_address("sanaga://settings"), None);
        assert_eq!(Section::of_address("sanaga://settings/nowhere"), None);
        assert_eq!(Section::of_address("https://example.com/sanaga://settings/privacy"), None);
        // each address is one of Sanaga's own pages
        for sec in Section::ALL {
            assert!(matches!(crate::app::internal_from_url(&sec.address()), Some(crate::app::Internal::Settings)));
        }
    }
}
