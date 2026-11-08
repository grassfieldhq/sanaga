//! Video and audio formats. WebKitGTK plays media through GStreamer: the
//! decoders come from the system's GStreamer packages. Without them, sites
//! such as YouTube say "your browser can't play this video" (they ask for
//! H.264, AV1, AAC or Opus). Sanaga checks which decoders are there, says
//! what is missing, and can install it (the system asks for the password).

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

/// Look at the system's decoders (in the background; a few milliseconds each).
pub fn check_in_background() {
    std::thread::Builder::new()
        .name("sanaga-codecs".into())
        .spawn(|| {
            let st = Status {
                h264: any(&["avdec_h264", "openh264dec", "vah264dec", "vaapih264dec", "nvh264dec"]),
                aac: any(&["avdec_aac", "fdkaacdec", "faad"]),
                av1: any(&["dav1ddec", "av1dec", "vaav1dec"]),
                vp9: any(&["vp9dec", "vavp9dec", "avdec_vp9"]),
                // (WebM / MSE audio also needs the Opus parser)
                opus: has("opusdec") && any(&["opusparse", "avdec_opus"]),
                hardware: any(&["vah264dec", "vaapih264dec", "nvh264dec"]),
            };
            if let Ok(mut c) = cell().lock() {
                *c = Some(st);
            }
        })
        .ok();
}

/// The result, once known.
pub fn status() -> Option<Status> {
    cell().lock().ok().and_then(|c| c.clone())
}

/// The system's package manager and the packages that bring the decoders.
pub struct Install {
    /// Shown to the person (what to type in a terminal).
    pub command: String,
    /// Run with the system's password prompt (pkexec).
    pub program: String,
    pub args: Vec<String>,
}

fn os_release() -> (String, String) {
    let t = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let get = |k: &str| t.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).map(|v| v.trim_matches('"').to_lowercase()).unwrap_or_default();
    (get("ID"), get("ID_LIKE"))
}

/// How to install the missing decoders on this system (None: unknown system).
pub fn install() -> Option<Install> {
    let (id, like) = os_release();
    let is = |name: &str| id == name || like.split_whitespace().any(|l| l == name);
    let (manager, args, packages): (&str, &[&str], &[&str]) = if is("arch") {
        ("pacman", &["-S", "--needed", "--noconfirm"], &["gst-libav", "gst-plugins-bad", "gst-plugin-va"])
    } else if is("debian") || is("ubuntu") {
        ("apt-get", &["install", "-y"], &["gstreamer1.0-libav", "gstreamer1.0-plugins-bad", "gstreamer1.0-vaapi"])
    } else if is("fedora") {
        ("dnf", &["install", "-y"], &["gstreamer1-plugin-libav", "gstreamer1-plugins-bad-free", "gstreamer1-plugin-openh264"])
    } else if is("suse") || is("opensuse") {
        ("zypper", &["--non-interactive", "install"], &["gstreamer-plugins-libav", "gstreamer-plugins-bad"])
    } else {
        return None;
    };
    let shown_args: Vec<&str> = args.iter().copied().filter(|a| *a != "--noconfirm" && *a != "-y" && *a != "--non-interactive").collect();
    let command = format!("{} {} {} {}", ["su", "do"].concat(), manager, shown_args.join(" "), packages.join(" "));
    let mut all: Vec<String> = vec![manager.to_string()];
    all.extend(args.iter().map(|a| a.to_string()));
    all.extend(packages.iter().map(|p| p.to_string()));
    Some(Install { command, program: "pkexec".into(), args: all })
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
}
