//! Page summaries and extensions written from a description.
//!
//! Three ways, chosen in Settings › Intelligence:
//! - Sanaga on its own (no AI, nothing leaves the computer): the summary keeps
//!   the page's most telling sentences. Writing extensions needs a model.
//! - A model on this computer through Ollama (nothing leaves the computer).
//! - Claude (Anthropic), with the person's own API key, kept in the keyring.
//!
//! All calls are blocking: run them on a thread.

use std::time::Duration;

pub const CLAUDE_MODEL: &str = "claude-sonnet-5-5";
const OLLAMA: &str = "http://127.0.0.1:11434";
/// Where the Claude key is kept in the keyring (profile, site, user).
pub const KEY_SITE: &str = "https://api.anthropic.com";

#[derive(Clone, Debug, PartialEq)]
pub enum Provider {
    /// No model: Sanaga's own summary.
    Local,
    Ollama { model: String },
    /// (its key is read from the keyring of this profile when a request is made)
    Claude { model: String, profile: String },
    /// An assistant installed on the computer (Claude Code, Codex…).
    Cli { tool: crate::ai_cli::CliTool, model: String },
}

impl Provider {
    pub fn label(&self) -> String {
        match self {
            Provider::Local => crate::i18n::tr("Sanaga (sans IA, sur cet ordinateur)").into(),
            Provider::Ollama { model } => format!("Ollama · {model}"),
            Provider::Claude { model, .. } => format!("Claude · {model}"),
            Provider::Cli { tool, model } if model.is_empty() => tool.name().to_string(),
            Provider::Cli { tool, model } => format!("{} · {model}", tool.name()),
        }
    }
}

/// May this provider receive the text of web pages (summary, translation)?
/// A page's text can try to give the model orders, so only models with no
/// tools and no access to the person's files may: Claude (API), Ollama, and
/// the assistants that can be started tool-less.
pub fn page_content_allowed(provider: &Provider) -> Result<(), String> {
    match provider {
        Provider::Cli { tool, .. } if !tool.tool_less() => Err(crate::i18n::tf(
            "{} peut lire vos fichiers : Sanaga ne lui envoie pas le texte des pages. Choisissez Claude Code, Claude ou Ollama dans Paramètres › Intelligence.",
            &[&tool.name()],
        )),
        _ => Ok(()),
    }
}

fn agent(read_secs: u64) -> ureq::Agent {
    // (the system's certificate authorities: see net::builder)
    crate::net::builder().timeout_connect(Duration::from_secs(5)).timeout_read(Duration::from_secs(read_secs)).build()
}

/// The models installed in Ollama (empty when Ollama is not running).
pub fn ollama_models() -> Vec<String> {
    let Ok(r) = agent(3).get(&format!("{OLLAMA}/api/tags")).call() else { return Vec::new() };
    let body = r.into_string().unwrap_or_default();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else { return Vec::new() };
    v["models"].as_array().map(|m| m.iter().filter_map(|x| x["name"].as_str()).map(|n| n.to_string()).collect()).unwrap_or_default()
}

/// Ask the model; returns its answer as text.
pub fn ask(provider: &Provider, system: &str, prompt: &str, max_tokens: u32) -> Result<String, String> {
    match provider {
        Provider::Local => Err(crate::i18n::tr("Choisissez un modèle d'IA dans Paramètres › Intelligence.").into()),
        Provider::Cli { tool, model } => crate::ai_cli::ask(*tool, model, system, prompt),
        Provider::Ollama { model } => {
            let body = format!("{{\"model\":{},\"system\":{},\"prompt\":{},\"stream\":false}}", json_str(model), json_str(system), json_str(prompt));
            let r = agent(240).post(&format!("{OLLAMA}/api/generate")).set("content-type", "application/json").send_string(&body).map_err(err_text)?;
            let text = r.into_string().map_err(|e| e.to_string())?;
            ollama_answer(&text).ok_or_else(|| crate::i18n::tr("Réponse du modèle illisible.").into())
        }
        Provider::Claude { model, profile } => {
            let key = crate::keyring::lookup(profile, KEY_SITE, "claude").filter(|k| !k.is_empty()).ok_or_else(|| crate::i18n::tr("Clé d'API refusée : vérifiez-la dans Paramètres › Intelligence.").to_string())?;
            let body = format!(
                "{{\"model\":{},\"max_tokens\":{max_tokens},\"system\":{},\"messages\":[{{\"role\":\"user\",\"content\":{}}}]}}",
                json_str(model),
                json_str(system),
                json_str(prompt)
            );
            let r = agent(180)
                .post("https://api.anthropic.com/v1/messages")
                .set("content-type", "application/json")
                .set("x-api-key", &key)
                .set("anthropic-version", "2023-06-01")
                .send_string(&body)
                .map_err(err_text)?;
            let text = r.into_string().map_err(|e| e.to_string())?;
            claude_answer(&text).ok_or_else(|| crate::i18n::tr("Réponse du modèle illisible.").into())
        }
    }
}

