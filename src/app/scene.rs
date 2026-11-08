//! How the mobile simulator is presented: the whole page area becomes a
//! "studio" — a chosen backdrop, the device in the middle with its name bar
//! above it, a floating tool bar on the left and the device list in a
//! floating card next to it (dark style, like device-testing tools).

use super::devices::DEVICES;
use super::*;
use crate::frames;
use crate::icons::{self, Icon};
use eframe::egui::{Align2, Color32, RichText, Sense};

/// Backdrops: name, top colour, bottom colour.
pub(super) const BACKDROPS: [(&str, [u8; 3], [u8; 3]); 8] = [
    ("Studio sombre", [0x23, 0x25, 0x2B], [0x15, 0x16, 0x1A]),
    ("Clair", [0xF1, 0xF3, 0xF6], [0xDD, 0xE1, 0xE7]),
    ("Bleu nuit", [0x1D, 0x35, 0x57], [0x0B, 0x13, 0x22]),
    ("Océan", [0x2B, 0x8C, 0xB8], [0x7F, 0xD3, 0xE8]),
    ("Aurore", [0x5B, 0x47, 0x9C], [0xE3, 0xA7, 0xC4]),
    ("Coucher de soleil", [0xF3, 0x8E, 0x5D], [0xFB, 0xD3, 0x8B]),
    ("Forêt", [0x18, 0x4D, 0x47], [0x6E, 0xB0, 0x82]),
    ("Sable", [0xE9, 0xDF, 0xCF], [0xC9, 0xB8, 0x9B]),
];

const CARD: Color32 = Color32::from_rgba_premultiplied(0x22, 0x23, 0x27, 0xF2);
const CARD_LINE: Color32 = Color32::from_rgb(0x3A, 0x3C, 0x42);
const ACCENT: Color32 = Color32::from_rgb(0x40, 0x9E, 0xFF);
const TEXT: Color32 = Color32::from_rgb(0xEC, 0xED, 0xEF);
const SUB: Color32 = Color32::from_rgb(0xA4, 0xA8, 0xB0);

