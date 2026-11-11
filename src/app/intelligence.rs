//! Summaries of pages and extensions written from a description (see `ai.rs`
//! for the models). The summary shows in a panel at the side of the page.

use super::*;
use crate::ai::{self, Provider};
use crate::icons::{self, Icon};
use eframe::egui::{Align, Color32, Layout, Margin, RichText, Sense, TextEdit};

/// Collects the readable text of the page (its article or main part when it
/// has one) and hands it to Sanaga.
const PAGE_TEXT_JS: &str = r#"(function(){
  var root = document.querySelector('article') || document.querySelector('main') || document.querySelector('[role=main]') || document.body;
  // the paragraphs of the text first (not menus, lists of links, references)
  var ps = Array.prototype.slice.call((root || document).querySelectorAll('p'))
    .filter(function (p) { return p.offsetParent !== null && !p.closest('nav,footer,aside,.references,.reflist,[role=navigation]'); })
    .map(function (p) { return p.innerText.trim(); })
    .filter(function (x) { return x.split(/\s+/).length >= 8; });
  var t = ps.join('\n');
  if (t.length < 600) t = (root && root.innerText) || '';
  if (t.length < 600 && document.body) t = document.body.innerText || '';
  if (window.ipc) window.ipc.postMessage('sum:' + (document.title || '').replace(/\n/g, ' ') + '\n' + t.slice(0, 80000));
})();"#;

/// One paragraph of a model's answer, as it is shown.
#[derive(Debug, PartialEq)]
pub(super) struct Para {
    /// A point of a list ("• ", "- ", "* ", "1. ").
    bullet: bool,
    /// A heading ("# …", or a line that is all bold).
    heading: bool,
    /// Its text, in parts: (words, in bold).
    spans: Vec<(String, bool)>,
}

/// A model's answer as paragraphs: models write lists ("• ", "- ", "1. "),
/// emphasis (`**…**`) and sometimes headings ("## …"); shown raw, the
/// signs clutter the text. Only these few are read; everything else is
/// kept as written (never HTML: the answer is shown as text).
pub(super) fn paragraphs(text: &str) -> Vec<Para> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        // (a point's sign whose words have not come yet, while the model writes)
        if line.is_empty() || matches!(line, "•" | "-" | "*" | "–") {
            continue;
        }
        let (mut bullet, mut heading, mut rest) = (false, false, line);
        if let Some(r) = line.strip_prefix("• ").or_else(|| line.strip_prefix("- ")).or_else(|| line.strip_prefix("* ")).or_else(|| line.strip_prefix("– ")) {
            (bullet, rest) = (true, r.trim_start());
        } else if let Some((n, r)) = line.split_once(". ").filter(|(n, _)| !n.is_empty() && n.len() <= 2 && n.chars().all(|c| c.is_ascii_digit())) {
            let _ = n;
            (bullet, rest) = (true, r.trim_start());
        } else if line.starts_with('#') {
            (heading, rest) = (true, line.trim_start_matches('#').trim_start());
        }
        // **bold** parts (a "**" left alone is shown as it is)
        let mut spans: Vec<(String, bool)> = Vec::new();
        let mut parts = rest.split("**").peekable();
        let mut bold = false;
        while let Some(part) = parts.next() {
            let closed = parts.peek().is_some();
            if bold && !closed {
                // an opening "**" never closed: its signs stay
                match spans.last_mut() {
                    Some(last) if !last.1 => last.0.push_str(&format!("**{part}")),
                    _ => spans.push((format!("**{part}"), false)),
                }
            } else if !part.is_empty() {
                spans.push((part.to_string(), bold));
            }
            bold = !bold;
        }
        if spans.is_empty() {
            continue;
        }
        // a line that is all bold is a heading
        if !bullet && spans.len() == 1 && spans[0].1 {
            heading = true;
        }
        out.push(Para { bullet, heading, spans });
    }
    out
}