/// What `ask_live` answers when the person stopped it (never shown).
pub const STOPPED: &str = "\u{1}stopped";

/// Ask the model and hand its answer over as it comes: `piece` gets each
/// part as soon as the model has written it (a summary appears word by
/// word instead of after a long wait); the whole answer is returned at the
/// end. When `stop` becomes true the request is dropped and the answer is
/// `Err(STOPPED)`. (An installed assistant gives its answer in one piece.)
pub fn ask_live(provider: &Provider, system: &str, prompt: &str, max_tokens: u32, stop: &std::sync::atomic::AtomicBool, piece: &mut dyn FnMut(&str)) -> Result<String, String> {
    use std::io::BufRead;
    let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
    let mut whole = String::new();
    let lines = match provider {
        Provider::Local => return Err(crate::i18n::tr("Choisissez un modèle d'IA dans Paramètres › Intelligence.").into()),
        Provider::Cli { tool, model } => {
            let answer = crate::ai_cli::ask_stoppable(*tool, model, system, prompt, Some(stop))?;
            piece(&answer);
            return Ok(answer);
        }
        Provider::Ollama { model } => {
            let body = format!("{{\"model\":{},\"system\":{},\"prompt\":{},\"stream\":true}}", json_str(model), json_str(system), json_str(prompt));
            let r = agent(240).post(&format!("{OLLAMA}/api/generate")).set("content-type", "application/json").send_string(&body).map_err(err_text)?;
            std::io::BufReader::new(r.into_reader()).lines()
        }
        Provider::Claude { model, profile } => {
            let key = crate::keyring::lookup(profile, KEY_SITE, "claude").filter(|k| !k.is_empty()).ok_or_else(|| crate::i18n::tr("Clé d'API refusée : vérifiez-la dans Paramètres › Intelligence.").to_string())?;
            let body = format!(
                "{{\"model\":{},\"max_tokens\":{max_tokens},\"stream\":true,\"system\":{},\"messages\":[{{\"role\":\"user\",\"content\":{}}}]}}",
                json_str(model),
                json_str(system),
                json_str(prompt)
            );
            let r = agent(180)
                .post("https://api.anthropic.com/v1/messages")
                .set("content-type", "application/json")
                .set("x-api-key", &key)
                .set("anthropic-version", "2023-06-01")
                .send_string(&body)
                .map_err(err_text)?;
            std::io::BufReader::new(r.into_reader()).lines()
        }
    };
    for line in lines {
        if stopped() {
            // (the connection is closed as the reader goes: the model stops writing)
            return Err(STOPPED.to_string());
        }
        let line = line.map_err(|e| e.to_string())?;
        match live_piece(&line)? {
            Live::Text(t) => {
                piece(&t);
                whole.push_str(&t);
            }
            Live::End => break,
            Live::Nothing => {}
        }
    }
    if stopped() {
        return Err(STOPPED.to_string());
    }
    if whole.trim().is_empty() {
        return Err(crate::i18n::tr("Réponse du modèle illisible.").into());
    }
    Ok(whole)
}

/// What one line of a model's live answer holds.
#[derive(Debug, PartialEq)]
enum Live {
    Text(String),
    End,
    Nothing,
}

