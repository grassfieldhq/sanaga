//! The tools Sanaga gives the assistant's model — and nothing else is ever
//! a tool of that model (see `chat.rs`).
//!
//! Two kinds:
//! - **tools that read** the person's browser: the history, the open tabs,
//!   a tab's page, the favourites, earlier conversations. A model on this
//!   computer uses them freely. A model outside it (Claude) is asked about
//!   the first time it wants each of them in a conversation: what they
//!   return leaves the computer.
//! - **tools that act**: open an address, close or group tabs, run one of
//!   Sanaga's commands. Each use is shown to the person with exactly what
//!   it would do, and waits for their answer — every time.
//!
//! What a tool returns is material for the model to read, like a page.

use super::assistant::{Confirm, ToolAsk};
use super::*;
use crate::chat::{Call, Tool, ToolResult};
use serde_json::{json, Value};

/// The tools given to a conversation at its start.
pub(super) fn all() -> Vec<Tool> {
    let tool = |name: &str, description: &str, schema: Value, acts: bool| Tool { name: name.into(), description: description.into(), schema, acts };
    let commands: Vec<&str> = super::buttons::commands().into_iter().map(|c| c.0).filter(|c| !matches!(*c, "summary" | "ask" | "translate")).collect();
    vec![
        tool(
            "search_history",
            "Search the pages the person visited: their titles and addresses, not their content. Each word given must be in the title or the address. \
             `from` and `to` are local days (YYYY-MM-DD), both included; without them the whole history is searched. Returns up to 40 pages, the latest first, \
             each once with the time of its latest visit in the period. Use it for questions like \"what was that page about X I read last week\".",
            json!({"type": "object", "properties": {
                "words": {"type": "string", "description": "Words of the title or the address; empty for every page of the period."},
                "from": {"type": "string", "description": "First day, YYYY-MM-DD."},
                "to": {"type": "string", "description": "Last day, YYYY-MM-DD."}
            }}),
            false,
        ),
        tool(
            "list_tabs",
            "List the tabs open in this window: each one's number (to use with the other tools), title, address and group, and which one is shown.",
            json!({"type": "object", "properties": {}}),
            false,
        ),
        tool(
            "read_tab",
            "Read the text of the page of an open tab, by its number from list_tabs. A tab that is asleep is loaded first, which can take a few seconds.",
            json!({"type": "object", "properties": {"tab": {"type": "integer", "description": "The tab's number."}}, "required": ["tab"]}),
            false,
        ),
        tool(
            "search_favourites",
            "Search the person's favourites (bookmarks) and reading list by words of their title or address; empty words list them all (up to 60).",
            json!({"type": "object", "properties": {"words": {"type": "string"}}}),
            false,
        ),
        tool(
            "search_conversations",
            "Search the person's earlier conversations with this assistant, kept on this computer, by words that were said in them. Returns the latest matching ones with what was said.",
            json!({"type": "object", "properties": {"words": {"type": "string"}}, "required": ["words"]}),
            false,
        ),
        tool(
            "read_form",
            "List the fields of the form on the page shown that can be filled: each one's number, label, kind, choices and present value. Fields that look like passwords, codes or payment cards, fields not visible on the page and fields inside frames are never listed. Use it before fill_form.",
            json!({"type": "object", "properties": {}}),
            false,
        ),
        tool(
            "fill_form",
            "Propose values for fields of the page shown, by their numbers from read_form. The person is shown each value next to its field's name and accepts or refuses; nothing is ever sent — they send the form themselves.              For a choice (select) give one of its choices; for a checkbox \"yes\" or \"no\"; for a radio button \"yes\" on the one to choose. Only fill what the person asked for, with what they gave you.",
            json!({"type": "object", "properties": {"fields": {"type": "array", "items": {"type": "object", "properties": {"n": {"type": "integer", "description": "The field's number."}, "value": {"type": "string"}}, "required": ["n", "value"]}}}, "required": ["fields"]}),
            true,
        ),
        tool(
            "open_tab",
            "Open an http or https address in a new tab, behind the one shown. The person is shown the address and accepts or refuses.",
            json!({"type": "object", "properties": {"address": {"type": "string"}}, "required": ["address"]}),
            true,
        ),
        tool(
            "close_tabs",
            "Close open tabs, by their numbers from list_tabs. The person is shown their titles and accepts or refuses.",
            json!({"type": "object", "properties": {"tabs": {"type": "array", "items": {"type": "integer"}}}, "required": ["tabs"]}),
            true,
        ),
        tool(
            "group_tabs",
            "Put open tabs (numbers from list_tabs) together in a new named tab group. The person accepts or refuses.",
            json!({"type": "object", "properties": {"tabs": {"type": "array", "items": {"type": "integer"}}, "name": {"type": "string", "description": "The group's name, a word or two."}}, "required": ["tabs", "name"]}),
            true,
        ),
        tool(
            "run_command",
            "Run one of the browser's own commands on the tab shown, as if the person had chosen it in the menu (reader, focus mode, print, sidebar…). The person accepts or refuses.",
            json!({"type": "object", "properties": {"command": {"type": "string", "enum": commands}}, "required": ["command"]}),
            true,
        ),
    ]
}

