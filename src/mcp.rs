//! Outside tools for the assistant, through the Model Context Protocol
//! (MCP): the person adds the servers they want — a program on this
//! computer, or an address — and the tools those servers offer can be used
//! in a conversation.
//!
//! They are the person's own tools, added by them and by nothing else: no
//! page and no model can add a server. And they reach outside the browser,
//! so every call is shown to the person with what is sent, unless they
//! chose to let a given tool run without asking (`app/mcppage.rs`,
//! `app/aitools.rs`).
//!
//! What is spoken here is the protocol's base: `initialize`, `tools/list`,
//! `tools/call`, over a program's standard input and output (one JSON
//! message per line) or over HTTP (one POST per message, the answer as
//! JSON or as a stream of events). A server at an address is given the
//! access token the person typed, if any (kept in the system keyring); the
//! sign-in windows some services need (OAuth) are not done here.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Where a server's access token is kept in the keyring (this, then its name).
pub const KEY_PREFIX: &str = "sanaga-mcp:";
/// The protocol's version Sanaga speaks.
const PROTOCOL: &str = "2025-06-18";
/// How long a server may take to answer.
const PATIENCE: Duration = Duration::from_secs(60);
/// The most tools taken from one server, and the longest answer kept.
const MAX_TOOLS: usize = 60;
const MAX_ANSWER: usize = 60_000;
/// The longest message (one line) taken from a server. A longer one ends
/// the call: it is not read whole into memory first.
const MAX_LINE: usize = 4 << 20;

/// A server the person added.
#[derive(Clone, Debug, PartialEq)]
pub struct Server {
    pub name: String,
    /// A program on this computer (its command line), or an address.
    pub program: bool,
    pub target: String,
    pub on: bool,
    /// Its tools the person lets run without being asked each time.
    pub free: Vec<String>,
}

/// A tool a server offers.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteTool {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

// ---------------------------------------------------------------- the list kept

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ").trim().to_string()
}

/// A server's name as the person typed it, kept short and plain.
pub fn server_name(typed: &str) -> String {
    clean(typed).chars().take(30).collect()
}

/// The servers kept in the profile's `mcp.tsv`: one per line — name, kind
/// ("program" or "address"), command line or address, "1" when on, the
/// tools let free (separated by commas).
pub fn parse_list(text: &str) -> Vec<Server> {
    let mut out: Vec<Server> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
        let mut p = line.split('\t');
        let (name, kind, target) = (server_name(p.next().unwrap_or("")), p.next().unwrap_or(""), p.next().unwrap_or("").trim().to_string());
        if name.is_empty() || target.is_empty() || out.iter().any(|s| s.name == name) {
            continue;
        }
        let on = p.next() == Some("1");
        let free = p.next().unwrap_or("").split(',').map(str::trim).filter(|t| !t.is_empty()).map(str::to_string).collect();
        out.push(Server { name, program: kind == "program", target, on, free });
    }
    out
}

pub fn format_list(list: &[Server]) -> String {
    let mut out = String::from("# Sanaga — the assistant's outside tools (MCP servers)\n");
    for s in list {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            clean(&s.name),
            if s.program { "program" } else { "address" },
            clean(&s.target),
            if s.on { "1" } else { "0" },
            s.free.iter().map(|t| clean(t)).collect::<Vec<_>>().join(",")
        ));
    }
    out
}

/// Is this an address a server can be reached at? Over https, or on this
/// computer: a token is never sent in the clear to another machine.
pub fn address_ok(target: &str) -> bool {
    let Ok(u) = url::Url::parse(target.trim()) else { return false };
    match (u.scheme(), u.host_str()) {
        ("https", Some(_)) => true,
        ("http", Some(h)) => is_local(h),
        _ => false,
    }
}

