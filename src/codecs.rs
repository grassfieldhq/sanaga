//! Video and audio formats. WebKitGTK plays media through GStreamer: the
//! decoders come from the system's GStreamer packages. Without them, sites
//! such as YouTube say "your browser can't play this video" (they ask for
//! H.264, AV1, AAC or Opus). Sanaga checks which decoders are there, says
//! what is missing, and can open a terminal running the exact install
//! command it shows (the package manager asks for confirmation and the
//! password there). On a distribution that ships the decoders with Sanaga's
//! package (`SANAGA_DISTRO=grassfield`, at build time or in the
//! environment), Sanaga never offers to install anything.

use std::sync::{Mutex, OnceLock};

/// What the system can decode.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub h264: bool,
    pub aac: bool,
    pub av1: bool,
    pub vp9: bool,
    pub opus: bool,
    /// Hardware decoding (VA-API): smoother video, less battery.
    pub hardware: bool,
    /// The graphics card decodes AV1 / VP9 itself.
    pub hw_av1: bool,
    pub hw_vp9: bool,
}

impl Status {
    /// Everything the big video sites need.
    pub fn complete(&self) -> bool {
        self.h264 && self.aac && self.av1 && self.vp9 && self.opus
    }

    /// The formats missing, by name.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        for (ok, name) in [(self.h264, "H.264"), (self.aac, "AAC"), (self.av1, "AV1"), (self.vp9, "VP9"), (self.opus, "Opus")] {
            if !ok {
                v.push(name);
            }
        }
        v
    }
}

fn has(element: &str) -> bool {
    std::process::Command::new("gst-inspect-1.0").args(["--exists", element]).status().is_ok_and(|s| s.success())
}

fn any(elements: &[&str]) -> bool {
    elements.iter().any(|e| has(e))
}

static STATUS: OnceLock<Mutex<Option<Status>>> = OnceLock::new();

fn cell() -> &'static Mutex<Option<Status>> {
    STATUS.get_or_init(|| Mutex::new(None))
}

/// Running as a Flatpak: the runtime and its ffmpeg extension bring the codecs.
fn in_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// What the system can decode now (a few milliseconds per decoder).
fn detect() -> Status {
    if in_flatpak() {
        return Status { h264: true, aac: true, av1: true, vp9: true, opus: true, hardware: true, hw_av1: true, hw_vp9: true };
    }
    Status {
        h264: any(&["avdec_h264", "openh264dec", "vah264dec", "vaapih264dec", "nvh264dec"]),
        aac: any(&["avdec_aac", "fdkaacdec", "faad"]),
        av1: any(&["dav1ddec", "av1dec", "vaav1dec"]),
        vp9: any(&["vp9dec", "vavp9dec", "avdec_vp9"]),
        // (WebM / MSE audio also needs the Opus parser)
        opus: has("opusdec") && any(&["opusparse", "avdec_opus"]),
        hardware: any(&["vah264dec", "vaapih264dec", "nvh264dec"]),
        hw_av1: any(&["vaav1dec", "vaapiav1dec", "nvav1dec"]),
        hw_vp9: any(&["vavp9dec", "vaapivp9dec", "nvvp9dec"]),
    }
}

/// Look again at the system's decoders, now (on the calling thread).
pub fn check_now() -> Status {
    let st = detect();
    if let Ok(mut c) = cell().lock() {
        *c = Some(st.clone());
    }
    st
}

/// Look at the system's decoders (in the background).
pub fn check_in_background() {
    if in_flatpak() {
        if let Ok(mut c) = cell().lock() {
            *c = Some(Status { h264: true, aac: true, av1: true, vp9: true, opus: true, hardware: true, hw_av1: true, hw_vp9: true });
        }
        return;
    }
    std::thread::Builder::new()
        .name("sanaga-codecs".into())
        .spawn(|| {
            check_now();
        })
        .ok();
}

/// The distribution this Sanaga was packaged for, if it ships the decoders
/// itself (`SANAGA_DISTRO`, set when building the package or in the
/// environment; e.g. "grassfield"). Then Sanaga never installs anything.
pub fn distribution() -> Option<String> {
    let built = option_env!("SANAGA_DISTRO").unwrap_or("");
    let run = std::env::var("SANAGA_DISTRO").unwrap_or_default();
    distribution_from(built, &run)
}

fn distribution_from(built: &str, run: &str) -> Option<String> {
    [built.trim(), run.trim()].into_iter().find(|d| !d.is_empty()).map(|d| d.to_ascii_lowercase())
}