fn words_of(call: &Call) -> String {
    call.input["words"].as_str().unwrap_or("").trim().chars().take(200).collect()
}

fn tab_ids(call: &Call) -> Vec<u64> {
    call.input["tabs"].as_array().map(|l| l.iter().filter_map(|v| v.as_u64()).collect()).unwrap_or_default()
}

/// A day typed by a model ("2026-11-17") as (year, month, day).
pub(super) fn day_of(text: &str) -> Option<(i32, i32, i32)> {
    let mut parts = text.trim().splitn(3, '-');
    let (y, m, d) = (parts.next()?.parse::<i32>().ok()?, parts.next()?.parse::<i32>().ok()?, parts.next()?.get(..2).unwrap_or("").parse::<i32>().ok()?);
    ((1970..=9999).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d)).then_some((y, m, d))
}

/// The first second (or the last, `end`) of a local day.
fn day_bound(text: &str, end: bool) -> Option<u64> {
    let (y, m, d) = day_of(text)?;
    let (h, min, s) = if end { (23, 59, 59.0) } else { (0, 0, 0.0) };
    let t = gtk::glib::DateTime::from_local(y, m, d, h, min, s).ok()?;
    Some(t.to_unix().max(0) as u64)
}

/// A time as the model and the person read it, in local time.
pub(super) fn when(time: u64) -> String {
    gtk::glib::DateTime::from_unix_local(time as i64).ok().and_then(|t| t.format("%Y-%m-%d %H:%M").ok()).map(|s| s.to_string()).unwrap_or_default()
}

/// Today, for a conversation's instructions ("2026-11-17, Tuesday").
pub(super) fn today() -> String {
    gtk::glib::DateTime::now_local().ok().and_then(|t| t.format("%Y-%m-%d, %A").ok()).map(|s| s.to_string()).unwrap_or_default()
}

/// An address a model may ask to open: a web page, nothing else (no file
/// of the computer, none of Sanaga's own pages, no script).
pub(super) fn openable(address: &str) -> Option<String> {
    let u = url::Url::parse(address.trim()).ok()?;
    (matches!(u.scheme(), "http" | "https") && u.host_str().is_some()).then(|| u.to_string())
}

