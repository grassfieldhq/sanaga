//! Passwords live in the system keyring (GNOME Keyring / KWallet through the
//! Secret Service), never in Sanaga's own files.

use std::io::Write;
use std::process::{Command, Stdio};

fn attrs<'a>(profile: &'a str, site: &'a str, user: &'a str) -> [&'a str; 8] {
    ["application", "sanaga", "profile", profile, "site", site, "user", user]
}

/// `secret-tool <action…> -- <attribute value…>`. The `--` ends the options:
/// without it a user or profile name that starts with `-` ("-anna") was read
/// as an option, and that account could not be saved, read or removed.
/// (secret-tool reads its arguments with GLib's option parser, which takes
/// everything after `--` as the attributes.)
fn tool(action: &[&str], attrs: &[&str]) -> Command {
    let mut c = Command::new("secret-tool");
    c.args(action).arg("--").args(attrs);
    c
}

pub fn available() -> bool {
    Command::new("secret-tool").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
}

pub fn store(profile: &str, site: &str, user: &str, password: &str) -> bool {
    let label = format!("Sanaga : {site} ({user})");
    let Ok(mut child) = tool(&["store", "--label", &label], &attrs(profile, site, user)).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(password.as_bytes());
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

pub fn lookup(profile: &str, site: &str, user: &str) -> Option<String> {
    let out = tool(&["lookup"], &attrs(profile, site, user)).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    Some(s.strip_suffix('\n').unwrap_or(&s).to_string())
}

/// Any secret of the keyring found by its attributes (e.g. Chrome's key).
pub fn lookup_attrs(attrs: &[&str]) -> Option<String> {
    let out = tool(&["lookup"], attrs).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    Some(s.strip_suffix('\n').unwrap_or(&s).to_string())
}

/// The accounts Sanaga saved in the keyring for this profile: (site, user).
/// The keyring is the reference (the list in Sanaga's folder is only a copy);
/// None when the keyring cannot be asked (no keyring service, a refusal):
/// the copy then stays as it is instead of being emptied.
pub fn list(profile: &str) -> Option<Vec<(String, String)>> {
    // secret-tool writes each item's path, label and secret on its normal
    // output, and only the `attribute.name = value` lines on its error
    // output. The names are all that is wanted: the normal output is thrown
    // away unread, so no password enters Sanaga's memory here.
    let out = tool(&["search", "--all"], &["application", "sanaga", "profile", profile]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(parse_list(&String::from_utf8_lossy(&out.stderr)))
}

/// Reads the attribute lines of `secret-tool search --all`. Nothing on that
/// output separates one account from the next (the `[path]` line of each
/// goes to the other output), and the attributes of an account come in no
/// fixed order: an account ends where an attribute name comes a second
/// time. (Taking `[` as the separator, as before, made one account of them
/// all: the list showed a single login however many were saved.)
fn parse_list(text: &str) -> Vec<(String, String)> {
    fn flush(item: &mut Vec<(&str, &str)>, out: &mut Vec<(String, String)>) {
        let get = |name: &str| item.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
        if let (Some("sanaga"), Some(site)) = (get("application"), get("site")) {
            let user = get("user").unwrap_or_default();
            // (Sanaga's own secrets — the AI key, the sync phrase — are not accounts)
            if site != crate::ai::KEY_SITE && site != crate::sync::KEY_SITE && !out.iter().any(|(s, u)| s == site && u == user) {
                out.push((site.to_string(), user.to_string()));
            }
        }
        item.clear();
    }
    let mut out = Vec::new();
    let mut item: Vec<(&str, &str)> = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("attribute.") else { continue };
        // ("name = " with nothing after it: an empty value, as the main profile's name)
        let Some((name, value)) = rest.split_once(" = ").or_else(|| rest.strip_suffix(" =").map(|n| (n, ""))) else { continue };
        if item.iter().any(|(n, _)| *n == name) {
            flush(&mut item, &mut out);
        }
        item.push((name, value));
    }
    flush(&mut item, &mut out);
    out
}

/// Everything Sanaga kept in the keyring for one profile (its passwords,
/// its sync phrase, its AI key): removed with the profile. They used to
/// stay, and a new profile given the same name found them all.
pub fn forget_profile(profile: &str) {
    // (never the main profile's, whose name is empty: it cannot be deleted)
    if !profile.is_empty() {
        let _ = tool(&["clear"], &["application", "sanaga", "profile", profile]).stderr(Stdio::null()).status();
    }
}

pub fn delete(profile: &str, site: &str, user: &str) {
    let _ = tool(&["clear"], &attrs(profile, site, user)).stderr(Stdio::null()).status();
}

// (how a text is written into a script: guard.rs, with the other security rules)
pub use crate::guard::js_str;

#[cfg(test)]
mod tests {
    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn reads_the_keyring_list() {
        // the error output of a real `secret-tool search --all application
        // sanaga profile ""` with three accounts saved, as it is (the item
        // paths, labels and secrets are on the other output)
        let real = "attribute.application = sanaga\nattribute.site = https://a.example\nattribute.user = alice\nattribute.profile = \nattribute.application = sanaga\nattribute.site = https://b.example\nattribute.user = bob\nattribute.profile = \nattribute.application = sanaga\nattribute.site = https://c.example\nattribute.user = carol\nattribute.profile = \n";
        assert_eq!(super::parse_list(real), pairs(&[("https://a.example", "alice"), ("https://b.example", "bob"), ("https://c.example", "carol")]));
        // the attributes of an account come in no fixed order, and other
        // lines may be mixed in (both outputs read together, a warning)
        let mixed = "[/org/freedesktop/secrets/collection/login/5]\nlabel = Sanaga : https://a.example (alice)\nattribute.site = https://a.example\nattribute.user = alice\nattribute.application = sanaga\nattribute.profile = \n[/org/freedesktop/secrets/collection/login/6]\nattribute.profile = \nattribute.user = bob\nattribute.application = sanaga\nattribute.site = https://b.example\nsecret-tool: a warning\n";
        assert_eq!(super::parse_list(mixed), pairs(&[("https://a.example", "alice"), ("https://b.example", "bob")]));
        // Sanaga's own secrets are not accounts; an account saved twice is
        // listed once; an attribute missing (no user, a value that lost its
        // trailing space) loses nothing else; another program's items are not Sanaga's
        let own = "attribute.site = https://api.anthropic.com\nattribute.user = claude\nattribute.application = sanaga\nattribute.site = https://a.example\nattribute.application = sanaga\nattribute.profile =\nattribute.site = https://a.example\nattribute.application = sanaga\nattribute.site = https://d.example\nattribute.application = other\nattribute.user = x\n";
        assert_eq!(super::parse_list(own), pairs(&[("https://a.example", "")]));
        assert!(super::parse_list("").is_empty() && super::parse_list("secret-tool: Cannot autolaunch D-Bus without X11 $DISPLAY\n").is_empty());
    }

    #[test]
    fn names_starting_with_a_dash_are_not_read_as_options() {
        let args = |c: std::process::Command| c.get_args().map(|a| a.to_string_lossy().to_string()).collect::<Vec<_>>();
        assert_eq!(args(super::tool(&["lookup"], &super::attrs("-travail", "https://a.example", "-anna"))), ["lookup", "--", "application", "sanaga", "profile", "-travail", "site", "https://a.example", "user", "-anna"]);
        assert_eq!(args(super::tool(&["store", "--label", "Sanaga : x (-anna)"], &["user", "-anna"])), ["store", "--label", "Sanaga : x (-anna)", "--", "user", "-anna"]);
        assert_eq!(args(super::tool(&["search", "--all"], &["profile", "--all"])), ["search", "--all", "--", "profile", "--all"]);
    }

    #[test]
    fn js_strings_are_escaped() {
        assert_eq!(super::js_str("a\"b\\c\n</script>"), "\"a\\\"b\\\\c\\n\\u003c/script>\"");
    }
}
