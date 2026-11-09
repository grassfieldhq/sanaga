//! Sanaga's security rules that need neither GTK nor WebKit (only std,
//! `url`, `percent-encoding` and `libc`): download names and marks, the
//! checks on page messages, .desktop escaping, the single-instance socket's
//! limits, private folders and permissions. Kept apart so they can be tested
//! anywhere: `tools/guard-tests` builds this file on its own.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// "https://www.site.com/x" → "www.site.com" (empty when there is none).
fn host_of(uri: &str) -> String {
    url::Url::parse(uri).ok().and_then(|u| u.host_str().map(|h| h.to_string())).unwrap_or_default()
}

// ---------------------------------------------------------------- downloads

/// The extensions the desktop treats as programs or services: a download
/// with one of them gets ".download" added, so opening it never runs it.
const INERT_EXTENSIONS: [&str; 9] = ["desktop", "service", "socket", "timer", "path", "mount", "automount", "target", "directory"];

/// Longest file name kept (in bytes; file systems allow 255).
const MAX_NAME: usize = 200;

/// Characters that change the direction of the text: they can make
/// "photo\u{202E}gpj.exe" look like "photoexe.jpg".
fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{200E}' | '\u{200F}' | '\u{061C}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// "name.ext" → ("name", ".ext"); a name without a dot (or starting with
/// one) has no extension.
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// A safe name for a downloaded file, from the name WebKit or the site
/// suggests: percent-decoded, then only its last part (a "/" or "\" can
/// never choose another folder), without control or direction-changing
/// characters, without leading or trailing dots and spaces (no hidden
/// file, no ".."), not too long, and made inert when the desktop could run
/// it (.desktop, .service…).
pub fn safe_download_name(suggested: &str) -> String {
    let decoded = percent_encoding::percent_decode_str(suggested).decode_utf8_lossy();
    let base = decoded.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base.chars().filter(|c| !c.is_control()).map(|c| if is_bidi_control(c) { '_' } else { c }).collect();
    let mut name = cleaned.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    if name.is_empty() {
        name = "telechargement".into();
    }
    if name.len() > MAX_NAME {
        let (stem, ext) = split_ext(&name);
        let ext = if ext.len() <= 16 { ext.to_string() } else { String::new() };
        let mut cut = MAX_NAME.saturating_sub(ext.len());
        while cut > 0 && !stem.is_char_boundary(cut.min(stem.len())) {
            cut -= 1;
        }
        name = format!("{}{ext}", &stem[..cut.min(stem.len())]);
    }
    let ext = split_ext(&name).1.trim_start_matches('.').to_ascii_lowercase();
    if INERT_EXTENSIONS.contains(&ext.as_str()) {
        name.push_str(".download");
    }
    name
}

/// A free place for `name` in `dir`: the name itself, else "name (2).ext",
/// "name (3).ext"… Anything already there counts as taken, even a link
/// pointing nowhere, and so do the names `taken` reserves (downloads that
/// have not created their file yet).
pub fn unique_path_with(dir: &Path, name: &str, taken: impl Fn(&Path) -> bool) -> PathBuf {
    let free = |p: &Path| std::fs::symlink_metadata(p).is_err() && !taken(p);
    let first = dir.join(name);
    if free(&first) {
        return first;
    }
    let (stem, ext) = split_ext(name);
    for n in 2..10_000 {
        let p = dir.join(format!("{stem} ({n}){ext}"));
        if free(&p) {
            return p;
        }
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    dir.join(format!("{stem} ({stamp}){ext}"))
}

pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    unique_path_with(dir, name, |_| false)
}

