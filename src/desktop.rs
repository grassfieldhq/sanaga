//! Sanaga draws its own title bar (window buttons in the toolbar, like Safari).
//! This module tells the desktop not to add a second one, and lets the window
//! be resized from its edges.

use eframe::egui::{self, pos2, Rect};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static MOVING: AtomicBool = AtomicBool::new(false);
/// Sanaga's window is a native GTK window: moving and resizing it are asked
/// to the desktop directly (smooth, no polling).
static NATIVE: AtomicBool = AtomicBool::new(false);

pub fn set_native() {
    NATIVE.store(true, Ordering::SeqCst);
}

fn hypr_socket() -> Option<PathBuf> {
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
    [runtime.join("hypr").join(&sig).join(".socket.sock"), PathBuf::from("/tmp/hypr").join(&sig).join(".socket.sock")].into_iter().find(|p| p.exists())
}

/// One request to the Hyprland compositor (same protocol as `hyprctl`).
fn hypr(sock: &PathBuf, cmd: &str) -> Option<String> {
    let mut s = UnixStream::connect(sock).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(500))).ok()?;
    s.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

fn two_ints(s: &str) -> Option<(i64, i64)> {
    let mut it = s.split(|c: char| !(c.is_ascii_digit() || c == '-')).filter(|t| !t.is_empty()).map(|t| t.parse::<i64>());
    Some((it.next()?.ok()?, it.next()?.ok()?))
}

/// Position of this process's window, from the compositor's window list.
fn window_pos(sock: &PathBuf, pid: u32) -> Option<(i64, i64)> {
    let list = hypr(sock, "j/clients")?;
    let obj = list.split("\"address\":").find(|o| o.contains(&format!("\"pid\": {pid},")))?;
    let at = obj.split("\"at\":").nth(1)?;
    two_ints(at.split(']').next()?)
}

/// Start moving the window with the mouse (called when a drag begins on an
/// empty part of the toolbar or tab bar). Returns false when the desktop isn't
/// Hyprland; the caller then asks the window system to do it.
pub fn begin_move() -> bool {
    if NATIVE.load(Ordering::SeqCst) {
        return false;
    }
    let Some(sock) = hypr_socket() else { return false };
    if MOVING.swap(true, Ordering::SeqCst) {
        return true;
    }
    let pid = std::process::id();
    std::thread::Builder::new()
        .name("sanaga-move".into())
        .spawn(move || {
            let start_cursor = hypr(&sock, "cursorpos").and_then(|c| two_ints(&c));
            let start_win = window_pos(&sock, pid);
            let (Some((cx0, cy0)), Some((wx0, wy0))) = (start_cursor, start_win) else {
                MOVING.store(false, Ordering::SeqCst);
                return;
            };
            let mut last = (wx0, wy0);
            while MOVING.load(Ordering::SeqCst) {
                if let Some((cx, cy)) = hypr(&sock, "cursorpos").and_then(|c| two_ints(&c)) {
                    let target = (wx0 + cx - cx0, wy0 + cy - cy0);
                    if target != last {
                        let lua = format!("dispatch hl.dsp.window.move({{ x = {}, y = {}, window = 'pid:{pid}' }})", target.0, target.1);
                        if hypr(&sock, &lua).map(|r| r.trim() != "ok").unwrap_or(true) {
                            let _ = hypr(&sock, &format!("dispatch movewindowpixel exact {} {},pid:{pid}", target.0, target.1));
                        }
                        last = target;
                    }
                }
                std::thread::sleep(Duration::from_millis(8));
            }
        })
        .ok();
    true
}

/// Stop following the mouse (button released).
pub fn end_move() {
    MOVING.store(false, Ordering::SeqCst);
    RESIZING.store(false, Ordering::SeqCst);
}

/// On Hyprland desktops whose title-bar plugin honours the
/// `grassfield-client-chrome` tag (as for Chromium), tag Sanaga's window so the
/// desktop doesn't draw its own title bar on top of Sanaga's toolbar.
pub fn hide_desktop_titlebar() {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return;
    }
    let pid = std::process::id();
    std::thread::Builder::new()
        .name("sanaga-titlebar".into())
        .spawn(move || {
            // wait for the window to appear, then tag it (a few tries)
            for _ in 0..25 {
                std::thread::sleep(Duration::from_millis(300));
                let Ok(out) = Command::new("hyprctl").args(["clients", "-j"]).output() else { return };
                let text = String::from_utf8_lossy(&out.stdout);
                if !text.contains(&format!("\"pid\": {pid},")) {
                    continue;
                }
                let lua = format!("hl.dsp.window.tag({{ tag = '+grassfield-client-chrome', window = 'pid:{pid}' }})");
                let ok = Command::new("hyprctl").args(["dispatch", &lua]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim() == "ok").unwrap_or(false);
                if !ok {
                    // older, non-Lua Hyprland configurations
                    let _ = Command::new("hyprctl").args(["dispatch", "tagwindow", "+grassfield-client-chrome", &format!("pid:{pid}")]).output();
                }
                return;
            }
        })
        .ok();
}

