//! Page features found in Safari: share, print, translate, page source, web
//! inspector, full screen, web apps ("Add to Dock"), per-site zoom and the
//! privacy report (cross-site tracking prevention).

use super::*;
use crate::icons::{self, Icon};
use super::chrome::site_badge;
use eframe::egui::{Align2, Color32, Margin, RichText, Sense};
use webkit6::prelude::*;

impl Sanaga {
    pub(super) fn current_page(&self) -> Option<(String, String)> {
        let t = self.tabs.get(self.active)?;
        if t.entries.is_empty() || t.url().starts_with("sanaga://") {
            return None;
        }
        Some((t.url().to_string(), t.title()))
    }

    pub(super) fn copy_link(&self) {
        if let Some((u, _)) = self.current_page() {
            self.ctx.copy_text(u);
        }
    }

    pub(super) fn email_link(&self) {
        if let Some((u, t)) = self.current_page() {
            let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>().replace('+', "%20");
            let mail = format!("mailto:?subject={}&body={}", enc(&t), enc(&u));
            let _ = std::process::Command::new("xdg-open").arg(mail).spawn();
        }
    }

    pub(super) fn print_page(&self) {
        if let Some(w) = self.tabs.get(self.active).and_then(|t| t.web.as_ref()) {
            let _ = w.print();
        }
    }

    pub(super) fn open_inspector(&mut self) {
        if let Some(w) = self.tabs.get(self.active).and_then(|t| t.web.as_ref()) {
            // docked under the page, inside Sanaga's window
            use webkit6::prelude::*;
            if let Some(ins) = w.webview().inspector() {
                // a second press closes it (as F12 does in Chrome)
                if ins.is_attached() {
                    ins.close();
                    return;
                }
                ins.show();
                ins.attach();
            } else {
                w.open_devtools();
            }
        }
    }

    pub(super) fn view_source(&mut self) {
        if let Some((u, _)) = self.current_page() {
            self.open_tab(&format!("view-source:{u}"), self.tabs[self.active].private);
        }
    }

