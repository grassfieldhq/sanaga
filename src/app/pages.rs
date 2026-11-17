//! Built-in pages: new tab, history, bookmarks, downloads, settings, about, errors.

use super::chrome::site_badge;
use super::chrome::Fit as _;
use super::*;
use crate::icons::{self, Icon};
use crate::store;
use eframe::egui::{Align, Align2, Color32, CursorIcon, Label, Layout, Margin, RichText, Sense, TextEdit};

pub(super) fn page_title(ui: &mut Ui, th: &Theme, title: &str) {
    ui.add_space(28.0);
    ui.label(RichText::new(title).size(26.0).family(bold()).color(th.text));
    ui.add_space(14.0);
}

pub(super) fn centered_column(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    let cw = (w - 64.0).min(760.0);
    let x = (w - cw) / 2.0;
    ui.horizontal(|ui| {
        ui.add_space(x);
        ui.vertical(|ui| {
            ui.set_width(cw);
            add(ui);
        });
    });
}

fn list_row(ui: &mut Ui, th: &Theme, url: &str, title: &str, meta: &str, go: &mut Option<(String, bool)>, remove: &mut Option<usize>, i: usize) {
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(8), th.card);
    }
    let c = pos2(r.left() + 20.0, r.center().y);
    site_badge(ui.painter(), c, 9.0, url, title);
    let right = r.right() - 40.0;
    let meta_w = if meta.is_empty() { 0.0 } else { 110.0 };
    let clip = Rect::from_min_max(pos2(c.x + 20.0, r.top()), pos2(right - meta_w, r.bottom()));
    let p = ui.painter().with_clip_rect(clip);
    let t = if title.is_empty() { url } else { title };
    let g = p.fit(pos2(clip.left(), r.center().y), Align2::LEFT_CENTER, t, FontId::proportional(14.0), th.text);
    p.fit(pos2(g.right() + 10.0, r.center().y), Align2::LEFT_CENTER, site_label(url), FontId::proportional(12.5), th.sub);
    if !meta.is_empty() {
        ui.painter().text(pos2(right - 4.0, r.center().y), Align2::RIGHT_CENTER, meta, FontId::proportional(12.0), th.sub);
    }
    let xr = Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(24.0, 24.0));
    let xresp = ui.interact(xr, ui.id().with(("rm", i, url)), Sense::click()).tip(tr("Supprimer"));
    if xresp.hovered() {
        ui.painter().circle_filled(xr.center(), 12.0, th.border);
    }
    if resp.hovered() || xresp.hovered() {
        icons::paint(ui.painter(), xr.shrink(6.0), Icon::Close, th.sub);
    }
    if xresp.clicked() {
        *remove = Some(i);
        return;
    }
    let resp = resp.tip(url).on_hover_cursor(CursorIcon::PointingHand);
    if resp.middle_clicked() || (resp.clicked() && ui.input(|i| i.modifiers.command)) {
        *go = Some((url.to_string(), true));
    } else if resp.clicked() {
        *go = Some((url.to_string(), false));
    }
}