/// Show a model's answer: paragraphs, points with their bullet hanging,
/// bold where the model put emphasis. The text can be selected.
fn rich_text(ui: &mut Ui, th: &Theme, text: &str) {
    let size = 14.5;
    for para in paragraphs(text) {
        let mut job = egui::text::LayoutJob::default();
        for (words, strong) in &para.spans {
            let family = if *strong || para.heading { bold() } else { FontFamily::Proportional };
            job.append(words, 0.0, egui::TextFormat { font_id: FontId::new(size, family), color: th.text, line_height: Some(21.0), ..Default::default() });
        }
        if para.bullet {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_sized(vec2(18.0, 21.0), egui::Label::new(RichText::new("•").size(size).color(th.accent)));
                job.wrap.max_width = ui.available_width();
                ui.add(egui::Label::new(job).selectable(true).wrap());
            });
            ui.add_space(4.0);
        } else {
            job.wrap.max_width = ui.available_width();
            ui.add(egui::Label::new(job).selectable(true).wrap());
            ui.add_space(8.0);
        }
    }
}

pub(super) struct Summary {
    tab: u64,
    title: String,
    text: String,
    busy: bool,
    by: String,
    note: String,
    /// The page's text has arrived (later ones are ignored).
    sent: bool,
    /// The request being answered (parts of an earlier one are dropped),
    /// and what stops it.
    run: u64,
    stop: std::sync::Arc<AtomicBool>,
}

impl Summary {
    pub(super) fn is_for(&self, tab: u64) -> bool {
        self.tab == tab
    }
}

pub(super) enum AiDone {
    Summary { tab: u64, run: u64, result: Result<String, String>, by: String, fallback: String },
    /// A part of a summary, as the model writes it.
    SummaryPiece { tab: u64, run: u64, text: String },
    Extension(u64, Result<(String, String), String>),
    /// A part of an extension's code, as the model writes it.
    ExtensionPiece(u64, String),
    Models(Vec<String>),
    /// The accounts read from the system keyring.
    Logins(Vec<(String, String)>),
    /// A batch of a page's texts, translated (from its place in the page,
    /// for the translation `generation` of that tab).
    Translation { tab: u64, from: usize, generation: u64, result: Result<Vec<String>, String> },
}

#[derive(Default)]
pub(super) struct ExtForm {
    name: String,
    description: String,
    sites: String,
    status: String,
    /// `status` says what went wrong (shown as such).
    failed: bool,
    busy: bool,
    /// When the writing began (the interface's clock); the request being
    /// answered, and what stops it.
    started: f64,
    run: u64,
    stop: std::sync::Arc<AtomicBool>,
    code: String,
    /// The extension just written: installed switched off, until the person
    /// has read it and clicks "Turn on" (its id, and where it would act).
    pending: Option<String>,
    matches: Vec<String>,
}

/// Could an extension's "where it acts" pattern (`*://*.youtube.com/*`)
/// be about this address? (For offering pages to try it on: the extension
/// itself checks exactly.)
pub(super) fn pattern_fits(pattern: &str, url: &str) -> bool {
    let site = pattern.split_once("://").map(|p| p.1).unwrap_or(pattern);
    let site = site.split('/').next().unwrap_or("").trim_start_matches("*.").trim_start_matches("www.");
    if site.is_empty() || site == "*" {
        return true;
    }
    let host = host_of(url);
    host == site || host.ends_with(&format!(".{site}"))
}

impl Sanaga {
    /// The extension written was switched on from its trial (see web.rs):
    /// the form says so.
    pub(super) fn extension_tried_and_kept(&mut self, id: &str) {
        if self.ext_form.pending.as_deref() == Some(id) {
            self.ext_form.pending = None;
            self.ext_form.failed = false;
            self.ext_form.status = tf("« {} » est activée : elle s'applique aux pages ouvertes.", &[&self.ext_form.name]);
        }
    }

