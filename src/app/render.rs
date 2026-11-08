//! Draws a web page. Only the blocks near the visible part are laid out each
//! frame, so even very long pages stay smooth.

use super::chrome::site_badge;
use super::*;
use crate::html::{Block, FieldKind, Span};
use crate::icons::{self, Icon};
use eframe::egui::{Align, Align2, Color32, CursorIcon, Label, Layout, Margin, Response, RichText, ScrollArea, Sense, TextEdit, UiBuilder};

const CONTENT_MAX_W: f32 = 820.0;
const READER_W: f32 = 680.0;

/// Reader colours: white, sepia or night, like Safari's Reader.
fn reader_theme(base: &Theme, which: u8) -> Theme {
    let c = Color32::from_rgb;
    let mut t = *base;
    let (page, text, sub, link, visited, card, border) = match which {
        1 => (c(0xF8, 0xF1, 0xE3), c(0x4B, 0x3A, 0x26), c(0x8A, 0x75, 0x56), c(0x9C, 0x5B, 0x1E), c(0x7A, 0x4A, 0x1E), c(0xEF, 0xE5, 0xD0), c(0xE2, 0xD5, 0xBA)),
        2 => (c(0x1E, 0x1E, 0x20), c(0xDA, 0xDA, 0xDD), c(0x98, 0x98, 0x9F), c(0x6F, 0xB0, 0xFF), c(0xC4, 0x9B, 0xFF), c(0x2C, 0x2C, 0x2F), c(0x3A, 0x3A, 0x3D)),
        _ => (c(0xFF, 0xFF, 0xFF), c(0x1D, 0x1D, 0x1F), c(0x6E, 0x6E, 0x73), c(0x00, 0x66, 0xCC), c(0x7D, 0x3C, 0xB5), c(0xF2, 0xF2, 0xF5), c(0xE0, 0xE0, 0xE5)),
    };
    t.page = page;
    t.text = text;
    t.sub = sub;
    t.link = link;
    t.visited = visited;
    t.card = card;
    t.code_bg = card;
    t.border = border;
    t
}

struct RenderCtx<'a> {
    th: &'a Theme,
    base: f32,
    images_on: bool,
    images: &'a mut HashMap<String, ImgSlot>,
    forced: &'a mut HashSet<String>,
    visited: &'a HashSet<String>,
    actions: &'a mut Vec<Action>,
    hover: &'a mut Option<String>,
    find: &'a str,
    frame: u64,
    doc: &'a Document,
    private: bool,
    /// Reader mode: serif text.
    serif: bool,
}