/// One line of a live answer: Ollama writes a JSON object per line
/// (`{"response":"…","done":false}`), Claude writes events (`data: {…}`,
/// the text in `content_block_delta` ones). An error the service reports
/// in the stream ends it.
fn live_piece(line: &str) -> Result<Live, String> {
    let line = line.trim();
    let json = line.strip_prefix("data:").map(str::trim).unwrap_or(line);
    if !json.starts_with('{') {
        return Ok(Live::Nothing); // (an event's name, a blank line, a comment)
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return Ok(Live::Nothing) };
    if v["type"] == "error" || (v["error"].is_string() || v["error"].is_object()) {
        let said = error_message(json);
        return Err(if said.is_empty() { crate::i18n::tr("Réponse du modèle illisible.").to_string() } else { said });
    }
    // Claude
    if v["type"] == "content_block_delta" {
        return Ok(match v["delta"]["text"].as_str() {
            Some(t) if !t.is_empty() => Live::Text(t.to_string()),
            _ => Live::Nothing,
        });
    }
    if v["type"] == "message_stop" {
        return Ok(Live::End);
    }
    // Ollama
    if let Some(t) = v["response"].as_str() {
        if !t.is_empty() {
            return Ok(Live::Text(t.to_string()));
        }
    }
    Ok(if v["done"] == true { Live::End } else { Live::Nothing })
}

/// Ollama's answer: the "response" text of its JSON.
fn ollama_answer(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v["response"].as_str().map(|s| s.to_string())
}

/// Claude's answer: the text blocks of its message, in order (other kinds
/// of blocks, and a "text" met elsewhere in the JSON, are not the answer).
fn claude_answer(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let text: String = v["content"].as_array()?.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("");
    (!text.is_empty()).then_some(text)
}

/// What went wrong, as the service says it: {"error": {"message": …}},
/// {"error": "…"} or {"message": …}.
fn error_message(body: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else { return String::new() };
    let said = v["error"]["message"].as_str().or_else(|| v["error"].as_str()).or_else(|| v["message"].as_str());
    said.unwrap_or_default().to_string()
}

fn err_text(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, r) => {
            let body = r.into_string().unwrap_or_default();
            let msg = error_message(&body);
            if code == 401 {
                crate::i18n::tr("Clé d'API refusée : vérifiez-la dans Paramètres › Intelligence.").into()
            } else {
                format!("{code} {msg}")
            }
        }
        ureq::Error::Transport(t) => crate::i18n::tf("Modèle injoignable ({})", &[&t.to_string()]),
    }
}

// ---------------------------------------------------------------- summaries

/// Instructions for a summary in the person's language.
pub fn summary_prompt(title: &str, text: &str) -> (String, String) {
    let lang = match crate::i18n::language() {
        "fr" => "French",
        "es" => "Spanish",
        "pt" => "Portuguese",
        "de" => "German",
        _ => "English",
    };
    let system = format!(
        "You summarise web pages for a browser's reader. Write in {lang}. Start with one sentence that says what the page is about, \
         then 3 to 6 short bullet points with the key facts (use \"• \" for bullets). No preamble, no markdown headings."
    );
    let cut: String = text.chars().take(24_000).collect();
    (system, format!("Title: {title}\n\nPage text:\n{cut}"))
}

/// Sanaga's own summary, without a model: the sentences that carry the
/// page's most frequent meaningful words, in the order of the page.
pub fn local_summary(text: &str, sentences: usize) -> String {
    let all = split_sentences(text);
    if all.is_empty() {
        return String::new();
    }
    let mut freq: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    for s in &all {
        for w in words(s) {
            *freq.entry(w).or_default() += 1.0;
        }
    }
    let mut scored: Vec<(usize, f32)> = all
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let ws = words(s);
            let n = ws.len().max(1) as f32;
            let mut score: f32 = ws.iter().map(|w| freq.get(w).copied().unwrap_or(0.0)).sum::<f32>() / n.sqrt();
            // the opening of a text usually says what it is about
            if i < 3 {
                score *= 1.4;
            }
            // references, links and captions are not what the page says
            if s.contains("http") || s.contains("www.") || s.contains('[') || s.contains('»') {
                score *= 0.2;
            }
            if !s.ends_with(['.', '!', '?']) {
                score *= 0.5;
            }
            (i, score)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    // the first real sentence opens the summary, then the best ones
    let first = (0..all.len()).find(|&i| all[i].ends_with(['.', '!', '?']) && !all[i].contains("http")).unwrap_or(0);
    let mut keep: Vec<usize> = vec![first];
    keep.extend(scored.iter().map(|x| x.0).filter(|&i| i != first).take(sentences.saturating_sub(1)));
    keep[1..].sort();
    let mut out = String::new();
    for (k, i) in keep.iter().enumerate() {
        if k == 0 {
            out.push_str(&all[*i]);
            out.push_str("\n\n");
        } else {
            out.push_str("• ");
            out.push_str(&all[*i]);
            out.push('\n');
        }
    }
    out.trim_end().to_string()
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let para = para.trim();
        // menus, buttons and captions are short: not sentences
        if para.split_whitespace().count() < 6 {
            continue;
        }
        let mut cur = String::new();
        let chars: Vec<char> = para.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            cur.push(c);
            let end = matches!(c, '.' | '!' | '?' | '。') && chars.get(i + 1).is_none_or(|n| n.is_whitespace());
            if end {
                push_sentence(&mut out, &cur);
                cur.clear();
            }
        }
        push_sentence(&mut out, &cur);
    }
    out
}