fn is_local(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// The name a server's tool has for the model: "mcp__server__tool", with
/// only the characters models accept in a tool's name, 64 at most.
pub fn tool_name(server: &str, tool: &str) -> String {
    let plain = |s: &str| -> String { s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect() };
    let name = format!("mcp__{}__{}", plain(server), plain(tool));
    name.chars().take(64).collect()
}

/// A command line as its words: spaces separate, quotes keep together.
pub fn split_command(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut quote, mut any) = (Vec::new(), String::new(), None::<char>, false);
    for c in line.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                any = true;
            }
            None if c.is_whitespace() => {
                if !cur.is_empty() || any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            None => cur.push(c),
        }
    }
    if !cur.is_empty() || any {
        out.push(cur);
    }
    out
}

/// One line of what a server sends, without its line break; `Err` when it
/// is longer than `most` bytes (then only `most` and a byte were read).
/// None: the end.
fn read_line(from: &mut impl BufRead, most: usize) -> std::io::Result<Option<Result<String, ()>>> {
    let mut buf = Vec::new();
    let n = from.take(most as u64 + 1).read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
    } else if buf.len() > most {
        return Ok(Some(Err(())));
    }
    Ok(Some(Ok(String::from_utf8_lossy(&buf).into_owned())))
}

/// The variables of Sanaga's own environment a server program is given:
/// what a program needs to start and to speak the person's language. The
/// rest — keys and tokens of other programs, Sanaga's sessions — is not
/// its business. (`PATH` is given apart: see `Session::open`.)
fn env_kept(name: &str) -> bool {
    matches!(name, "HOME" | "LANG" | "LANGUAGE" | "XDG_RUNTIME_DIR") || name.starts_with("LC_")
}

/// The part of an environment a server program is given (`env_kept`).
fn kept_env(vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    vars.filter(|(k, _)| k.to_str().is_some_and(env_kept)).collect()
}

/// The variables the person gave a server, as in a shell: `NAME=value`
/// words before the program (`GITHUB_TOKEN=… npx -y server`). Gives them,
/// and the words of the command itself.
fn server_env(words: &[String]) -> (Vec<(String, String)>, &[String]) {
    let is_name = |n: &str| n.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let mut vars = Vec::new();
    let mut k = 0;
    while let Some((name, value)) = words.get(k).and_then(|w| w.split_once('=')).filter(|(n, _)| is_name(n)) {
        vars.push((name.to_string(), value.to_string()));
        k += 1;
    }
    (vars, &words[k..])
}

// ---------------------------------------------------------------- messages

fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

/// What a line holds for the request `id`: its result, its error, or
/// nothing (another message: a notification, a request of the server's).
fn answer_to(line: &str, id: u64) -> Option<Result<Value, String>> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if v["id"].as_u64() != Some(id) || v.get("method").is_some() {
        return None;
    }
    if let Some(e) = v.get("error").filter(|e| !e.is_null()) {
        return Some(Err(e["message"].as_str().unwrap_or("error").to_string()));
    }
    Some(Ok(v["result"].clone()))
}

/// A request the server makes of Sanaga (it offers the server nothing):
/// the refusal to send back, if this line is one.
fn refusal_of(line: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    let id = v.get("id").filter(|i| !i.is_null())?;
    v.get("method")?;
    Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}}))
}

/// The tools of a `tools/list` answer.
fn tools_of(result: &Value) -> Vec<RemoteTool> {
    let list = result["tools"].as_array().cloned().unwrap_or_default();
    list.iter()
        .filter_map(|t| {
            let name = t["name"].as_str().filter(|n| !n.is_empty() && n.len() <= 128)?.to_string();
            let schema = if t["inputSchema"]["type"] == "object" { t["inputSchema"].clone() } else { json!({"type": "object", "properties": {}}) };
            Some(RemoteTool { name, description: t["description"].as_str().unwrap_or("").chars().take(1_000).collect(), schema })
        })
        .take(MAX_TOOLS)
        .collect()
}