impl Sanaga {
    pub(super) fn page_area(&mut self, ui: &mut Ui, th: &Theme) {
        let a = self.active;
        if self.overview_open {
            self.hover_url = None;
            self.tab_overview(ui, th);
            return;
        }
        if let View::Internal(Internal::Onboarding) = self.tabs[a].view {
            self.page_onboarding(ui, th);
            self.hover_url = None;
            return;
        }
        if let View::Internal(Internal::Settings) = self.tabs[a].view {
            // Settings has its own sidebar and scrolling (macOS System Settings)
            self.page_settings(ui, th);
            self.hover_url = None;
            return;
        }
        if let View::Internal(i) = self.tabs[a].view {
            let tab_id = self.tabs[a].id;
            ScrollArea::vertical().id_salt(("internal", tab_id, i as u8)).auto_shrink(false).show(ui, |ui| match i {
                Internal::NewTab => self.page_newtab(ui, th),
                Internal::History => self.page_history(ui, th),
                Internal::Bookmarks => self.page_bookmarks(ui, th),
                Internal::Downloads => self.page_downloads(ui, th),
                Internal::Settings => self.page_settings(ui, th),
                Internal::About => self.page_about(ui, th),
                Internal::Passwords => self.page_passwords(ui, th),
                Internal::Extensions => self.page_extensions(ui, th),
                Internal::Profiles => self.page_profiles(ui, th),
                Internal::ClearData => self.page_clear_data(ui, th),
                Internal::Sites => self.page_sites(ui, th),
                Internal::Import => self.page_import(ui, th),
                Internal::CreateExtension => self.page_create_extension(ui, th),
                Internal::Onboarding => self.page_onboarding(ui, th),
            });
            self.hover_url = None;
            return;
        }
        match &self.tabs[a].view {
            View::Loading => {
                self.hover_url = None;
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Spinner::new().size(28.0).color(th.accent));
                });
            }
            View::Error(msg) => {
                self.hover_url = None;
                let msg = msg.clone();
                self.page_error(ui, th, &msg);
            }
            View::Doc(doc) => {
                let doc = doc.clone();
                self.render_doc(ui, th, &doc);
            }
            View::Web => {
                // WebKit draws the page itself on top of this area.
                self.hover_url = None;
                let area = ui.max_rect();
                let device = self.device_viewport(area);
                if device.is_some() {
                    self.paint_scene_background(ui.painter(), area);
                }
                if let Some((id, tex)) = &self.freeze {
                    if *id == self.tabs[a].id {
                        let r = match device {
                            Some(d) => d.translate(vec2(0.0, -1.0)),
                            None => area,
                        };
                        // the picture keeps its real size (the docked inspector, if
                        // open, is not in it: it is not stretched over it)
                        let size = tex.size_vec2() / ui.ctx().pixels_per_point();
                        let pic = Rect::from_min_size(pos2(r.left(), r.top() + 1.0), size);
                        egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), size)).paint_at(&ui.new_child(egui::UiBuilder::new().max_rect(r)), pic);
                    }
                }
                // the drawn device over it (its screen is see-through)
                if device.is_some() {
                    self.paint_device_frame(ui, area, th);
                    // the studio's controls: tool bar, name bar, device list
                    self.device_scene(ui, area);
                }
                if self.tabs[a].loading && self.tabs[a].web.is_none() {
                    ui.centered_and_justified(|ui| {
                        ui.add(egui::Spinner::new().size(28.0).color(th.accent));
                    });
                }
            }
            View::Internal(_) => {}
        }
    }

    fn render_doc(&mut self, ui: &mut Ui, th: &Theme, doc: &Arc<Document>) {
        let a = self.active;
        let reader = self.tabs[a].reader;
        let reader_size = self.store.settings.reader_size;
        let reader_which = self.store.settings.reader_theme;
        let rth = reader_theme(th, reader_which);
        let th = if reader { &rth } else { th };
        if reader {
            ui.painter().rect_filled(ui.max_rect(), 0.0, th.page);
        }
        let images_on = self.store.settings.images;
        let find_q = if self.find_open { self.find_query.to_lowercase() } else { String::new() };
        let current_hit = if self.find_open { self.find_hits.get(self.find_pos).copied() } else { None };
        let frame_no = self.frame;
        let mut actions: Vec<Action> = Vec::new();
        let mut hover: Option<String> = None;
        let private;
        {
            let Sanaga { tabs, images, forced_images, visited, .. } = self;
            let tab = &mut tabs[a];
            private = tab.private;
            let full_w = ui.available_width();
            let max_w = if reader { READER_W * (reader_size / 19.0).max(1.0) } else { CONTENT_MAX_W };
            let content_w = (full_w - 64.0).min(max_w * tab.zoom.max(1.0)).max(200.0);
            let x0 = ((full_w - content_w) / 2.0).max(16.0);
            let base = if reader { reader_size * tab.zoom } else { 15.5 * tab.zoom };
            let key = (content_w as u32, (base * 10.0) as u32 + if reader { 1 } else { 0 });
            let top_pad = if reader { 150.0 } else if doc.trimmed { 70.0 } else { 24.0 };
            if tab.heights.len() != doc.blocks.len() || tab.layout_key != key {
                tab.heights = doc.blocks.iter().map(|b| estimate_height(b, content_w, base)).collect();
                tab.layout_key = key;
                tab.reader_skip = if reader { reader_skip(doc) } else { Vec::new() };
                for (h, skip) in tab.heights.iter_mut().zip(&tab.reader_skip) {
                    if *skip {
                        *h = 0.0;
                    }
                }
            }
            // Jump to a block (anchor / find result), repeated a few frames while heights settle.
            if let Some((b, left)) = tab.target_block {
                let y: f32 = tab.heights.iter().take(b).sum::<f32>() + top_pad;
                tab.scroll_to = Some((y - 60.0).max(0.0));
                tab.target_block = if left > 1 { Some((b, left - 1)) } else { None };
                ui.ctx().request_repaint();
            }
            let mut sa = ScrollArea::vertical().id_salt(("doc", tab.id, tab.nav)).auto_shrink(false);
            if let Some(y) = tab.scroll_to.take() {
                sa = sa.vertical_scroll_offset(y);
            }
            let heights = &mut tab.heights;
            let forms = &mut tab.forms;
            let skip = &tab.reader_skip;
            let out = sa.show_viewport(ui, |ui, viewport| {
                let origin = ui.max_rect().min;
                let mut y = top_pad;
                if reader {
                    y = reader_header(ui, th, doc, origin, x0, content_w, base, reader_which, &mut actions);
                }
                if doc.trimmed && !reader {
                    let r = Rect::from_min_size(pos2(origin.x + x0, origin.y + 14.0), vec2(content_w, 36.0));
                    let mut child = ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::left_to_right(Align::Center)).id_salt("trim-banner"));
                    Frame::new().fill(th.card).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(12, 6)).show(&mut child, |ui| {
                        ui.set_width(content_w - 24.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tr("Vue allégée : seul le contenu principal de la page est affiché.")).size(13.0).color(th.sub));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.link(RichText::new(tr("Afficher toute la page")).size(13.0)).clicked() {
                                    actions.push(Action::ShowFullPage);
                                }
                            });
                        });
                    });
                }
                let mut rc = RenderCtx {
                    th,
                    base,
                    images_on,
                    images,
                    forced: forced_images,
                    visited,
                    actions: &mut actions,
                    hover: &mut hover,
                    find: &find_q,
                    frame: frame_no,
                    doc,
                    private,
                    serif: reader,
                };
                for i in 0..doc.blocks.len() {
                    if skip.get(i).copied().unwrap_or(false) {
                        continue;
                    }
                    let h = heights[i];
                    if y + h >= viewport.min.y - 300.0 && y <= viewport.max.y + 300.0 {
                        let rect = Rect::from_min_size(pos2(origin.x + x0, origin.y + y), vec2(content_w, h.max(1.0)));
                        let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)).id_salt(("blk", i)));
                        if current_hit == Some(i) {
                            child.painter().rect_filled(Rect::from_min_size(pos2(rect.left() - 12.0, rect.top()), vec2(4.0, h)), 2.0, th.accent);
                        }
                        render_block(&mut child, &doc.blocks[i], i, forms, content_w, &mut rc);
                        let real = child.min_rect().height() + block_gap(&doc.blocks[i], base);
                        heights[i] = real;
                        y += real;
                    } else {
                        y += h;
                    }
                }
                if doc.blocks.is_empty() {
                    let r = Rect::from_min_size(pos2(origin.x + x0, origin.y + y), vec2(content_w, 60.0));
                    ui.painter().text(r.left_top(), Align2::LEFT_TOP, tr("Cette page n'a pas de contenu lisible sans JavaScript."), FontId::proportional(base), th.sub);
                    y += 60.0;
                }
                ui.set_min_size(vec2(full_w, y + 60.0));
            });
            tab.scroll_y = out.state.clamped_offset().y;
        }
        self.hover_url = hover;
        for act in actions {
            match act {
                Action::Navigate(u) => self.navigate(a, u, None, true),
                Action::OpenTab { url, private, focus } => {
                    if focus {
                        self.open_tab(&url, private);
                    } else {
                        self.open_background(&url, private);
                    }
                }
                Action::Submit { form, button } => self.submit(a, form, button),
                Action::LoadImage(u) => self.request_image(u, private),
                Action::ReaderSize(d) => {
                    let s = &mut self.store.settings;
                    s.reader_size = (s.reader_size + d).clamp(14.0, 30.0);
                    self.store.save_settings();
                }
                Action::ReaderTheme(t) => {
                    self.store.settings.reader_theme = t;
                    self.store.save_settings();
                }
                Action::ExitReader => self.toggle_reader(a),
                Action::ShowFullPage => {
                    self.tabs[a].full = true;
                    self.reload(a);
                }
            }
        }
    }
}