    pub(super) fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        crate::desktop::toggle_fullscreen(&self.ctx, self.fullscreen);
    }

    /// "Add to Dock": an application-menu entry that opens this site on its own.
    pub(super) fn make_web_app(&mut self) -> Option<String> {
        let (u, title) = self.current_page()?;
        let host = host_of(&u);
        let exe = std::env::current_exe().ok()?;
        let dir = std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")))?
            .join("applications");
        std::fs::create_dir_all(&dir).ok()?;
        let safe: String = host.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        let name = if title.trim().is_empty() { host.clone() } else { title.replace(['\n', '\r'], " ") };
        let exe_q = exe.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
        let desktop = format!(
            "[Desktop Entry]\nType=Application\nName={name}\nComment={host} — application web Sanaga\nExec=\"{exe_q}\" --app \"{u}\"\nIcon=web-browser\nTerminal=false\nCategories=Network;WebBrowser;\nStartupWMClass=sanaga\n"
        );
        let path = dir.join(format!("sanaga-web-{safe}.desktop"));
        std::fs::write(&path, desktop).ok()?;
        Some(name)
    }

    /// Zoom remembered per site.
    pub(super) fn remember_zoom(&mut self) {
        let t = &self.tabs[self.active];
        let host = host_of(t.url());
        if t.url().starts_with("http") {
            let z = t.zoom;
            self.store.set_site_zoom(&host, z);
        }
    }

    pub(super) fn apply_site_zoom(&mut self, ti: usize) {
        let Some(t) = self.tabs.get_mut(ti) else { return };
        let host = host_of(t.url());
        t.zoom = self.store.site_zoom.get(&host).copied().unwrap_or(self.store.settings.zoom);
    }

    /// Ask WebKit which trackers it kept from following the person (privacy report).
    pub(super) fn refresh_privacy(&mut self, now: f64) {
        if now - self.privacy_asked < 15.0 {
            return;
        }
        self.privacy_asked = now;
        let Some(w) = self.tabs.iter().find_map(|t| t.web.as_ref()) else { return };
        let Some(dm) = w.webview().network_session() else { return };
        let tx = self.web_tx.clone();
        dm.itp_summary(None::<&webkit6::gio::Cancellable>, move |res| {
            if let Ok(list) = res {
                let mut out: Vec<(String, usize)> = list
                    .iter()
                    .filter_map(|tp| tp.domain().map(|d| (d.to_string(), tp.first_parties().len())))
                    .collect();
                out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                let _ = tx.send(web::WebEvent::Privacy(out));
            }
        });
    }

    /// The Share popover: the page (picture, title, site), its link with a
    /// Copy button, "Send to" (e-mail and the usual sharing sites, opened in
    /// a new tab), then the actions as tiles (reading list, favourites,
    /// print, web app, QR code).
    pub(super) fn share_popover(&mut self, ctx: &egui::Context, th: &Theme) {
        if !self.share_open {
            return;
        }
        let w = 364.0;
        let (pos, pivot, _) = super::layout::popover_place(ctx, self.share_rect, (self.share_rect.right() - w).max(8.0), 6.0);
        let mut act: Option<&'static str> = None;
        let url = self.tabs[self.active].url().to_string();
        let title = self.tabs[self.active].title();
        let in_list = self.store.in_reading_list(&url);
        let fav = self.store.is_bookmarked(&url);
        let web = self.tabs[self.active].web.is_some() && matches!(self.tabs[self.active].view, View::Web);
        let shareable = url.starts_with("http");
        let qr = if self.share_qr && shareable { self.qr_texture(&url) } else { None };
        let now = ctx.input(|i| i.time);
        let copied = now - self.share_copied_at < 2.0;
        let dark = self.store.settings.dark;
        let host = host_of(&url);
        let area = egui::Area::new(egui::Id::new("share-pop")).order(egui::Order::Foreground).pivot(pivot).fixed_pos(pos).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.hairline.gamma_multiply(0.6)))
                .corner_radius(CornerRadius::same(18))
                .shadow(egui::Shadow { offset: [0, 10], blur: 32, spread: 0, color: Color32::from_black_alpha(if dark { 120 } else { 50 }) })
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    let iw = w - 28.0;
                    ui.set_width(iw);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    // the page
                    let (head, _) = ui.allocate_exact_size(vec2(iw, 46.0), Sense::hover());
                    let tile = Rect::from_min_size(head.min, vec2(46.0, 46.0));
                    ui.painter().rect_filled(tile, CornerRadius::same(12), th.omnibox);
                    site_badge(ui.painter(), tile.center(), 11.0, &url, &title);
                    let clip = Rect::from_min_max(pos2(tile.right() + 12.0, head.top()), head.max);
                    let p = ui.painter().with_clip_rect(clip);
                    p.text(pos2(clip.left(), head.top() + 13.0), Align2::LEFT_CENTER, if title.is_empty() { &host } else { &title }, FontId::new(14.5, bold()), th.text);
                    p.text(pos2(clip.left(), head.top() + 33.0), Align2::LEFT_CENTER, &host, FontId::proportional(12.5), th.sub);
                    ui.add_space(12.0);
                    // the link and Copy
                    let (lr, _) = ui.allocate_exact_size(vec2(iw, 36.0), Sense::hover());
                    ui.painter().rect_filled(lr, CornerRadius::same(10), th.omnibox);
                    let bw = 86.0;
                    let br = Rect::from_min_size(pos2(lr.right() - bw - 3.0, lr.top() + 3.0), vec2(bw, 30.0));
                    let bresp = ui.interact(br, egui::Id::new("share-copy"), Sense::click());
                    let bcol = if copied { Color32::from_rgb(0x2E, 0xA0, 0x4F) } else { th.accent };
                    ui.painter().rect_filled(br, CornerRadius::same(8), if bresp.hovered() { bcol.gamma_multiply(0.85) } else { bcol });
                    ui.painter().text(br.center(), Align2::CENTER_CENTER, if copied { tr("Copié ✓") } else { tr("Copier") }, FontId::new(13.0, bold()), Color32::WHITE);
                    let lclip = Rect::from_min_max(pos2(lr.left() + 12.0, lr.top()), pos2(br.left() - 8.0, lr.bottom()));
                    ui.painter().with_clip_rect(lclip).text(pos2(lclip.left(), lr.center().y), Align2::LEFT_CENTER, &url, FontId::proportional(12.5), th.sub);
                    if bresp.clicked() {
                        act = Some("copy");
                    }
                    if shareable {
                        ui.add_space(16.0);
                        ui.label(RichText::new(tr("Envoyer vers")).size(11.5).family(bold()).color(th.sub));
                        ui.add_space(8.0);
                        // e-mail, then sites: round buttons (initials, no logos)
                        let targets: [(&'static str, &str, Option<Icon>); 6] = [
                            ("mail", tr("E-mail"), Some(Icon::Mail)),
                            ("wa", "WhatsApp", None),
                            ("tg", "Telegram", None),
                            ("x", "X", None),
                            ("fb", "Facebook", None),
                            ("in", "LinkedIn", None),
                        ];
                        let cw = iw / targets.len() as f32;
                        let (row, _) = ui.allocate_exact_size(vec2(iw, 66.0), Sense::hover());
                        for (k, (id, label, icon)) in targets.iter().enumerate() {
                            let cell = Rect::from_min_size(pos2(row.left() + k as f32 * cw, row.top()), vec2(cw, 66.0));
                            let c = pos2(cell.center().x, cell.top() + 21.0);
                            let resp = ui.interact(cell, egui::Id::new(("share-to", k)), Sense::click());
                            let fill = if resp.hovered() { th.accent } else { th.omnibox };
                            let ink = if resp.hovered() { Color32::WHITE } else { th.text };
                            ui.painter().circle_filled(c, 21.0, fill);
                            match icon {
                                Some(i) => icons::paint(ui.painter(), Rect::from_center_size(c, vec2(18.0, 18.0)), *i, ink),
                                None => {
                                    let ini: String = label.chars().take(if *id == "in" { 2 } else { 1 }).collect();
                                    ui.painter().text(c, Align2::CENTER_CENTER, ini, FontId::new(15.0, bold()), ink);
                                }
                            }
                            ui.painter().with_clip_rect(cell.shrink2(vec2(1.0, 0.0))).text(pos2(c.x, cell.bottom() - 8.0), Align2::CENTER_CENTER, *label, FontId::proportional(10.5), th.sub);
                            if resp.clicked() {
                                act = Some(id);
                            }
                        }
                    }
                    ui.add_space(14.0);
                    // the actions, as tiles (two per row)
                    let tiles: [(Icon, &str, &'static str, bool, bool); 6] = [
                        (Icon::Glasses, if in_list { tr("Dans la liste de lecture") } else { tr("Liste de lecture") }, "reading", true, in_list),
                        (if fav { Icon::StarFilled } else { Icon::Star }, if fav { tr("Dans les favoris") } else { tr("Ajouter aux favoris") }, "fav", true, fav),
                        (Icon::Print, tr("Imprimer…"), "print", web, false),
                        (Icon::Plus, tr("Application web"), "webapp", true, false),
                        (Icon::Grid, if qr.is_some() { tr("Masquer le code QR") } else { tr("Code QR") }, "qr", shareable, qr.is_some()),
                        (Icon::Share, tr("Copier titre et lien"), "copy-both", true, false),
                    ];
                    let tw = (iw - 8.0) / 2.0;
                    for pair in tiles.chunks(2) {
                        let (row, _) = ui.allocate_exact_size(vec2(iw, 44.0), Sense::hover());
                        for (k, (icon, label, id, enabled, on)) in pair.iter().enumerate() {
                            let r = Rect::from_min_size(pos2(row.left() + k as f32 * (tw + 8.0), row.top()), vec2(tw, 38.0));
                            let resp = ui.interact(r, egui::Id::new(("share-tile", *id)), if *enabled { Sense::click() } else { Sense::hover() });
                            let fill = if *enabled && resp.hovered() { th.omnibox_hover } else { th.omnibox };
                            ui.painter().rect_filled(r, CornerRadius::same(10), fill);
                            let ink = if !*enabled { th.sub.gamma_multiply(0.6) } else if *on { th.accent } else { th.text };
                            icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(15.0, 15.0)), *icon, ink);
                            let clip = Rect::from_min_max(pos2(r.left() + 36.0, r.top()), pos2(r.right() - 6.0, r.bottom()));
                            ui.painter().with_clip_rect(clip).text(pos2(clip.left(), r.center().y), Align2::LEFT_CENTER, *label, FontId::proportional(12.5), ink);
                            if *enabled && resp.clicked() {
                                act = Some(id);
                            }
                        }
                    }
                    if let Some(tex) = &qr {
                        ui.add_space(6.0);
                        ui.vertical_centered(|ui| {
                            Frame::new().fill(Color32::WHITE).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(10)).show(ui, |ui| {
                                ui.add(egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), vec2(168.0, 168.0))));
                            });
                            ui.add_space(6.0);
                            ui.label(RichText::new(tr("Scannez-le avec votre téléphone")).size(11.5).color(th.sub));
                        });
                    }
                });
        });
        let share_rect = self.share_rect;
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) || (ctx.input(|i| i.pointer.any_pressed()) && !ctx.pointer_interact_pos().map(|p| area.response.rect.contains(p) || share_rect.contains(p)).unwrap_or(false)) {
            self.share_open = false;
        }
        if copied {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        let Some(id) = act else { return };
        let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
        let send_to = match id {
            "wa" => Some(format!("https://wa.me/?text={}", enc(&format!("{title} {url}")))),
            "tg" => Some(format!("https://t.me/share/url?url={}&text={}", enc(&url), enc(&title))),
            "x" => Some(format!("https://x.com/intent/post?url={}&text={}", enc(&url), enc(&title))),
            "fb" => Some(format!("https://www.facebook.com/sharer/sharer.php?u={}", enc(&url))),
            "in" => Some(format!("https://www.linkedin.com/sharing/share-offsite/?url={}", enc(&url))),
            _ => None,
        };
        if let Some(u) = send_to {
            self.share_open = false;
            self.open_tab(&u, self.tabs[self.active].private);
            return;
        }
        match id {
            "qr" => self.share_qr = !self.share_qr,
            "copy" => {
                self.copy_link();
                self.share_copied_at = now;
            }
            "copy-both" => {
                ctx.copy_text(if title.is_empty() { url.clone() } else { format!("{title}\n{url}") });
                self.share_copied_at = now;
            }
            "mail" => {
                self.share_open = false;
                self.email_link();
            }
            "print" => {
                self.share_open = false;
                self.print_page();
            }
            "reading" => self.toggle_reading_list(),
            "fav" => self.toggle_bookmark(),
            "webapp" => {
                self.share_open = false;
                if let Some(name) = self.make_web_app() {
                    self.notice = Some((tf("« {} » a été ajoutée au menu des applications.", &[&name]), now));
                }
            }
            _ => {}
        }
    }

    /// Short message shown at the bottom of the window for a few seconds.
    pub(super) fn show_notice(&mut self, ctx: &egui::Context, content: Rect, th: &Theme) {
        let Some((msg, t0)) = &self.notice else { return };
        let now = ctx.input(|i| i.time);
        if now - t0 > 4.0 {
            self.notice = None;
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("notice")));
        let g = painter.layout_no_wrap(msg.clone(), FontId::proportional(13.0), Color32::WHITE);
        let r = Rect::from_center_size(pos2(content.center().x, content.bottom() - 40.0), vec2(g.size().x + 36.0, 34.0));
        painter.rect_filled(r, CornerRadius::same(17), Color32::from_rgba_unmultiplied(30, 30, 32, 230));
        painter.galley(pos2(r.left() + 18.0, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let _ = th;
    }
}
