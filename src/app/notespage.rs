//! Notes (see `notes.rs`): an answer of the assistant kept by the person,
//! and the page that lists and shows them (`sanaga://notes`).

use super::chrome::Fit as _;
use super::*;
use crate::chat::{self, Source, SourceKind};
use crate::notes::{self, Note};
use eframe::egui::{Margin, RichText, Sense};

#[derive(Default)]
pub(super) struct NotesState {
    /// The notes kept, once read (None: not asked yet).
    list: Option<Vec<Note>>,
    /// The note shown: its name and its text (empty while it is read).
    shown: Option<(String, Option<String>)>,
}

/// The name of the note an address is for ("sanaga://notes/NAME").
fn note_of(address: &str) -> Option<&str> {
    address.trim_end_matches('/').strip_prefix("sanaga://notes/").filter(|n| !n.is_empty())
}

impl Sanaga {
    fn notes_dir(&self) -> Option<std::path::PathBuf> {
        self.store.data_dir().filter(|_| !self.guest).map(|d| d.join("notes"))
    }

    fn reload_notes(&mut self) {
        let Some(dir) = self.notes_dir() else { return };
        self.off_thread(move || notes::list(&dir), |app, list| app.notes.list = Some(list));
    }

    /// Keep the assistant's last answer as a note (on a thread).
    pub(super) fn keep_note(&mut self) {
        let Some(dir) = self.notes_dir() else {
            self.say(tr("Une fenêtre d'invité ne garde pas de notes."));
            return;
        };
        let Some((asked, answer, sources, by)) = self.assistant.as_ref().and_then(|a| a.last_exchange()) else { return };
        let when = super::aitools::when(crate::store::now());
        let kept = if by.is_empty() { tf("Note gardée le {}.", &[&when]) } else { tf("Note gardée le {} — réponse de {}.", &[&when, &by]) };
        let text = notes::compose(&asked, &answer, &sources, &kept);
        self.off_thread(
            move || notes::save(&dir, &when, &asked, &text),
            |app, done| match done {
                Ok(name) => {
                    if let Some(a) = app.assistant.as_mut() {
                        a.kept_note = Some(name);
                    }
                    app.say(tr("Réponse gardée dans les notes."));
                    app.reload_notes();
                }
                Err(e) => app.say(&e.to_string()),
            },
        );
    }