fn c3(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// The parts of the studio: tool bar, device list card (when open), and the
/// room left for the device (its name bar goes above it).
pub(super) fn scene_parts(content: Rect, panel: bool) -> (Rect, Option<Rect>, Rect) {
    let toolbar = Rect::from_min_size(pos2(content.left() + 14.0, content.top() + 14.0), vec2(52.0, (7.0f32 * 46.0 + 14.0).min(content.height() - 28.0)));
    let card = panel.then(|| Rect::from_min_max(pos2(toolbar.right() + 12.0, content.top() + 14.0), pos2(toolbar.right() + 12.0 + 300.0, content.bottom() - 14.0)));
    let left = card.map(|c| c.right()).unwrap_or(toolbar.right()) + 18.0;
    let room = Rect::from_min_max(pos2(left, content.top() + 62.0), pos2(content.right() - 18.0, content.bottom() - 16.0));
    (toolbar, card, room)
}

fn card(p: &egui::Painter, r: Rect, radius: u8) {
    p.rect_filled(r.translate(vec2(0.0, 4.0)).expand(2.0), CornerRadius::same(radius + 2), Color32::from_black_alpha(40));
    p.rect_filled(r, CornerRadius::same(radius), CARD);
    p.rect_stroke(r, CornerRadius::same(radius), Stroke::new(1.0, CARD_LINE), egui::StrokeKind::Inside);
}

/// Dark look for the widgets inside the studio's cards.
fn dark_style(ui: &mut Ui) {
    let mut v = egui::Visuals::dark();
    v.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.widgets.inactive.corner_radius = CornerRadius::same(7);
    v.widgets.hovered.corner_radius = CornerRadius::same(7);
    v.widgets.active.corner_radius = CornerRadius::same(7);
    v.extreme_bg_color = Color32::from_rgb(0x16, 0x17, 0x1A);
    ui.style_mut().visuals = v;
}

impl Sanaga {
    /// The studio's backdrop.
    pub(super) fn paint_scene_background(&self, p: &egui::Painter, area: Rect) {
        let k = self.device.as_ref().map(|d| d.background).unwrap_or(0).min(BACKDROPS.len() - 1);
        let (_, top, bottom) = BACKDROPS[k];
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(area.left_top(), c3(top));
        mesh.colored_vertex(area.right_top(), c3(top));
        mesh.colored_vertex(area.right_bottom(), c3(bottom));
        mesh.colored_vertex(area.left_bottom(), c3(bottom));
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        p.add(egui::Shape::mesh(mesh));
    }

    /// The studio's controls: tool bar, the device's name bar, the device list.
    pub(super) fn device_scene(&mut self, ui: &mut Ui, area: Rect) {
        let Some(mut d) = self.device.clone() else { return };
        let Some(lay) = self.device_layout(area) else { return };
        let (toolbar, card_rect, _) = scene_parts(area, d.panel);
        let mut close = false;
        let mut capture: Option<u8> = None;

        // ---- floating tool bar (left)
        card(ui.painter(), toolbar, 14);
        let mut y = toolbar.top() + 7.0;
        let mut button = |ui: &mut Ui, icon: Icon, on: bool, tip: &str| -> egui::Response {
            let r = Rect::from_min_size(pos2(toolbar.left() + 6.0, y), vec2(40.0, 40.0));
            y += 46.0;
            let resp = ui.interact(r, egui::Id::new(("studio-tool", tip.to_string())), Sense::click());
            if on {
                ui.painter().rect_filled(r, CornerRadius::same(10), ACCENT);
            } else if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(10), Color32::from_white_alpha(22));
            }
            icons::paint(ui.painter(), r.shrink(10.0), icon, if on { Color32::WHITE } else { TEXT });
            resp.tip(tip.to_string())
        };
        if button(ui, Icon::Phone, d.panel, tr("Appareils")).clicked() {
            d.panel = !d.panel;
        }
        if button(ui, Icon::Rotate, d.landscape, tr("Tourner l'appareil (portrait / paysage)")).clicked() {
            d.landscape = !d.landscape;
        }
        let look = button(ui, Icon::Palette, false, tr("Apparence : fond et couleur de l'appareil"));
        egui::Popup::menu(&look).align(egui::RectAlign::RIGHT_START).gap(8.0).show(|ui| {
            ui.set_min_width(250.0);
            ui.label(RichText::new(tr("Fond")).strong());
            ui.horizontal_wrapped(|ui| {
                for (k, (name, top, bottom)) in BACKDROPS.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(44.0, 30.0), Sense::click());
                    let p = ui.painter();
                    let mut mesh = egui::Mesh::default();
                    mesh.colored_vertex(r.left_top(), c3(*top));
                    mesh.colored_vertex(r.right_top(), c3(*top));
                    mesh.colored_vertex(r.right_bottom(), c3(*bottom));
                    mesh.colored_vertex(r.left_bottom(), c3(*bottom));
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    p.add(egui::Shape::mesh(mesh));
                    let ring = if d.background == k { Stroke::new(2.5, ACCENT) } else { Stroke::new(1.0, Color32::from_black_alpha(50)) };
                    p.rect_stroke(r, CornerRadius::same(6), ring, egui::StrokeKind::Outside);
                    if resp.tip(tr(name)).clicked() {
                        d.background = k;
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Couleur de l'appareil")).strong());
            ui.horizontal(|ui| {
                for (f, name, c) in frames::FINISHES {
                    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                    ui.painter().circle_filled(r.center(), 10.0, c3(c));
                    ui.painter().circle_stroke(r.center(), 10.0, Stroke::new(1.0, Color32::from_black_alpha(60)));
                    if d.finish == f && d.frame {
                        ui.painter().circle_stroke(r.center(), 13.0, Stroke::new(2.0, ACCENT));
                    }
                    if resp.tip(tr(name)).clicked() {
                        d.finish = f;
                        d.frame = true;
                    }
                }
            });
            ui.add_space(4.0);
            ui.checkbox(&mut d.frame, tr("Montrer l'appareil autour de l'écran"));
            ui.checkbox(&mut d.browser_ui, tr("Montrer le navigateur de l'appareil (Safari sur Apple, Chrome sur Android et ordinateur)"));
        });
        let rec_on = self.recording.as_ref().is_some_and(|r| !r.stopping);
        let mut toggle_rec = false;
        if button(ui, Icon::Record, rec_on, if rec_on { tr("Arrêter l'enregistrement vidéo") } else { tr("Enregistrer une vidéo de l'appareil") }).clicked() {
            toggle_rec = true;
        }
        let shot = button(ui, Icon::Camera, false, tr("Capture d'écran"));
        egui::Popup::menu(&shot).align(egui::RectAlign::RIGHT_START).gap(8.0).show(|ui| {
            if ui.button(tr("Capture avec l'appareil")).clicked() {
                capture = Some(0);
            }
            if ui.button(tr("Capture de l'écran seul")).clicked() {
                capture = Some(1);
            }
            if ui.button(tr("Capture de la page entière")).clicked() {
                capture = Some(2);
            }
        });
        if button(ui, Icon::Touch, d.touch, tr("Écran tactile (les pages voient un écran tactile)")).clicked() {
            d.touch = !d.touch;
        }
        if button(ui, Icon::Close, false, tr("Quitter le simulateur (Ctrl+Maj+M)")).clicked() {
            close = true;
        }

        // ---- the device's name bar, above it
        let s = d.screen();
        let mut name = format!("{}   {} × {}", d.label(), s.x, s.y);
        if let Some(dev) = d.preset.and_then(|i| DEVICES.get(i)) {
            match dev.os {
                super::devices::Os::Ios => name.push_str("   iOS"),
                super::devices::Os::Android => name.push_str("   Android"),
                _ => {}
            }
        }
        // wide enough for the name and the zoom buttons
        let name_w = ui.painter().layout_no_wrap(name.clone(), FontId::proportional(13.0), TEXT).size().x;
        let bar_w = lay.frame.width().max(name_w + 44.0 + 200.0).min(area.width() - 24.0);
        let bar = Rect::from_center_size(pos2(lay.frame.center().x, lay.frame.top() - 26.0), vec2(bar_w, 36.0));
        card(ui.painter(), bar, 18);
        let name_r = Rect::from_min_max(bar.min + vec2(6.0, 4.0), pos2(bar.right() - 196.0, bar.bottom() - 4.0));
        let nresp = ui.interact(name_r, egui::Id::new("studio-name"), Sense::click());
        if nresp.hovered() {
            ui.painter().rect_filled(name_r, CornerRadius::same(14), Color32::from_white_alpha(18));
        }
        icons::paint(ui.painter(), Rect::from_center_size(pos2(name_r.left() + 16.0, bar.center().y), vec2(15.0, 15.0)), Icon::Phone, ACCENT);
        ui.painter().with_clip_rect(name_r.shrink(2.0)).text(pos2(name_r.left() + 32.0, bar.center().y), Align2::LEFT_CENTER, name, FontId::proportional(13.0), TEXT);
        if nresp.tip(tr("Choisir l'appareil")).clicked() {
            d.panel = !d.panel;
        }
        // zoom: − value +  and "fit"
        let zr = Rect::from_min_max(pos2(bar.right() - 192.0, bar.top() + 4.0), pos2(bar.right() - 6.0, bar.bottom() - 4.0));
        let cur = if d.zoom > 0.0 { d.zoom.min(lay.scale) } else { lay.scale };
        let minus = Rect::from_min_size(zr.min, vec2(28.0, zr.height()));
        let plus = Rect::from_min_size(pos2(zr.left() + 76.0, zr.top()), vec2(28.0, zr.height()));
        let fit = Rect::from_min_max(pos2(plus.right() + 8.0, zr.top()), zr.max);
        for (r, label, id) in [(minus, "−", "studio-zoom-out"), (plus, "+", "studio-zoom-in")] {
            let resp = ui.interact(r, egui::Id::new(id), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(12), Color32::from_white_alpha(22));
            }
            ui.painter().text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(17.0), TEXT);
            if resp.clicked() {
                let step = if id.ends_with("in") { 1.15 } else { 1.0 / 1.15 };
                d.zoom = (cur * step).clamp(0.15, 1.0);
            }
        }
        ui.painter().text(pos2(minus.right() + 24.0, bar.center().y), Align2::CENTER_CENTER, format!("{:.0} %", cur * 100.0), FontId::proportional(12.5), SUB);
        let fresp = ui.interact(fit, egui::Id::new("studio-fit"), Sense::click());
        let fit_on = d.zoom == 0.0;
        ui.painter().rect_filled(fit, CornerRadius::same(12), if fit_on { ACCENT } else if fresp.hovered() { Color32::from_white_alpha(22) } else { Color32::TRANSPARENT });
        ui.painter().text(fit.center(), Align2::CENTER_CENTER, tr("Ajuster"), FontId::proportional(12.0), if fit_on { Color32::WHITE } else { TEXT });
        if fresp.tip(tr("Adapter l'appareil à la fenêtre")).clicked() {
            d.zoom = 0.0;
        }

        // ---- the device's own browser bars work: back, forward, reload, address
        let mut hot_action: Option<frames::Hot> = None;
        let mut addr_rect: Option<Rect> = None;
        if let (Some(style), None) = (lay.style, &lay.custom) {
            let s = d.screen();
            for (k, (hot, x, y, w, h)) in frames::hotspots(style, s.x, s.y, d.landscape, d.browser()).into_iter().enumerate() {
                let r = Rect::from_min_size(lay.frame.min + vec2(x, y) * lay.scale, vec2(w, h) * lay.scale);
                if std::env::var_os("SANAGA_DEBUG_FRAME").is_some() && self.frame_count().is_multiple_of(50) {
                    eprintln!("browser button {hot:?} at {:.0},{:.0}", r.center().x, r.center().y);
                }
                if hot == frames::Hot::Address {
                    // the whole address field (the button inside it is cut off the click area)
                    addr_rect = Some(Rect::from_min_size(r.min, vec2(r.width() + 40.0 * lay.scale, r.height())));
                }
                let resp = ui.interact(r, egui::Id::new(("device-browser", k)), Sense::click());
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let tip = match hot {
                    frames::Hot::Back => tr("Page précédente"),
                    frames::Hot::Forward => tr("Page suivante"),
                    frames::Hot::Reload => tr("Actualiser"),
                    frames::Hot::Address => tr("Saisir une adresse"),
                };
                if resp.tip(tip).clicked() && hot_action.is_none() {
                    hot_action = Some(hot);
                }
            }
        }

        // recording: a red light with the time, left of the name bar
        if let Some(r) = self.recording.as_ref() {
            let secs = (ui.input(|i| i.time) - r.started).max(0.0) as u64;
            let label = if r.stopping { tr("Finalisation de la vidéo…").to_string() } else { format!("REC  {:02}:{:02}", secs / 60, secs % 60) };
            let lw = ui.painter().layout_no_wrap(label.clone(), FontId::proportional(12.5), Color32::WHITE).size().x + 34.0;
            let rr = Rect::from_min_size(pos2(bar.left() - lw - 8.0, bar.top()), vec2(lw, bar.height()));
            ui.painter().rect_filled(rr, CornerRadius::same(18), Color32::from_rgb(0xD9, 0x30, 0x25));
            let blink = (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0 || r.stopping;
            if blink {
                ui.painter().circle_filled(pos2(rr.left() + 15.0, rr.center().y), 5.0, Color32::WHITE);
            }
            ui.painter().text(pos2(rr.left() + 26.0, rr.center().y), Align2::LEFT_CENTER, label, FontId::proportional(12.5), Color32::WHITE);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }

        self.device = Some(d);
        let a = self.active;
        match hot_action {
            Some(frames::Hot::Back) => self.go_history(a, -1),
            Some(frames::Hot::Forward) => self.go_history(a, 1),
            Some(frames::Hot::Reload) => self.reload(a),
            Some(frames::Hot::Address) => {
                // type an address right in the device's own address field
                self.device_url_edit = Some(self.tabs[a].url().to_string());
                self.device_url_new = true;
            }
            None => {}
        }
        if toggle_rec {
            self.toggle_recording();
        }
        // the address being typed, over the drawn address field
        let mut go: Option<String> = None;
        let mut cancel = false;
        if let (Some(r), Some(text)) = (addr_rect, self.device_url_edit.as_mut()) {
            let id = egui::Id::new("device-url");
            let p = ui.painter();
            p.rect_filled(r, CornerRadius::same((r.height() / 2.0).min(40.0) as u8), Color32::WHITE);
            p.rect_stroke(r, CornerRadius::same((r.height() / 2.0).min(40.0) as u8), Stroke::new(2.0, ACCENT), egui::StrokeKind::Outside);
            let font = FontId::proportional((r.height() * 0.42).clamp(10.0, 16.0));
            let resp = ui.put(
                r.shrink2(vec2(10.0, 1.0)),
                egui::TextEdit::singleline(text).id(id).frame(egui::Frame::NONE).font(font).text_color(Color32::from_rgb(0x15, 0x15, 0x18)).vertical_align(egui::Align::Center),
            );
            if self.device_url_new {
                // just opened: it takes the keyboard, the address all selected
                self.device_url_new = false;
                resp.request_focus();
                super::chrome::select_all(ui.ctx(), id, text.chars().count());
            } else if resp.lost_focus() {
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = Some(text.clone());
                } else {
                    cancel = true;
                }
            }
        }
        if let Some(t) = go {
            self.device_url_edit = None;
            let (engine, home) = (self.store.settings.engine, self.store.settings.home.clone());
            let u = input_to_url(&t, engine, &home);
            self.navigate(a, u, None, true);
        } else if cancel || (self.device_url_edit.is_some() && addr_rect.is_none()) {
            self.device_url_edit = None;
        }

        // ---- device list in a floating card
        if let Some(cr) = card_rect {
            card(ui.painter(), cr, 14);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(cr.shrink2(vec2(2.0, 4.0))).layout(egui::Layout::top_down(egui::Align::Min)));
            dark_style(&mut child);
            child.set_clip_rect(cr);
            let dark = crate::theme::theme(true, false);
            self.device_panel(&mut child, &dark);
        }

        if close {
            self.toggle_devices();
            return;
        }
        if let Some(what) = capture {
            self.device_capture(what);
        }
    }
}