/// An address as recorded with a downloaded file (the "where from" mark):
/// no user name or password, no #fragment, and not too long for an
/// extended attribute (a data: address keeps only its type). None when
/// there is nothing worth recording.
pub fn address_for_mark(url: &str) -> Option<String> {
    let mut u = url::Url::parse(url.trim()).ok()?;
    match u.scheme() {
        "about" | "javascript" => return None,
        "data" => {
            let head: String = url.trim().split([',', ';']).next().unwrap_or("data:").chars().take(128).collect();
            return Some(head);
        }
        _ => {}
    }
    let _ = u.set_username("");
    let _ = u.set_password(None);
    u.set_fragment(None);
    let mut s = u.to_string();
    if s.len() > 2048 {
        let mut cut = 2048;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
    Some(s)
}

/// Mark a downloaded file with where it came from, as the freedesktop.org
/// convention says (`user.xdg.origin.url`, `user.xdg.referrer.url`): the
/// file manager reads them and asks before running a downloaded program.
/// Set on the opened file itself (never through a link).
#[cfg(target_os = "linux")]
pub fn mark_download(path: &Path, origin: &str, referrer: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    let file = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("not a regular file"));
    }
    let mut result = Ok(());
    for (name, value) in [(c"user.xdg.origin.url", address_for_mark(origin)), (c"user.xdg.referrer.url", address_for_mark(referrer))] {
        let Some(value) = value else { continue };
        // SAFETY: a valid open descriptor, a NUL-terminated name, and a
        // buffer of `value.len()` bytes that lives through the call.
        let rc = unsafe { libc::fsetxattr(file.as_raw_fd(), name.as_ptr(), value.as_ptr().cast(), value.len(), 0) };
        if rc != 0 {
            result = Err(std::io::Error::last_os_error());
        }
    }
    result
}

#[cfg(not(target_os = "linux"))]
pub fn mark_download(_path: &Path, _origin: &str, _referrer: &str) -> std::io::Result<()> {
    Ok(())
}

/// Safari's rule for downloads: they go straight to the Downloads folder
/// when the person just acted on the page and no other download used that
/// action; a site that starts a download by itself, or several at once,
/// must be allowed first (unless the person already allowed that site).
pub fn download_needs_ask(gesture_fresh: bool, gesture_used: bool, site_allowed: bool) -> bool {
    !site_allowed && (!gesture_fresh || gesture_used)
}

/// The site named in "Allow downloads on …?": the page's, else the file's.
pub fn download_site(referrer: &str, uri: &str) -> String {
    let h = host_of(referrer);
    if !h.is_empty() {
        return h;
    }
    let h = host_of(uri);
    if !h.is_empty() {
        return h;
    }
    uri.split(':').next().unwrap_or("").to_string()
}

// ---------------------------------------------------------------- page messages

fn without_fragment(u: &str) -> &str {
    u.split('#').next().unwrap_or(u)
}

/// A message of Sanaga's page scripts ("address\nbody"): its body when it
/// comes from the page the view shows now (the #fragment may differ),
/// nothing when that page has navigated since.
pub fn message_body<'a>(msg: &'a str, current: &str) -> Option<&'a str> {
    let (from, body) = msg.split_once('\n')?;
    (!current.is_empty() && without_fragment(from) == without_fragment(current)).then_some(body)
}

/// A keyboard shortcut forwarded by the page script ("cs:t", ":f5"…).
pub fn is_shortcut_message(body: &str) -> bool {
    body.split_once(':').is_some_and(|(mods, key)| !key.is_empty() && mods.chars().all(|c| matches!(c, 'c' | 's' | 'a')))
}

/// A selector the element remover could have made (it becomes part of the
/// site's style sheet): short, one line, nothing that could end the rule.
pub fn plausible_selector(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty() && s.len() <= 512 && !s.chars().any(|c| c.is_control() || matches!(c, '{' | '}' | '<' | '@' | ';' | '\\'))
}

/// May a link of a page be opened elsewhere by Sanaga (Ctrl+click, middle
/// click, "open in a private tab")? Web addresses only, or a local file from
/// a local page: never Sanaga's own pages, scripts or other schemes.
pub fn link_may_open_elsewhere(link: &str, page: &str) -> bool {
    let scheme = |u: &str| url::Url::parse(u).ok().map(|x| x.scheme().to_string()).unwrap_or_default();
    match scheme(link).as_str() {
        "http" | "https" => true,
        "file" => scheme(page) == "file",
        _ => false,
    }
}

// ---------------------------------------------------------------- .desktop entries

