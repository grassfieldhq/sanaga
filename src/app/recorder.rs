//! Video recording of the mobile simulator: the device (frame, browser bars
//! and page) is filmed 20 times a second and encoded to an MP4 file by ffmpeg,
//! in the background. When a new picture of the page is late, the previous
//! one is repeated, so the video keeps the real timing.

use super::devices::compose_capture;
use super::*;
use crate::frames;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use webkit2gtk::WebViewExt;

const FPS: f64 = 20.0;

pub(super) struct Recording {
    pub started: f64,
    next_tick: f64,
    /// Size of the video (set by the first picture).
    size: Rc<RefCell<Option<(i32, i32)>>>,
    /// The latest picture, ready for the video (BGRA rows, `size`).
    latest: Rc<RefCell<Option<Vec<u8>>>>,
    /// A picture of the page is being taken.
    busy: Rc<RefCell<bool>>,
    frames: Option<mpsc::SyncSender<Vec<u8>>>,
    done: mpsc::Receiver<Result<std::path::PathBuf, String>>,
    path: std::path::PathBuf,
    pub stopping: bool,
}

/// The video encoder, fed with raw pictures (runs in its own thread).
fn start_encoder(path: std::path::PathBuf, w: i32, h: i32, rx: mpsc::Receiver<Vec<u8>>, done: mpsc::Sender<Result<std::path::PathBuf, String>>) {
    std::thread::Builder::new()
        .name("sanaga-video".into())
        .spawn(move || {
            use std::io::Write;
            let child = std::process::Command::new("ffmpeg")
                .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "bgra", "-s", &format!("{w}x{h}"), "-r", &FPS.to_string(), "-i", "-"])
                .args(["-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2", "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
                .arg(&path)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn();
            let mut child = match child {
                Ok(c) => c,
                Err(_) => {
                    let _ = done.send(Err(tr("ffmpeg est nécessaire pour enregistrer des vidéos (paquet « ffmpeg »).").to_string()));
                    return;
                }
            };
            if let Some(mut stdin) = child.stdin.take() {
                for frame in rx {
                    if stdin.write_all(&frame).is_err() {
                        break;
                    }
                }
            }
            let out = child.wait_with_output();
            let _ = done.send(match out {
                Ok(o) if o.status.success() => Ok(path),
                Ok(o) => Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg").to_string()),
                Err(e) => Err(e.to_string()),
            });
        })
        .ok();
}

/// A composed picture as video rows (BGRA, `w` × `h`), fitted and centred on `bg`.
fn to_frame(img: &gtk::cairo::ImageSurface, w: i32, h: i32, bg: [u8; 3]) -> Option<Vec<u8>> {
    use gtk::cairo::{Context, Format, ImageSurface};
    let mut out = ImageSurface::create(Format::ARgb32, w, h).ok()?;
    {
        let cr = Context::new(&out).ok()?;
        cr.set_source_rgb(bg[0] as f64 / 255.0, bg[1] as f64 / 255.0, bg[2] as f64 / 255.0);
        let _ = cr.paint();
        let k = (w as f64 / img.width() as f64).min(h as f64 / img.height() as f64);
        cr.translate((w as f64 - img.width() as f64 * k) / 2.0, (h as f64 - img.height() as f64 * k) / 2.0);
        cr.scale(k, k);
        let _ = cr.set_source_surface(img, 0.0, 0.0);
        let _ = cr.paint();
    }
    out.flush();
    let stride = out.stride() as usize;
    let data = out.data().ok()?;
    let mut rows = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        rows.extend_from_slice(&data[y * stride..y * stride + w as usize * 4]);
    }
    Some(rows)
}

impl Sanaga {
    pub(super) fn toggle_recording(&mut self) {
        if let Some(r) = self.recording.as_mut() {
            // stop: the encoder finishes the file, then says so
            r.frames = None;
            r.stopping = true;
            return;
        }
        let Some(d) = self.device.as_ref() else { return };
        let stamp = crate::store::now();
        let path = crate::net::unique_path(&crate::net::download_dir(), &format!("Sanaga {} {}.mp4", d.label().replace(['/', '\\'], "-"), stamp));
        let (_done_tx, done) = mpsc::channel();
        let now = self.ctx.input(|i| i.time);
        self.recording = Some(Recording {
            started: now,
            next_tick: now,
            size: Rc::new(RefCell::new(None)),
            latest: Rc::new(RefCell::new(None)),
            busy: Rc::new(RefCell::new(false)),
            frames: None,
            done,
            path,
            stopping: false,
        });
        self.notice = Some((tr("Enregistrement vidéo commencé.").to_string(), now));
    }