/// What a `tools/call` answer says: its text, and whether the tool failed.
fn called(result: &Value) -> (String, bool) {
    let mut text = String::new();
    for part in result["content"].as_array().cloned().unwrap_or_default() {
        let piece = match part["type"].as_str() {
            Some("text") => part["text"].as_str().unwrap_or("").to_string(),
            Some("resource") => part["resource"]["text"].as_str().unwrap_or("").to_string(),
            Some(other) => format!("[{other}]"),
            None => String::new(),
        };
        if !piece.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&piece);
        }
    }
    if text.is_empty() {
        if let Some(s) = result.get("structuredContent").filter(|s| !s.is_null()) {
            text = s.to_string();
        }
    }
    if text.chars().count() > MAX_ANSWER {
        text = text.chars().take(MAX_ANSWER).collect::<String>() + "\n[…cut: the answer was too long]";
    }
    (text, result["isError"] == true)
}

// ---------------------------------------------------------------- sessions

enum Link {
    Program {
        child: std::process::Child,
        stdin: std::process::ChildStdin,
        /// Its lines; `Err`: one was longer than `MAX_LINE` (nothing more
        /// is read).
        lines: Receiver<Result<String, ()>>,
        /// The empty private folder it runs in (removed with the session).
        _dir: crate::store::PrivateDir,
    },
    Address {
        url: String,
        token: Option<String>,
        /// The session the server opened, if it names one.
        session: Option<String>,
        ready: bool,
    },
}

