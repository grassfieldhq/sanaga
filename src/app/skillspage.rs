//! Saved requests for the assistant (see `skills.rs`): the page that lists
//! them and lets the person write their own (`sanaga://skills`), and how
//! one is called from the assistant's field, the palette or a button.

use super::*;
use crate::skills::{self, Skill};
use eframe::egui::{Color32, Margin, RichText, TextEdit};

#[derive(Default)]
pub(super) struct SkillForm {
    name: String,
    description: String,
    page: bool,
    prompt: String,
    /// The saved request being changed (its name), if not a new one.
    editing: Option<String>,
    open: bool,
    status: String,
    failed: bool,
    /// A model is writing the request from the description.
    busy: bool,
    run: u64,
}

impl Sanaga {
    /// The folder of the profile's saved requests (none in a guest window).
    fn skills_dir(&self) -> Option<std::path::PathBuf> {
        self.store.data_dir().filter(|_| !self.guest).map(|d| d.join("skills"))
    }

    /// Read the saved requests again (on a thread): at first use, and
    /// after one was written or removed.
    pub(super) fn reload_skills(&mut self) {
        let dir = self.skills_dir();
        self.off_thread(move || skills::all(dir.as_deref()), |app, list| app.skills = list);
    }

    /// The saved requests, read once.
    pub(super) fn ensure_skills(&mut self) {
        if self.first_time("skills") {
            self.skills = skills::provided();
            self.reload_skills();
        }
    }

    /// Call a saved request by its name on the page shown (palette, button).
    pub(super) fn run_skill(&mut self, name: &str) {
        self.ensure_skills();
        self.open_assistant();
        self.assistant_call(&format!("/{name}"));
    }