static RESIZING: AtomicBool = AtomicBool::new(false);
static MAXIMIZED: AtomicBool = AtomicBool::new(false);
const MIN_W: i64 = 560;
const MIN_H: i64 = 380;

/// Position and size of this process's window.
fn window_geom(sock: &PathBuf, pid: u32) -> Option<((i64, i64), (i64, i64))> {
    let list = hypr(sock, "j/clients")?;
    let obj = list.split("\"address\":").find(|o| o.contains(&format!("\"pid\": {pid},")))?;
    let at = two_ints(obj.split("\"at\":").nth(1)?.split(']').next()?)?;
    let size = two_ints(obj.split("\"size\":").nth(1)?.split(']').next()?)?;
    Some((at, size))
}

/// Green button / double-click: fill the screen, or go back.
/// Full screen (F11): the page takes the whole screen.
pub fn toggle_fullscreen(ctx: &egui::Context, on: bool) {
    if let Some(sock) = hypr_socket() {
        let pid = std::process::id();
        let lua = format!("dispatch hl.dsp.window.fullscreen({{ mode = 'fullscreen', window = 'pid:{pid}' }})");
        if hypr(&sock, &lua).map(|r| r.trim() == "ok").unwrap_or(false) {
            return;
        }
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
}

pub fn toggle_maximize(ctx: &egui::Context) {
    let now = !MAXIMIZED.load(Ordering::SeqCst);
    MAXIMIZED.store(now, Ordering::SeqCst);
    if let Some(sock) = hypr_socket() {
        let pid = std::process::id();
        let lua = format!("dispatch hl.dsp.window.fullscreen({{ mode = 'maximized', window = 'pid:{pid}' }})");
        if hypr(&sock, &lua).map(|r| r.trim() == "ok").unwrap_or(false) {
            return;
        }
        if hypr(&sock, "dispatch fullscreen 1").map(|r| r.trim() == "ok").unwrap_or(false) {
            return;
        }
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(now));
}

/// Yellow button: put the window away.
pub fn minimize(ctx: &egui::Context) {
    if hypr_socket().is_some() && std::path::Path::new("/usr/share/grassfield/shell").exists() {
        // the desktop's own "minimize" (it acts on the active window: this one)
        let ok = Command::new("env")
            .args(["-u", "QS_CONFIG_PATH", "-u", "QS_CONFIG_NAME", "-u", "QS_MANIFEST", "qs", "ipc", "-p", "/usr/share/grassfield/shell", "call", "windows", "minimize"])
            .spawn()
            .is_ok();
        if ok {
            return;
        }
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
}

/// Follow the mouse while an edge is dragged. `sx`/`sy`: -1 left/top edge, 1 right/bottom, 0 untouched.
fn begin_resize(sx: i64, sy: i64) -> bool {
    if NATIVE.load(Ordering::SeqCst) {
        return false;
    }
    let Some(sock) = hypr_socket() else { return false };
    if RESIZING.swap(true, Ordering::SeqCst) {
        return true;
    }
    let pid = std::process::id();
    std::thread::Builder::new()
        .name("sanaga-resize".into())
        .spawn(move || {
            let start_cursor = hypr(&sock, "cursorpos").and_then(|c| two_ints(&c));
            let (Some((cx0, cy0)), Some(((x0, y0), (w0, h0)))) = (start_cursor, window_geom(&sock, pid)) else {
                RESIZING.store(false, Ordering::SeqCst);
                return;
            };
            let mut last = (x0, y0, w0, h0);
            while RESIZING.load(Ordering::SeqCst) {
                if let Some((cx, cy)) = hypr(&sock, "cursorpos").and_then(|c| two_ints(&c)) {
                    let w = (w0 + sx * (cx - cx0)).max(MIN_W);
                    let h = (h0 + sy * (cy - cy0)).max(MIN_H);
                    let x = if sx < 0 { x0 + w0 - w } else { x0 };
                    let y = if sy < 0 { y0 + h0 - h } else { y0 };
                    let next = (x, y, w, h);
                    if next != last {
                        if (w, h) != (last.2, last.3) {
                            let _ = hypr(&sock, &format!("dispatch hl.dsp.window.resize({{ x = {w}, y = {h}, exact = true, window = 'pid:{pid}' }})"));
                        }
                        let _ = hypr(&sock, &format!("dispatch hl.dsp.window.move({{ x = {x}, y = {y}, window = 'pid:{pid}' }})"));
                        last = next;
                    }
                }
                // ~60 updates a second: smooth without flooding the window with size changes
                std::thread::sleep(Duration::from_millis(16));
            }
        })
        .ok();
    true
}

/// True when the desktop itself resizes windows from their borders
/// (Hyprland's `resize_on_border`): its own resizing follows the mouse
/// directly and is smoother than Sanaga asking for each new size.
fn desktop_resizes_borders() -> bool {
    static ANSWER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ANSWER.get_or_init(|| {
        let Some(sock) = hypr_socket() else { return false };
        hypr(&sock, "j/getoption general:resize_on_border")
            .map(|o| {
                let o = o.replace([' ', '\n'], "");
                o.contains("\"bool\":true") || o.contains("\"int\":1")
            })
            .unwrap_or(false)
    })
}

/// Without a system title bar the window's edges resize it.
pub fn resize_edges(ctx: &egui::Context) {
    use egui::{CursorIcon as C, ResizeDirection as D};
    if MAXIMIZED.load(Ordering::SeqCst) || ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
        return;
    }
    if desktop_resizes_borders() {
        return;
    }
    let r = ctx.content_rect();
    let e = 6.0;
    let edges = [
        (Rect::from_min_max(r.left_top(), pos2(r.left() + e * 2.0, r.top() + e * 2.0)), D::NorthWest, C::ResizeNorthWest, (-1, -1)),
        (Rect::from_min_max(pos2(r.right() - e * 2.0, r.top()), pos2(r.right(), r.top() + e * 2.0)), D::NorthEast, C::ResizeNorthEast, (1, -1)),
        (Rect::from_min_max(pos2(r.left(), r.bottom() - e * 2.0), pos2(r.left() + e * 2.0, r.bottom())), D::SouthWest, C::ResizeSouthWest, (-1, 1)),
        (Rect::from_min_max(pos2(r.right() - e * 2.0, r.bottom() - e * 2.0), r.right_bottom()), D::SouthEast, C::ResizeSouthEast, (1, 1)),
        (Rect::from_min_max(r.left_top(), pos2(r.right(), r.top() + e)), D::North, C::ResizeNorth, (0, -1)),
        (Rect::from_min_max(pos2(r.left(), r.bottom() - e), r.right_bottom()), D::South, C::ResizeSouth, (0, 1)),
        (Rect::from_min_max(r.left_top(), pos2(r.left() + e, r.bottom())), D::West, C::ResizeWest, (-1, 0)),
        (Rect::from_min_max(pos2(r.right() - e, r.top()), r.right_bottom()), D::East, C::ResizeEast, (1, 0)),
    ];
    let Some(p) = ctx.pointer_hover_pos() else { return };
    for (rect, dir, cursor, (sx, sy)) in edges {
        if rect.contains(p) {
            ctx.set_cursor_icon(cursor);
            if ctx.input(|i| i.pointer.primary_pressed()) && !begin_resize(sx, sy) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
            }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_compositor_answers() {
        assert_eq!(two_ints("3850, 636"), Some((3850, 636)));
        assert_eq!(two_ints("-12, 40\n"), Some((-12, 40)));
        let list = r#"[{"address": "0x1", "at": [10, 20], "pid": 7, "x": 1}, {"address": "0x2", "at": [5060, -35], "size": [1280, 800], "pid": 42, "y": 2}]"#;
        let obj = list.split("\"address\":").find(|o| o.contains("\"pid\": 42,")).unwrap();
        assert_eq!(two_ints(obj.split("\"at\":").nth(1).unwrap().split(']').next().unwrap()), Some((5060, -35)));
    }

    #[test]
    fn live_window_position_if_on_hyprland() {
        // Only meaningful on a Hyprland desktop; elsewhere there's no socket.
        if let Some(sock) = hypr_socket() {
            assert!(hypr(&sock, "cursorpos").and_then(|c| two_ints(&c)).is_some());
        }
    }
}