/// Reader controls (text size, theme, quit) and the article title.
/// Returns where the article text starts.
#[allow(clippy::too_many_arguments)]
fn reader_header(ui: &mut Ui, th: &Theme, doc: &Document, origin: egui::Pos2, x0: f32, w: f32, base: f32, which: u8, actions: &mut Vec<Action>) -> f32 {
    let row = Rect::from_min_size(pos2(origin.x + x0, origin.y + 14.0), vec2(w, 32.0));
    let cy = row.center().y;
    icons::paint(ui.painter(), Rect::from_center_size(pos2(row.left() + 9.0, cy), vec2(16.0, 16.0)), Icon::Reader, th.sub);
    ui.painter().text(pos2(row.left() + 24.0, cy), Align2::LEFT_CENTER, tr("Lecteur"), FontId::new(13.0, bold()), th.sub);
    let mut rx = row.right();
    // quit
    let qg = ui.painter().layout_no_wrap(tr("Quitter le Lecteur").into(), FontId::proportional(13.0), th.text);
    let qr = Rect::from_min_max(pos2(rx - qg.size().x - 22.0, cy - 14.0), pos2(rx, cy + 14.0));
    let qresp = ui.interact(qr, egui::Id::new("reader-quit"), Sense::click());
    ui.painter().rect_filled(qr, CornerRadius::same(8), if qresp.hovered() { th.border } else { th.card });
    ui.painter().galley(pos2(qr.left() + 11.0, cy - qg.size().y / 2.0), qg, th.text);
    if qresp.tip(tr("Revenir à la page normale (Ctrl+Maj+R)")).clicked() {
        actions.push(Action::ExitReader);
    }
    rx = qr.left() - 14.0;
    // themes: white, sepia, night
    let swatches = [(2u8, Color32::from_rgb(0x1E, 0x1E, 0x20), tr("Nuit")), (1, Color32::from_rgb(0xF8, 0xF1, 0xE3), tr("Sépia")), (0, Color32::WHITE, tr("Blanc"))];
    for (t, col, name) in swatches {
        let c = pos2(rx - 11.0, cy);
        let resp = ui.interact(Rect::from_center_size(c, vec2(24.0, 24.0)), egui::Id::new(("reader-theme", t)), Sense::click());
        ui.painter().circle_filled(c, 10.0, col);
        let ring = if which == t { Stroke::new(2.0, th.accent) } else { Stroke::new(1.0, th.border) };
        ui.painter().circle_stroke(c, if which == t { 12.0 } else { 10.0 }, ring);
        if resp.tip(tf("Thème {}", &[&name])).clicked() {
            actions.push(Action::ReaderTheme(t));
        }
        rx -= 30.0;
    }
    rx -= 8.0;
    // text size
    for (delta, size, tip) in [(1.0, 17.0, tr("Texte plus grand")), (-1.0, 12.0, tr("Texte plus petit"))] {
        let r = Rect::from_min_max(pos2(rx - 30.0, cy - 14.0), pos2(rx, cy + 14.0));
        let resp = ui.interact(r, egui::Id::new(("reader-size", size as i32)), Sense::click());
        ui.painter().rect_filled(r, CornerRadius::same(8), if resp.hovered() { th.border } else { th.card });
        ui.painter().text(r.center(), Align2::CENTER_CENTER, "A", FontId::new(size, FontFamily::Name("serif".into())), th.text);
        if resp.tip(tip).clicked() {
            actions.push(Action::ReaderSize(delta));
        }
        rx -= 34.0;
    }
    // title and site
    let title_rect = Rect::from_min_size(pos2(origin.x + x0, row.bottom() + 26.0), vec2(w, 600.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(title_rect).layout(Layout::top_down(Align::Min)).id_salt("reader-title"));
    let title = if doc.title.is_empty() { host_of(&doc.url) } else { reader_title(&doc.title) };
    child.add(Label::new(RichText::new(title).family(FontFamily::Name("serif-bold".into())).size(base * 1.6).color(th.text)).wrap());
    child.add_space(6.0);
    child.label(RichText::new(host_of(&doc.url)).size(base * 0.72).color(th.sub));
    let bottom = child.min_rect().bottom() + 14.0;
    ui.painter().line_segment([pos2(origin.x + x0, bottom), pos2(origin.x + x0 + w, bottom)], Stroke::new(1.0, th.border));
    bottom - origin.y + 24.0
}

/// What the Reader leaves out, like Safari's: form controls, menus made of
/// short links, and everything before the article really starts.
fn reader_skip(doc: &Document) -> Vec<bool> {
    let text_of = |b: &Block| -> Option<(String, bool)> {
        let spans = match b {
            Block::Para(s) | Block::Quote(s) | Block::ListItem { spans: s, .. } | Block::Heading(_, s) => s,
            _ => return None,
        };
        let all_links = spans.iter().all(|s| s.link.is_some() || s.text.trim().chars().all(|c| !c.is_alphanumeric()));
        Some((Document::plain_text(spans), all_links))
    };
    // the article starts at the first real paragraph
    let start = doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Para(_)) && text_of(b).map(|(t, links)| t.chars().count() >= 80 && !links).unwrap_or(false))
        .unwrap_or(0);
    let mut lead_image = false;
    doc.blocks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if matches!(b, Block::Input { .. } | Block::Button { .. }) {
                return true;
            }
            if i < start {
                // keep one lead picture from before the article, nothing else
                if let Block::Image { width, .. } = b {
                    if !lead_image && width.map(|w| w >= 150.0).unwrap_or(true) {
                        lead_image = true;
                        return false;
                    }
                }
                return true;
            }
            match text_of(b) {
                Some((t, true)) => t.chars().count() < 60, // a menu entry
                _ => false,
            }
        })
        .collect()
}