    /// The model chosen in Settings › Intelligence (with its key, for Claude).
    pub(super) fn ai_provider(&self) -> Provider {
        let st = &self.store.settings;
        if let Some(tool) = crate::ai_cli::CliTool::from_key(&st.ai) {
            return Provider::Cli { tool, model: st.ai_model.clone() };
        }
        match st.ai.as_str() {
            "ollama" => {
                let model = if !st.ai_model.is_empty() {
                    st.ai_model.clone()
                } else {
                    self.ai_models.as_ref().and_then(|m| m.first().cloned()).unwrap_or_else(|| "llama3.2".into())
                };
                Provider::Ollama { model }
            }
            // (a window without a profile folder, such as a guest window,
            // never reads a profile's key)
            "claude" if self.store.data_dir().is_none() => Provider::Local,
            // (no key in the keyring: Sanaga's own summary; the key itself
            // is read where the request is made, on its thread)
            "claude" if self.ai_key_known == Some(false) => Provider::Local,
            "claude" => Provider::Claude { model: if st.ai_model.is_empty() { ai::CLAUDE_MODEL.into() } else { st.ai_model.clone() }, profile: self.store.profile.clone() },
            _ => Provider::Local,
        }
    }

    /// Summarise the page shown (toolbar button, menu).
    pub(super) fn summarize_page(&mut self) {
        let t = &self.tabs[self.active];
        let (id, title) = (t.id, t.title());
        let Some(w) = t.web.as_ref().filter(|_| matches!(t.view, View::Web)) else {
            self.notice = Some((tr("Le résumé marche sur les pages des sites.").into(), self.ctx.input(|i| i.time)));
            return;
        };
        let _ = w.evaluate_own(PAGE_TEXT_JS);
        // (a summary still being written gives way to the new one)
        if let Some(old) = &self.summary {
            old.stop.store(true, Ordering::Relaxed);
        }
        self.ai_run += 1;
        self.summary = Some(Summary { tab: id, title, text: String::new(), busy: true, by: String::new(), note: String::new(), sent: false, run: self.ai_run, stop: Default::default() });
    }

    pub(super) fn summary_text_arrived(&mut self, tab: u64, text: String) {
        // only the text the person asked for (one answer per request)
        if !self.summary.as_ref().is_some_and(|s| s.tab == tab && s.busy && s.text.is_empty() && !s.sent) {
            return;
        }
        if let Some(s) = self.summary.as_mut() {
            s.sent = true;
        }
        let (title, body) = text.split_once('\n').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or_default();
        let fallback = ai::local_summary(&body, 6);
        let provider = self.ai_provider();
        if let Some(s) = self.summary.as_mut() {
            if !title.trim().is_empty() {
                s.title = title.trim().to_string();
            }
        }
        if body.split_whitespace().count() < 30 {
            if let Some(s) = self.summary.as_mut() {
                s.busy = false;
                s.text = tr("Cette page contient trop peu de texte à résumer.").into();
            }
            return;
        }
        // the page's text only goes to a model with no tools and no files
        let refused = ai::page_content_allowed(&provider).err();
        if provider == Provider::Local || refused.is_some() {
            if let Some(s) = self.summary.as_mut() {
                s.busy = false;
                s.text = fallback;
                s.by = Provider::Local.label();
                s.note = refused.unwrap_or_default();
            }
            return;
        }
        let tx = self.ai_tx.clone();
        let repaint = self.ctx.clone();
        let by = provider.label();
        let (run, stop) = self.summary.as_ref().map(|s| (s.run, s.stop.clone())).unwrap_or_default();
        if let Some(s) = self.summary.as_mut() {
            s.by = by.clone();
        }
        std::thread::Builder::new()
            .name("sanaga-summary".into())
            .spawn(move || {
                let (system, prompt) = ai::summary_prompt(&title, &body);
                // (shown as the model writes it)
                let (tx2, repaint2) = (tx.clone(), repaint.clone());
                let result = ai::ask_live(&provider, &system, &prompt, 900, &stop, &mut |text| {
                    let _ = tx2.send(AiDone::SummaryPiece { tab, run, text: text.to_string() });
                    repaint2.request_repaint();
                });
                let _ = tx.send(AiDone::Summary { tab, run, result, by, fallback });
                repaint.request_repaint();
            })
            .ok();
    }