fn push_sentence(out: &mut Vec<String>, s: &str) {
    let s = s.trim();
    let n = s.split_whitespace().count();
    if (6..=60).contains(&n) && !out.iter().any(|x| x == s) {
        out.push(s.to_string());
    }
}

fn words(s: &str) -> Vec<String> {
    const STOP: [&str; 60] = [
        "this", "that", "with", "from", "have", "were", "which", "their", "there", "about", "would", "these", "other", "into", "more", "also", "been", "than", "they", "will",
        "dans", "pour", "avec", "sont", "être", "elle", "leur", "plus", "mais", "comme", "cette", "nous", "vous", "était", "aussi", "tout", "fait", "dont", "entre", "sans",
        "para", "como", "pero", "esta", "este", "sobre", "entre", "desde", "eine", "einer", "nicht", "sich", "auch", "wird", "oder", "über", "werden", "dass", "sind", "mais",
    ];
    s.split(|c: char| !c.is_alphanumeric())
        .map(|w| w.to_lowercase())
        .filter(|w| w.chars().count() >= 4 && !STOP.contains(&w.as_str()) && !w.chars().all(|c| c.is_numeric()))
        .collect()
}

// ---------------------------------------------------------------- extensions

/// A user extension written by the model from a description: returns the
/// file name and the file's text (with Sanaga's extension header).
pub fn write_extension(provider: &Provider, name: &str, description: &str, sites: &str, stop: &std::sync::atomic::AtomicBool, piece: &mut dyn FnMut(&str)) -> Result<(String, String), String> {
    let system = "You write small, safe browser user scripts (like Greasemonkey scripts) for the Sanaga browser. \
        The script runs at document start in the page's top frame. Wait for DOMContentLoaded when you need the page, \
        and use a MutationObserver when content can appear later. To add CSS, create a <style> element. \
        Never send data to other servers, never read passwords or cookies, never use eval. \
        Reply with JavaScript code only: no explanations, no markdown fences.";
    let prompt = format!("What the extension must do:\n{description}\n\nSites: {}", if sites.trim().is_empty() { "all sites" } else { sites.trim() });
    let code = ask_live(provider, system, &prompt, 4096, stop, piece)?;
    let code = strip_fences(&code);
    if code.trim().is_empty() {
        return Err(crate::i18n::tr("Le modèle n'a pas écrit de code.").into());
    }
    let mut header = format!("// @name        {}\n// @description {}\n", one_line(name), one_line(description));
    for m in sites.split([',', ' ', '\n']).map(str::trim).filter(|s| !s.is_empty()) {
        if m.contains("://") {
            header.push_str(&format!("// @match       {m}\n"));
        } else {
            // "youtube.com": the site and its sub-domains (www., m.…)
            let d = m.trim_start_matches("*.").trim_start_matches("www.").trim_end_matches('/');
            header.push_str(&format!("// @match       *://{d}/*\n// @match       *://*.{d}/*\n"));
        }
    }
    header.push_str("// (written by AI in Sanaga — check it before use)\n\n");
    let file = format!("{}.js", slug(name));
    Ok((file, header + &code))
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect()
}

fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "mon-extension".into() } else { s.chars().take(48).collect() }
}

fn strip_fences(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        let rest = rest.split_once('\n').map(|x| x.1).unwrap_or("");
        return rest.trim_end().trim_end_matches("```").trim_end().to_string();
    }
    t.to_string()
}