    /// Each frame: take pictures of the page, feed the encoder in time, and
    /// tell when the video is ready.
    pub(super) fn recording_step(&mut self) {
        let Some(r) = self.recording.as_ref() else { return };
        let now = self.ctx.input(|i| i.time);
        // finished?
        if let Ok(res) = r.done.try_recv() {
            self.notice = Some((
                match res {
                    Ok(p) => tf("Vidéo enregistrée : {}", &[&p.display()]),
                    Err(e) => tf("Vidéo impossible : {}", &[&e]),
                },
                now,
            ));
            self.recording = None;
            return;
        }
        if r.stopping {
            if r.frames.is_none() && r.size.borrow().is_none() {
                // stopped before the first picture
                self.recording = None;
            } else {
                self.ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            return;
        }
        // a new picture of the page (one at a time)
        if !*r.busy.borrow() {
            self.record_snapshot();
        }
        let Some(r) = self.recording.as_mut() else { return };
        // start the encoder with the first picture's size
        if r.frames.is_none() {
            if let Some((w, h)) = *r.size.borrow() {
                let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(60);
                let (done_tx, done) = mpsc::channel();
                start_encoder(r.path.clone(), w, h, rx, done_tx);
                r.frames = Some(tx);
                r.done = done;
                r.next_tick = now;
            }
        }
        // one video frame per tick (the latest picture, repeated if needed)
        if let (Some(tx), Some(frame)) = (&r.frames, r.latest.borrow().as_ref()) {
            let mut n = 0;
            while now >= r.next_tick && n < 10 {
                let _ = tx.try_send(frame.clone());
                r.next_tick += 1.0 / FPS;
                n += 1;
            }
            if now >= r.next_tick {
                r.next_tick = now; // far behind (window hidden...): catch up
            }
        }
        self.ctx.request_repaint_after(std::time::Duration::from_millis((1000.0 / FPS) as u64 / 2));
    }

    fn record_snapshot(&mut self) {
        let Some(r) = self.recording.as_ref() else { return };
        let Some(d) = self.device.clone() else { return };
        let Some(w) = self.tabs.get(self.active).and_then(|t| t.web.as_ref()) else { return };
        let style = if d.frame { d.device().map(|x| x.style) } else { None };
        let custom = if d.frame { d.device().and_then(|dev| self.custom_frame(dev.name, d.landscape)) } else { None };
        let (screen, finish, landscape) = (d.screen(), d.finish, d.landscape);
        let time = super::devices::now_hm();
        let browser = d.browser();
        let (domain, title, secure) = self.device_page_info();
        let bg = super::scene::BACKDROPS[d.background.min(super::scene::BACKDROPS.len() - 1)].1;
        let (size, latest, busy) = (r.size.clone(), r.latest.clone(), r.busy.clone());
        *busy.borrow_mut() = true;
        let ctx = self.ctx.clone();
        w.webview().snapshot(webkit2gtk::SnapshotRegion::Visible, webkit2gtk::SnapshotOptions::NONE, None::<&webkit2gtk::gio::Cancellable>, move |res| {
            *busy.borrow_mut() = false;
            let Ok(page) = res else { return };
            let look = frames::Page { browser, domain: &domain, title: &title, secure };
            let Ok(img) = compose_capture(&page, style, custom.as_deref(), finish, screen, landscape, &time, &look) else { return };
            let (vw, vh) = *size.borrow_mut().get_or_insert((img.width() + img.width() % 2, img.height() + img.height() % 2));
            if let Some(frame) = to_frame(&img, vw, vh, bg) {
                *latest.borrow_mut() = Some(frame);
            }
            ctx.request_repaint();
        });
    }
}