    /// Answers of the models, when they come.
    pub(super) fn poll_ai(&mut self) {
        while let Ok(done) = self.ai_rx.try_recv() {
            match done {
                AiDone::SummaryPiece { tab, run, text } => {
                    if let Some(s) = self.summary.as_mut().filter(|s| s.tab == tab && s.run == run && s.busy) {
                        s.text.push_str(&text);
                    }
                }
                AiDone::Summary { tab, run, result, by, fallback } => {
                    let Some(s) = self.summary.as_mut().filter(|s| s.tab == tab && s.run == run && s.busy) else { continue };
                    s.busy = false;
                    match result {
                        Ok(t) => {
                            s.text = t.trim().to_string();
                            s.by = by;
                        }
                        // (stopped by the person: what was written stays)
                        Err(e) if e == ai::STOPPED => {}
                        Err(e) => {
                            // the model failed: Sanaga's own summary instead
                            s.text = fallback;
                            s.by = Provider::Local.label();
                            s.note = e;
                        }
                    }
                }
                AiDone::ExtensionPiece(run, text) => {
                    if self.ext_form.busy && self.ext_form.run == run {
                        self.ext_form.code.push_str(&text);
                    }
                }
                AiDone::Extension(run, r) => {
                    // (an answer to a request that was cancelled or replaced)
                    if !self.ext_form.busy || self.ext_form.run != run {
                        continue;
                    }
                    self.ext_form.busy = false;
                    self.ext_form.code.clear();
                    match r {
                        Ok((file, text)) => {
                            let dir = crate::extensions::prepare_folder();
                            let mut path = dir.join(&file);
                            let mut n = 2;
                            while path.exists() {
                                path = dir.join(file.replace(".js", &format!("-{n}.js")));
                                n += 1;
                            }
                            match crate::store::write_atomic(&path, &text) {
                                Ok(()) => {
                                    // installed switched off: the person reads it first
                                    let id = format!("fichier:{}", path.file_name().unwrap_or_default().to_string_lossy());
                                    self.ext_form.status = tf("« {} » est écrite mais désactivée. Lisez son code et les sites où elle agit, puis activez-la si elle vous convient.", &[&self.ext_form.name]);
                                    self.ext_form.matches = crate::extensions::matches_in(&text);
                                    self.ext_form.pending = Some(id);
                                    self.ext_form.code = text;
                                }
                                Err(e) => {
                                    self.ext_form.status = e.to_string();
                                    self.ext_form.failed = true;
                                }
                            }
                        }
                        Err(e) if e == ai::STOPPED => {}
                        Err(e) => {
                            self.ext_form.status = e;
                            self.ext_form.failed = true;
                        }
                    }
                }
                AiDone::Models(m) => self.ai_models = Some(m),
                AiDone::Translation { tab, from, generation, result } => self.translation_done(tab, from, generation, result),
                AiDone::Logins(list) => {
                    // the keyring is the reference; an empty answer (a locked
                    // or missing keyring) leaves the list as it was
                    if !list.is_empty() && list != self.store.logins {
                        self.store.logins = list;
                        self.store.save_logins();
                    }
                }
            }
        }
    }

    /// Is there a Claude key in the keyring? (Asked on a thread.)
    pub(super) fn check_ai_key(&mut self) {
        if self.store.data_dir().is_none() || !self.first_time("ai-key") {
            return;
        }
        let profile = self.store.profile.clone();
        self.off_thread(move || crate::keyring::lookup(&profile, ai::KEY_SITE, "claude").is_some_and(|k| !k.is_empty()), |app, known| {
            // (a key saved or forgotten meanwhile is what counts)
            if app.ai_key_known.is_none() {
                app.ai_key_known = Some(known);
            }
        });
    }

    /// Read the saved accounts from the system keyring (in the background).
    pub(super) fn refresh_logins(&mut self) {
        if !self.web_ok || self.store.data_dir().is_none() {
            return;
        }
        let tx = self.ai_tx.clone();
        let repaint = self.ctx.clone();
        let profile = self.store.profile.clone();
        std::thread::spawn(move || {
            if let Some(list) = crate::keyring::list(&profile) {
                let _ = tx.send(AiDone::Logins(list));
                repaint.request_repaint();
            }
        });
    }

    /// Look for the models installed in Ollama (in the background).
    pub(super) fn refresh_ollama_models(&mut self) {
        let tx = self.ai_tx.clone();
        let repaint = self.ctx.clone();
        self.ai_models = Some(Vec::new());
        std::thread::spawn(move || {
            let _ = tx.send(AiDone::Models(ai::ollama_models()));
            repaint.request_repaint();
        });
    }