/// "Sanaga — Wikipédia" → "Sanaga": the site name is shown on its own line.
fn reader_title(title: &str) -> String {
    for sep in [" — ", " – ", " | ", " - ", " · "] {
        if let Some((head, _)) = title.rsplit_once(sep) {
            if head.chars().count() >= 4 {
                return head.trim().to_string();
            }
        }
    }
    title.trim().to_string()
}

fn block_gap(b: &Block, base: f32) -> f32 {
    match b {
        Block::ListItem { .. } => base * 0.3,
        Block::Heading(..) => base * 0.5,
        _ => base * 0.75,
    }
}

fn estimate_height(b: &Block, w: f32, base: f32) -> f32 {
    block_gap(b, base) + estimate_body(b, w, base)
}

fn estimate_body(b: &Block, w: f32, base: f32) -> f32 {
    let lines = |chars: usize, size: f32| -> f32 {
        let per_line = (w / (size * 0.5)).max(8.0);
        (chars as f32 / per_line).ceil().max(1.0) * size * 1.4
    };
    match b {
        Block::Heading(l, s) => lines(Document::plain_text(s).chars().count(), heading_size(*l, base)) + 8.0,
        Block::Para(s) | Block::Quote(s) | Block::ListItem { spans: s, .. } => lines(Document::plain_text(s).chars().count(), base),
        Block::Pre(t) => t.lines().count().max(1) as f32 * base * 1.3 + 20.0,
        Block::Rule => 12.0,
        Block::Image { width, .. } => width.map(|x| (x * 0.6).min(260.0)).unwrap_or(140.0),
        Block::Input { .. } | Block::Button { .. } => 32.0,
    }
}