/// How a tool call is said to the person, in the conversation.
pub(super) fn line(call: &Call, tab_titles: &HashMap<u64, String>) -> String {
    let titles = |ids: &[u64]| ids.iter().map(|id| tab_titles.get(id).cloned().unwrap_or_else(|| format!("n° {id}"))).collect::<Vec<_>>().join(", ");
    match call.name.as_str() {
        "search_history" => {
            let (words, from, to) = (words_of(call), call.input["from"].as_str().unwrap_or(""), call.input["to"].as_str().unwrap_or(""));
            let mut s = if words.is_empty() { tr("Recherche dans l'historique").to_string() } else { tf("Recherche dans l'historique : « {} »", &[&words]) };
            if !from.is_empty() || !to.is_empty() {
                s.push_str(&format!(" ({} → {})", if from.is_empty() { "…" } else { from }, if to.is_empty() { "…" } else { to }));
            }
            s
        }
        "list_tabs" => tr("Liste des onglets ouverts").to_string(),
        "read_form" => tr("Lecture des champs du formulaire").to_string(),
        "fill_form" => tr("Remplissage du formulaire").to_string(),
        "read_tab" => tf("Lecture de l'onglet « {} »", &[&titles(&[call.input["tab"].as_u64().unwrap_or(0)])]),
        "search_favourites" => tf("Recherche dans les favoris : « {} »", &[&words_of(call)]),
        "search_conversations" => tf("Recherche dans les conversations : « {} »", &[&words_of(call)]),
        "open_tab" => tf("Ouvrir {}", &[&call.input["address"].as_str().unwrap_or("")]),
        "close_tabs" => tf("Fermer : {}", &[&titles(&tab_ids(call))]),
        "group_tabs" => tf("Grouper sous « {} » : {}", &[&call.input["name"].as_str().unwrap_or(""), &titles(&tab_ids(call))]),
        "run_command" => {
            let name = call.input["command"].as_str().unwrap_or("");
            let label = super::buttons::commands().into_iter().find(|c| c.0 == name).map(|c| c.1.to_string()).unwrap_or_else(|| name.to_string());
            tf("Commande : {}", &[&label])
        }
        other => other.to_string(),
    }
}

/// The most of an outside call's input said in the conversation's steps
/// (the question put to the person shows all of it: `outside_input`).
const OUTSIDE_SAID: usize = 600;
/// The longest input of an outside call that can be shown to the person.
/// A longer one is refused, not shown in part: what is not read can't be
/// accepted.
pub(super) const OUTSIDE_MAX: usize = 32_000;

/// How a call of an outside tool is said in the conversation's steps: its
/// server, its name, and what is sent to it — cut when long, and then said
/// to be.
pub(super) fn outside_line(server: &str, tool: &str, input: &Value) -> String {
    let whole = input.to_string();
    let n = whole.chars().count();
    let sent = if n <= OUTSIDE_SAID {
        whole
    } else {
        let start: String = whole.chars().take(OUTSIDE_SAID).collect();
        format!("{start}… {}", tf("(coupé ici : {} caractères de plus)", &[&(n - OUTSIDE_SAID)]))
    };
    tf("{} › {} — envoyé : {}", &[&server, &tool, &sent])
}

/// What an outside call sends, whole, as the person reads it before they
/// accept: indented JSON, the characters that draw nothing (zero-width,
/// direction marks) written out so that nothing in it is hidden. None:
/// too long to be read (`OUTSIDE_MAX`).
pub(super) fn outside_input(input: &Value) -> Option<String> {
    let pretty = serde_json::to_string_pretty(input).unwrap_or_default();
    let mut out = String::with_capacity(pretty.len());
    for c in pretty.chars() {
        if invisible(c) {
            out.push_str(&format!("\\u{{{:04X}}}", c as u32));
        } else {
            out.push(c);
        }
    }
    (out.chars().count() <= OUTSIDE_MAX).then_some(out)
}

/// A character that draws nothing, or changes the order of what follows
/// (C1 controls, soft hyphen, joiners and marks, variation selectors,
/// shorthand format controls, musical formatting, tags…).
fn invisible(c: char) -> bool {
    matches!(c as u32, 0x80..=0x9F | 0xAD | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0x34F | 0x61C | 0x115F | 0x1160 | 0x17B4 | 0x17B5 | 0x180B..=0x180F | 0x200B..=0x200F | 0x2028..=0x202E | 0x2060..=0x206F | 0x3164 | 0xFE00..=0xFE0F | 0xFEFF | 0xFFA0 | 0xFFF0..=0xFFFB | 0xE0000..=0xE0FFF)
}

/// Does a word of `text` mix letters of alphabets that have look-alikes
/// (Latin, Greek, Cyrillic, Armenian, Cherokee)? "pаypal" with a Cyrillic
/// "а" reads as "paypal". A warning, not a proof: some words mix
/// alphabets on purpose.
pub(super) fn mixed_scripts(text: &str) -> bool {
    let script = |c: char| match c as u32 {
        0x41..=0x5A | 0x61..=0x7A | 0xC0..=0x24F => 1,
        0x370..=0x3FF => 2,
        0x400..=0x52F => 3,
        0x530..=0x58F => 4,
        0x13A0..=0x13FF => 5,
        _ => 0,
    };
    text.split(|c: char| !c.is_alphanumeric()).any(|word| {
        let mut seen = 0u8;
        for c in word.chars() {
            let s = script(c);
            if s != 0 {
                seen |= 1 << s;
            }
        }
        seen.count_ones() > 1
    })
}

