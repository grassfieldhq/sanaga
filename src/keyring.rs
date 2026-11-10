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

/// The accounts Sanaga saved in the keyring for this profile: (site, user).
/// The keyring is the reference (the list in Sanaga's folder is only a copy);
/// None when the keyring cannot be asked.
pub fn list(profile: &str) -> Option<Vec<(String, String)>> {
    let out = Command::new("secret-tool").args(["search", "--all", "application", "sanaga", "profile", profile]).output().ok()?;
    // (secret-tool writes the attributes on its error output, the secrets on
    // its normal output: only the attributes are read)
    Some(parse_list(&String::from_utf8_lossy(&out.stderr)))
}

/// Reads the attributes of `secret-tool search --all` (the secrets are
/// skipped: only names are kept).
fn parse_list(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let (mut site, mut user, mut ours) = (None::<String>, None::<String>, false);
    let flush = |site: &mut Option<String>, user: &mut Option<String>, ours: &mut bool, out: &mut Vec<(String, String)>| {
        if let (Some(s), true) = (site.take(), *ours) {
            if s != crate::ai::KEY_SITE && s != crate::sync::KEY_SITE {
                let u = user.take().unwrap_or_default();
                if !out.iter().any(|(a, b)| *a == s && *b == u) {
                    out.push((s, u));
                }
            }
        }
        *user = None;
        *ours = false;
    };
    for line in text.lines() {
        if line.starts_with('[') {
            flush(&mut site, &mut user, &mut ours, &mut out);
        } else if let Some(v) = line.strip_prefix("attribute.site = ") {
            site = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("attribute.user = ") {
            user = Some(v.to_string());
        } else if line == "attribute.application = sanaga" {
            ours = true;
        }
    }
    flush(&mut site, &mut user, &mut ours, &mut out);
    out
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
    fn reads_the_keyring_list() {
        let text = "[/org/freedesktop/secrets/collection/login/5]\nlabel = Sanaga : https://a.example (alice)\nsecret = hunter2\nattribute.site = https://a.example\nattribute.user = alice\nattribute.application = sanaga\nattribute.profile = \n[/org/freedesktop/secrets/collection/login/6]\nlabel = Sanaga : https://api.anthropic.com (claude)\nattribute.site = https://api.anthropic.com\nattribute.user = claude\nattribute.application = sanaga\n";
        assert_eq!(super::parse_list(text), vec![("https://a.example".to_string(), "alice".to_string())]);
    }

    #[test]
    fn js_strings_are_escaped() {
        assert_eq!(super::js_str("a\"b\\c\n</script>"), "\"a\\\"b\\\\c\\n\\u003c/script>\"");
    }
}
