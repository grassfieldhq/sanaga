//! AI assistants already installed on the computer, used through their
//! command line with the person's own account: Claude Code, Codex and Gemini
//! CLI. No API key to give Sanaga.
//!
//! They are agents able to run commands and read or edit files, and the text
//! of a web page could try to give them orders. Only an assistant that can
//! be started with no tools at all and no access to files receives the text
//! of web pages (summaries, translation): Claude Code, with its documented
//! switches (`--tools ""`, `--disallowedTools "*"`, no MCP servers, safe
//! mode: no CLAUDE.md, hooks or plugins). Codex's read-only sandbox and
//! Gemini CLI's default mode still let them read local files, so they are
//! refused for page content (they may still write an extension from the
//! person's own description, which the person reviews before enabling it).
//! All run in an empty private folder, and Sanaga only reads their answer.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliTool {
    ClaudeCode,
    Codex,
    Gemini,
}

impl CliTool {
    pub const ALL: [CliTool; 3] = [CliTool::ClaudeCode, CliTool::Codex, CliTool::Gemini];

    /// The value kept in the settings (`ai=`).
    pub fn key(self) -> &'static str {
        match self {
            CliTool::ClaudeCode => "claude-code",
            CliTool::Codex => "codex",
            CliTool::Gemini => "gemini-cli",
        }
    }

    pub fn from_key(k: &str) -> Option<CliTool> {
        CliTool::ALL.into_iter().find(|t| t.key() == k)
    }

    pub fn name(self) -> &'static str {
        match self {
            CliTool::ClaudeCode => "Claude Code",
            CliTool::Codex => "Codex",
            CliTool::Gemini => "Gemini CLI",
        }
    }

    /// Can it be started with no tools and no access to files? Only such
    /// an assistant may receive the text of web pages.
    pub fn tool_less(self) -> bool {
        matches!(self, CliTool::ClaudeCode)
    }

    fn program(self) -> &'static str {
        match self {
            CliTool::ClaudeCode => "claude",
            CliTool::Codex => "codex",
            CliTool::Gemini => "gemini",
        }
    }
}

/// Where command-line tools live: the PATH Sanaga got, plus the usual places
/// of npm, mise, bun, volta… (a launcher often gives a shorter PATH than a
/// terminal).
fn search_path() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for d in [
            ".local/bin",
            ".claude/local",
            ".local/share/mise/command-wrappers/bin",
            ".local/share/mise/shims",
            ".npm-global/bin",
            ".volta/bin",
            ".bun/bin",
            ".cargo/bin",
            ".opencode/bin",
            "bin",
        ] {
            dirs.push(home.join(d));
        }
    }
    dirs.extend(["/usr/local/bin", "/usr/bin", "/opt/homebrew/bin"].map(PathBuf::from));
    let mut seen = Vec::new();
    dirs.retain(|d| {
        let new = !seen.contains(d);
        seen.push(d.clone());
        new
    });
    dirs
}