fn answer(ask: ToolAsk, text: impl Into<String>, error: bool) {
    let _ = ask.reply.send(ToolResult { id: ask.call.id.clone(), name: ask.call.name.clone(), text: text.into(), error });
}

impl Sanaga {
    /// The titles of the open tabs by their number (for the lines that say
    /// what a tool does).
    pub(super) fn tab_titles(&self) -> HashMap<u64, String> {
        self.tabs.iter().map(|t| (t.id, t.title())).collect()
    }

    /// A model asks for a tool: answered at once, or after the person has
    /// been asked (see the top of this file).
    pub(super) fn run_tool(&mut self, ask: ToolAsk) {
        let titles = self.tab_titles();
        let outside = self.mcp.names.get(&ask.call.name).cloned();
        let free = self.mcp_tool_free(&ask.call.name);
        let Some(a) = self.assistant.as_mut() else { return answer(ask, "Stopped by the person.", true) };
        let Some(tool) = a.tools.iter().find(|t| t.name == ask.call.name).cloned() else { return answer(ask, "There is no such tool.", true) };
        // an outside tool of the person's: shown with what is sent to it,
        // unless they let that tool run without asking
        if tool.name.starts_with("mcp__") {
            let Some((server, name)) = outside else { return answer(ask, "This outside tool is no longer there.", true) };
            if free {
                return self.call_mcp_tool(ask);
            }
            // (the person is shown all that is sent, or the call is refused)
            let Some(input) = outside_input(&ask.call.input) else {
                return answer(ask, "What this call would send is too long to be shown to the person: it was not made. Send less.", true);
            };
            let mut what = tf("{} › {} — envoyé :", &[&server, &name]);
            if mixed_scripts(&input) {
                what.push('\n');
                what.push_str(tr("Attention : des lettres de plusieurs alphabets se mêlent dans un même mot (des lettres qui se ressemblent peuvent tromper)."));
            }
            a.confirm = Some(Confirm::new(ask, what, false).with_detail(input));
            return;
        }
        let mut what = line(&ask.call, &titles);
        if tool.acts {
            // what it would do, checked before the person is asked
            if let Err(why) = self.check_action(&ask.call) {
                return answer(ask, why, true);
            }
            // (a form: each value next to its field's name)
            if ask.call.name == "fill_form" {
                match self.fill_summary(&ask.call) {
                    Ok(lines) => what = lines,
                    Err(why) => return answer(ask, why, true),
                }
            }
            if let Some(a) = self.assistant.as_mut() {
                a.confirm = Some(Confirm::new(ask, what, false));
            }
            return;
        }
        if a.outside && !a.allowed.contains(&tool.name) {
            a.confirm = Some(Confirm::new(ask, what, true));
            return;
        }
        self.do_tool(ask);
    }

    /// The person answered the question about a tool.
    pub(super) fn tool_answered(&mut self, allow: bool) {
        let Some(c) = self.assistant.as_mut().and_then(|a| a.confirm.take()) else { return };
        if !allow {
            return answer(c.ask, "The person refused. Do not ask again; go on without it.", true);
        }
        if c.once {
            if let Some(a) = self.assistant.as_mut() {
                a.allowed.insert(c.ask.call.name.clone());
            }
        }
        if c.ask.call.name.starts_with("mcp__") {
            return self.call_mcp_tool(c.ask);
        }
        self.do_tool(c.ask);
    }

