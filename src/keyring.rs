//! Passwords live in the system keyring (GNOME Keyring / KWallet through the
//! Secret Service), never in Sanaga's own files.

use std::io::Write;
use std::process::{Command, Stdio};

fn attrs<'a>(profile: &'a str, site: &'a str, user: &'a str) -> [&'a str; 8] {
    ["application", "sanaga", "profile", profile, "site", site, "user", user]
}

pub fn available() -> bool {
    Command::new("secret-tool").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
}

pub fn store(profile: &str, site: &str, user: &str, password: &str) -> bool {
    let label = format!("Sanaga : {site} ({user})");
    let Ok(mut child) = Command::new("secret-tool")
        .arg("store")
        .arg("--label")
        .arg(&label)
        .args(attrs(profile, site, user))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(password.as_bytes());
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

pub fn lookup(profile: &str, site: &str, user: &str) -> Option<String> {
    let out = Command::new("secret-tool").arg("lookup").args(attrs(profile, site, user)).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    Some(s.strip_suffix('\n').unwrap_or(&s).to_string())
}

/// Any secret of the keyring found by its attributes (e.g. Chrome's key).
pub fn lookup_attrs(attrs: &[&str]) -> Option<String> {
    let out = Command::new("secret-tool").arg("lookup").args(attrs).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    Some(s.strip_suffix('\n').unwrap_or(&s).to_string())
}

pub fn delete(profile: &str, site: &str, user: &str) {
    let _ = Command::new("secret-tool").arg("clear").args(attrs(profile, site, user)).stderr(Stdio::null()).status();
}

/// A JavaScript string literal (for handing values to a page safely).
pub fn js_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '<' => out.push_str("\\u003c"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn js_strings_are_escaped() {
        assert_eq!(super::js_str("a\"b\\c\n</script>"), "\"a\\\"b\\\\c\\n\\u003c/script>\"");
    }
}