/// Video formats a site may only use when the graphics card decodes them
/// (WebKit's "content types requiring hardware support"; the site then picks
/// another format). Decoded by the processor, AV1 costs several times what
/// H.264 costs, and keeps a laptop's fans running: without an AV1 decoder in
/// the graphics card, sites get VP9 or H.264. `video_site` (YouTube, which
/// offers everything in H.264 too): VP9 as well when the card has no VP9
/// decoder. None: everything allowed (not known yet, or the Flatpak).
pub fn hardware_only_types(video_site: bool) -> Option<String> {
    let st = status()?;
    if in_flatpak() {
        return None;
    }
    let mut v = Vec::new();
    if !st.hw_av1 && (st.vp9 || st.h264) {
        v.push(r#"video/*; codecs="av01*""#);
    }
    if video_site && !st.hw_vp9 && st.h264 {
        v.push(r#"video/*; codecs="vp09*""#);
        v.push(r#"video/*; codecs="vp9*""#);
    }
    (!v.is_empty()).then(|| v.join(":"))
}

/// The result, once known.
pub fn status() -> Option<Status> {
    cell().lock().ok().and_then(|c| c.clone())
}

/// The system's package manager and the packages that bring the decoders.
pub struct Install {
    /// The exact command: shown to the person, and what runs (in a
    /// terminal, where the package manager asks for confirmation).
    pub command: String,
    pub argv: Vec<String>,
}

fn os_release() -> (String, String) {
    let t = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let get = |k: &str| t.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).map(|v| v.trim_matches('"').to_lowercase()).unwrap_or_default();
    (get("ID"), get("ID_LIKE"))
}

/// How to install the missing decoders on this system: None on an unknown
/// system, and always None when the distribution ships them with Sanaga.
pub fn install() -> Option<Install> {
    // (a distribution or a Flatpak that ships the decoders: never)
    if distribution().is_some() || in_flatpak() {
        return None;
    }
    let (id, like) = os_release();
    install_for(&id, &like)
}

/// The install command for a system (its os-release ID and ID_LIKE): the
/// package manager asks for confirmation itself (no --noconfirm, no -y).
fn install_for(id: &str, like: &str) -> Option<Install> {
    let is = |name: &str| id == name || like.split_whitespace().any(|l| l == name);
    let (manager, args, packages): (&str, &[&str], &[&str]) = if is("arch") {
        ("pacman", &["-S", "--needed"], &["gst-libav", "gst-plugins-bad", "gst-plugin-va", "libnice"])
    } else if is("debian") || is("ubuntu") {
        ("apt-get", &["install"], &["gstreamer1.0-libav", "gstreamer1.0-plugins-bad", "gstreamer1.0-vaapi", "gstreamer1.0-nice"])
    } else if is("fedora") {
        ("dnf", &["install"], &["gstreamer1-plugin-libav", "gstreamer1-plugins-bad-free", "gstreamer1-plugin-openh264", "libnice-gstreamer1"])
    } else if is("suse") || is("opensuse") {
        ("zypper", &["install"], &["gstreamer-plugins-libav", "gstreamer-plugins-bad"])
    } else {
        return None;
    };
    let mut argv: Vec<String> = vec![["su", "do"].concat(), manager.to_string()];
    argv.extend(args.iter().map(|a| a.to_string()));
    argv.extend(packages.iter().map(|p| p.to_string()));
    Some(Install { command: argv.join(" "), argv })
}

/// A terminal to run the install in: the desktop's standard launcher
/// (xdg-terminal-exec), else the Debian-style alternative. None: the
/// command can only be copied.
pub fn terminal() -> Option<Vec<String>> {
    let on_path = |p: &str| std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|d| d.join(p).is_file()));
    if on_path("xdg-terminal-exec") {
        Some(vec!["xdg-terminal-exec".into()])
    } else if on_path("x-terminal-emulator") {
        Some(vec!["x-terminal-emulator".into(), "-e".into()])
    } else {
        None
    }
}

/// Sites whose videos need the formats above (a bar is offered there).
pub fn is_video_site(host: &str) -> bool {
    const SITES: [&str; 14] = [
        "youtube.com", "youtu.be", "youtube-nocookie.com", "vimeo.com", "twitch.tv", "dailymotion.com", "facebook.com", "instagram.com",
        "tiktok.com", "x.com", "twitter.com", "reddit.com", "linkedin.com", "ted.com",
    ];
    let h = host.trim_start_matches("www.").trim_start_matches("m.");
    SITES.iter().any(|s| h == *s || h.ends_with(&format!(".{s}")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn video_sites_and_missing_formats() {
        assert!(super::is_video_site("www.youtube.com"));
        assert!(super::is_video_site("m.youtube.com"));
        assert!(!super::is_video_site("example.com"));
        let s = super::Status { vp9: true, ..Default::default() };
        assert_eq!(s.missing(), vec!["H.264", "AAC", "AV1", "Opus"]);
        assert!(!s.complete());
    }

    #[test]
    fn the_command_shown_is_the_command_run() {
        let i = super::install_for("arch", "").unwrap();
        assert_eq!(i.command, i.argv.join(" "));
        assert!(i.command.ends_with("pacman -S --needed gst-libav gst-plugins-bad gst-plugin-va libnice"));
        for (id, like) in [("arch", ""), ("ubuntu", "debian"), ("fedora", ""), ("opensuse-tumbleweed", "suse opensuse")] {
            let i = super::install_for(id, like).unwrap();
            assert!(!i.argv.iter().any(|a| a == "--noconfirm" || a == "-y" || a == "--non-interactive"), "{id}");
            assert_eq!(i.command, i.argv.join(" "));
        }
        assert!(super::install_for("plan9", "").is_none());
    }

    #[test]
    fn a_distribution_that_ships_codecs_turns_the_install_off() {
        assert_eq!(super::distribution_from("grassfield", ""), Some("grassfield".into()));
        assert_eq!(super::distribution_from("", "Grassfield"), Some("grassfield".into()));
        assert_eq!(super::distribution_from(" ", ""), None);
        assert_eq!(super::distribution_from("", ""), None);
    }
}