fn heading_size(level: u8, base: f32) -> f32 {
    base * match level {
        1 => 1.9,
        2 => 1.5,
        3 => 1.25,
        4 => 1.1,
        _ => 1.0,
    }
}

fn split_find<'t>(text: &'t str, q: &str) -> Vec<(&'t str, bool)> {
    if q.is_empty() {
        return vec![(text, false)];
    }
    let lower = text.to_lowercase();
    if lower.len() != text.len() {
        // lowercasing changed byte offsets: highlight the whole span instead
        return vec![(text, lower.contains(q))];
    }
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(i) = lower[pos..].find(q) {
        let s = pos + i;
        if s > pos {
            out.push((&text[pos..s], false));
        }
        out.push((&text[s..s + q.len()], true));
        pos = s + q.len();
    }
    if pos < text.len() {
        out.push((&text[pos..], false));
    }
    out
}

fn render_spans(ui: &mut Ui, spans: &[Span], size: f32, color: Color32, rc: &mut RenderCtx) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, size * 0.32);
        for span in spans {
            let link_url = span.link.and_then(|l| rc.doc.links.get(l as usize)).cloned();
            for (seg, hl) in split_find(&span.text, rc.find) {
                let mut rt = RichText::new(seg).size(if span.code { size * 0.9 } else { size });
                rt = if span.code {
                    rt.family(FontFamily::Monospace).background_color(rc.th.code_bg)
                } else if span.bold {
                    rt.family(if rc.serif { FontFamily::Name("serif-bold".into()) } else { bold() })
                } else if span.italic {
                    rt.family(FontFamily::Name(if rc.serif { "serif-italic" } else { "italic" }.into()))
                } else if rc.serif {
                    rt.family(FontFamily::Name("serif".into()))
                } else {
                    rt
                };
                let col = match &link_url {
                    Some(u) if rc.visited.contains(u) => rc.th.visited,
                    Some(_) => rc.th.link,
                    None => color,
                };
                rt = rt.color(col);
                if hl {
                    rt = rt.background_color(rc.th.highlight).color(Color32::BLACK);
                }
                match &link_url {
                    Some(u) => {
                        let resp = ui.add(Label::new(rt).sense(Sense::click()));
                        link_behaviour(ui, &resp, u, rc);
                    }
                    None => {
                        ui.add(Label::new(rt));
                    }
                }
            }
        }
    });
}