// ---------------------------------------------------------------- tiny JSON

/// A JSON string literal.
pub fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// The first string value of `"key"` in a JSON text.
pub fn json_string(body: &str, key: &str) -> Option<String> {
    json_strings(body, key).into_iter().next()
}

/// Every string value of `"key"` in a JSON text, in order.
fn json_strings(body: &str, key: &str) -> Vec<String> {
    let pat = format!("\"{key}\"");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = body[from..].find(&pat) {
        let mut rest = body[from + i + pat.len()..].trim_start();
        from += i + pat.len();
        let Some(r) = rest.strip_prefix(':') else { continue };
        rest = r.trim_start();
        let Some(r) = rest.strip_prefix('"') else { continue };
        if let Some(s) = read_json_string(r) {
            out.push(s);
        }
    }
    out
}

/// A JSON array of strings.
pub fn json_array(list: &[String]) -> String {
    format!("[{}]", list.iter().map(|s| json_str(s)).collect::<Vec<_>>().join(","))
}

/// A JSON string's text (after its opening quote) and how many bytes it used
/// up to and with its closing quote.
pub fn read_json_string_end(s: &str) -> Option<(String, usize)> {
    let mut depth_escape = false;
    for (i, c) in s.char_indices() {
        if depth_escape {
            depth_escape = false;
            continue;
        }
        if c == '\\' {
            depth_escape = true;
        } else if c == '"' {
            return read_json_string(&s[..=i]).map(|t| (t, i + 1));
        }
    }
    None
}

fn read_json_string(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        match c {
            '"' => return Some(out),
            '\\' => match it.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' | 'f' => {}
                'u' => {
                    let hex: String = (0..4).filter_map(|_| it.next()).collect();
                    let mut cp = u32::from_str_radix(&hex, 16).ok()?;
                    if (0xD800..0xDC00).contains(&cp) {
                        // a pair of UTF-16 halves
                        let mut look = it.clone();
                        if look.next() == Some('\\') && look.next() == Some('u') {
                            let lo: String = (0..4).filter_map(|_| look.next()).collect();
                            if let Ok(lo) = u32::from_str_radix(&lo, 16) {
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo.wrapping_sub(0xDC00) & 0x3FF);
                                it = look;
                            }
                        }
                    }
                    out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                }
                o => out.push(o),
            },
            c => out.push(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_model_answers() {
        let claude = r#"{"id":"m","content":[{"type":"text","text":"Bonjour \"Douala\"\n• ok é 😀"}],"stop_reason":"end_turn"}"#;
        assert_eq!(json_string(claude, "text").unwrap(), "Bonjour \"Douala\"\n• ok é 😀");
        let tags = r#"{"models":[{"name":"llama3.2:3b","x":1},{"name":"qwen3:8b"}]}"#;
        assert_eq!(json_strings(tags, "name"), vec!["llama3.2:3b", "qwen3:8b"]);
        assert_eq!(json_str("a\"b\n"), "\"a\\\"b\\n\"");
    }

    #[test]
    fn summary_without_ai_keeps_telling_sentences() {
        let text = "Menu\nHome\nDouala is the largest city in Cameroon and its economic capital on the Wouri river.\n\
                    The port of Douala handles most of the imports and exports of Cameroon and neighbouring countries.\n\
                    The weather was nice yesterday and people walked around the market for a while.\n\
                    Douala international airport connects the city of Douala to many African and European cities.";
        let s = local_summary(text, 3);
        assert!(s.starts_with("Douala is the largest city"));
        assert!(s.contains("• "));
        assert!(!s.contains("Menu"));
    }

    #[test]
    fn page_text_goes_only_to_tool_less_models() {
        use crate::ai_cli::CliTool;
        assert!(page_content_allowed(&Provider::Cli { tool: CliTool::ClaudeCode, model: String::new() }).is_ok());
        assert!(page_content_allowed(&Provider::Cli { tool: CliTool::Codex, model: String::new() }).is_err());
        assert!(page_content_allowed(&Provider::Cli { tool: CliTool::Gemini, model: String::new() }).is_err());
        assert!(page_content_allowed(&Provider::Ollama { model: "llama3.2".into() }).is_ok());
        assert!(page_content_allowed(&Provider::Local).is_ok());
    }

    #[test]
    fn extension_files_get_a_header() {
        assert_eq!(slug("Masquer les Shorts !"), "masquer-les-shorts");
        assert_eq!(strip_fences("```js\nalert(1)\n```"), "alert(1)");
    }
}