/// A conversation with one server.
pub struct Session {
    link: Link,
    next: u64,
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Link::Program { child, .. } = &mut self.link {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Session {
    /// Reach a server and greet it (`initialize`).
    pub fn open(server: &Server, token: Option<String>) -> Result<Session, String> {
        let link = if server.program {
            let words = split_command(&server.target);
            let (vars, words) = server_env(&words);
            let (program, args) = words.split_first().ok_or_else(|| "no program".to_string())?;
            let dir = crate::store::PrivateDir::new("sanaga-mcp").map_err(|e| e.to_string())?;
            let mut cmd = std::process::Command::new(program);
            cmd.args(args).current_dir(dir.path()).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
            // its environment: a few of Sanaga's variables (env_kept), and
            // those the person gave this server — nothing else
            cmd.env_clear().envs(kept_env(std::env::vars_os()));
            // (a launcher often gives a shorter PATH than a terminal)
            if let Ok(path) = std::env::join_paths(crate::ai_cli::search_path()) {
                cmd.env("PATH", path);
            }
            cmd.envs(vars);
            let mut child = cmd.spawn().map_err(|e| format!("{program}: {e}"))?;
            let stdin = child.stdin.take().ok_or("no input")?;
            let stdout = child.stdout.take().ok_or("no output")?;
            // (its lines are read on a thread: a server that says nothing
            // is waited for a while, not for ever)
            let (tx, lines) = channel();
            std::thread::Builder::new()
                .name("sanaga-mcp".into())
                .spawn(move || {
                    let mut out = std::io::BufReader::new(stdout);
                    while let Ok(Some(line)) = read_line(&mut out, MAX_LINE) {
                        let too_long = line.is_err();
                        if tx.send(line).is_err() || too_long {
                            break;
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
            Link::Program { child, stdin, lines, _dir: dir }
        } else {
            if !address_ok(&server.target) {
                return Err(crate::i18n::tr("Adresse refusée : https, ou un serveur sur cet ordinateur.").into());
            }
            Link::Address { url: server.target.trim().to_string(), token: token.filter(|t| !t.is_empty()), session: None, ready: false }
        };
        let mut s = Session { link, next: 1 };
        let hello = s.ask("initialize", json!({"protocolVersion": PROTOCOL, "capabilities": {}, "clientInfo": {"name": "Sanaga", "version": env!("CARGO_PKG_VERSION")}}))?;
        if !hello["capabilities"].is_object() && !hello["serverInfo"].is_object() {
            return Err(crate::i18n::tr("Ce n'est pas un serveur MCP (réponse inattendue).").into());
        }
        if let Link::Address { ready, .. } = &mut s.link {
            *ready = true;
        }
        s.tell("notifications/initialized")?;
        Ok(s)
    }

    /// Send a message that expects no answer.
    fn tell(&mut self, method: &str) -> Result<(), String> {
        let msg = json!({"jsonrpc": "2.0", "method": method});
        match &mut self.link {
            Link::Program { stdin, .. } => writeln!(stdin, "{msg}").and_then(|()| stdin.flush()).map_err(|e| e.to_string()),
            Link::Address { .. } => self.post(&msg).map(|_| ()),
        }
    }

    /// One POST to a server at an address; gives the answer's body.
    fn post(&mut self, msg: &Value) -> Result<ureq::Response, String> {
        let Link::Address { url, token, session, ready } = &mut self.link else { return Err("not an address".into()) };
        let local = url::Url::parse(url).ok().and_then(|u| u.host_str().map(is_local)).unwrap_or(false);
        // (a server on this computer is never reached through a proxy)
        let agent = if local { crate::net::local_builder() } else { crate::net::builder() }.timeout_connect(Duration::from_secs(10)).timeout_read(PATIENCE).build();
        let mut req = agent.post(url).set("Content-Type", "application/json").set("Accept", "application/json, text/event-stream");
        if *ready {
            req = req.set("MCP-Protocol-Version", PROTOCOL);
        }
        if let Some(t) = token {
            req = req.set("Authorization", &format!("Bearer {t}"));
        }
        if let Some(s) = session {
            req = req.set("Mcp-Session-Id", s);
        }
        let resp = req.send_string(&msg.to_string()).map_err(|e| match e {
            ureq::Error::Status(401 | 403, _) => crate::i18n::tr("Accès refusé : vérifiez le jeton d'accès.").to_string(),
            ureq::Error::Status(code, r) => format!("{code} {}", r.into_string().unwrap_or_default().chars().take(200).collect::<String>()),
            ureq::Error::Transport(t) => t.to_string(),
        })?;
        if let Some(id) = resp.header("Mcp-Session-Id") {
            *session = Some(id.to_string());
        }
        Ok(resp)
    }

    /// Send a request and wait for its answer.
    fn ask(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        let msg = request(id, method, params);
        let silent = || crate::i18n::tr("Le serveur ne répond pas.").to_string();
        let too_long = || crate::i18n::tr("Le serveur a envoyé un message trop long : l'appel est arrêté.").to_string();
        match &mut self.link {
            Link::Program { stdin, lines, .. } => {
                writeln!(stdin, "{msg}").and_then(|()| stdin.flush()).map_err(|e| e.to_string())?;
                let until = std::time::Instant::now() + PATIENCE;
                loop {
                    let left = until.saturating_duration_since(std::time::Instant::now());
                    let line = lines.recv_timeout(left).map_err(|_| silent())?.map_err(|()| too_long())?;
                    if let Some(answer) = answer_to(&line, id) {
                        return answer;
                    }
                    if let Some(no) = refusal_of(&line) {
                        let _ = writeln!(stdin, "{no}").and_then(|()| stdin.flush());
                    }
                }
            }
            Link::Address { .. } => {
                let resp = self.post(&msg)?;
                if resp.content_type() == "text/event-stream" {
                    // (the answer comes among the events of a stream)
                    let mut events = std::io::BufReader::new(resp.into_reader());
                    while let Ok(Some(line)) = read_line(&mut events, MAX_LINE) {
                        let line = line.map_err(|()| too_long())?;
                        if let Some(answer) = line.strip_prefix("data:").and_then(|d| answer_to(d, id)) {
                            return answer;
                        }
                    }
                    Err(silent())
                } else {
                    let body = resp.into_string().map_err(|e| e.to_string())?;
                    answer_to(&body, id).unwrap_or_else(|| Err(crate::i18n::tr("Ce n'est pas un serveur MCP (réponse inattendue).").into()))
                }
            }
        }
    }

    /// The tools the server offers.
    pub fn tools(&mut self) -> Result<Vec<RemoteTool>, String> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        // (a few pages at most)
        for _ in 0..5 {
            let params = cursor.as_ref().map(|c| json!({"cursor": c})).unwrap_or_else(|| json!({}));
            let result = self.ask("tools/list", params)?;
            out.extend(tools_of(&result));
            cursor = result["nextCursor"].as_str().filter(|c| !c.is_empty()).map(str::to_string);
            if cursor.is_none() || out.len() >= MAX_TOOLS {
                break;
            }
        }
        out.truncate(MAX_TOOLS);
        Ok(out)
    }

    /// Use a tool: what it answered, and whether it says it failed.
    pub fn call(&mut self, tool: &str, input: &Value) -> Result<(String, bool), String> {
        let input = if input.is_object() { input.clone() } else { json!({}) };
        Ok(called(&self.ask("tools/call", json!({"name": tool, "arguments": input}))?))
    }
}

/// The servers being talked to: a server's program stays started between
/// two calls. Cheap to copy: every copy is the same set.
#[derive(Clone, Default)]
pub struct Hub(Arc<Mutex<HashMap<String, Arc<Mutex<Session>>>>>);

impl Hub {
    /// Do something with a server's session, opened if need be; a session
    /// that failed is opened afresh once.
    pub fn with<T>(&self, server: &Server, token: Option<String>, mut work: impl FnMut(&mut Session) -> Result<T, String>) -> Result<T, String> {
        let mut last = String::new();
        for _ in 0..2 {
            let kept = self.0.lock().unwrap_or_else(|e| e.into_inner()).get(&server.name).cloned();
            let session = match kept {
                Some(s) => s,
                None => {
                    let s = Arc::new(Mutex::new(Session::open(server, token.clone())?));
                    self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(server.name.clone(), s.clone());
                    s
                }
            };
            let done = work(&mut session.lock().unwrap_or_else(|e| e.into_inner()));
            match done {
                Ok(v) => return Ok(v),
                Err(e) => {
                    last = e;
                    self.forget(&server.name);
                }
            }
        }
        Err(last)
    }

    /// End a server's session (it was removed, switched off, or failed).
    pub fn forget(&self, name: &str) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_of_servers_is_kept_and_read_back() {
        let list = vec![
            Server { name: "Notes".into(), program: true, target: "python3 /home/me/notes server.py --quiet".into(), on: true, free: vec!["search".into(), "list".into()] },
            Server { name: "Suivi".into(), program: false, target: "https://example.org/mcp".into(), on: false, free: Vec::new() },
        ];
        assert_eq!(parse_list(&format_list(&list)), list);
        // a damaged file: what can be read is, the rest is passed over
        let read = parse_list("Notes\tprogram\tnotes-mcp\t1\ta, b\n\nSans cible\taddress\t\t1\t\nNotes\tprogram\tautre\t1\t\nSeul\n\u{0}\t\t\n");
        assert_eq!(read, vec![Server { name: "Notes".into(), program: true, target: "notes-mcp".into(), on: true, free: vec!["a".into(), "b".into()] }]);
        crate::mutate::run(&[format_list(&list).as_bytes()], 10_000, |d| {
            let _ = parse_list(&String::from_utf8_lossy(d));
        });
    }

    #[test]
    fn only_safe_addresses_are_reached() {
        for ok in ["https://example.org/mcp", "http://localhost:8080/mcp", "http://127.0.0.1:3000/", "http://[::1]:9/mcp"] {
            assert!(address_ok(ok), "{ok}");
        }
        for bad in ["http://example.org/mcp", "ftp://example.org/", "file:///etc/passwd", "example.org", "", "http://localhost.evil.example/"] {
            assert!(!address_ok(bad), "{bad}");
        }
    }

    #[test]
    fn names_and_command_lines() {
        assert_eq!(tool_name("Mes notes", "search.all"), "mcp__Mes_notes__search_all");
        assert!(tool_name(&"s".repeat(80), "t").len() <= 64);
        assert_eq!(split_command("npx -y  @acme/notes --dir \"/home/me/Mes notes\" 'a b' \"\""), ["npx", "-y", "@acme/notes", "--dir", "/home/me/Mes notes", "a b", ""]);
        assert!(split_command("   ").is_empty());
    }

    #[test]
    fn a_servers_lines_are_bounded() {
        let mut r = std::io::Cursor::new(b"{\"a\":1}\r\nshort\n".to_vec());
        assert_eq!(read_line(&mut r, 16).unwrap(), Some(Ok("{\"a\":1}".to_string())));
        assert_eq!(read_line(&mut r, 16).unwrap(), Some(Ok("short".to_string())));
        assert_eq!(read_line(&mut r, 16).unwrap(), None);
        // exactly the most, with or without its line break: taken
        let mut r = std::io::Cursor::new([vec![b'x'; 16], b"\n".to_vec(), vec![b'y'; 16]].concat());
        assert_eq!(read_line(&mut r, 16).unwrap(), Some(Ok("x".repeat(16))));
        assert_eq!(read_line(&mut r, 16).unwrap(), Some(Ok("y".repeat(16))));
        // longer: refused, and only the most and a byte are read
        let mut r = std::io::Cursor::new(vec![b'z'; 1_000]);
        assert_eq!(read_line(&mut r, 16).unwrap(), Some(Err(())));
        assert_eq!(r.position(), 17);
        const { assert!(MAX_LINE >= MAX_ANSWER * 4, "an answer that is cut afterwards still arrives") };
    }

    #[test]
    fn a_server_program_gets_a_small_environment() {
        use std::ffi::OsString;
        let os = |k: &str, v: &str| (OsString::from(k), OsString::from(v));
        let given = kept_env(
            [
                os("HOME", "/home/me"),
                os("LANG", "fr_FR.UTF-8"),
                os("LC_TIME", "fr_FR.UTF-8"),
                os("XDG_RUNTIME_DIR", "/run/user/1000"),
                os("ANTHROPIC_API_KEY", "sk-secret"),
                os("GITHUB_TOKEN", "ghp_x"),
                os("AWS_SECRET_ACCESS_KEY", "x"),
                os("SSH_AUTH_SOCK", "/run/user/1000/ssh"),
                os("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
                os("LD_PRELOAD", "/tmp/x.so"),
                os("PATH", "/usr/bin"),
            ]
            .into_iter(),
        );
        let names: Vec<&str> = given.iter().map(|(k, _)| k.to_str().unwrap()).collect();
        assert_eq!(names, ["HOME", "LANG", "LC_TIME", "XDG_RUNTIME_DIR"], "PATH is given apart");
        // variables the person wrote before the program, as in a shell
        let words = split_command("GITHUB_TOKEN=ghp_1 _X=\"a b\" npx -y server A=1");
        let (vars, command) = server_env(&words);
        assert_eq!(vars, [("GITHUB_TOKEN".to_string(), "ghp_1".to_string()), ("_X".to_string(), "a b".to_string())]);
        assert_eq!(command, ["npx", "-y", "server", "A=1"], "only before the program");
        let words = split_command("./serve --opt=1");
        assert_eq!(server_env(&words), (Vec::new(), &words[..]));
        let words = split_command("1A=x =y prog");
        assert!(server_env(&words).0.is_empty(), "not names: the program's words");
        assert!(server_env(&split_command("ONLY=1")).1.is_empty());
        assert_eq!(server_name("  Mes\tnotes\n"), "Mes notes");
    }

    #[test]
    fn answers_are_read_from_among_other_messages() {
        assert_eq!(answer_to(r#"{"jsonrpc":"2.0","id":3,"result":{"tools":[]}}"#, 3), Some(Ok(json!({"tools": []}))));
        assert_eq!(answer_to(r#"{"jsonrpc":"2.0","id":3,"error":{"code":-1,"message":"non"}}"#, 3), Some(Err("non".into())));
        assert_eq!(answer_to(r#"{"jsonrpc":"2.0","id":4,"result":{}}"#, 3), None, "the answer to another request");
        assert_eq!(answer_to(r#"{"jsonrpc":"2.0","method":"notifications/progress"}"#, 3), None);
        assert_eq!(answer_to(r#"{"jsonrpc":"2.0","id":3,"method":"roots/list"}"#, 3), None, "a request of the server's with the same number");
        assert_eq!(answer_to("not json", 3), None);
        // the server asks something of Sanaga: refused
        let no = refusal_of(r#"{"jsonrpc":"2.0","id":"a","method":"sampling/createMessage","params":{}}"#).unwrap();
        assert_eq!((no["id"].as_str(), no["error"]["code"].as_i64()), (Some("a"), Some(-32601)));
        assert!(refusal_of(r#"{"jsonrpc":"2.0","method":"notifications/message"}"#).is_none());
    }

    #[test]
    fn tools_and_their_answers_are_read() {
        let tools = tools_of(&json!({"tools": [
            {"name": "search", "description": "Search the notes", "inputSchema": {"type": "object", "properties": {"q": {"type": "string"}}}},
            {"name": "bare"},
            {"name": "", "description": "no name"},
            {"description": "no name at all"}
        ]}));
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[1], RemoteTool { name: "bare".into(), description: String::new(), schema: json!({"type": "object", "properties": {}}) });
        assert_eq!(called(&json!({"content": [{"type": "text", "text": "a"}, {"type": "image", "data": "…"}, {"type": "text", "text": "b"}]})), ("a\n[image]\nb".to_string(), false));
        assert_eq!(called(&json!({"content": [{"type": "text", "text": "échec"}], "isError": true})), ("échec".to_string(), true));
        assert_eq!(called(&json!({"content": [], "structuredContent": {"n": 1}})), ("{\"n\":1}".to_string(), false));
        assert!(called(&json!({"content": [{"type": "text", "text": "x".repeat(MAX_ANSWER + 10)}]})).0.ends_with("too long]"));
        // whatever a server sends, reading it never panics
        crate::mutate::run(&[br#"{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"t","inputSchema":{"type":"object"}}],"content":[{"type":"text","text":"a"}],"nextCursor":"c"}}"#], 10_000, |d| {
            let t = String::from_utf8_lossy(d);
            let _ = refusal_of(&t);
            if let Some(Ok(v)) = answer_to(&t, 1) {
                let _ = tools_of(&v);
                let _ = called(&v);
            }
        });
    }

    /// The whole exchange with a real server: tools/fake-mcp.py, as a
    /// program. (Skipped where python3 is not installed.)
    #[test]
    fn a_program_is_greeted_listed_and_called() {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/fake-mcp.py");
        if std::process::Command::new("python3").arg("--version").output().is_err() {
            return;
        }
        let server = Server { name: "Essai".into(), program: true, target: format!("python3 '{}'", script.display()), on: true, free: Vec::new() };
        let hub = Hub::default();
        let tools = hub.with(&server, None, |s| s.tools()).unwrap();
        assert_eq!(tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["echo", "add_note", "fail"]);
        assert_eq!(hub.with(&server, None, |s| s.call("echo", &json!({"text": "bonjour é"}))).unwrap(), ("bonjour é".to_string(), false));
        assert_eq!(hub.with(&server, None, |s| s.call("fail", &json!({}))).unwrap(), ("this tool always fails".to_string(), true));
        assert!(hub.with(&server, None, |s| s.call("unknown", &json!({}))).is_err());
        // not a server at all
        let cat = Server { target: "true".into(), ..server.clone() };
        assert!(Hub::default().with(&cat, None, |s| s.tools()).is_err());
        let none = Server { target: "/no/such/program".into(), ..server };
        assert!(Hub::default().with(&none, None, |s| s.tools()).is_err());
    }
}