fn link_behaviour(ui: &Ui, resp: &Response, url: &str, rc: &mut RenderCtx) {
    if resp.hovered() {
        *rc.hover = Some(url.to_string());
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let cmd = ui.input(|i| i.modifiers.command);
    if resp.middle_clicked() || (resp.clicked() && cmd) {
        rc.actions.push(Action::OpenTab { url: url.to_string(), private: rc.private, focus: false });
    } else if resp.clicked() {
        rc.actions.push(Action::Navigate(url.to_string()));
    }
    let private = rc.private;
    let mut act: Option<Action> = None;
    resp.menu_on_press(|ui| {
        if ui.button(tr("Ouvrir dans un nouvel onglet")).clicked() {
            act = Some(Action::OpenTab { url: url.to_string(), private, focus: false });
            ui.close();
        }
        if ui.button(tr("Ouvrir dans un onglet privé")).clicked() {
            act = Some(Action::OpenTab { url: url.to_string(), private: true, focus: true });
            ui.close();
        }
        if ui.button(tr("Copier l'adresse du lien")).clicked() {
            ui.ctx().copy_text(url.to_string());
            ui.close();
        }
    });
    if let Some(a) = act {
        rc.actions.push(a);
    }
}

fn render_block(ui: &mut Ui, b: &Block, idx: usize, forms: &mut [crate::html::Form], w: f32, rc: &mut RenderCtx) {
    let th = rc.th;
    let base = rc.base;
    match b {
        Block::Heading(l, spans) => {
            if *l <= 2 {
                ui.add_space(base * 0.4);
            }
            let spans: Vec<Span> = spans.iter().map(|s| Span { bold: true, ..s.clone() }).collect();
            render_spans(ui, &spans, heading_size(*l, base), th.text, rc);
            if *l <= 2 {
                let left = ui.min_rect().left();
                let (sep, _) = ui.allocate_exact_size(vec2(w, 6.0), Sense::hover());
                ui.painter().line_segment([pos2(left, sep.center().y), pos2(left + w, sep.center().y)], Stroke::new(1.0, th.border));
            }
        }
        Block::Para(spans) => render_spans(ui, spans, base, th.text, rc),
        Block::Quote(spans) => {
            Frame::new().inner_margin(Margin { left: 14, right: 0, top: 2, bottom: 2 }).show(ui, |ui| {
                render_spans(ui, spans, base, th.sub, rc);
            });
            let r = ui.min_rect();
            ui.painter().rect_filled(Rect::from_min_max(r.left_top(), pos2(r.left() + 3.0, r.bottom())), 1.5, th.accent.gamma_multiply(0.6));
        }
        Block::ListItem { depth, marker, spans } => {
            let indent = 8.0 + (*depth as f32 - 1.0) * 22.0;
            ui.horizontal_top(|ui| {
                ui.add_space(indent);
                let (r, _) = ui.allocate_exact_size(vec2(22.0, base * 1.45), Sense::hover());
                if !marker.is_empty() {
                    ui.painter().text(pos2(r.right() - 6.0, r.center().y + 1.5), Align2::RIGHT_CENTER, marker, FontId::proportional(base), th.sub);
                }
                ui.vertical(|ui| {
                    ui.set_width(w - indent - 26.0);
                    render_spans(ui, spans, base, th.text, rc);
                });
            });
        }
        Block::Pre(text) => {
            let hl = !rc.find.is_empty() && text.to_lowercase().contains(rc.find);
            let fill = if hl { th.highlight.gamma_multiply(0.4) } else { th.code_bg };
            Frame::new().fill(fill).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(10)).show(ui, |ui| {
                ui.set_width(w - 20.0);
                ScrollArea::horizontal().id_salt(("pre", idx)).show(ui, |ui| {
                    ui.add(Label::new(RichText::new(text).family(FontFamily::Monospace).size(base * 0.85).color(th.text)).extend());
                });
            });
        }
        Block::Rule => {
            let (r, _) = ui.allocate_exact_size(vec2(w, 12.0), Sense::hover());
            ui.painter().line_segment([pos2(r.left(), r.center().y), pos2(r.right(), r.center().y)], Stroke::new(1.0, th.border));
        }
        Block::Image { src, alt, link, width } => render_image(ui, src, alt, *link, *width, w, rc),
        Block::Input { form, field } => {
            let Some(f) = forms.get_mut(*form).and_then(|f| f.fields.get_mut(*field)) else { return };
            let mut submit = false;
            match f.kind.clone() {
                FieldKind::Text | FieldKind::Password => {
                    let resp = ui.add(
                        TextEdit::singleline(&mut f.value)
                            .hint_text(f.placeholder.clone())
                            .password(f.kind == FieldKind::Password)
                            .font(FontId::proportional(base * 0.95))
                            .desired_width((w * 0.7).min(460.0)),
                    );
                    submit = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                }
                FieldKind::TextArea => {
                    ui.add(TextEdit::multiline(&mut f.value).hint_text(f.placeholder.clone()).desired_rows(4).desired_width(w.min(600.0)));
                }
                FieldKind::Checkbox => {
                    let label = if f.placeholder.is_empty() { f.name.clone() } else { f.placeholder.clone() };
                    ui.checkbox(&mut f.checked, label);
                }
                FieldKind::Select(opts) => {
                    let sel = opts.iter().find(|o| o.0 == f.value).map(|o| o.1.clone()).unwrap_or_default();
                    egui::ComboBox::from_id_salt(("sel", *form, *field)).selected_text(sel).show_ui(ui, |ui| {
                        for (v, l) in &opts {
                            ui.selectable_value(&mut f.value, v.clone(), l);
                        }
                    });
                }
                FieldKind::Hidden => {}
            }
            if submit {
                rc.actions.push(Action::Submit { form: *form, button: None });
            }
        }
        Block::Button { form, label, name, value } => {
            let b = egui::Button::new(RichText::new(label).size(base * 0.9).color(Color32::WHITE)).fill(th.accent).corner_radius(CornerRadius::same(14));
            if ui.add(b).clicked() {
                rc.actions.push(Action::Submit { form: *form, button: Some((name.clone(), value.clone())) });
            }
        }
    }
}