#[cfg(test)]
mod answer_tests {
    use super::*;

    #[test]
    fn live_answers_are_read_line_by_line() {
        // Ollama: one JSON object per line
        assert_eq!(live_piece(r#"{"model":"m","response":"Bon","done":false}"#), Ok(Live::Text("Bon".into())));
        assert_eq!(live_piece(r#"{"model":"m","response":"","done":true,"total_duration":1}"#), Ok(Live::End));
        assert!(live_piece(r#"{"error":"model 'x' not found"}"#).is_err());
        // Claude: events
        assert_eq!(live_piece("event: content_block_delta"), Ok(Live::Nothing));
        assert_eq!(live_piece(r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"jour"}}"#), Ok(Live::Text("jour".into())));
        assert_eq!(live_piece(r#"data: {"type":"message_start","message":{"id":"m","content":[]}}"#), Ok(Live::Nothing));
        assert_eq!(live_piece(r#"data: {"type":"ping"}"#), Ok(Live::Nothing));
        assert_eq!(live_piece(r#"data: {"type":"message_stop"}"#), Ok(Live::End));
        assert_eq!(live_piece(r#"data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#), Err("Overloaded".to_string()));
        assert_eq!(live_piece(""), Ok(Live::Nothing));
        // damaged lines never panic
        crate::mutate::run(&[br#"data: {"type":"content_block_delta","delta":{"text":"a"}}"#, br#"{"response":"x","done":false}"#], 5_000, |d| {
            let _ = live_piece(&String::from_utf8_lossy(d));
        });
    }

    /// A model's or a service's answer is read whatever it holds (see mutate.rs).
    #[test]
    fn damaged_answers_do_not_panic() {
        let seeds: [&[u8]; 4] = [
            br#"{"id":"m","content":[{"type":"text","text":"Bonjour \"Douala\"\n ok \u00e9 \ud83d\ude00"}],"stop_reason":"end_turn"}"#,
            br#"{"model":"x","response":"R\u00e9sum\u00e9","done":true,"error":{"message":"m"},"models":[{"name":"a"}]}"#,
            "```js\n// @name x\nalert(1)\n```\nPremière phrase. Deuxième phrase ! Une troisième ? Et « une autre »… fin".as_bytes(),
            "中文句子。第二句！Third sentence. Ça c'est M. Dupont, p. ex. 3.14 etc.".as_bytes(),
        ];
        crate::mutate::run(&seeds, 20_000, |d| {
            let t = String::from_utf8_lossy(d);
            let _ = claude_answer(&t);
            let _ = ollama_answer(&t);
            let _ = error_message(&t);
            let _ = json_string(&t, "text");
            let _ = json_strings(&t, "name");
            let _ = read_json_string_end(&t);
            let _ = strip_fences(&t);
            let _ = slug(&t);
            let _ = one_line(&t);
            for n in [0, 1, 6] {
                let _ = local_summary(&t, n);
            }
            let _ = summary_prompt(&t, &t);
        });
    }

    #[test]
    fn answers_are_read_from_their_place_in_the_json() {
        // a "text" met before the answer (a thinking block, a key's value) is not the answer
        let body = r#"{"id":"m","type":"message","content":[{"type":"thinking","thinking":"text"},{"type":"text","text":"Bonjour \"vous\""},{"type":"text","text":" !"}],"stop_reason":"end_turn"}"#;
        assert_eq!(claude_answer(body).as_deref(), Some("Bonjour \"vous\" !"));
        assert_eq!(claude_answer(r#"{"content":[]}"#), None);
        assert_eq!(claude_answer("not json"), None);
        assert_eq!(ollama_answer(r#"{"model":"response","response":"ok\n","done":true}"#).as_deref(), Some("ok\n"));
        assert_eq!(error_message(r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad key"}}"#), "bad key");
        assert_eq!(error_message(r#"{"error":"model not found"}"#), "model not found");
        assert_eq!(error_message("<html>"), "");
    }
}