/// One argument of a .desktop file's Exec key, as the Desktop Entry
/// specification wants it: in double quotes, with a backslash before `"`,
/// `` ` ``, `$` and `\`, and `%` doubled (else it would be a field code);
/// then every backslash doubled again, because the whole value is also a
/// key-file string. Control characters (a line break would start a new
/// key) are dropped.
pub fn exec_arg(arg: &str) -> String {
    let mut quoted = String::from("\"");
    for c in arg.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            c if c.is_control() => {}
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted.replace('\\', "\\\\")
}

/// A plain value of a .desktop file (Name=, Comment=, Icon=): one line, its
/// backslashes escaped.
pub fn desktop_value(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect::<String>().replace('\\', "\\\\")
}

// ---------------------------------------------------------------- single-instance socket

/// Largest message another launch may send, and how many it may send.
pub const MAX_MESSAGE: usize = 64 * 1024;
pub const MAX_LINES: usize = 64;
const RATE_MAX: usize = 20;
const RATE_WINDOW: Duration = Duration::from_secs(10);

/// Connections read at the same time, and how long one may take to send
/// its message.
pub const MAX_READERS: usize = 8;
pub const READ_DEADLINE: Duration = Duration::from_millis(500);

/// Read one message from another launch: at most `max` bytes, and all of it
/// before `deadline` (a slow or silent peer is dropped). None: too big, too
/// slow, or an error.
pub fn read_bounded(stream: &std::os::unix::net::UnixStream, max: usize, deadline: Duration) -> Option<Vec<u8>> {
    use std::io::Read;
    let end = Instant::now() + deadline;
    let mut reader = stream;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let left = end.checked_duration_since(Instant::now()).filter(|d| !d.is_zero())?;
        reader.set_read_timeout(Some(left)).ok()?;
        match reader.read(&mut chunk) {
            Ok(0) => return Some(buf),
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > max {
                    return None;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
    }
}

/// At most RATE_MAX messages in RATE_WINDOW: true when this one may pass.
pub fn allow_message(times: &mut std::collections::VecDeque<Instant>, now: Instant) -> bool {
    while times.front().is_some_and(|t| now.duration_since(*t) > RATE_WINDOW) {
        times.pop_front();
    }
    if times.len() >= RATE_MAX {
        return false;
    }
    times.push_back(now);
    true
}

/// What another launch of Sanaga may hand over: web addresses (http,
/// https, and file: for local pages opened from the file manager), text
/// without a scheme (it becomes an https address or a search), and
/// Sanaga's own flags. Anything else (javascript:, data:, sanaga://,
/// view-source:…) is dropped.
pub fn accept_forwarded(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.len() > 8192 || line.chars().any(|c| c.is_control()) {
        return None;
    }
    if line == "nouvelle-fenetre:" || line == "apercu:" {
        return Some(line.to_string());
    }
    for flag in ["lecteur:", "preview:"] {
        if let Some(rest) = line.strip_prefix(flag) {
            return accept_forwarded(rest).filter(|r| !r.ends_with(':')).map(|r| format!("{flag}{r}"));
        }
    }
    match url::Url::parse(line) {
        Ok(u) if matches!(u.scheme(), "http" | "https" | "file") => Some(line.to_string()),
        // "localhost:8080/x", "example.org:8443": a host and port typed
        // without a scheme (the parser takes the host for a scheme)
        Ok(u) if (u.scheme() == "localhost" || u.scheme().contains('.')) && line.get(u.scheme().len() + 1..).is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit())) => Some(line.to_string()),
        Ok(_) => None,
        // no scheme ("example.com", "fleuve du Cameroun"): looked up as typed
        Err(_) => Some(line.to_string()),
    }
}

// ---------------------------------------------------------------- private folders and files

/// $XDG_RUNTIME_DIR, when it is a real folder of this user's, closed to
/// everyone else (the place for sockets and short-lived private files).
pub fn runtime_dir() -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let d = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
    let m = std::fs::symlink_metadata(&d).ok()?;
    // SAFETY: getuid has no preconditions and cannot fail.
    let me = unsafe { libc::getuid() };
    (d.is_absolute() && m.is_dir() && m.uid() == me && m.mode() & 0o077 == 0).then_some(d)
}