fn render_image(ui: &mut Ui, src: &str, alt: &str, link: Option<u32>, width: Option<f32>, w: f32, rc: &mut RenderCtx) {
    let th = rc.th;
    let base = rc.base;
    let link_url = link.and_then(|l| rc.doc.links.get(l as usize)).cloned();
    if !(rc.images_on || rc.forced.contains(src)) {
        let label = if alt.is_empty() { tr("Image").to_string() } else { tf("Image : {}", &[&alt]) };
        let b = egui::Button::new(RichText::new(label).size(base * 0.85).color(th.sub)).fill(th.card).corner_radius(CornerRadius::same(6));
        if ui.add(b).tip(tr("Cliquer pour charger cette image")).clicked() {
            rc.forced.insert(src.to_string());
            rc.actions.push(Action::LoadImage(src.to_string()));
        }
        return;
    }
    let mut request = false;
    match rc.images.get_mut(src) {
        Some(ImgSlot::Ready(tex, size, last)) => {
            *last = rc.frame;
            let zoom = base / 15.5;
            let disp_w = (width.unwrap_or(size.x) * zoom).min(w);
            let disp = vec2(disp_w, disp_w * size.y / size.x.max(1.0));
            let img = egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), disp)).corner_radius(CornerRadius::same(4));
            let resp = ui.add(img.sense(if link_url.is_some() { Sense::click() } else { Sense::hover() }));
            let resp = if alt.is_empty() { resp } else { resp.tip(alt) };
            if let Some(u) = &link_url {
                link_behaviour(ui, &resp, u, rc);
            }
        }
        Some(ImgSlot::Failed) => {
            if !alt.is_empty() {
                ui.label(RichText::new(format!("[{alt}]")).size(base * 0.85).color(th.sub));
            }
        }
        other => {
            request = other.is_none();
            let pw = width.unwrap_or(w * 0.5).clamp(40.0, w);
            let (r, _) = ui.allocate_exact_size(vec2(pw, (pw * 0.5).clamp(24.0, 200.0)), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(6), th.card);
            if alt.is_empty() {
                site_badge(ui.painter(), r.center(), 6.0, "sanaga://", "");
            } else {
                ui.painter().with_clip_rect(r).text(r.center(), Align2::CENTER_CENTER, alt, FontId::proportional(12.0), th.sub);
            }
        }
    }
    if request {
        rc.actions.push(Action::LoadImage(src.to_string()));
    }
}
