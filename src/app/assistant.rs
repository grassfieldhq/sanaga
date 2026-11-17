//! The assistant: a conversation at the side of the page. The person asks
//! about the page shown (or a selection, other tabs, files), the model
//! answers as it writes, and the person can go on asking. A page's summary
//! is such a conversation's first question.
//!
//! What may go to which model is decided here and in `chat.rs`:
//! - a page's text only goes to a model that Sanaga reaches without giving
//!   it tools of its own (`ai::page_content_allowed`);
//! - from a private tab, only to a model on this computer;
//! - a page's text is sent once, with the question it is attached to — not
//!   again with every later question.

use super::intelligence::AiDone;
use super::*;
use crate::ai::{self, Provider};
use crate::chat::{self, Attachment, Source, SourceKind};
use crate::icons::{self, Icon};
use eframe::egui::{Align, Color32, Layout, Margin, RichText, Sense, TextEdit};

/// The page's readable text, its selection and its title, as JSON. Runs in
/// Sanaga's own world. The text is the page's main part (its article when
/// it has one) as the person sees it, then what `innerText` does not reach:
/// the open shadow trees (web components) and the frames of the same site.
/// (Frames of other sites cannot be read from here: the engine keeps them
/// apart.)
const PAGE_JS: &str = r#"(function(){
  var MAX = 400000;
  function tidy(t){ return (t || '').replace(/[ \t ]+\n/g, '\n').replace(/\n{3,}/g, '\n\n').trim(); }
  var root = document.querySelector('article') || document.querySelector('main') || document.querySelector('[role=main]') || document.body;
  var text = tidy(root && root.innerText);
  if (text.length < 600 && document.body) text = tidy(document.body.innerText);
  var extra = [], seen = 0;
  function deep(node, depth){
    if (depth > 5 || !node || !node.querySelectorAll) return;
    var all = node.querySelectorAll('*');
    for (var i = 0; i < all.length && seen < MAX; i++) {
      var el = all[i];
      if (el.shadowRoot) {
        var kids = el.shadowRoot.children, parts = [];
        for (var k = 0; k < kids.length; k++) { var t = kids[k].innerText; if (t) parts.push(t); }
        var s = tidy(parts.join('\n'));
        if (s.length > 40 && text.indexOf(s.slice(0, 60)) < 0) { extra.push(s); seen += s.length; }
        deep(el.shadowRoot, depth + 1);
      }
      if (el.tagName === 'IFRAME' || el.tagName === 'FRAME') {
        try {
          var d = el.contentDocument;
          if (d && d.body) {
            var f = tidy(d.body.innerText);
            if (f.length > 40) { extra.push(f); seen += f.length; }
            deep(d, depth + 1);
          }
        } catch (e) {}
      }
    }
  }
  deep(document, 0);
  if (extra.length) text = (text + '\n\n' + extra.join('\n\n')).trim();
  return JSON.stringify({
    title: (document.title || '').replace(/\s+/g, ' ').trim(),
    text: text.slice(0, MAX),
    whole: text.length,
    selection: String(window.getSelection() || '').slice(0, 20000)
  });
})()"#;

/// What a summary asks the model (the person sees "Summarise the page").
const SUMMARY_ASK: &str = "Summarise this page: one sentence that says what it is about, then 3 to 6 short bullet points with the key facts (start each with \"• \"). No preamble, no headings.";

/// A page's text as it comes back from the page.
#[derive(Debug, PartialEq)]
pub(super) struct PageText {
    pub title: String,
    pub text: String,
    /// The page held more text than was read (its whole length).
    pub whole: usize,
    pub selection: String,
}

pub(super) fn read_page_text(json: &str) -> Option<PageText> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    Some(PageText { title: s("title"), text: s("text"), whole: v["whole"].as_u64().unwrap_or(0) as usize, selection: s("selection") })
}

/// What happens to a conversation while a model answers (sent from its thread).
pub(super) enum ChatEvent {
    /// Some more words.
    Piece(String),
    /// The model asks for a tool (a line that says which, for the person).
    Calling(String),
    /// That tool answered (false: it failed or was refused).
    Called(bool),
    /// Something the person should know.
    Note(String),
    /// The answer is over: the conversation as it is now.
    Done { result: Result<(), String>, entries: Vec<chat::Entry> },
    /// A tool the window must run (or ask the person about).
    Ask(ToolAsk),
}

/// A tool call waiting for the window's answer.
pub(super) struct ToolAsk {
    pub call: chat::Call,
    pub reply: Sender<chat::ToolResult>,
}