/// A new, empty folder only this user can enter (mode 0700), removed when
/// dropped: in $XDG_RUNTIME_DIR when there is one, else in the temp folder,
/// under a random name created atomically (like mkdtemp: never a folder
/// that already exists, never through a link).
pub struct PrivateDir(PathBuf);

impl PrivateDir {
    pub fn new(prefix: &str) -> std::io::Result<PrivateDir> {
        use std::hash::{BuildHasher, Hasher};
        use std::os::unix::fs::DirBuilderExt;
        // (the temp folder when the runtime folder can't be written to)
        let bases: Vec<PathBuf> = runtime_dir().into_iter().chain(std::iter::once(std::env::temp_dir())).collect();
        let mut last = std::io::Error::other("no free name for a private folder");
        for base in bases {
            for n in 0..16u32 {
                let mut h = std::collections::hash_map::RandomState::new().build_hasher();
                h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
                h.write_u32(std::process::id());
                h.write_u32(n);
                let path = base.join(format!("{prefix}-{:016x}", h.finish()));
                match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                    Ok(()) => return Ok(PrivateDir(path)),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => {
                        last = e;
                        break;
                    }
                }
            }
        }
        Err(last)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A folder of Sanaga's data (history, sessions, cookies…): created if
/// needed, and closed to other users (0700).
pub fn ensure_private_dir(dir: &Path) {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
    if let Ok(m) = std::fs::symlink_metadata(dir) {
        if m.is_dir() && m.permissions().mode() & 0o077 != 0 {
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
    }
}

/// The files of a data folder that earlier versions left readable by
/// others: closed (0600). Links are left alone.
pub fn close_files(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        // (the entry's own metadata: a link is not followed)
        let Ok(m) = e.metadata() else { continue };
        let mode = m.permissions().mode();
        if m.is_file() && mode & 0o077 != 0 {
            let _ = std::fs::set_permissions(e.path(), std::fs::Permissions::from_mode(mode & 0o700));
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_names_stay_in_the_folder() {
        // an encoded "/" or "\" can't choose the folder: only the last part is kept
        assert_eq!(safe_download_name("..%2F..%2F.config%2Fautostart%2Fevil.desktop"), "evil.desktop.download");
        assert_eq!(safe_download_name("a%2F..%2F..%2Fx.txt"), "x.txt");
        assert_eq!(safe_download_name("..\\..\\x.txt"), "x.txt");
        assert_eq!(safe_download_name("../../.local/share/applications/app.desktop"), "app.desktop.download");
        // no hidden file, no "..", never empty
        assert_eq!(safe_download_name(".bashrc"), "bashrc");
        assert_eq!(safe_download_name(".."), "telechargement");
        assert_eq!(safe_download_name("..."), "telechargement");
        assert_eq!(safe_download_name(""), "telechargement");
        assert_eq!(safe_download_name(" . report.pdf . "), "report.pdf");
    }

    #[test]
    fn download_names_lose_control_characters_and_runnable_extensions() {
        assert_eq!(safe_download_name("in\u{0}voice\n.pdf"), "invoice.pdf");
        assert_eq!(safe_download_name("in%00voice%0A.pdf"), "invoice.pdf");
        // a direction change can't disguise the extension
        assert_eq!(safe_download_name("photo\u{202E}gpj.exe"), "photo_gpj.exe");
        // the desktop never runs what was downloaded
        assert_eq!(safe_download_name("Setup.DESKTOP"), "Setup.DESKTOP.download");
        assert_eq!(safe_download_name("x.service"), "x.service.download");
        assert_eq!(safe_download_name("x.desktop."), "x.desktop.download");
        assert_eq!(safe_download_name("x.desktop%20"), "x.desktop.download");
        // ordinary names are kept as they are
        assert_eq!(safe_download_name("Rapport annuel 2026.pdf"), "Rapport annuel 2026.pdf");
        assert_eq!(safe_download_name("archive%20(1).tar.gz"), "archive (1).tar.gz");
        assert_eq!(safe_download_name("été.txt"), "été.txt");
    }

    #[test]
    fn long_download_names_are_cut_on_a_character() {
        let long = format!("{}.pdf", "é".repeat(300));
        let n = safe_download_name(&long);
        assert!(n.len() <= MAX_NAME);
        assert!(n.ends_with(".pdf"));
        let long = "x".repeat(400);
        assert_eq!(safe_download_name(&long).len(), MAX_NAME);
    }


    #[test]
    fn download_marks_hide_passwords() {
        assert_eq!(address_for_mark("https://user:secret@files.example/a.zip?x=1#top").as_deref(), Some("https://files.example/a.zip?x=1"));
        assert_eq!(address_for_mark("https://files.example/a.zip").as_deref(), Some("https://files.example/a.zip"));
        assert_eq!(address_for_mark("data:application/x-sh;base64,ZWNobyBoaQ==").as_deref(), Some("data:application/x-sh"));
        assert_eq!(address_for_mark("blob:https://site.example/1234").as_deref(), Some("blob:https://site.example/1234"));
        assert_eq!(address_for_mark("about:blank"), None);
        assert_eq!(address_for_mark(""), None);
        let long = format!("https://site.example/{}", "é".repeat(3000));
        assert!(address_for_mark(&long).is_some_and(|m| m.len() <= 2048));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn downloaded_files_carry_their_origin() {
        let dir = std::env::temp_dir().join(format!("sanaga-mark-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("setup.sh");
        std::fs::write(&f, b"echo hi").unwrap();
        let read = |name: &std::ffi::CStr| -> Option<String> {
            let path = std::ffi::CString::new(f.to_string_lossy().as_bytes()).unwrap();
            let mut buf = vec![0u8; 4096];
            // SAFETY: NUL-terminated strings and a buffer of buf.len() bytes.
            let n = unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), buf.as_mut_ptr().cast(), buf.len()) };
            (n >= 0).then(|| String::from_utf8_lossy(&buf[..n as usize]).to_string())
        };
        match mark_download(&f, "https://me:pw@dl.example/setup.sh#x", "https://site.example/page") {
            Ok(()) => {
                assert_eq!(read(c"user.xdg.origin.url").as_deref(), Some("https://dl.example/setup.sh"));
                assert_eq!(read(c"user.xdg.referrer.url").as_deref(), Some("https://site.example/page"));
            }
            // (a file system without user extended attributes)
            Err(e) => assert_eq!(e.raw_os_error(), Some(libc::EOPNOTSUPP), "{e}"),
        }
        // never through a link
        let link = dir.join("link.sh");
        std::os::unix::fs::symlink(&f, &link).unwrap();
        assert!(mark_download(&link, "https://dl.example/x", "").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn downloads_never_replace_a_file() {
        let dir = std::env::temp_dir().join(format!("sanaga-download-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique_path(&dir, "a.pdf"), dir.join("a.pdf"));
        std::fs::write(dir.join("a.pdf"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "a.pdf"), dir.join("a (2).pdf"));
        // a link pointing nowhere is taken too
        std::os::unix::fs::symlink(dir.join("nowhere"), dir.join("b.txt")).unwrap();
        assert_eq!(unique_path(&dir, "b.txt"), dir.join("b (2).txt"));
        // names reserved by downloads in progress
        let reserved = dir.join("c (2)");
        assert_eq!(unique_path_with(&dir, "c", |p| p == dir.join("c") || p == reserved), dir.join("c (3)"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn downloads_follow_safaris_rule() {
        // a click, one download: no question
        assert!(!download_needs_ask(true, false, false));
        // no click: asked
        assert!(download_needs_ask(false, false, false));
        // a second download from the same click: asked
        assert!(download_needs_ask(true, true, false));
        // a site the person allowed: never asked
        assert!(!download_needs_ask(false, false, true));
        assert!(!download_needs_ask(true, true, true));
    }

    #[test]
    fn messages_from_a_page_that_navigated_are_dropped() {
        assert_eq!(message_body("https://a.example/x\ntool:zap-done:#ad", "https://a.example/x"), Some("tool:zap-done:#ad"));
        assert_eq!(message_body("https://a.example/x#top\nform:dirty", "https://a.example/x#end"), Some("form:dirty"));
        assert_eq!(message_body("https://a.example/x\nform:dirty", "https://b.example/"), None);
        assert_eq!(message_body("https://a.example/x\nform:dirty", ""), None);
        assert_eq!(message_body("no address", "https://a.example/x"), None);
        // the body may hold line breaks of its own
        assert_eq!(message_body("about:blank\ntr:0\n[\"a\"]", "about:blank"), Some("tr:0\n[\"a\"]"));
    }

    #[test]
    fn shortcut_messages_are_recognised() {
        assert!(is_shortcut_message("cs:t"));
        assert!(is_shortcut_message(":f5"));
        assert!(is_shortcut_message("c:q"));
        assert!(!is_shortcut_message("tool:zap-done:x"));
        assert!(!is_shortcut_message("form:dirty"));
        assert!(!is_shortcut_message("sum:x"));
        assert!(!is_shortcut_message("c:"));
    }

    #[test]
    fn download_question_names_the_site() {
        assert_eq!(download_site("https://www.example.org/page", "https://cdn.example.net/f.zip"), "www.example.org");
        assert_eq!(download_site("", "https://cdn.example.net/f.zip"), "cdn.example.net");
        assert_eq!(download_site("about:blank", "data:text/plain,hi"), "data");
    }

    #[test]
    fn other_launches_hand_over_web_addresses_only() {
        assert_eq!(accept_forwarded("https://example.org/a?b=1").as_deref(), Some("https://example.org/a?b=1"));
        assert_eq!(accept_forwarded("http://example.org").as_deref(), Some("http://example.org"));
        assert_eq!(accept_forwarded("file:///home/me/page.html").as_deref(), Some("file:///home/me/page.html"));
        assert_eq!(accept_forwarded("example.org").as_deref(), Some("example.org"));
        assert_eq!(accept_forwarded("localhost:8080/x").as_deref(), Some("localhost:8080/x"));
        assert_eq!(accept_forwarded("fleuve du Cameroun").as_deref(), Some("fleuve du Cameroun"));
        assert_eq!(accept_forwarded("nouvelle-fenetre:").as_deref(), Some("nouvelle-fenetre:"));
        assert_eq!(accept_forwarded("apercu:").as_deref(), Some("apercu:"));
        assert_eq!(accept_forwarded("preview:https://example.org/").as_deref(), Some("preview:https://example.org/"));
        assert_eq!(accept_forwarded("lecteur:https://example.org/").as_deref(), Some("lecteur:https://example.org/"));
        for bad in ["javascript:alert(1)", "data:text/html,<script>x</script>", "sanaga://settings", "view-source:https://a.org", "about:config", "preview:javascript:x", "preview:apercu:", "profil:Travail", "https://a.org/\u{0}x", ""] {
            assert_eq!(accept_forwarded(bad), None, "{bad}");
        }
        assert_eq!(accept_forwarded(&format!("https://a.org/{}", "x".repeat(9000))), None);
    }

    #[test]
    fn slow_or_big_messages_are_dropped() {
        use std::io::Write;
        use std::os::unix::net::UnixStream;
        // a message sent at once
        let (mut a, b) = UnixStream::pair().unwrap();
        a.write_all(b"https://example.org/").unwrap();
        a.shutdown(std::net::Shutdown::Write).unwrap();
        assert_eq!(read_bounded(&b, 1024, Duration::from_millis(500)).as_deref(), Some(&b"https://example.org/"[..]));
        // a peer that says nothing: dropped at the deadline
        let (_a, b) = UnixStream::pair().unwrap();
        let t = Instant::now();
        assert_eq!(read_bounded(&b, 1024, Duration::from_millis(100)), None);
        assert!(t.elapsed() < Duration::from_secs(2));
        // too big
        let (mut a, b) = UnixStream::pair().unwrap();
        a.write_all(&[b'x'; 2048]).unwrap();
        a.shutdown(std::net::Shutdown::Write).unwrap();
        assert_eq!(read_bounded(&b, 1024, Duration::from_millis(500)), None);
    }

    #[test]
    fn links_opened_elsewhere_are_web_addresses() {
        assert!(link_may_open_elsewhere("https://a.example/x", "https://b.example/"));
        assert!(link_may_open_elsewhere("http://a.example/", "https://b.example/"));
        assert!(!link_may_open_elsewhere("javascript:alert(1)", "https://b.example/"));
        assert!(!link_may_open_elsewhere("sanaga://settings", "https://b.example/"));
        assert!(!link_may_open_elsewhere("file:///etc/passwd", "https://b.example/"));
        assert!(link_may_open_elsewhere("file:///home/me/b.html", "file:///home/me/a.html"));
        assert!(!link_may_open_elsewhere("data:text/html,x", "https://b.example/"));
        assert!(!link_may_open_elsewhere("", "https://b.example/"));
    }

    #[test]
    fn other_launches_are_rate_limited() {
        let mut times = std::collections::VecDeque::new();
        let now = Instant::now();
        for _ in 0..RATE_MAX {
            assert!(allow_message(&mut times, now));
        }
        assert!(!allow_message(&mut times, now));
        assert!(allow_message(&mut times, now + RATE_WINDOW + Duration::from_secs(1)));
    }

    /// How a desktop reads one quoted Exec argument back (key-file string
    /// first, then the quoting rule, then %%).
    fn read_exec_arg(v: &str) -> Option<String> {
        let mut s = String::new();
        let mut it = v.chars();
        while let Some(c) = it.next() {
            if c == '\\' {
                match it.next()? {
                    '\\' => s.push('\\'),
                    's' => s.push(' '),
                    o => {
                        s.push('\\');
                        s.push(o);
                    }
                }
            } else {
                s.push(c);
            }
        }
        let inner = s.strip_prefix('"')?.strip_suffix('"')?;
        let mut out = String::new();
        let mut it = inner.chars();
        while let Some(c) = it.next() {
            match c {
                '\\' => out.push(it.next()?),
                '"' | '`' | '$' => return None, // unescaped: would end or expand
                c => out.push(c),
            }
        }
        Some(out.replace("%%", "%"))
    }

    #[test]
    fn hostile_addresses_stay_one_exec_argument() {
        let hostile = "https://e.example/\"; rm -rf ~; echo \"$HOME`id`%f\\x%20y\nExec=evil";
        let q = exec_arg(hostile);
        assert!(!q.contains('\n'));
        assert_eq!(read_exec_arg(&q).as_deref(), Some("https://e.example/\"; rm -rf ~; echo \"$HOME`id`%f\\x%20yExec=evil"));
        assert_eq!(exec_arg("https://a.example/?q=1%202"), "\"https://a.example/?q=1%%202\"");
        assert_eq!(exec_arg("a$b"), "\"a\\\\$b\"");
        assert_eq!(desktop_value("Site\nExec=evil \\o/"), "SiteExec=evil \\\\o/");
    }

    #[test]
    fn element_remover_selectors_stay_selectors() {
        assert!(plausible_selector("body > div.ad:nth-of-type(2)"));
        assert!(plausible_selector("#banner"));
        assert!(!plausible_selector(""));
        assert!(!plausible_selector("x{} @import url(//evil)"));
        assert!(!plausible_selector("a;b"));
        assert!(!plausible_selector("a\nb"));
        assert!(!plausible_selector(&"a".repeat(600)));
    }

    #[test]
    fn private_folders_are_closed_and_new() {
        use std::os::unix::fs::PermissionsExt;
        let a = PrivateDir::new("sanaga-test").unwrap();
        let b = PrivateDir::new("sanaga-test").unwrap();
        assert_ne!(a.path(), b.path());
        let mode = std::fs::metadata(a.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        let kept = a.path().to_path_buf();
        drop(a);
        assert!(!kept.exists());
    }
}