    /// The summary, at the side of the page.
    pub(super) fn summary_panel(&mut self, ui: &mut Ui, th: &Theme) {
        let mut close = false;
        let mut again = false;
        let mut copy = false;
        let mut choose = false;
        let mut stop = false;
        let no_model = self.ai_provider() == Provider::Local;
        let active_id = self.tabs[self.active].id;
        let Some(s) = self.summary.as_ref() else { return };
        Frame::new().inner_margin(Margin::symmetric(18, 14)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                icons::paint(ui.painter(), r, Icon::Sparkle, th.accent);
                ui.label(RichText::new(tr("Résumé")).size(17.0).family(bold()).color(th.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if super::chrome::icon_button_small(ui, Icon::Close, th).tip(tr("Fermer")).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(6.0);
            ui.label(RichText::new(&s.title).size(13.5).family(bold()).color(th.sub));
            ui.add_space(10.0);
            if s.busy {
                // (the answer shows as the model writes it)
                if !s.text.is_empty() {
                    egui::ScrollArea::vertical().id_salt("summary-scroll").max_height(ui.available_height() - 80.0).stick_to_bottom(true).show(ui, |ui| {
                        rich_text(ui, th, &s.text);
                    });
                    ui.add_space(10.0);
                }
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(if s.text.is_empty() { tr("Lecture de la page…") } else { tr("Écriture du résumé…") }).color(th.sub));
                    // (only a model's answer can be stopped; Sanaga's own is immediate)
                    if !s.by.is_empty() && ui.button(tr("Arrêter")).clicked() {
                        stop = true;
                    }
                });
            } else {
                egui::ScrollArea::vertical().id_salt("summary-scroll").max_height(ui.available_height() - 80.0).show(ui, |ui| {
                    rich_text(ui, th, &s.text);
                });
                ui.add_space(12.0);
                if !s.note.is_empty() {
                    ui.label(RichText::new(&s.note).size(11.5).color(Color32::from_rgb(0xD0, 0x60, 0x30)));
                }
                ui.label(RichText::new(tf("Par : {}", &[&s.by])).size(11.5).color(th.sub));
                // no model chosen: say that one would do better, and where
                if no_model && ui.link(RichText::new(tr("Choisir une IA pour des résumés plus fins…")).size(12.0)).clicked() {
                    choose = true;
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(tr("Copier")).clicked() {
                        copy = true;
                    }
                    let label = if s.tab == active_id { tr("Résumer à nouveau") } else { tr("Résumer cette page") };
                    if ui.button(label).clicked() {
                        again = true;
                    }
                });
            }
        });
        if copy {
            if let Some(s) = &self.summary {
                self.ctx.copy_text(format!("{}\n\n{}", s.title, s.text));
            }
        }
        if stop {
            if let Some(s) = self.summary.as_mut() {
                s.stop.store(true, Ordering::Relaxed);
                s.busy = false;
                s.note = if s.text.trim().is_empty() { tr("Résumé arrêté.").to_string() } else { tr("Résumé arrêté avant la fin.").to_string() };
            }
        }
        if choose {
            self.open_settings(&super::settings::Section::Intelligence.address());
        }
        if close {
            self.summary = None;
        } else if again {
            self.summarize_page();
        }
    }

    /// sanaga://create-extension: describe what you want, a model writes it.
    pub(super) fn page_create_extension(&mut self, ui: &mut Ui, th: &Theme) {
        let provider_label = (!self.store.settings.ai.is_empty()).then(|| self.store.settings.ai.clone());
        let found = crate::ai_cli::installed();
        let mut use_tool: Option<crate::ai_cli::CliTool> = None;
        let mut go_settings = false;
        let mut create = false;
        let mut enable = false;
        let mut discard = false;
        let mut cancel = false;
        let mut try_on: Option<u64> = None;
        // open pages the extension written could be tried on (where it acts)
        let fits = |url: &str| self.ext_form.matches.is_empty() || self.ext_form.matches.iter().any(|m| pattern_fits(m, url));
        let pages: Vec<(u64, String)> = self.tabs.iter().filter(|t| t.web.is_some() && matches!(t.view, View::Web) && !t.private && fits(t.url())).map(|t| (t.id, t.title())).take(5).collect();
        let now = ui.input(|i| i.time);
        super::pages::centered_column(ui, |ui| {
            super::pages::page_title(ui, th, tr("Créer une extension"));
            ui.label(RichText::new(tr("Décrivez ce que vous voulez changer sur les sites : Sanaga écrit l'extension, vous la lisez, puis vous l'activez.")).size(14.0).color(th.sub));
            ui.add_space(14.0);
            if provider_label.is_none() {
                Frame::new().fill(th.accent.gamma_multiply(0.12)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(12)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(tr("Il faut un modèle d'IA : Ollama sur cet ordinateur (gratuit, rien ne sort de chez vous) ou Claude avec votre clé.")).color(th.text));
                    if !found.is_empty() {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(tr("Trouvés sur cet ordinateur :")).color(th.text));
                            for t in &found {
                                let b = egui::Button::new(RichText::new(tf("Utiliser {}", &[&t.name()])).color(Color32::WHITE)).fill(th.accent);
                                if ui.add(b).clicked() {
                                    use_tool = Some(*t);
                                }
                            }
                        });
                    }
                    if ui.link(tr("Choisir dans Paramètres › Intelligence")).clicked() {
                        go_settings = true;
                    }
                });
                ui.add_space(12.0);
            }
            let f = &mut self.ext_form;
            ui.label(RichText::new(tr("Nom (facultatif)")).family(bold()).color(th.text));
            ui.add(TextEdit::singleline(&mut f.name).hint_text(tr("Ex. : YouTube sans distractions")).desired_width(f32::INFINITY));
            ui.add_space(10.0);
            ui.label(RichText::new(tr("Ce qu'elle doit faire")).family(bold()).color(th.text));
            ui.add(TextEdit::multiline(&mut f.description).hint_text(tr("Ex. : masquer les Shorts et les vidéos recommandées à droite, et agrandir le lecteur.")).desired_rows(5).desired_width(f32::INFINITY));
            ui.add_space(10.0);
            ui.label(RichText::new(tr("Sur quels sites")).family(bold()).color(th.text));
            ui.add(TextEdit::singleline(&mut f.sites).hint_text(tr("youtube.com — vide : tous les sites")).desired_width(f32::INFINITY));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr("Idées :")).size(12.5).color(th.sub));
                for (name, desc, site) in [
                    (tr("Lecture zen"), tr("Masquer les bannières de cookies, les fenêtres d'inscription et les publicités collantes."), ""),
                    (tr("YouTube calme"), tr("Masquer les Shorts, les recommandations et les commentaires."), "youtube.com"),
                    (tr("Liens dans un nouvel onglet"), tr("Ouvrir les liens vers d'autres sites dans un nouvel onglet."), ""),
                ] {
                    if ui.small_button(name).clicked() {
                        f.name = name.into();
                        f.description = desc.into();
                        f.sites = site.into();
                    }
                }
            });
            ui.add_space(14.0);
            let ready = !f.description.trim().is_empty() && !f.busy && provider_label.is_some();
            ui.horizontal(|ui| {
                // (once one is written: the same button writes it again, in its place)
                let label = if f.pending.is_some() { tr("Écrire à nouveau") } else { tr("Créer l'extension") };
                let b = egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(if ready { th.accent } else { th.sub }).min_size(vec2(160.0, 34.0)).corner_radius(CornerRadius::same(10));
                if ui.add_enabled(ready, b).clicked() {
                    create = true;
                }
                if f.busy {
                    ui.spinner();
                    // (a model takes from a few seconds to a minute or two:
                    // the time shows that it is still at work)
                    let secs = (now - f.started).max(0.0) as u64;
                    ui.label(RichText::new(tf("Écriture de l'extension… {} s", &[&secs])).color(th.sub));
                    ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                    if ui.button(tr("Annuler")).clicked() {
                        cancel = true;
                    }
                } else if !ready {
                    // why the button can't be pressed
                    let why = if provider_label.is_none() { tr("Choisissez d'abord un modèle d'IA, ci-dessus.") } else { tr("Décrivez d'abord ce qu'elle doit faire.") };
                    ui.label(RichText::new(why).size(12.5).color(th.sub));
                }
            });
            if !f.status.is_empty() {
                ui.add_space(10.0);
                let (fill, text) = if f.failed { (Color32::from_rgb(0xD0, 0x40, 0x30).gamma_multiply(0.12), Color32::from_rgb(0xC0, 0x38, 0x2B)) } else { (th.accent.gamma_multiply(0.12), th.text) };
                Frame::new().fill(fill).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(RichText::new(&f.status).color(text)).wrap());
                });
            }
            if !f.code.is_empty() {
                ui.add_space(8.0);
                if f.pending.is_some() {
                    // what it would be allowed to do, before it is turned on
                    let where_ = if f.matches.is_empty() { tr("Tous les sites").to_string() } else { f.matches.join(", ") };
                    ui.label(RichText::new(tf("Où elle agirait : {}", &[&where_])).family(bold()).color(th.text));
                    ui.label(RichText::new(tr("Sur ces sites, elle pourrait lire et modifier tout ce que la page affiche, y compris ce que vous y tapez.")).size(12.5).color(th.sub));
                    ui.add_space(6.0);
                }
                if f.pending.is_some() {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let b = egui::Button::new(RichText::new(tr("Activer l'extension")).color(Color32::WHITE)).fill(th.accent).min_size(vec2(160.0, 34.0)).corner_radius(CornerRadius::same(10));
                        if ui.add(b).clicked() {
                            enable = true;
                        }
                        // not what was wanted: its file goes
                        if ui.add(egui::Button::new(tr("Jeter")).min_size(vec2(80.0, 34.0)).corner_radius(CornerRadius::same(10))).clicked() {
                            discard = true;
                        }
                    });
                    // see it work on a real page first, in that tab only
                    ui.add_space(10.0);
                    if pages.is_empty() {
                        ui.label(RichText::new(tr("Pour l'essayer avant de l'activer : ouvrez une page où elle agit dans un autre onglet, puis revenez ici.")).size(12.5).color(th.sub));
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(tr("L'essayer d'abord sur :")).size(12.5).color(th.sub));
                            for (tab, title) in &pages {
                                let short: String = title.chars().take(36).collect();
                                if ui.button(short).clicked() {
                                    try_on = Some(*tab);
                                }
                            }
                        });
                    }
                }
                ui.add_space(8.0);
                egui::CollapsingHeader::new(tr("Voir le code")).default_open(f.pending.is_some() || f.busy).show(ui, |ui| {
                    let mut code = f.code.clone();
                    // (its own scrolling: the buttons above stay in view)
                    egui::ScrollArea::vertical().id_salt("ext-code").max_height(300.0).stick_to_bottom(f.busy).show(ui, |ui| {
                        ui.add(TextEdit::multiline(&mut code).code_editor().desired_width(f32::INFINITY).interactive(false));
                    });
                });
            }
            ui.add_space(10.0);
            ui.label(RichText::new(tr("Les extensions créées sont de simples fichiers dans le dossier des extensions : vous pouvez les désactiver ou les supprimer dans Extensions.")).size(12.0).color(th.sub));
        });
        if let Some(tab) = try_on {
            if let Some(id) = self.ext_form.pending.clone() {
                let name = self.ext_form.name.clone();
                self.try_extension(tab, &id, &name);
            }
        }
        if cancel {
            let f = &mut self.ext_form;
            f.stop.store(true, Ordering::Relaxed);
            f.busy = false;
            f.code.clear();
            f.failed = false;
            f.status = tr("Écriture annulée.").into();
        }
        if discard {
            self.end_extension_trial(false);
            if let Some(id) = self.ext_form.pending.take() {
                self.delete_extension(&id);
            }
            self.ext_form.code.clear();
            self.ext_form.status.clear();
            self.ext_form.matches.clear();
        }
        // the person read it and turned it on
        if enable {
            self.ext_trial = None;
            if let Some(id) = self.ext_form.pending.take() {
                if !self.store.extensions_on.contains(&id) {
                    self.toggle_extension(&id);
                }
                self.ext_form.status = tf("« {} » est activée : elle s'applique aux pages ouvertes.", &[&self.ext_form.name]);
                self.ext_form.failed = false;
            }
        }
        if let Some(t) = use_tool {
            self.store.settings.ai = t.key().to_string();
            self.store.settings.ai_model.clear();
            self.store.save_settings();
        }
        if go_settings {
            self.navigate(self.active, super::settings::Section::Intelligence.address(), None, true);
        }
        if create {
            let provider = self.ai_provider();
            if provider == Provider::Local {
                self.ext_form.status = tr("Ajoutez votre clé Claude ou lancez Ollama (Paramètres › Intelligence).").into();
                self.ext_form.failed = true;
                return;
            }
            // written again: the draft not turned on gives way to the new one
            self.end_extension_trial(false);
            if let Some(id) = self.ext_form.pending.take() {
                self.delete_extension(&id);
                self.notice = None;
            }
            self.ai_run += 1;
            let run = self.ai_run;
            let f = &mut self.ext_form;
            f.busy = true;
            f.failed = false;
            f.started = now;
            f.run = run;
            f.stop = Default::default();
            let stop = f.stop.clone();
            f.status.clear();
            f.code.clear();
            f.pending = None;
            f.matches.clear();
            if f.name.trim().is_empty() {
                f.name = f.description.split_whitespace().take(5).collect::<Vec<_>>().join(" ");
            }
            let (name, desc, sites) = (f.name.clone(), f.description.clone(), f.sites.clone());
            let tx = self.ai_tx.clone();
            let repaint = self.ctx.clone();
            std::thread::Builder::new()
                .name("sanaga-extension".into())
                .spawn(move || {
                    // (the code shows as the model writes it)
                    let (tx2, repaint2) = (tx.clone(), repaint.clone());
                    let result = ai::write_extension(&provider, &name, &desc, &sites, &stop, &mut |text| {
                        let _ = tx2.send(AiDone::ExtensionPiece(run, text.to_string()));
                        repaint2.request_repaint();
                    });
                    let _ = tx.send(AiDone::Extension(run, result));
                    repaint.request_repaint();
                })
                .ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(p: &Para) -> String {
        p.spans.iter().map(|s| if s.1 { format!("[{}]", s.0) } else { s.0.clone() }).collect()
    }

    #[test]
    fn pages_to_try_an_extension_on() {
        assert!(pattern_fits("*://*.youtube.com/*", "https://www.youtube.com/watch?v=1"));
        assert!(pattern_fits("*://youtube.com/*", "https://m.youtube.com/"));
        assert!(pattern_fits("https://example.org/a/*", "https://example.org/b"));
        assert!(pattern_fits("*://*/*", "https://anything.example/"));
        assert!(!pattern_fits("*://*.youtube.com/*", "https://notyoutube.com/"));
        assert!(!pattern_fits("*://*.youtube.com/*", "https://example.com/youtube.com"));
    }

    #[test]
    fn a_models_answer_becomes_paragraphs() {
        let p = paragraphs("## Résumé\n\nLa **Sanaga** est un fleuve.\n• Premier **point**\n- Deuxième\n* Troisième\n1. Quatrième\n12. Douzième\n**Titre**\nUn ** seul et 3. pas une liste\n\n");
        let shown: Vec<(bool, bool, String)> = p.iter().map(|x| (x.bullet, x.heading, plain(x))).collect();
        assert_eq!(
            shown,
            vec![
                (false, true, "Résumé".to_string()),
                (false, false, "La [Sanaga] est un fleuve.".to_string()),
                (true, false, "Premier [point]".to_string()),
                (true, false, "Deuxième".to_string()),
                (true, false, "Troisième".to_string()),
                (true, false, "Quatrième".to_string()),
                (true, false, "Douzième".to_string()),
                (false, true, "[Titre]".to_string()),
                (false, false, "Un ** seul et 3. pas une liste".to_string()),
            ]
        );
        assert!(paragraphs("").is_empty() && paragraphs("\n \n").is_empty());
        // whatever a model writes, nothing is lost and nothing panics
        crate::mutate::run(&["• a **b** c\n## d\n1. e **f".as_bytes(), "**\n****\n- \n#".as_bytes()], 10_000, |d| {
            let _ = paragraphs(&String::from_utf8_lossy(d));
        });
    }
}