/// One thing shown in the conversation.
#[derive(Debug, PartialEq)]
enum Step {
    /// The person's question: what is shown, the names of what was attached
    /// (with the note of a text that was cut).
    Asked { text: String, attached: Vec<String> },
    /// The model's words.
    Said(String),
    /// A tool the model used: its line, and how it went (None: under way).
    Tool(String, Option<bool>),
}

/// Something the person attaches to the question being typed.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Attach {
    /// A text already in hand (a selection, a text file).
    Text(Source),
    /// A picture or a PDF.
    File(Attachment),
}

impl Attach {
    fn name(&self) -> String {
        match self {
            Attach::Text(s) if s.kind == SourceKind::Selection => tr("Sélection").to_string(),
            Attach::Text(s) => s.title.clone(),
            Attach::File(f) => f.name.clone(),
        }
    }
}

/// A question sent, waiting for the text of the pages attached to it.
struct Pending {
    text: String,
    /// What the person sees in place of `text` (a summary's long request).
    label: String,
    summary: bool,
    /// The pages asked for: (ticket, tab, address when asked, what came).
    pages: Vec<(u64, u64, String, Option<Source>)>,
    extra: Vec<Attach>,
    since: f64,
}

pub(super) struct Assistant {
    /// The steps settled so far.
    entries: Vec<chat::Entry>,
    /// What the person sees for the questions whose text is not what they
    /// typed (by the question's place among the questions).
    labels: HashMap<usize, String>,
    /// The conversation's instructions and tools: those it began with.
    system: String,
    tools: Vec<chat::Tool>,
    /// Began in a private tab: only a model on this computer, nothing kept.
    private: bool,
    /// The question being typed, and what is attached to it.
    input: String,
    attach: Vec<Attach>,
    /// The page shown goes with the next question.
    with_page: bool,
    /// The pages already given to the model (tab, address): not sent again
    /// unless the person asks.
    given: HashSet<(u64, String)>,
    pending: Option<Pending>,
    /// A model is answering: its request, what stops it, what it has
    /// written and done so far.
    busy: bool,
    run: u64,
    stop: Arc<AtomicBool>,
    live: Vec<Step>,
    /// Who answered last, a remark under the answer, Sanaga's own summary
    /// kept in case the model fails.
    by: String,
    note: String,
    fallback: Option<String>,
    /// The field takes the keyboard at the next frame.
    focus: bool,
    /// The tab the panel was opened from (the toolbar button closes it there).
    tab: u64,
}

impl Assistant {
    fn new(tab: u64, private: bool) -> Assistant {
        Assistant {
            entries: Vec::new(),
            labels: HashMap::new(),
            system: String::new(),
            tools: Vec::new(),
            private,
            input: String::new(),
            attach: Vec::new(),
            with_page: true,
            given: HashSet::new(),
            pending: None,
            busy: false,
            run: 0,
            stop: Default::default(),
            live: Vec::new(),
            by: String::new(),
            note: String::new(),
            fallback: None,
            focus: true,
            tab,
        }
    }

    pub(super) fn is_for(&self, tab: u64) -> bool {
        self.tab == tab
    }

    fn questions(&self) -> usize {
        self.entries.iter().filter(|e| matches!(e, chat::Entry::User { .. })).count()
    }
}

/// The name a source is shown under, with the note of a text that was cut.
fn source_name(s: &Source) -> String {
    let name = match s.kind {
        SourceKind::Selection => tr("Sélection").to_string(),
        _ if s.title.trim().is_empty() => s.address.clone(),
        _ => s.title.clone(),
    };
    match s.cut {
        Some((shown, whole)) => tf("{} — texte trop long : {} % lus", &[&name, &(shown * 100 / whole.max(1))]),
        None => name,
    }
}

/// The conversation as it is shown: questions, answers, tools used.
fn steps_of(entries: &[chat::Entry], labels: &HashMap<usize, String>, tool_line: &dyn Fn(&chat::Call) -> String) -> Vec<Step> {
    let mut out = Vec::new();
    let mut question = 0;
    for (i, e) in entries.iter().enumerate() {
        match e {
            chat::Entry::User { text, sources, files } => {
                let mut attached: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
                attached.extend(sources.iter().map(source_name));
                out.push(Step::Asked { text: labels.get(&question).cloned().unwrap_or_else(|| text.clone()), attached });
                question += 1;
            }
            chat::Entry::Assistant { text, calls, .. } => {
                if !text.trim().is_empty() {
                    out.push(Step::Said(text.clone()));
                }
                // how each call went is in the step after
                let results = match entries.get(i + 1) {
                    Some(chat::Entry::Results(r)) => r.as_slice(),
                    _ => &[],
                };
                for c in calls {
                    let went = results.iter().find(|r| r.id == c.id).map(|r| !r.error);
                    out.push(Step::Tool(tool_line(c), went.or(Some(false))));
                }
            }
            chat::Entry::Results(_) => {}
        }
    }
    out
}