fn find(program: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    search_path().into_iter().map(|d| d.join(program)).find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// The assistants found on this computer (looked for once).
pub fn installed() -> Vec<CliTool> {
    static FOUND: std::sync::OnceLock<Vec<CliTool>> = std::sync::OnceLock::new();
    FOUND.get_or_init(|| CliTool::ALL.into_iter().filter(|t| find(t.program()).is_some()).collect()).clone()
}

/// A new, empty folder where an assistant runs (nothing of the person's
/// around): private (0700, in $XDG_RUNTIME_DIR when there is one), removed
/// after the answer.
fn work_dir() -> std::io::Result<crate::store::PrivateDir> {
    crate::store::PrivateDir::new("sanaga-ai")
}

/// Ask an installed assistant; returns its answer.
pub fn ask(tool: CliTool, model: &str, system: &str, prompt: &str) -> Result<String, String> {
    ask_stoppable(tool, model, system, prompt, None)
}

/// The same; when `stop` becomes true the assistant is ended and
/// `ai::STOPPED` is the answer.
pub fn ask_stoppable(tool: CliTool, model: &str, system: &str, prompt: &str, stop: Option<&std::sync::atomic::AtomicBool>) -> Result<String, String> {
    let program = find(tool.program()).ok_or_else(|| crate::i18n::tf("{} n'est plus installé.", &[&tool.name()]))?;
    let work = work_dir().map_err(|e| e.to_string())?;
    let dir = work.path().to_path_buf();
    let path = std::env::join_paths(search_path()).ok();
    let mut cmd = Command::new(&program);
    cmd.current_dir(&dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(p) = &path {
        cmd.env("PATH", p);
    }
    let mut out_file: Option<PathBuf> = None;
    let input;
    match tool {
        CliTool::ClaudeCode => {
            // print mode; no built-in tools ("") and every tool denied ("*",
            // MCP ones too); no MCP servers; safe mode: no CLAUDE.md, hooks,
            // plugins or skills; nothing kept
            cmd.args([
                "-p",
                "--tools",
                "",
                "--disallowedTools",
                "*",
                "--strict-mcp-config",
                "--safe-mode",
                "--disable-slash-commands",
                "--no-session-persistence",
                "--output-format",
                "text",
                "--system-prompt",
                system,
            ]);
            if !model.is_empty() {
                cmd.args(["--model", model]);
            }
            input = prompt.to_string();
        }
        CliTool::Codex => {
            // one run, read-only sandbox (no writing, no network), its last message in a file
            let f = dir.join(format!("answer-{}.txt", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
            cmd.args(["exec", "--skip-git-repo-check", "--ephemeral", "-s", "read-only", "--color", "never", "-o"]).arg(&f);
            if !model.is_empty() {
                cmd.args(["-m", model]);
            }
            cmd.arg("-");
            out_file = Some(f);
            input = format!("{system}\n\nDo not run commands or read files: answer from the text below only.\n\n{prompt}");
        }
        CliTool::Gemini => {
            // non-interactive: nothing that needs approval runs
            cmd.args(["-p", "Follow the instructions at the top of the input.", "--approval-mode", "default"]);
            if !model.is_empty() {
                cmd.args(["-m", model]);
            }
            input = format!("{system}\n\n{prompt}");
        }
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    // read the outputs on threads (a full pipe would block the tool)
    let mut so = child.stdout.take();
    let mut se = child.stderr.take();
    let t_out = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = so.as_mut() {
            let _ = o.read_to_string(&mut s);
        }
        s
    });
    let t_err = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = se.as_mut() {
            let _ = o.read_to_string(&mut s);
        }
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            break st;
        }
        if stop.is_some_and(|s| s.load(std::sync::atomic::Ordering::Relaxed)) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(crate::ai::STOPPED.to_string());
        }
        if start.elapsed() > Duration::from_secs(300) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(crate::i18n::tf("{} ne répond pas.", &[&tool.name()]));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let stdout = t_out.join().unwrap_or_default();
    let stderr = t_err.join().unwrap_or_default();
    let answer = match &out_file {
        Some(f) => {
            let a = std::fs::read_to_string(f).unwrap_or_default();
            let _ = std::fs::remove_file(f);
            a
        }
        None => stdout,
    };
    if !status.success() || answer.trim().is_empty() {
        let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
        let hint = if last.to_lowercase().contains("login") || last.to_lowercase().contains("auth") {
            crate::i18n::tf("Connectez-vous d'abord à {} dans un terminal.", &[&tool.name()])
        } else {
            last
        };
        return Err(format!("{} : {}", tool.name(), hint));
    }
    Ok(answer.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_tool_less_assistants_read_pages() {
        assert!(CliTool::ClaudeCode.tool_less());
        assert!(!CliTool::Codex.tool_less());
        assert!(!CliTool::Gemini.tool_less());
    }

    #[test]
    fn keys_round_trip_and_folder_is_private() {
        for t in CliTool::ALL {
            assert_eq!(CliTool::from_key(t.key()), Some(t));
        }
        let w = work_dir().unwrap();
        assert!(w.path().file_name().is_some_and(|n| n.to_string_lossy().starts_with("sanaga-ai-")));
        let mode = std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(w.path()).unwrap().permissions());
        assert_eq!(mode & 0o777, 0o700);
        assert!(search_path().iter().any(|d| d.ends_with("usr/bin")));
    }
}