    /// sanaga://notes: the notes kept, or one of them.
    pub(super) fn page_notes(&mut self, ui: &mut Ui, th: &Theme) {
        let Some(dir) = self.notes_dir() else {
            super::pages::centered_column(ui, |ui| {
                super::pages::page_title(ui, th, tr("Notes"));
                ui.label(RichText::new(tr("Une fenêtre d'invité ne garde pas de notes.")).color(th.sub));
            });
            return;
        };
        if self.notes.list.is_none() && self.first_time("notes") {
            self.reload_notes();
        }
        let a = self.active;
        let wanted = note_of(self.tabs[a].url()).map(str::to_string);
        // the note this address is for is read when the address changes
        if wanted.as_deref() != self.notes.shown.as_ref().map(|s| s.0.as_str()) {
            self.notes.shown = wanted.clone().map(|n| (n, None));
            if let Some(name) = wanted.clone() {
                let from = dir.clone();
                self.off_thread(
                    move || (notes::load(&from, &name).unwrap_or_default(), name),
                    |app, (text, name)| {
                        if let Some(s) = app.notes.shown.as_mut().filter(|s| s.0 == name) {
                            s.1 = Some(text);
                        }
                    },
                );
            }
        }
        let (mut go, mut remove, mut copy, mut go_on, mut print) = (None::<String>, None::<String>, None::<String>, None::<(String, String)>, None::<(String, String)>);
        super::pages::centered_column(ui, |ui| match &self.notes.shown {
            Some((name, text)) => {
                ui.add_space(20.0);
                if ui.link(RichText::new(tr("‹ Toutes les notes")).size(13.0)).clicked() {
                    go = Some("sanaga://notes".into());
                }
                ui.add_space(8.0);
                match text.as_deref() {
                    None => {
                        ui.spinner();
                    }
                    Some("") => {
                        ui.label(RichText::new(tr("Cette note n'existe plus.")).color(th.sub));
                    }
                    Some(text) => {
                        ui.horizontal(|ui| {
                            if ui.button(tr("Copier")).clicked() {
                                copy = Some(text.to_string());
                            }
                            if ui.button(tr("Poursuivre avec l'assistant")).clicked() {
                                go_on = Some((name.clone(), text.to_string()));
                            }
                            if ui.button(tr("Ouvrir comme page à imprimer")).on_hover_text(tr("La note devient une page : Ctrl+P l'imprime ou l'enregistre en PDF.")).clicked() {
                                print = Some((name.clone(), text.to_string()));
                            }
                            if super::chrome::confirm_button(ui, ("note", name), tr("Supprimer")) {
                                remove = Some(name.clone());
                            }
                        });
                        ui.add_space(12.0);
                        Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::symmetric(22, 18)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            // the answer, then — smaller, under a line — where it came from
                            let (answer, origin) = text.split_once("\n---\n").unwrap_or((text, ""));
                            super::intelligence::rich_text(ui, th, answer);
                            if !origin.trim().is_empty() {
                                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
                                ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, th.hairline));
                                for line in origin.lines().map(|l| l.trim().trim_start_matches("- ")).filter(|l| !l.is_empty()) {
                                    ui.add(egui::Label::new(RichText::new(line).size(12.0).color(th.sub)).selectable(true).wrap());
                                }
                            }
                        });
                    }
                }
                ui.add_space(24.0);
            }
            None => {
                super::pages::page_title(ui, th, tr("Notes"));
                ui.add(
                    egui::Label::new(
                        RichText::new(tr("Les réponses de l'assistant que vous avez gardées. Ce sont des fichiers texte dans le dossier du profil : ils restent lisibles sans Sanaga."))
                            .size(14.0)
                            .color(th.sub),
                    )
                    .wrap(),
                );
                ui.add_space(14.0);
                match &self.notes.list {
                    None => {
                        ui.spinner();
                    }
                    Some(list) if list.is_empty() => {
                        ui.label(RichText::new(tr("Aucune note. Sous une réponse de l'assistant : « Garder comme note ».")).size(13.5).color(th.sub));
                    }
                    Some(list) => {
                        // (only the rows in view are laid out)
                        let row_h = 52.0;
                        egui::ScrollArea::vertical().id_salt("notes-list").auto_shrink([false, false]).show_rows(ui, row_h, list.len(), |ui, rows| {
                            for n in &list[rows] {
                                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h - 6.0), Sense::click());
                                super::chrome::focus_ring(ui, &resp, r, 10.0, th);
                                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, &n.title));
                                ui.painter().rect_filled(r, CornerRadius::same(10), if resp.hovered() { th.accent.gamma_multiply(0.14) } else { th.card });
                                let clip = Rect::from_min_max(pos2(r.left() + 14.0, r.top()), pos2(r.right() - 14.0, r.bottom()));
                                ui.painter().with_clip_rect(clip).fit(pos2(r.left() + 14.0, r.top() + 15.0), egui::Align2::LEFT_CENTER, &n.title, FontId::new(14.0, bold()), th.text);
                                ui.painter().with_clip_rect(clip).text(
                                    pos2(r.left() + 14.0, r.top() + 33.0),
                                    egui::Align2::LEFT_CENTER,
                                    super::aitools::when(n.time),
                                    FontId::proportional(11.5),
                                    th.sub,
                                );
                                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    go = Some(format!("sanaga://notes/{}", n.name));
                                }
                                ui.add_space(6.0);
                            }
                        });
                    }
                }
            }
        });
        if let Some(text) = copy {
            self.ctx.copy_text(text);
            self.say(tr("Note copiée."));
        }
        if let Some(address) = go {
            self.navigate(a, address, None, true);
        }
        if let Some(name) = remove {
            let from = dir.clone();
            self.off_thread(move || notes::delete(&from, &name), |app, _| app.reload_notes());
            self.navigate(a, "sanaga://notes".into(), None, true);
        }
        if let Some((name, text)) = go_on {
            // a new conversation with the note attached, like a file
            self.new_assistant_with(Source { kind: SourceKind::File, title: format!("{name}.md"), address: String::new(), text, cut: None });
        }
        if let Some((name, text)) = print {
            // (written beside the notes as a page; nothing in it can run)
            let folder = dir.join(".print");
            self.off_thread(
                move || {
                    std::fs::create_dir_all(&folder)?;
                    let path = folder.join(format!("{name}.html"));
                    crate::store::write_atomic(&path, notes::html(&text))?;
                    Ok::<_, std::io::Error>(path)
                },
                |app, done| match done {
                    Ok(path) => {
                        if let Ok(address) = url::Url::from_file_path(&path) {
                            app.open_tab(address.as_str(), false);
                        }
                    }
                    Err(e) => app.say(&e.to_string()),
                },
            );
        }
    }
}

/// What a note holds of a conversation's last exchange.
pub(super) type Exchange = (String, String, Vec<(String, String)>, String);

/// The pages given to the model in a conversation: (title, address), each once.
pub(super) fn pages_given(entries: &[chat::Entry]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for e in entries {
        if let chat::Entry::User { sources, .. } = e {
            for s in sources.iter().filter(|s| s.kind != SourceKind::File || !s.address.is_empty()) {
                let item = (s.title.clone(), s.address.clone());
                if !(item.0.is_empty() && item.1.is_empty()) && !out.contains(&item) {
                    out.push(item);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notes_address_names_it() {
        assert_eq!(note_of("sanaga://notes/2026-11-17-fleuve"), Some("2026-11-17-fleuve"));
        assert_eq!(note_of("sanaga://notes/x/"), Some("x"));
        assert_eq!(note_of("sanaga://notes"), None);
        assert_eq!(note_of("sanaga://notes/"), None);
        assert_eq!(note_of("sanaga://settings/notes"), None);
    }

    #[test]
    fn a_note_says_which_pages_the_answer_came_from() {
        let page = |title: &str, address: &str| Source { kind: SourceKind::Page, title: title.into(), address: address.into(), text: "x".into(), cut: None };
        let entries = vec![
            chat::Entry::User { text: "q".into(), sources: vec![page("A", "https://a.example/"), page("B", "https://b.example/")], files: Vec::new() },
            chat::Entry::Assistant { text: "r".into(), calls: Vec::new(), blocks: None, stopped: false },
            chat::Entry::User {
                text: "q2".into(),
                sources: vec![page("A", "https://a.example/"), Source { kind: SourceKind::File, title: "notes.txt".into(), address: String::new(), text: "x".into(), cut: None }],
                files: Vec::new(),
            },
        ];
        assert_eq!(pages_given(&entries), vec![("A".to_string(), "https://a.example/".to_string()), ("B".to_string(), "https://b.example/".to_string())]);
    }
}