impl Sanaga {
    /// The model a conversation may use in this tab, or why there is none.
    fn chat_provider(&self, tab: u64) -> Result<Provider, String> {
        let chosen = self.ai_provider();
        let provider = self.page_text_provider(tab);
        if provider == Provider::Local {
            return Err(if chosen != Provider::Local {
                tr("Dans un onglet privé, seul un modèle sur cet ordinateur (Ollama) peut répondre : le texte de la page ne sort pas.").to_string()
            } else {
                tr("Pour poser des questions, choisissez un modèle d'IA : Ollama sur cet ordinateur, ou Claude avec votre clé.").to_string()
            });
        }
        ai::page_content_allowed(&provider)?;
        Ok(provider)
    }

    /// The language answers are asked in.
    fn answer_language() -> &'static str {
        match crate::i18n::language() {
            "fr" => "French",
            "es" => "Spanish",
            "pt" => "Portuguese",
            "de" => "German",
            _ => "English",
        }
    }

    /// Open the assistant at the side of the page (or give it the keyboard).
    pub(super) fn open_assistant(&mut self) {
        let t = &self.tabs[self.active];
        let (id, private) = (t.id, t.private);
        match self.assistant.as_mut() {
            Some(a) => a.focus = true,
            None => self.assistant = Some(Assistant::new(id, private)),
        }
        self.ai_menu_open = false;
    }

    /// Close the panel; a model still answering is stopped.
    pub(super) fn close_assistant(&mut self) {
        if let Some(a) = self.assistant.take() {
            a.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Start a new conversation in the panel.
    fn new_conversation(&mut self) {
        if let Some(old) = &self.assistant {
            old.stop.store(true, Ordering::Relaxed);
        }
        let t = &self.tabs[self.active];
        self.assistant = Some(Assistant::new(t.id, t.private));
    }

    /// Summarise the page shown (toolbar button, menu, Ctrl+Shift+Y): a new
    /// conversation whose first question is the summary.
    pub(super) fn summarize_page(&mut self) {
        let t = &self.tabs[self.active];
        if t.web.is_none() || !matches!(t.view, View::Web) {
            self.notice = Some((tr("Le résumé marche sur les pages des sites.").into(), self.ctx.input(|i| i.time)));
            return;
        }
        self.new_conversation();
        self.assistant_ask(SUMMARY_ASK.to_string(), tr("Résumer la page").to_string(), true);
    }

    /// Words selected in a page, to ask about (the page's right-click menu).
    pub(super) fn ask_about_selection(&mut self, tab: u64, words: String) {
        let Some(t) = self.tabs.iter().find(|t| t.id == tab) else { return };
        let source = Source { kind: SourceKind::Selection, title: t.title(), address: t.url().to_string(), text: words, cut: None };
        self.open_assistant();
        if let Some(a) = self.assistant.as_mut() {
            a.attach.retain(|x| !matches!(x, Attach::Text(s) if s.kind == SourceKind::Selection));
            a.attach.push(Attach::Text(source));
            // (the selection is the subject: the whole page does not go with it)
            a.with_page = false;
            a.focus = true;
        }
    }

    /// Ask a page for its text; it comes back as `AiDone::PageText`.
    fn request_page_text(&mut self, tab: u64) -> Option<u64> {
        let t = self.tabs.iter().find(|t| t.id == tab)?;
        let w = t.web.as_ref().filter(|_| matches!(t.view, View::Web))?;
        self.ai_run += 1;
        let ticket = self.ai_run;
        let (tx, repaint) = (self.ai_tx.clone(), self.ctx.clone());
        w.ask_own(PAGE_JS, move |answer| {
            let _ = tx.send(AiDone::PageText { ticket, text: answer.as_deref().and_then(read_page_text) });
            repaint.request_repaint();
        });
        Some(ticket)
    }

    /// Send a question: `text` goes to the model, `label` is what the
    /// person sees when it differs. The pages attached are read first.
    fn assistant_ask(&mut self, text: String, label: String, summary: bool) {
        let active = self.tabs[self.active].id;
        let Some(a) = self.assistant.as_mut() else { return };
        if a.busy || a.pending.is_some() {
            return;
        }
        let with_page = a.with_page || summary;
        let extra = std::mem::take(&mut a.attach);
        a.note.clear();
        let mut pages = Vec::new();
        if with_page {
            let address = self.tabs[self.active].url().to_string();
            if let Some(ticket) = self.request_page_text(active) {
                pages.push((ticket, active, address, None));
            }
        }
        let now = self.ctx.input(|i| i.time);
        if let Some(a) = self.assistant.as_mut() {
            a.pending = Some(Pending { text, label, summary, pages, extra, since: now });
        }
    }

    /// A page's text arrived.
    pub(super) fn page_text_arrived(&mut self, ticket: u64, text: Option<PageText>) {
        let Some(p) = self.assistant.as_mut().and_then(|a| a.pending.as_mut()) else { return };
        let Some(slot) = p.pages.iter_mut().find(|x| x.0 == ticket) else { return };
        let got = text.unwrap_or(PageText { title: String::new(), text: String::new(), whole: 0, selection: String::new() });
        let shown = got.text.chars().count();
        slot.3 = Some(Source {
            kind: SourceKind::Page,
            title: got.title,
            address: slot.2.clone(),
            // (a page longer than what the page script hands over)
            cut: (got.whole > got.text.len() && shown > 0).then_some((shown, got.whole.max(shown))),
            text: got.text,
        });
    }

    /// Once the pages of a question have answered (or been waited for long
    /// enough), the question goes to the model.
    pub(super) fn assistant_step(&mut self) {
        let now = self.ctx.input(|i| i.time);
        let ready = self.assistant.as_ref().and_then(|a| a.pending.as_ref()).is_some_and(|p| p.pages.iter().all(|x| x.3.is_some()) || now - p.since > 8.0);
        if !ready {
            if self.assistant.as_ref().is_some_and(|a| a.pending.is_some()) {
                self.ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
            return;
        }
        let active = self.tabs[self.active].id;
        let Some(p) = self.assistant.as_mut().and_then(|a| a.pending.take()) else { return };
        let mut sources: Vec<Source> = Vec::new();
        let mut files: Vec<Attachment> = Vec::new();
        let mut given: Vec<(u64, String)> = Vec::new();
        for x in p.extra {
            match x {
                Attach::Text(s) => sources.push(s),
                Attach::File(f) => files.push(f),
            }
        }
        for (_, tab, address, got) in p.pages {
            if let Some(s) = got.filter(|s| !s.text.trim().is_empty()) {
                given.push((tab, address));
                sources.push(s);
            }
        }
        let page_words: usize = sources.iter().filter(|s| s.kind == SourceKind::Page).map(|s| s.text.split_whitespace().count()).sum();
        let own_summary = sources.iter().find(|s| s.kind == SourceKind::Page).map(|s| ai::local_summary(&s.text, 6)).unwrap_or_default();
        let provider = self.chat_provider(active);
        let Some(a) = self.assistant.as_mut() else { return };
        if !p.label.is_empty() {
            a.labels.insert(a.questions(), p.label.clone());
        }
        let said = |a: &mut Assistant, text: String, by: String, note: String| {
            a.entries.push(chat::Entry::Assistant { text, calls: Vec::new(), blocks: None, stopped: false });
            a.by = by;
            a.note = note;
        };
        if p.summary && page_words < 30 {
            a.entries.push(chat::Entry::User { text: p.text, sources: Vec::new(), files: Vec::new() });
            said(a, tr("Cette page contient trop peu de texte à résumer.").into(), String::new(), String::new());
            return;
        }
        let provider = match provider {
            Ok(p) => p,
            // no model for this tab: Sanaga's own summary, and why
            Err(why) => {
                a.entries.push(chat::Entry::User { text: p.text, sources: Vec::new(), files: Vec::new() });
                if p.summary {
                    let chosen_none = self.store.settings.ai.is_empty();
                    said(a, own_summary, Provider::Local.label(), if chosen_none { String::new() } else { why });
                } else {
                    a.entries.pop();
                    a.labels.remove(&a.questions());
                    a.note = why;
                }
                return;
            }
        };
        // the conversation's instructions are those of its first question
        if a.system.is_empty() {
            a.system = chat::system_prompt(Self::answer_language(), !a.tools.is_empty() && chat::tools_possible(&provider));
        }
        chat::fit_sources(&mut sources, chat::source_budget(&provider));
        a.given.extend(given);
        a.with_page = false;
        a.entries.push(chat::Entry::User { text: p.text, sources, files });
        a.fallback = p.summary.then_some(own_summary);
        a.by = provider.label();
        self.assistant_run(provider);
    }

    /// Let the model answer the conversation as it stands, on a thread.
    fn assistant_run(&mut self, provider: Provider) {
        self.ai_run += 1;
        let run = self.ai_run;
        let Some(a) = self.assistant.as_mut() else { return };
        a.busy = true;
        a.run = run;
        a.stop = Default::default();
        a.live.clear();
        let (system, tools, mut entries, stop) = (a.system.clone(), a.tools.clone(), a.entries.clone(), a.stop.clone());
        let (tx, repaint) = (self.ai_tx.clone(), self.ctx.clone());
        let lines = self.tool_lines();
        std::thread::Builder::new()
            .name("sanaga-chat".into())
            .spawn(move || {
                let send = |ev: ChatEvent| {
                    let _ = tx.send(AiDone::Chat { run, ev });
                    repaint.request_repaint();
                };
                // a tool is run by the window (it may ask the person): its
                // answer is waited for here, never there
                let mut call_tool = |call: &chat::Call| {
                    let (reply, answer) = channel();
                    send(ChatEvent::Ask(ToolAsk { call: call.clone(), reply }));
                    loop {
                        match answer.recv_timeout(std::time::Duration::from_millis(200)) {
                            Ok(r) => return r,
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if !stop.load(Ordering::Relaxed) => {}
                            Err(_) => return chat::ToolResult { id: call.id.clone(), name: call.name.clone(), text: "Stopped by the person.".into(), error: true },
                        }
                    }
                };
                let mut on = |ev: chat::Event| match ev {
                    chat::Event::Text(t) => send(ChatEvent::Piece(t.to_string())),
                    chat::Event::Calling(c) => send(ChatEvent::Calling(lines(c))),
                    chat::Event::Result(r) => send(ChatEvent::Called(!r.error)),
                    chat::Event::Note(n) => send(ChatEvent::Note(n)),
                };
                let result = chat::converse(&provider, &system, &tools, &mut entries, &stop, &mut call_tool, &mut on);
                send(ChatEvent::Done { result, entries });
            })
            .ok();
    }

    /// How a tool call is said to the person (a function a thread can keep).
    fn tool_lines(&self) -> impl Fn(&chat::Call) -> String + Send + 'static {
        |c: &chat::Call| c.name.clone()
    }

    /// What a model's thread says about the conversation it answers.
    pub(super) fn chat_event(&mut self, run: u64, ev: ChatEvent) {
        let Some(a) = self.assistant.as_mut().filter(|a| a.run == run && a.busy) else {
            // (an answer to a conversation that was closed or replaced: a
            // tool it asks for is refused, so that its thread ends)
            if let ChatEvent::Ask(ask) = ev {
                let _ = ask.reply.send(chat::ToolResult { id: ask.call.id.clone(), name: ask.call.name.clone(), text: "Stopped by the person.".into(), error: true });
            }
            return;
        };
        match ev {
            ChatEvent::Piece(t) => match a.live.last_mut() {
                Some(Step::Said(s)) => s.push_str(&t),
                _ => a.live.push(Step::Said(t)),
            },
            ChatEvent::Calling(line) => a.live.push(Step::Tool(line, None)),
            ChatEvent::Called(ok) => {
                if let Some(Step::Tool(_, went)) = a.live.iter_mut().rev().find(|s| matches!(s, Step::Tool(_, None))) {
                    *went = Some(ok);
                }
            }
            ChatEvent::Note(n) => a.note = n,
            ChatEvent::Ask(ask) => self.run_tool(ask),
            ChatEvent::Done { result, entries } => {
                a.busy = false;
                a.live.clear();
                a.entries = entries;
                match result {
                    Ok(()) => {}
                    // (stopped by the person: what was written stays)
                    Err(e) if e == ai::STOPPED => a.note = tr("Réponse arrêtée avant la fin.").to_string(),
                    Err(e) => match a.fallback.take().filter(|f| !f.is_empty()) {
                        // the model failed on a summary: Sanaga's own, and why
                        Some(own) => {
                            a.entries.push(chat::Entry::Assistant { text: own, calls: Vec::new(), blocks: None, stopped: false });
                            a.by = Provider::Local.label();
                            a.note = e;
                        }
                        None => a.note = e,
                    },
                }
                a.fallback = None;
                a.focus = true;
            }
        }
    }

    /// Run a tool the model asked for. (No tool is given to models yet:
    /// whatever is asked for is refused.)
    fn run_tool(&mut self, ask: ToolAsk) {
        let _ = ask.reply.send(chat::ToolResult { id: ask.call.id.clone(), name: ask.call.name.clone(), text: "There is no such tool.".into(), error: true });
    }

    /// The assistant, at the side of the page.
    pub(super) fn assistant_panel(&mut self, ui: &mut Ui, th: &Theme) {
        let active = &self.tabs[self.active];
        let (active_id, active_url, active_title) = (active.id, active.url().to_string(), active.title());
        let page_can = active.web.is_some() && matches!(active.view, View::Web);
        let no_model = self.ai_provider() == Provider::Local;
        let available = self.chat_provider(active_id);
        let Some(a) = self.assistant.as_mut() else { return };
        // the page shown goes with the next question when the model has
        // not been given it yet (a new page, another tab)
        let fresh = page_can && !a.given.contains(&(active_id, active_url.clone()));
        let (mut close, mut renew, mut choose, mut stop, mut send, mut again, mut summarise, mut edit, mut copy) = (false, false, false, false, false, false, false, false, None::<String>);
        let mut drop_attach: Option<usize> = None;
        let lines = |c: &chat::Call| c.name.clone();
        let mut steps = steps_of(&a.entries, &a.labels, &lines);
        if let Some(p) = &a.pending {
            steps.push(Step::Asked { text: if p.label.is_empty() { p.text.clone() } else { p.label.clone() }, attached: Vec::new() });
        }
        let working = a.busy || a.pending.is_some();

        egui::Panel::top("assistant-head").show_separator_line(false).frame(Frame::NONE.inner_margin(Margin { left: 18, right: 12, top: 12, bottom: 6 })).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                icons::paint(ui.painter(), r, Icon::Sparkle, th.accent);
                ui.label(RichText::new(tr("Assistant")).size(17.0).family(bold()).color(th.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if super::chrome::icon_button_small(ui, Icon::Close, th).tip(tr("Fermer")).clicked() {
                        close = true;
                    }
                    if !steps.is_empty() && super::chrome::icon_button_small(ui, Icon::Plus, th).tip(tr("Nouvelle conversation")).clicked() {
                        renew = true;
                    }
                });
            });
        });

        egui::Panel::bottom("assistant-foot").show_separator_line(false).frame(Frame::NONE.inner_margin(Margin { left: 14, right: 14, top: 6, bottom: 12 })).show(ui, |ui| {
            if !a.note.is_empty() {
                ui.add(egui::Label::new(RichText::new(&a.note).size(12.0).color(Color32::from_rgb(0xD0, 0x60, 0x30))).wrap());
                ui.add_space(4.0);
            }
            if let Err(why) = &available {
                if a.note != *why && !steps.is_empty() || steps.is_empty() {
                    ui.add(egui::Label::new(RichText::new(why).size(12.5).color(th.sub)).wrap());
                }
                if ui.link(RichText::new(if no_model { tr("Choisir une IA…") } else { tr("Paramètres › Intelligence…") }).size(12.5)).clicked() {
                    choose = true;
                }
                ui.add_space(4.0);
            }
            // what goes with the question: the page shown, what was attached
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
                if page_can {
                    let on = a.with_page;
                    let name: String = active_title.chars().take(30).collect();
                    let label = if on { tf("Avec cette page : {}", &[&name]) } else { tr("Sans la page").to_string() };
                    let chip = egui::Button::new(RichText::new(label).size(12.0).color(if on { th.text } else { th.sub }))
                        .fill(if on { th.accent.gamma_multiply(0.16) } else { th.card })
                        .corner_radius(CornerRadius::same(10));
                    let tip =
                        if on { tr("Le texte de la page part avec la question. Cliquer pour ne pas l'envoyer.") } else { tr("La question part seule. Cliquer pour joindre le texte de la page.") };
                    if ui.add_enabled(!working, chip).tip(tip).clicked() {
                        a.with_page = !on;
                    }
                }
                for (k, x) in a.attach.iter().enumerate() {
                    let name: String = x.name().chars().take(28).collect();
                    let chip = egui::Button::new(RichText::new(format!("{name}  ×")).size(12.0).color(th.text)).fill(th.accent.gamma_multiply(0.16)).corner_radius(CornerRadius::same(10));
                    if ui.add(chip).tip(tr("Retirer")).clicked() {
                        drop_attach = Some(k);
                    }
                }
            });
            ui.add_space(4.0);
            let hint = if steps.is_empty() { tr("Posez une question sur cette page…") } else { tr("Poursuivre la conversation…") };
            let id = egui::Id::new("assistant-input");
            let field = TextEdit::multiline(&mut a.input)
                .id(id)
                .hint_text(hint)
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                // (Enter sends; Shift+Enter starts a new line)
                .return_key(Some(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, Key::Enter)));
            let resp = ui.add_enabled(available.is_ok(), field);
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, tr("Question à l'assistant")));
            if a.focus && available.is_ok() {
                resp.request_focus();
                a.focus = false;
            }
            let enter = (resp.has_focus() || resp.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if working {
                    ui.spinner();
                    let what = if a.pending.is_some() { tr("Lecture de la page…") } else { tr("Écriture de la réponse…") };
                    ui.label(RichText::new(what).size(12.5).color(th.sub));
                    if a.busy && ui.button(tr("Arrêter")).clicked() {
                        stop = true;
                    }
                } else {
                    let ready = available.is_ok() && !a.input.trim().is_empty();
                    let b = egui::Button::new(RichText::new(tr("Envoyer")).color(Color32::WHITE)).fill(if ready { th.accent } else { th.sub }).corner_radius(CornerRadius::same(9));
                    if ui.add_enabled(ready, b).clicked() || (ready && enter) {
                        send = true;
                    }
                    if !a.by.is_empty() {
                        ui.label(RichText::new(tf("Par : {}", &[&a.by])).size(11.5).color(th.sub));
                    }
                }
            });
        });

        egui::CentralPanel::no_frame().frame(Frame::NONE.inner_margin(Margin { left: 18, right: 14, top: 4, bottom: 4 })).show(ui, |ui| {
            if steps.is_empty() && a.live.is_empty() {
                // nothing asked yet: what can be done from here
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(tr("Posez une question sur la page affichée, ou demandez-en le résumé. Le texte de la page part avec votre première question.")).size(13.5).color(th.sub),
                    )
                    .wrap(),
                );
                ui.add_space(10.0);
                if page_can && ui.button(tr("Résumer la page")).clicked() {
                    summarise = true;
                }
                return;
            }
            egui::ScrollArea::vertical().id_salt("assistant-scroll").auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                let last_said = steps.iter().rposition(|s| matches!(s, Step::Said(_)));
                // (a question shown under another name — a summary — is asked again, not edited)
                let last_asked = steps.iter().rposition(|s| matches!(s, Step::Asked { .. })).filter(|_| !a.labels.contains_key(&(a.questions().saturating_sub(1))));
                for (k, s) in steps.iter().chain(a.live.iter()).enumerate() {
                    match s {
                        Step::Asked { text, attached } => {
                            ui.add_space(6.0);
                            Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.add(egui::Label::new(RichText::new(text).size(14.0).color(th.text)).selectable(true).wrap());
                                // the last question can be taken back and asked differently
                                if !working && Some(k) == last_asked && ui.link(RichText::new(tr("Modifier")).size(11.5)).clicked() {
                                    edit = true;
                                }
                                for name in attached {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 5.0;
                                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::hover());
                                        icons::paint(ui.painter(), r, Icon::Reader, th.sub);
                                        ui.add(egui::Label::new(RichText::new(name).size(11.5).color(th.sub)).truncate());
                                    });
                                }
                            });
                            ui.add_space(8.0);
                        }
                        Step::Said(text) => {
                            super::intelligence::rich_text(ui, th, text);
                            if !working && Some(k) == last_said {
                                ui.horizontal(|ui| {
                                    if ui.small_button(tr("Copier")).clicked() {
                                        copy = Some(text.clone());
                                    }
                                    if a.entries.len() <= 2
                                        && a.labels.contains_key(&0)
                                        && ui.small_button(if a.tab == active_id && !fresh { tr("Résumer à nouveau") } else { tr("Résumer cette page") }).clicked()
                                    {
                                        again = true;
                                    }
                                });
                            }
                        }
                        Step::Tool(line, went) => {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                match went {
                                    None => {
                                        ui.add(egui::Spinner::new().size(12.0));
                                    }
                                    Some(ok) => {
                                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::hover());
                                        icons::paint(ui.painter(), r, if *ok { Icon::Search } else { Icon::Close }, th.sub);
                                    }
                                }
                                ui.add(egui::Label::new(RichText::new(line).size(12.0).color(th.sub)).wrap());
                            });
                            ui.add_space(4.0);
                        }
                    }
                }
                ui.add_space(6.0);
            });
        });

        if let Some(k) = drop_attach {
            a.attach.remove(k);
        }
        if fresh && !working && !a.with_page && a.input.is_empty() && a.attach.is_empty() && a.tab != active_id {
            // (the person moved to another tab: its page is offered again)
            a.with_page = true;
            a.tab = active_id;
        }
        if stop {
            a.stop.store(true, Ordering::Relaxed);
        }
        if edit {
            // the last question and what followed it leave the conversation
            // (only ever its end: see chat.rs); its words go back to the field
            if let Some(at) = a.entries.iter().rposition(|e| matches!(e, chat::Entry::User { .. })) {
                if let chat::Entry::User { text, sources, files } = a.entries.remove(at) {
                    a.entries.truncate(at);
                    a.input = text;
                    for s in sources {
                        match s.kind {
                            // (a page is read again when the question is sent)
                            SourceKind::Page => {
                                a.given.retain(|(_, address)| *address != s.address);
                                a.with_page = true;
                            }
                            _ => a.attach.push(Attach::Text(s)),
                        }
                    }
                    a.attach.extend(files.into_iter().map(Attach::File));
                    a.note.clear();
                    a.focus = true;
                }
            }
        }
        if let Some(text) = copy {
            self.ctx.copy_text(text);
        }
        if choose {
            self.open_settings(&super::settings::Section::Intelligence.address());
        }
        if close {
            self.close_assistant();
        } else if renew {
            self.new_conversation();
        } else if again || summarise {
            self.summarize_page();
        } else if send {
            let text = self.assistant.as_mut().map(|a| std::mem::take(&mut a.input).trim().to_string()).unwrap_or_default();
            self.assistant_ask(text, String::new(), false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pages_answer_is_read() {
        let got = read_page_text(r#"{"title":"Sanaga — Wikipédia","text":"La Sanaga est un fleuve.","whole":24,"selection":"fleuve"}"#).unwrap();
        assert_eq!(got, PageText { title: "Sanaga — Wikipédia".into(), text: "La Sanaga est un fleuve.".into(), whole: 24, selection: "fleuve".into() });
        // what a page's script could not answer
        assert_eq!(read_page_text("undefined"), None);
        assert_eq!(read_page_text(""), None);
        assert!(read_page_text("{}").is_some_and(|p| p.text.is_empty()));
        crate::mutate::run(&[br#"{"title":"t","text":"x","whole":1,"selection":""}"#], 5_000, |d| {
            let _ = read_page_text(&String::from_utf8_lossy(d));
        });
    }

    #[test]
    fn a_conversation_is_shown_as_questions_answers_and_tools() {
        let page = Source { kind: SourceKind::Page, title: "Douala".into(), address: "https://example.org".into(), text: "x".into(), cut: Some((50, 200)) };
        let call = chat::Call { id: "1".into(), name: "search_history".into(), input: serde_json::json!({}) };
        let entries = vec![
            chat::Entry::User { text: SUMMARY_ASK.into(), sources: vec![page], files: Vec::new() },
            chat::Entry::Assistant { text: "Je cherche.".into(), calls: vec![call.clone()], blocks: None, stopped: false },
            chat::Entry::Results(vec![chat::ToolResult { id: "1".into(), name: "search_history".into(), text: "rien".into(), error: false }]),
            chat::Entry::Assistant { text: "Voilà.".into(), calls: Vec::new(), blocks: None, stopped: false },
            chat::Entry::User { text: "Et ensuite ?".into(), sources: Vec::new(), files: Vec::new() },
        ];
        let labels = HashMap::from([(0, "Résumer la page".to_string())]);
        let steps = steps_of(&entries, &labels, &|c| format!("outil {}", c.name));
        assert_eq!(steps.len(), 5);
        // the summary's long request is shown under its short name; a cut text says so
        assert!(matches!(&steps[0], Step::Asked { text, attached } if text == "Résumer la page" && attached.len() == 1 && attached[0].contains("25")));
        assert_eq!(steps[1], Step::Said("Je cherche.".into()));
        assert_eq!(steps[2], Step::Tool("outil search_history".into(), Some(true)));
        assert_eq!(steps[3], Step::Said("Voilà.".into()));
        assert!(matches!(&steps[4], Step::Asked { text, .. } if text == "Et ensuite ?"));
    }
}