impl Sanaga {
    /// The history page: every visit, newest first, in groups of days.
    ///
    /// The whole history is reachable — it showed 400 lines, then asked to
    /// "refine the search" — and searched; and none of it is held here:
    /// the page knows how many lines there are (`Listing`), takes the room
    /// they need, and reads and lays out only the lines on screen, by
    /// groups of 200 as the person scrolls (`history_rows_ask`).
    pub(super) fn page_history(&mut self, ui: &mut Ui, th: &Theme) {
        use super::hist::CHUNK;
        const ROW: f32 = 40.0;
        const HEAD: f32 = 36.0;
        let mut go: Option<(String, bool)> = None;
        let mut clear = false;
        let mut remove: Option<i64> = None;
        // what is listed is asked again when the search or the history
        // changes (a visit in any window, a line removed), not at every frame
        let filter = self.history_filter.trim().to_lowercase();
        let searched = filter.clone();
        self.history_ask(|app| &mut app.hist.listing, (self.store.history.version(), filter.clone()), move |h| Arc::new(h.list(&searched, store::now())));
        // (while a new answer is on its way, the list shown is the one before)
        let (listing, answer) = (self.hist.listing.got.clone(), self.hist.listing.answers);
        // (another search: its first results are shown, wherever the list
        // of before had been scrolled to)
        if self.hist.listed_for != filter {
            self.hist.listed_for = filter.clone();
            ui.scroll_to_cursor(Some(Align::TOP));
        }
        let mut wanted: Vec<usize> = Vec::new();
        let mut on_screen: Option<(usize, usize)> = None;
        let chunks = &self.hist.chunks;
        let hf = &mut self.history_filter;
        // the keyboard alone: End goes to the oldest visit, Home to the newest
        if !ui.ctx().egui_wants_keyboard_input() {
            let page = ui.clip_rect().height() - ROW;
            let (home, end, up, down) = ui.input(|i| (i.key_pressed(Key::Home), i.key_pressed(Key::End), i.key_pressed(Key::PageUp), i.key_pressed(Key::PageDown)));
            let by = if home {
                1.0e9
            } else if end {
                -1.0e9
            } else {
                page * (f32::from(u8::from(up)) - f32::from(u8::from(down)))
            };
            if by != 0.0 {
                ui.scroll_with_delta(vec2(0.0, by));
            }
        }
        centered_column(ui, |ui| {
            page_title(ui, th, tr("Historique"));
            ui.horizontal(|ui| {
                ui.add(TextEdit::singleline(hf).hint_text(tr("Rechercher dans l'historique")).desired_width(320.0));
                if listing.total > 0 {
                    let n = listing.total;
                    let count = match (filter.is_empty(), n) {
                        (true, 1) => tr("1 visite").to_string(),
                        (true, _) => tf("{} visites", &[&n]),
                        (false, 1) => tr("1 résultat").to_string(),
                        (false, _) => tf("{} résultats", &[&n]),
                    };
                    ui.label(RichText::new(count).size(12.5).color(th.sub));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if (listing.total > 0 || !filter.is_empty()) && super::chrome::confirm_button(ui, "clear-history", tr("Effacer l'historique")) {
                        clear = true;
                    }
                });
            });
            ui.add_space(12.0);
            if listing.total == 0 {
                // (nothing said before the history has answered once)
                if answer > 0 {
                    ui.label(RichText::new(if filter.is_empty() { tr("Les pages que vous visitez apparaîtront ici.") } else { tr("Aucun résultat") }).color(th.sub));
                }
                return;
            }
            // The room of every line is taken at once (the scroll bar spans
            // the whole history); only the lines inside the window are
            // looked up and drawn.
            let w = ui.available_width();
            // (a line and the gap under it, as when they were laid out one by one)
            let pitch = ROW + ui.spacing().item_spacing.y;
            let heads = listing.groups.iter().filter(|n| **n > 0).count();
            let (area, _) = ui.allocate_exact_size(vec2(w, listing.total as f32 * pitch + heads as f32 * HEAD), Sense::hover());
            let clip = ui.clip_rect();
            let (mut y, mut first) = (area.top(), 0);
            for (k, n) in listing.groups.iter().copied().enumerate() {
                if n == 0 {
                    continue;
                }
                if y + HEAD > clip.top() && y < clip.bottom() {
                    ui.painter().text(pos2(area.left(), y + 12.0), Align2::LEFT_TOP, store::day_group_name(k), FontId::new(13.0, bold()), th.accent);
                }
                y += HEAD;
                let from = (((clip.top() - y) / pitch).floor().max(0.0) as usize).min(n);
                let to = (((clip.bottom() - y) / pitch).ceil().max(0.0) as usize).min(n);
                for line in first + from..first + to {
                    let rect = Rect::from_min_size(pos2(area.left(), y + (line - first) as f32 * pitch), vec2(w, ROW));
                    let chunk = line / CHUNK;
                    on_screen = Some(on_screen.map_or((chunk, chunk), |(a, b)| (a.min(chunk), b.max(chunk))));
                    let read = chunks.get(&chunk);
                    // (not read yet, or read for the list of before: asked for)
                    if !read.is_some_and(|(of, _)| *of == answer) && !wanted.contains(&chunk) {
                        wanted.push(chunk);
                    }
                    match read.and_then(|(_, rows)| rows.get(line % CHUNK)) {
                        Some(row) => {
                            let mut line_ui = ui.new_child(egui::UiBuilder::new().id_salt(("history-line", row.id)).max_rect(rect).layout(Layout::top_down(Align::Min)));
                            let mut removed = None;
                            list_row(&mut line_ui, th, &row.url, &row.title, &store::relative_time(row.time), &mut go, &mut removed, line);
                            if removed.is_some() {
                                remove = Some(row.id);
                            }
                        }
                        // (on its way: its place is marked)
                        None => {
                            ui.painter().circle_filled(pos2(rect.left() + 20.0, rect.center().y), 9.0, th.card);
                            ui.painter().rect_filled(Rect::from_min_size(pos2(rect.left() + 40.0, rect.center().y - 5.0), vec2(rect.width() * 0.4, 10.0)), CornerRadius::same(5), th.card);
                        }
                    }
                }
                y += n as f32 * pitch;
                first += n;
            }
        });
        for chunk in wanted {
            self.history_rows_ask(chunk);
        }
        // (the lines far from the window are let go, and read again if the
        // person comes back to them)
        match on_screen {
            Some((a, b)) => self.hist.chunks.retain(|c, _| *c + 2 >= a && *c <= b + 2),
            None => self.hist.chunks.clear(),
        }
        if clear {
            self.store.clear_history();
            self.visited.clear();
        }
        if let Some(id) = remove {
            self.store.history.remove_visit(id);
        }
        self.apply_go(go);
    }

    /// The system's file picker (GTK), when WebKit/GTK is running.
    pub(super) fn choose_file(&mut self, title: &str, patterns: &[&str], filter_name: &str) -> Option<std::path::PathBuf> {
        if !self.web_ok {
            self.notice = Some((tr("Le sélecteur de fichiers n'est pas disponible ici.").into(), self.ctx.input(|i| i.time)));
            return None;
        }
        crate::shell::choose_file(title, patterns, filter_name, None)
    }

    pub(super) fn page_downloads(&mut self, ui: &mut Ui, th: &Theme) {
        let store = &self.store;
        centered_column(ui, |ui| {
            page_title(ui, th, tr("Téléchargements"));
            // the downloads under way: how far each one is, and a way to
            // stop it
            let running = crate::page::running_downloads();
            if store.downloads.is_empty() && running.is_empty() {
                ui.label(RichText::new(tr("Les fichiers que vous téléchargez apparaîtront ici. Ils sont enregistrés dans votre dossier Téléchargements.")).color(th.sub));
            }
            for d in &running {
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(12)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                        icons::paint(ui.painter(), r, Icon::Download, th.accent);
                        ui.vertical(|ui| {
                            let name = d.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            ui.label(RichText::new(name).size(14.0).color(th.text));
                            let s = if d.total > 0 { tf("{} sur {}", &[&net::human_bytes(d.received), &net::human_bytes(d.total)]) } else { net::human_bytes(d.received) };
                            ui.label(RichText::new(s).size(12.0).color(th.sub));
                            if d.total > 0 {
                                let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width().min(360.0), 4.0), Sense::hover());
                                ui.painter().rect_filled(bar, 2.0, th.border);
                                let done = (d.received as f32 / d.total as f32).clamp(0.0, 1.0);
                                ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * done, bar.height())), 2.0, th.accent);
                            }
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button(tr("Annuler")).clicked() {
                                crate::page::stop_download(&d.path);
                            }
                        });
                    });
                });
                ui.add_space(6.0);
            }
            if !running.is_empty() {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            }
            for d in store.downloads.iter().rev() {
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(12)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                        icons::paint(ui.painter(), r, Icon::Download, th.accent);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&d.name).size(14.0).color(th.text));
                            let s = if d.ok { format!("{} · {} · {}", net::human_bytes(d.bytes), store::relative_time(d.time), d.path.display()) } else { tf("Échec : {}", &[&d.error]) };
                            ui.label(RichText::new(s).size(12.0).color(th.sub));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if d.ok && ui.button(tr("Afficher dans le dossier")).clicked() {
                                if let Some(dir) = d.path.parent() {
                                    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                                }
                            }
                        });
                    });
                });
                ui.add_space(6.0);
            }
        });
    }

    /// `sanaga://activity`: what Sanaga uses right now, process by process,
    /// and tab by tab (memory, processor, state) — any running tab can be
    /// put to sleep.
    pub(super) fn page_activity(&mut self, ui: &mut Ui, th: &Theme) {
        let list = net::processes_cached();
        // which process serves which tab: each page's process says who it is
        // (page-helper/); pages opened by one another share one
        let serves: Vec<Option<usize>> = self
            .tabs
            .iter()
            .map(|t| {
                let (start, inner) = t.web.as_ref()?.process()?;
                list.iter().position(|p| p.kind == 2 && p.start == start && p.inner_pid == inner)
            })
            .collect();
        let shared = |k: usize| serves.iter().filter(|s| **s == Some(k)).count() > 1;
        let any_known = serves.iter().any(|s| s.is_some());
        let any_shared = (0..list.len()).any(shared);
        // (measured every 2 s: the page follows)
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(2));
        let total: u64 = list.iter().map(|p| p.bytes).sum();
        let cpu: f32 = list.iter().map(|p| p.cpu).sum();
        let running = self.tabs.iter().filter(|t| t.web.is_some()).count();
        let mut sleep: Option<usize> = None;
        let mut show: Option<usize> = None;
        let active = self.active;
        centered_column(ui, |ui| {
            ui.add_space(26.0);
            ui.label(RichText::new(tr("Activité")).size(26.0).family(bold()).color(th.text));
            ui.label(
                RichText::new(tf("{} de mémoire · {} % d'un cœur · {} page(s) en cours sur {} onglet(s)", &[&net::human_bytes(total), &format!("{cpu:.0}"), &running, &self.tabs.len()])).color(th.sub),
            );
            ui.add_space(16.0);
            let card = |ui: &mut Ui, body: &mut dyn FnMut(&mut Ui)| {
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(16)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    body(ui);
                });
            };
            ui.label(RichText::new(tr("Processus")).size(15.0).family(bold()).color(th.text));
            ui.add_space(6.0);
            card(ui, &mut |ui| {
                let mut page = 0;
                for (k, p) in list.iter().enumerate() {
                    let name = match p.kind {
                        0 => tr("Interface de Sanaga").to_string(),
                        1 => tr("Réseau").to_string(),
                        2 => {
                            page += 1;
                            // named after the tab it serves (the first one, when shared)
                            match serves.iter().position(|s| *s == Some(k)) {
                                Some(i) => {
                                    let title: String = self.tabs[i].title().chars().take(48).collect();
                                    if shared(k) {
                                        format!("{title} *")
                                    } else {
                                        title
                                    }
                                }
                                None => tf("Page {}", &[&page]),
                            }
                        }
                        _ => continue,
                    };
                    if p.kind == 2 && p.bytes == 0 {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).color(th.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(format!("{:.0} %", p.cpu)).color(if p.cpu >= 20.0 { th.accent } else { th.sub }));
                            ui.add_space(18.0);
                            ui.label(RichText::new(net::human_bytes(p.bytes)).color(th.text));
                        });
                    });
                }
                // the small helpers of the engine (sandbox, display), together
                let (other_bytes, other_cpu) = list.iter().filter(|p| p.kind == 3).fold((0u64, 0f32), |a, p| (a.0 + p.bytes, a.1 + p.cpu));
                if other_bytes > 1 << 20 || other_cpu >= 1.0 {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("Autres (bac à sable, auxiliaires)")).color(th.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(format!("{other_cpu:.0} %")).color(th.sub));
                            ui.add_space(18.0);
                            ui.label(RichText::new(net::human_bytes(other_bytes)).color(th.text));
                        });
                    });
                }
                if list.is_empty() {
                    ui.label(RichText::new(tr("Mesure en cours…")).color(th.sub));
                }
            });
            ui.add_space(4.0);
            let note = if any_known {
                tr("Chaque page a son propre processus. Ceux sans nom sont gardés prêts par le moteur pour la prochaine page, ou finissent de se fermer.")
            } else {
                tr("Chaque page a son propre processus ; le moteur ne dit pas à quel onglet chacun appartient.")
            };
            ui.label(RichText::new(note).size(12.0).color(th.sub));
            if any_shared {
                ui.label(
                    RichText::new(tr("* Processus partagé par plusieurs onglets (des pages ouvertes l'une par l'autre) : sa mémoire et son processeur comptent pour chacun.")).size(12.0).color(th.sub),
                );
            }
            ui.add_space(18.0);
            ui.label(RichText::new(tr("Onglets")).size(15.0).family(bold()).color(th.text));
            ui.add_space(6.0);
            card(ui, &mut |ui| {
                for (i, t) in self.tabs.iter().enumerate() {
                    let title = t.title();
                    let state = if t.web.as_ref().is_some_and(|w| !w.responsive()) {
                        tr("ne répond plus")
                    } else if i == active {
                        tr("affiché")
                    } else if t.web.is_some() && t.audio {
                        tr("en cours · son")
                    } else if t.web.is_some() {
                        tr("en cours")
                    } else {
                        tr("en veille")
                    };
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                        site_badge(ui.painter(), r.center(), 8.0, t.url(), &title);
                        let short: String = title.chars().take(56).collect();
                        if ui.add(Label::new(RichText::new(short).color(th.text)).sense(Sense::click()).truncate()).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            show = Some(i);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            // (the tab shown, pinned tabs and unsent forms never sleep)
                            if i != active && t.web.is_some() && !t.pinned && !t.form_dirty && ui.button(tr("Mettre en veille")).clicked() {
                                sleep = Some(i);
                            }
                            // what this tab's page uses (its process)
                            if let Some(k) = serves[i] {
                                let p = &list[k];
                                let star = if shared(k) { " *" } else { "" };
                                ui.label(RichText::new(format!("{:.0} %", p.cpu)).color(if p.cpu >= 20.0 { th.accent } else { th.sub }));
                                ui.add_space(10.0);
                                ui.label(RichText::new(format!("{}{star}", net::human_bytes(p.bytes))).color(th.text));
                                ui.add_space(10.0);
                            }
                            ui.label(RichText::new(state).color(th.sub));
                        });
                    });
                }
            });
            ui.add_space(30.0);
        });
        if let Some(i) = sleep {
            super::plus::put_to_sleep(&mut self.tabs[i]);
        }
        if let Some(i) = show {
            self.current_group = self.tabs[i].group;
            self.active = i;
        }
    }

    pub(super) fn page_about(&mut self, ui: &mut Ui, th: &Theme) {
        let cached: usize = self.tabs.iter().flat_map(|t| t.entries.iter()).filter_map(|e| e.doc.as_ref()).map(|d| d.bytes).sum();
        let rows = [
            (tr("Moteur web"), format!("WebKitGTK {}", crate::page::engine_version())),
            (tr("Mémoire utilisée"), net::human_bytes(self.mem.1)),
            (tr("Onglets ouverts"), self.tabs.len().to_string()),
            (tr("Pages gardées en mémoire"), net::human_bytes(cached as u64)),
            (tr("Images en mémoire"), net::human_bytes(self.img_bytes as u64)),
            (tr("Pages chargées depuis le démarrage"), self.pages_loaded.to_string()),
            (tr("Données téléchargées"), net::human_bytes(self.bytes_loaded)),
        ];
        let mut open_welcome = false;
        let mut open_crashes = false;
        let (mut check_update, mut open_release) = (false, false);
        let mut open_activity = false;
        centered_column(ui, |ui| {
            ui.add_space(30.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
                icons::logo(ui.painter(), r.center(), 34.0);
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Sanaga").size(30.0).family(bold()).color(th.text));
                    ui.label(RichText::new(tf("Version {} · navigateur léger", &[&env!("CARGO_PKG_VERSION")])).color(th.sub));
                });
            });
            // A new version: asked on GitHub when the person clicks, never
            // by itself (Sanaga makes no request of its own). Until now
            // nothing told that a newer Sanaga existed.
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let asking = matches!(self.update_check, Some(None));
                if ui.add_enabled(!asking, egui::Button::new(tr("Rechercher une nouvelle version"))).clicked() {
                    check_update = true;
                }
                match &self.update_check {
                    Some(None) => {
                        ui.add(egui::Spinner::new().size(14.0));
                    }
                    Some(Some(Ok(v))) if net::is_newer(v, env!("CARGO_PKG_VERSION")) => {
                        ui.label(RichText::new(tf("Sanaga {} est disponible.", &[v])).color(th.text));
                        if ui.link(tr("Voir la nouvelle version")).clicked() {
                            open_release = true;
                        }
                    }
                    Some(Some(Ok(_))) => {
                        ui.label(RichText::new(tr("Sanaga est à jour.")).color(th.sub));
                    }
                    Some(Some(Err(_))) => {
                        ui.label(RichText::new(tr("Vérification impossible (pas de connexion ?).")).color(th.sub));
                    }
                    None => {}
                }
            });
            ui.add_space(20.0);
            Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (k, v) in rows {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(k).color(th.sub));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(RichText::new(v).color(th.text)));
                    });
                }
            });
            ui.add_space(16.0);
            ui.add(Label::new(RichText::new(tr("Sanaga affiche les sites avec WebKit, un moteur web mature (celui de Safari et de GNOME Web). Pour économiser davantage, le moteur Sanaga Léger (dans les Paramètres) affiche l'essentiel des pages sans JavaScript, avec très peu de mémoire. La mémoire indiquée comprend les processus du moteur.")).color(th.text)).wrap());
            ui.add_space(10.0);
            if ui.link(tr("Revoir la page de bienvenue")).clicked() {
                open_welcome = true;
            }
            ui.add_space(4.0);
            if ui.link(tr("Activité : mémoire et processeur")).clicked() {
                open_activity = true;
            }
            ui.add_space(4.0);
            if ui.link(tr("Rapports de plantage")).clicked() {
                open_crashes = true;
            }
        });
        if open_welcome {
            self.navigate(self.active, "sanaga://welcome".into(), None, true);
        }
        if check_update {
            self.update_check = Some(None);
            self.off_thread(net::latest_release, |app, found| app.update_check = Some(Some(found)));
        }
        if open_release {
            self.open_tab(net::RELEASES_PAGE, false);
        }
        if open_crashes {
            self.open_tab("sanaga://crashes", false);
        }
        if open_activity {
            self.open_tab("sanaga://activity", false);
        }
    }

    pub(super) fn page_error(&mut self, ui: &mut Ui, th: &Theme, msg: &str) {
        let url = self.tabs[self.active].url().to_string();
        let mut retry = false;
        let mut archive = false;
        let mut report = false;
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.18);
            let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 30.0, th.card);
            icons::paint(ui.painter(), r.shrink(14.0), Icon::Info, th.sub);
            ui.add_space(14.0);
            let crashed = msg.starts_with(super::CRASH_MARK);
            let msg = msg.trim_start_matches(super::CRASH_MARK);
            let title = if crashed {
                tr("Cette page a cessé de fonctionner")
            } else if msg.is_empty() {
                tr("Page non chargée")
            } else {
                tr("Impossible d'accéder à ce site")
            };
            ui.label(RichText::new(title).size(24.0).family(bold()).color(th.text));
            ui.add_space(6.0);
            ui.label(RichText::new(host_of(&url)).size(15.0).color(th.sub));
            ui.add_space(6.0);
            let m = if msg.is_empty() { tr("Cliquez sur Actualiser pour ouvrir cette page.") } else { msg };
            ui.set_max_width(560.0);
            ui.add(Label::new(RichText::new(m).size(14.0).color(th.sub)).wrap());
            ui.add_space(18.0);
            let b = egui::Button::new(RichText::new(tr("Actualiser")).color(Color32::WHITE).size(14.0)).fill(th.accent).corner_radius(CornerRadius::same(16)).min_size(vec2(120.0, 34.0));
            if ui.add(b).clicked() {
                retry = true;
            }
            // gone or not loading: the copy kept by the Internet Archive
            if crashed {
                ui.add_space(8.0);
                if ui.add(egui::Button::new(RichText::new(tr("Voir le rapport")).size(13.5)).corner_radius(CornerRadius::same(16)).min_size(vec2(120.0, 30.0))).clicked() {
                    report = true;
                }
            }
            if !crashed && (url.starts_with("http://") || url.starts_with("https://")) {
                ui.add_space(8.0);
                if ui.add(egui::Button::new(RichText::new(tr("Version archivée")).size(13.5)).corner_radius(CornerRadius::same(16)).min_size(vec2(120.0, 30.0))).clicked() {
                    archive = true;
                }
            }
        });
        if retry {
            self.reload(self.active);
        }
        if archive {
            self.open_archive();
        }
        if report {
            self.open_tab("sanaga://crashes", false);
        }
    }
}