    /// sanaga://skills: the saved requests, and the form to write one.
    pub(super) fn page_skills(&mut self, ui: &mut Ui, th: &Theme) {
        self.ensure_skills();
        let dir = self.skills_dir();
        let has_model = self.ai_provider() != crate::ai::Provider::Local;
        let (mut save, mut remove, mut edit, mut open_folder, mut write, mut go_settings) = (false, None::<String>, None::<Skill>, false, false, false);
        let list = self.skills.clone();
        super::pages::centered_column(ui, |ui| {
            super::pages::page_title(ui, th, tr("Demandes enregistrées"));
            ui.add(
                egui::Label::new(
                    RichText::new(tr("Une demande que vous faites souvent à l'assistant, gardée sous un nom : tapez « / » puis ce nom dans son champ. Ce que vous tapez ensuite part avec elle."))
                        .size(14.0)
                        .color(th.sub),
                )
                .wrap(),
            );
            ui.add_space(14.0);
            for s in &list {
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width((ui.available_width() - 200.0).max(80.0));
                            ui.label(RichText::new(format!("/{}", s.name)).size(14.5).family(bold()).color(th.text));
                            let sub = if s.own { s.description.clone() } else { tf("{} — fournie avec Sanaga", &[&s.description]) };
                            ui.add(egui::Label::new(RichText::new(sub).size(12.5).color(th.sub)).wrap());
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if s.own && super::chrome::confirm_button(ui, ("skill", &s.name), tr("Supprimer")) {
                                remove = Some(s.name.clone());
                            }
                            // (a provided one is changed as a copy of the person's, under the same name)
                            if ui.button(if s.own { tr("Modifier") } else { tr("Adapter") }).clicked() {
                                edit = Some(s.clone());
                            }
                        });
                    });
                });
                ui.add_space(6.0);
            }
            ui.add_space(10.0);
            let f = &mut self.skill_form;
            if !f.open {
                ui.horizontal(|ui| {
                    if dir.is_some()
                        && ui
                            .add(egui::Button::new(RichText::new(tr("Nouvelle demande")).color(Color32::WHITE)).fill(th.accent).min_size(vec2(150.0, 32.0)).corner_radius(CornerRadius::same(10)))
                            .clicked()
                    {
                        *f = SkillForm { page: true, open: true, ..Default::default() };
                    }
                    if dir.is_some() && ui.button(tr("Ouvrir le dossier…")).clicked() {
                        open_folder = true;
                    }
                });
                if dir.is_none() {
                    ui.label(RichText::new(tr("Une fenêtre d'invité ne garde pas de demandes.")).size(12.5).color(th.sub));
                }
            } else {
                ui.label(RichText::new(if f.editing.is_some() { tr("Modifier la demande") } else { tr("Nouvelle demande") }).size(17.0).family(bold()).color(th.text));
                ui.add_space(8.0);
                ui.label(RichText::new(tr("Nom (ce que vous taperez après « / »)")).family(bold()).color(th.text));
                ui.add_enabled(f.editing.is_none(), TextEdit::singleline(&mut f.name).hint_text(tr("Ex. : avis")).desired_width(260.0));
                ui.add_space(8.0);
                ui.label(RichText::new(tr("Ce qu'elle fait, en quelques mots")).family(bold()).color(th.text));
                ui.add(TextEdit::singleline(&mut f.description).hint_text(tr("Ex. : les points faibles d'un texte")).desired_width(f32::INFINITY));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    super::settings::switch(ui, th, &mut f.page);
                    ui.label(RichText::new(tr("Joindre la page affichée")).color(th.text));
                });
                ui.add_space(8.0);
                ui.label(RichText::new(tr("La demande, telle qu'elle part au modèle")).family(bold()).color(th.text));
                ui.add(
                    TextEdit::multiline(&mut f.prompt)
                        .hint_text(tr("Ex. : Relis ce texte comme un lecteur exigeant : dis ce qui n'est pas clair, ce qui manque, et ce qui est de trop."))
                        .desired_rows(6)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let ready = !skills::slug(&f.name).is_empty() && !f.prompt.trim().is_empty() && !f.busy;
                    let b = egui::Button::new(RichText::new(tr("Enregistrer")).color(Color32::WHITE))
                        .fill(if ready { th.accent } else { th.sub })
                        .min_size(vec2(130.0, 32.0))
                        .corner_radius(CornerRadius::same(10));
                    if ui.add_enabled(ready, b).clicked() {
                        save = true;
                    }
                    if ui.add(egui::Button::new(tr("Annuler")).min_size(vec2(90.0, 32.0)).corner_radius(CornerRadius::same(10))).clicked() {
                        *f = SkillForm::default();
                    }
                    // the request written by the model from the description:
                    // put in the field, to be read and changed before it is kept
                    if f.busy {
                        ui.spinner();
                    } else if ui
                        .add_enabled(
                            has_model && !f.description.trim().is_empty(),
                            egui::Button::new(tr("Écrire la demande avec l'IA")).min_size(vec2(0.0, 32.0)).corner_radius(CornerRadius::same(10)),
                        )
                        .clicked()
                    {
                        write = true;
                    }
                });
                if !has_model && ui.link(RichText::new(tr("Choisir une IA…")).size(12.5)).clicked() {
                    go_settings = true;
                }
            }
            let f = &self.skill_form;
            if !f.status.is_empty() {
                ui.add_space(10.0);
                let (fill, text) = if f.failed { (Color32::from_rgb(0xD0, 0x40, 0x30).gamma_multiply(0.12), Color32::from_rgb(0xC0, 0x38, 0x2B)) } else { (th.accent.gamma_multiply(0.12), th.text) };
                Frame::new().fill(fill).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(RichText::new(&f.status).color(text)).wrap());
                });
            }
            ui.add_space(24.0);
        });
        if let Some(s) = edit {
            self.skill_form = SkillForm { name: s.name.clone(), description: s.description, page: s.page, prompt: s.prompt, editing: Some(s.name), open: true, ..Default::default() };
        }
        if go_settings {
            self.open_settings(&super::settings::Section::Intelligence.address());
        }
        if open_folder {
            if let Some(dir) = dir.clone() {
                self.off_thread(
                    move || {
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
                    },
                    |_, ()| {},
                );
            }
        }
        if let (Some(name), Some(dir)) = (remove, dir.clone()) {
            self.off_thread(
                move || skills::delete(&dir, &name).map(|()| name).map_err(|e| e.to_string()),
                |app, done| {
                    match done {
                        Ok(name) => (app.skill_form.status, app.skill_form.failed) = (tf("« /{} » est supprimée.", &[&name]), false),
                        Err(e) => (app.skill_form.status, app.skill_form.failed) = (e, true),
                    }
                    app.reload_skills();
                },
            );
        }
        if let (true, Some(dir)) = (save, dir) {
            let f = &self.skill_form;
            let skill = Skill { name: skills::slug(&f.name), description: f.description.trim().to_string(), page: f.page, prompt: f.prompt.trim().to_string(), own: true, summary: false };
            self.off_thread(
                move || skills::save(&dir, &skill).map(|()| skill.name).map_err(|e| e.to_string()),
                |app, done| {
                    match done {
                        Ok(name) => app.skill_form = SkillForm { status: tf("« /{} » est enregistrée : tapez-la dans le champ de l'assistant.", &[&name]), ..Default::default() },
                        Err(e) => (app.skill_form.status, app.skill_form.failed) = (e, true),
                    }
                    app.reload_skills();
                },
            );
        }
        if write {
            let provider = self.ai_provider();
            self.ai_run += 1;
            let run = self.ai_run;
            let f = &mut self.skill_form;
            f.busy = true;
            f.run = run;
            f.status.clear();
            let description = f.description.clone();
            self.off_thread(
                move || {
                    let system = "You write a reusable request (a prompt) that a person will send to the assistant of their web browser, with the page they are reading attached. \
                                  From their description of what they want, write the request itself: addressed to the assistant, in the description's language, three to six sentences, \
                                  saying what to look at, what to produce and in what form (short paragraphs, or lines that start with \"• \"). Reply with the request only: no title, no quotes, no explanation.";
                    crate::ai::ask(&provider, system, &description, 1200)
                },
                move |app, written| {
                    let f = &mut app.skill_form;
                    if !f.busy || f.run != run {
                        return;
                    }
                    f.busy = false;
                    match written {
                        Ok(text) => f.prompt = text.trim().to_string(),
                        Err(e) => (f.status, f.failed) = (e, true),
                    }
                },
            );
        }
    }
}