    /// Is this action one that can be done at all? (Said to the model
    /// instead of asking the person about something impossible.)
    fn check_action(&self, call: &Call) -> Result<(), String> {
        let known = |ids: &[u64]| !ids.is_empty() && ids.iter().all(|id| self.tabs.iter().any(|t| t.id == *id));
        match call.name.as_str() {
            "open_tab" => openable(call.input["address"].as_str().unwrap_or("")).map(|_| ()).ok_or_else(|| "Only http and https addresses can be opened.".to_string()),
            "close_tabs" | "group_tabs" if !known(&tab_ids(call)) => Err("No such tab: use the numbers list_tabs gives.".into()),
            "group_tabs" if call.input["name"].as_str().unwrap_or("").trim().is_empty() => Err("The group needs a name.".into()),
            "run_command" if !super::buttons::commands().iter().any(|c| Some(c.0) == call.input["command"].as_str()) => Err("There is no such command.".into()),
            _ => Ok(()),
        }
    }

    /// Run a tool (the person has agreed where they had to).
    fn do_tool(&mut self, ask: ToolAsk) {
        let private = self.assistant.as_ref().is_some_and(|a| a.private);
        match ask.call.name.as_str() {
            "list_tabs" => {
                let shown = self.tabs[self.active].id;
                let mut out = String::new();
                for t in self.tabs.iter().filter(|t| (!t.private || private) && !t.companion) {
                    out.push_str(&format!("tab {} — {} — {}", t.id, t.title(), t.url()));
                    if t.group > 0 {
                        out.push_str(&format!(" — group: {}", self.groups.get(t.group).map(String::as_str).unwrap_or("")));
                    }
                    for (flag, on) in [("shown", t.id == shown), ("pinned", t.pinned), ("asleep", t.sleeping || (t.web.is_none() && matches!(t.view, View::Web)))] {
                        if on {
                            out.push_str(&format!(" [{flag}]"));
                        }
                    }
                    out.push('\n');
                }
                answer(ask, out, false);
            }
            "search_favourites" => {
                let words = crate::history::words_of(&words_of(&ask.call));
                let mut out = String::new();
                for (kind, list) in [("favourite", &self.store.bookmarks), ("reading list", &self.store.reading)] {
                    for b in list.iter().filter(|b| crate::history::matches(&words, &b.url, &b.title)).take(60) {
                        out.push_str(&format!("{kind} — {} — {}\n", b.title, b.url));
                    }
                }
                answer(ask, if out.is_empty() { "Nothing found.".to_string() } else { out }, false);
            }
            "search_history" => {
                let words = words_of(&ask.call);
                let from = ask.call.input["from"].as_str().and_then(|d| day_bound(d, false)).unwrap_or(0);
                let to = ask.call.input["to"].as_str().and_then(|d| day_bound(d, true)).unwrap_or(u64::MAX);
                let history = self.store.history.clone();
                // (the visits' times are said in local time, here: the
                // thread only searches)
                self.off_thread(
                    move || history.visited_between(&words, from, to, 40),
                    move |_, rows| {
                        let mut out = String::new();
                        for r in &rows {
                            out.push_str(&format!("{} — {} — {}\n", when(r.time), r.title, r.url));
                        }
                        answer(ask, if out.is_empty() { "No page found.".to_string() } else { out }, false);
                    },
                );
            }
            "search_conversations" => {
                let words = words_of(&ask.call);
                let Some(chats) = self.chats.clone() else { return answer(ask, "No conversation is kept.", false) };
                let current = self.assistant.as_ref().and_then(|a| a.id);
                self.off_thread(
                    move || chats.list_for_models(&words, 6).into_iter().filter(|k| Some(k.id) != current).map(|k| (k.updated, k.title.clone(), chats.said_in(k.id, 1_200))).collect::<Vec<_>>(),
                    move |_, found| {
                        let mut out = String::new();
                        for (updated, title, said) in &found {
                            out.push_str(&format!("Conversation of {} — {}\n{}\n", when(*updated), title, said));
                        }
                        answer(ask, if out.is_empty() { "No conversation found.".to_string() } else { out }, false);
                    },
                );
            }
            "read_form" => self.read_form_for_tool(ask),
            "fill_form" => self.fill_form_for_tool(ask),
            "read_tab" => {
                let id = ask.call.input["tab"].as_u64().unwrap_or(0);
                match self.tabs.iter().find(|t| t.id == id) {
                    None => answer(ask, "No such tab: use the numbers list_tabs gives.", true),
                    Some(t) if t.private && !private => answer(ask, "This tab is private: its page is not given.", true),
                    Some(t) if !crate::policy::get().ai_allowed_on(t.url()) => answer(ask, "The administrator of this computer does not let this site's pages be given.", true),
                    Some(_) => self.read_tab_for_tool(id, ask),
                }
            }
            "open_tab" => match openable(ask.call.input["address"].as_str().unwrap_or("")) {
                Some(address) => {
                    self.open_background(&address, false);
                    answer(ask, "Opened in a new tab, behind the one shown.", false);
                }
                None => answer(ask, "Only http and https addresses can be opened.", true),
            },
            "close_tabs" => {
                let (mut closed, mut kept) = (0, Vec::new());
                for id in tab_ids(&ask.call) {
                    let Some(i) = self.tabs.iter().position(|t| t.id == id) else { continue };
                    // (never a tab where the person typed something not sent yet)
                    if self.tabs[i].form_dirty {
                        kept.push(self.tabs[i].title());
                    } else if self.tabs.len() > 1 {
                        self.close_tab(i);
                        closed += 1;
                    }
                }
                let mut said = format!("{closed} tab(s) closed. They can be reopened with \"Reopen closed tab\".");
                if !kept.is_empty() {
                    said.push_str(&format!(" Left open because text typed in them was not sent: {}.", kept.join(", ")));
                }
                answer(ask, said, false);
            }
            "group_tabs" => {
                let ids = tab_ids(&ask.call);
                let name: String = ask.call.input["name"].as_str().unwrap_or("").trim().chars().take(40).collect();
                self.groups.push(name);
                self.sync_group_meta();
                let g = self.groups.len() - 1;
                let used: Vec<u8> = (1..g).filter_map(|k| self.group_colors.get(k).copied()).collect();
                if let Some(free) = (1..super::tabs::GROUP_COLORS.len() as u8).find(|c| !used.contains(c)) {
                    self.group_colors[g] = free;
                }
                let mut n = 0;
                for t in self.tabs.iter_mut().filter(|t| ids.contains(&t.id)) {
                    t.pinned = false;
                    t.group = g;
                    n += 1;
                }
                self.save_session();
                answer(ask, format!("{n} tab(s) grouped."), false);
            }
            "run_command" => {
                let name = ask.call.input["command"].as_str().unwrap_or("").to_string();
                self.run_shortcut(&name);
                answer(ask, "Done.", false);
            }
            _ => answer(ask, "There is no such tool.", true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, input: Value) -> Call {
        Call { id: "1".into(), name: name.into(), input }
    }

    #[test]
    fn tools_are_well_formed_and_only_some_act() {
        let tools = all();
        let acts: Vec<&str> = tools.iter().filter(|t| t.acts).map(|t| t.name.as_str()).collect();
        assert_eq!(acts, ["fill_form", "open_tab", "close_tabs", "group_tabs", "run_command"]);
        for t in &tools {
            assert_eq!(t.schema["type"], "object", "{}", t.name);
            assert!(t.schema["properties"].is_object(), "{}", t.name);
            assert!(t.name.chars().all(|c| c.is_ascii_lowercase() || c == '_') && t.description.len() > 40);
        }
        // the assistant's own commands are not commands it can run
        let commands = tools.iter().find(|t| t.name == "run_command").unwrap().schema["properties"]["command"]["enum"].as_array().unwrap().clone();
        assert!(commands.iter().any(|c| c == "reader") && !commands.iter().any(|c| c == "summary" || c == "ask"));
    }

    #[test]
    fn only_web_addresses_can_be_opened() {
        assert_eq!(openable(" https://example.org/a?b=1 ").as_deref(), Some("https://example.org/a?b=1"));
        assert!(openable("http://nas.lan:8080/").is_some());
        for bad in ["file:///etc/passwd", "sanaga://settings", "javascript:alert(1)", "data:text/html,x", "example.org", "", "https://", "about:blank"] {
            assert_eq!(openable(bad), None, "{bad}");
        }
    }

    #[test]
    fn days_typed_by_a_model_are_read() {
        assert_eq!(day_of("2026-11-17"), Some((2026, 11, 17)));
        assert_eq!(day_of(" 2026-01-05T10:00 "), Some((2026, 1, 5)));
        for bad in ["", "yesterday", "2026-13-01", "2026-11-32", "17/11/2026", "1800-01-01", "2026-11"] {
            assert_eq!(day_of(bad), None, "{bad}");
        }
        // a day's bounds are its first and last seconds, in local time
        let (start, end) = (day_bound("2026-11-17", false).unwrap(), day_bound("2026-11-17", true).unwrap());
        assert!(end > start && end - start >= 23 * 3600 - 1 && end - start <= 25 * 3600);
        assert!(when(start).starts_with("2026-11-17 00:00"));
        assert!(today().len() > 10);
    }

    #[test]
    fn an_outside_call_is_shown_whole_before_it_is_accepted() {
        // a page made the model pad the first argument: the part that
        // matters comes after 600 characters
        let input = json!({"a_note": "x".repeat(900), "z_command": "rm -rf ~"});
        let whole = outside_input(&input).unwrap();
        assert!(whole.contains("rm -rf ~") && whole.contains(&"x".repeat(900)), "the person reads all of it");
        // in the steps of the conversation it is cut, and said to be
        let said = outside_line("srv", "run", &input);
        assert!(!said.contains("rm -rf ~") && said.contains('…') && said.contains("336"), "{said}");
        let short = outside_line("srv", "run", &json!({"a": 1}));
        assert!(short.ends_with(r#"{"a":1}"#) && !short.contains('…'), "{short}");
        // what draws nothing, or turns the text around, is written out
        let hidden = outside_input(&json!({"a": "ok\u{202E}\u{200B}evil"})).unwrap();
        assert!(hidden.contains("\\u{202E}") && hidden.contains("\\u{200B}") && !hidden.contains('\u{202E}'), "{hidden}");
        // C1 controls, shorthand format controls, musical formatting
        let more = outside_input(&json!({"a": "x\u{85}\u{9B}\u{1BCA1}\u{1D175}y"})).unwrap();
        for hidden in ["\\u{0085}", "\\u{009B}", "\\u{1BCA1}", "\\u{1D175}"] {
            assert!(more.contains(hidden), "{hidden} in {more}");
        }
        // look-alike letters of another alphabet in a word are pointed out
        assert!(mixed_scripts("send to p\u{430}ypal.com"), "a Cyrillic а in a Latin word");
        assert!(mixed_scripts("\u{3BF}k"));
        assert!(!mixed_scripts("paypal.com — Москва — Ελλάδα — café"), "each word in one alphabet");
        assert!(!mixed_scripts("rm -rf ~ 123"));
        // too long to be read: refused, never shown in part
        assert_eq!(outside_input(&json!({"a": "y".repeat(OUTSIDE_MAX)})), None);
        assert!(outside_input(&json!({"a": "y".repeat(OUTSIDE_MAX - 20)})).is_some());
    }

    #[test]
    fn a_call_is_said_in_words() {
        let titles = HashMap::from([(7, "Wikipédia".to_string())]);
        // (the words around depend on the interface's language: what is
        // checked is what the call itself holds)
        let said = line(&call("search_history", json!({"words": "douala", "from": "2026-11-10"})), &titles);
        assert!(said.contains("« douala »") && said.ends_with("(2026-11-10 → …)"), "{said}");
        assert!(line(&call("read_tab", json!({"tab": 7})), &titles).contains("« Wikipédia »"));
        assert!(line(&call("close_tabs", json!({"tabs": [7, 9]})), &titles).ends_with("Wikipédia, n° 9"));
        assert!(line(&call("open_tab", json!({"address": "https://example.org/"})), &titles).ends_with("https://example.org/"));
        assert!(!line(&call("run_command", json!({"command": "reader"})), &titles).contains("reader"), "a command is said by its name in the menus");
        // whatever a model sends, it is said without a panic
        crate::mutate::run(&[br#"{"tabs":[1,"x",null],"words":7,"name":{"a":1},"command":[],"address":3}"#], 5_000, |d| {
            if let Ok(v) = serde_json::from_slice::<Value>(d) {
                for name in ["search_history", "read_tab", "close_tabs", "group_tabs", "run_command", "open_tab", "x"] {
                    let _ = line(&call(name, v.clone()), &titles);
                }
            }
        });
    }
}
