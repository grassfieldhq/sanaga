//! Sanaga's window: one native GTK window (Wayland or X11).
//!
//! Sanaga's interface (tabs, toolbar, menus, internal pages) is drawn with the
//! graphics card in a GL area filling the window, and each web page is a normal
//! GTK part of the same window, placed over the page area. GTK lays out and
//! draws both together, in the same frame: when the window is resized the page
//! follows exactly, with no black strips and no lag between the two.

use crate::app::Sanaga;
use eframe::egui::{self, pos2, vec2, Event, Modifiers, PointerButton, Pos2, Rect, ViewportCommand, ViewportId};
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Host {
    overlay: gtk::Overlay,
    base: gtk::EventBox,
    area: gtk::GLArea,
    window: gtk::Window,
    insets: Cell<[i32; 4]>,
    /// Arrow shown while swiping back / forward over a page.
    swipe_hint: gtk::Label,
    /// The mobile simulator's drawn device, above the page (clicks go through).
    device_frame: gtk::Box,
    device_pic: RefCell<Option<gtk::cairo::ImageSurface>>,
    /// Back (-1) or forward (1) asked by a gesture or a mouse button.
    nav: Cell<i32>,
    shot: RefCell<Option<String>>,
    tip_shown: RefCell<Option<String>>,
}

thread_local! {
    static HOST: RefCell<Option<Rc<Host>>> = const { RefCell::new(None) };
    /// Hover help asked during this frame.
    static TOOLTIP: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Input made up by tests (clicks...).
    static INJECT: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static INJECT_NOW: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
}

pub fn set_tooltip(text: String) {
    TOOLTIP.with(|t| *t.borrow_mut() = Some(text));
}

/// Test aid: a mouse click delivered through GTK itself (as a real one is).
pub fn gtk_click(x: f64, y: f64, button: u32) {
    use gtk::glib::translate::{IntoGlib, ToGlibPtr};
    let Some(h) = host() else { return };
    let Some(win) = h.base.window() else { return };
    let Some(dev) = gdk::Display::default().and_then(|d| d.default_seat()).and_then(|s| s.pointer()) else { return };
    for (kind, delay) in [(gdk::EventType::ButtonPress, 0u64), (gdk::EventType::ButtonRelease, 90)] {
        let (win, dev) = (win.clone(), dev.clone());
        glib::timeout_add_local_once(Duration::from_millis(delay), move || unsafe {
            let ev = gdk::ffi::gdk_event_new(kind.into_glib());
            let b = &mut (*ev).button;
            b.window = win.to_glib_full();
            b.send_event = 1;
            b.time = 0;
            b.x = x;
            b.y = y;
            b.button = button;
            b.state = if kind == gdk::EventType::ButtonRelease && button == 3 { gdk::ffi::GDK_BUTTON3_MASK } else { 0 };
            let (_, ox, oy) = win.origin();
            b.x_root = ox as f64 + x;
            b.y_root = oy as f64 + y;
            gdk::ffi::gdk_event_set_device(ev, dev.to_glib_none().0);
            gtk::ffi::gtk_main_do_event(ev);
            gdk::ffi::gdk_event_free(ev);
        });
    }
}

/// Test aid: like `inject`, optionally all in one frame.
pub fn inject_together(together: bool, events: Vec<Event>) {
    if together {
        INJECT_NOW.with(|i| i.borrow_mut().extend(events));
        if let Some(h) = host() {
            h.area.queue_render();
        }
    } else {
        inject(events);
    }
}

/// Test aid: input events added to the next frames.
pub fn inject(events: Vec<Event>) {
    INJECT.with(|i| i.borrow_mut().extend(events));
    if let Some(h) = host() {
        h.area.queue_render();
    }
}

fn host() -> Option<Rc<Host>> {
    HOST.with(|h| h.borrow().clone())
}

/// Is Sanaga running in its GTK window (so web pages can be shown)?
pub fn active() -> bool {
    host().is_some()
}

fn apply_insets(w: &impl IsA<gtk::Widget>, [l, t, r, b]: [i32; 4]) {
    w.set_margin_start(l);
    w.set_margin_top(t);
    w.set_margin_end(r);
    w.set_margin_bottom(b);
}

/// Put a web page into the window, over the page area.
pub fn attach(view: &webkit2gtk::WebView) {
    let Some(h) = host() else { return };
    view.set_halign(gtk::Align::Fill);
    view.set_valign(gtk::Align::Fill);
    apply_insets(view, h.insets.get());
    h.overlay.add_overlay(view);
    h.overlay.reorder_overlay(&h.device_frame, -1);
    h.overlay.reorder_overlay(&h.swipe_hint, -1);
    raise_overlays(&h);
    watch_gestures(view);
}

/// The simulator's frame and the swipe arrow stay above the pages (a page
/// added later gets its own window on top otherwise).
fn raise_overlays(h: &Host) {
    for w in [h.device_frame.upcast_ref::<gtk::Widget>(), h.swipe_hint.upcast_ref::<gtk::Widget>()] {
        if let Some(win) = w.window() {
            if Some(&win) != h.overlay.window().as_ref() {
                win.raise();
            }
        }
    }
}

/// Show the simulator's device picture at `pos` (window coordinates), or hide it.
pub fn set_device_frame(pic: Option<([i32; 2], gtk::cairo::ImageSurface)>) {
    let Some(h) = host() else { return };
    let a = &h.device_frame;
    match pic {
        Some(([x, y], surf)) => {
            let scale = a.scale_factor().max(1);
            let (w, hh) = (surf.width() / scale, surf.height() / scale);
            if std::env::var_os("SANAGA_DEBUG_FRAME").is_some() {
                eprintln!("device frame at {x},{y} size {w}x{hh}");
            }
            let same = h.device_pic.borrow().as_ref().is_some_and(|s| s.to_raw_none() == surf.to_raw_none());
            if a.margin_start() != x || a.margin_top() != y || a.width_request() != w || a.height_request() != hh {
                a.set_margin_start(x);
                a.set_margin_top(y);
                a.set_size_request(w, hh);
            }
            if !same {
                *h.device_pic.borrow_mut() = Some(surf);
                a.queue_draw();
            }
            if !a.is_visible() {
                a.show();
                raise_overlays(&h);
            }
        }
        None => {
            if a.is_visible() {
                a.hide();
            }
            h.device_pic.borrow_mut().take();
        }
    }
}

/// Test aid: where each part of the window really is.
pub fn debug_allocations() {
    let Some(h) = host() else { return };
    for c in h.overlay.children() {
        let a = c.allocation();
        eprintln!(
            "alloc {} visible={} x={} y={} w={} h={} margins l={} t={} r={} b={} min_h={:?}",
            c.type_().name(),
            c.is_visible(),
            a.x(),
            a.y(),
            a.width(),
            a.height(),
            c.margin_start(),
            c.margin_top(),
            c.margin_end(),
            c.margin_bottom(),
            c.preferred_height()
        );
    }
}

/// A gesture or a mouse button asked to go back (-1) or forward (1).
pub fn take_nav() -> i32 {
    host().map(|h| h.nav.replace(0)).unwrap_or(0)
}

fn ask_nav(dir: i32) {
    if let Some(h) = host() {
        h.nav.set(dir);
        h.area.queue_render();
    }
}

/// Two fingers sliding sideways on a touchpad go back / forward (Chrome,
/// Firefox); the mouse's side buttons too.
fn watch_gestures(view: &webkit2gtk::WebView) {
    view.connect_button_press_event(|_, ev| {
        match ev.button() {
            8 => ask_nav(-1),
            9 => ask_nav(1),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    });
    // (sum sideways, sum up-down, decided: Some(true) = a sideways swipe)
    let state: Rc<Cell<(f64, f64, Option<bool>)>> = Rc::new(Cell::new((0.0, 0.0, None)));
    view.connect_scroll_event(move |_, ev| {
        let touchpad = ev.source_device().map(|d| d.source() == gdk::InputSource::Touchpad).unwrap_or(false);
        if !touchpad || ev.direction() != gdk::ScrollDirection::Smooth {
            return glib::Propagation::Proceed;
        }
        let (mut sx, mut sy, mut decided) = state.get();
        const GO: f64 = 22.0;
        if ev.is_stop() {
            if decided == Some(true) && sx.abs() >= GO {
                ask_nav(if sx < 0.0 { -1 } else { 1 });
            }
            state.set((0.0, 0.0, None));
            show_swipe(0, 0.0);
            return glib::Propagation::Proceed;
        }
        let (dx, dy) = ev.delta();
        sx += dx;
        sy += dy;
        if decided.is_none() && sx.abs() + sy.abs() > 3.0 {
            decided = Some(sx.abs() > 2.5 * sy.abs());
        }
        state.set((sx, sy, decided));
        if decided == Some(true) {
            show_swipe(if sx < 0.0 { -1 } else { 1 }, (sx.abs() / GO).min(1.0));
        }
        glib::Propagation::Proceed
    });
}

/// Test aid: the swipe arrow, and the navigation a finished swipe asks for.
pub fn test_swipe(dir: i32, progress: f64, finish: bool) {
    show_swipe(dir, progress);
    if finish {
        show_swipe(0, 0.0);
        ask_nav(dir);
    }
}

/// The arrow at the page's edge while swiping (`dir` 0 hides it).
fn show_swipe(dir: i32, progress: f64) {
    let Some(h) = host() else { return };
    let l = &h.swipe_hint;
    if dir == 0 || progress < 0.15 {
        l.hide();
        return;
    }
    let [left, _, right, _] = h.insets.get();
    l.set_text(if dir < 0 { "←" } else { "→" });
    l.set_halign(if dir < 0 { gtk::Align::Start } else { gtk::Align::End });
    let slide = (progress * 40.0) as i32;
    l.set_margin_start(left + 8 + if dir < 0 { slide } else { 0 });
    l.set_margin_end(right + 8 + if dir > 0 { slide } else { 0 });
    l.set_opacity(0.35 + 0.65 * progress);
    let ctx = l.style_context();
    if progress >= 1.0 {
        ctx.add_class("ready");
    } else {
        ctx.remove_class("ready");
    }
    l.show();
}

/// The page area, as distances from the window's left, top, right and bottom
/// edges. Pages keep these distances by themselves when the window changes size.
pub fn set_page_area(insets: [i32; 4]) {
    let Some(h) = host() else { return };
    if h.insets.get() == insets {
        return;
    }
    h.insets.set(insets);
    for c in h.overlay.children() {
        if c != *h.base.upcast_ref::<gtk::Widget>() && c != *h.swipe_hint.upcast_ref::<gtk::Widget>() && c != *h.device_frame.upcast_ref::<gtk::Widget>() {
            apply_insets(&c, insets);
        }
    }
}

/// Save a picture of the whole window (test aid).
pub fn request_shot(path: String) {
    if let Some(h) = host() {
        *h.shot.borrow_mut() = Some(path);
        h.area.queue_render();
    }
}

fn save_shot(window: &gtk::Window, path: &str) {
    let (w, h) = (window.allocated_width(), window.allocated_height());
    let Ok(surf) = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w.max(1), h.max(1)) else { return };
    if let Ok(cr) = gtk::cairo::Context::new(&surf) {
        window.draw(&cr);
    }
    if let Ok(mut f) = std::fs::File::create(path) {
        let _ = surf.write_to_png(&mut f);
        eprintln!("live shot {path}");
    }
}

/// What the input handlers collect between two frames.
#[derive(Default)]
struct Input {
    events: Vec<Event>,
    modifiers: Modifiers,
    sent_modifiers: Modifiers,
    focused: bool,
    maximized: bool,
    fullscreen: bool,
    /// Last mouse press: button, position on screen, time (to start a window move).
    press: (u32, f64, f64, u32),
    /// The keyboard went from Sanaga's interface to a page.
    chrome_lost_focus: bool,
}

struct Gui {
    app: Sanaga,
    ctx: egui::Context,
    gl: Option<Arc<glow::Context>>,
    painter: Option<egui_glow::Painter>,
    start: Instant,
    cursor: egui::CursorIcon,
    text_focus: bool,
}

fn modifiers_of(state: gdk::ModifierType) -> Modifiers {
    let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
    Modifiers {
        alt: state.contains(gdk::ModifierType::MOD1_MASK),
        ctrl,
        shift: state.contains(gdk::ModifierType::SHIFT_MASK),
        mac_cmd: false,
        command: ctrl,
    }
}

fn key_of(kv: gdk::keys::Key) -> Option<egui::Key> {
    let name = kv.to_lower().name()?;
    let n = match name.as_str() {
        "Return" | "KP_Enter" | "ISO_Enter" => "Enter",
        "BackSpace" => "Backspace",
        "Tab" | "ISO_Left_Tab" | "KP_Tab" => "Tab",
        "Left" | "KP_Left" => "ArrowLeft",
        "Right" | "KP_Right" => "ArrowRight",
        "Up" | "KP_Up" => "ArrowUp",
        "Down" | "KP_Down" => "ArrowDown",
        "Page_Up" | "KP_Page_Up" => "PageUp",
        "Page_Down" | "KP_Page_Down" => "PageDown",
        "KP_Home" => "Home",
        "KP_End" => "End",
        "KP_Delete" => "Delete",
        "KP_Insert" => "Insert",
        "space" | "KP_Space" => "Space",
        "minus" | "KP_Subtract" => "Minus",
        "plus" | "KP_Add" => "Plus",
        "equal" | "KP_Equal" => "Equals",
        "comma" => "Comma",
        "period" | "KP_Decimal" => "Period",
        "slash" | "KP_Divide" => "Slash",
        "backslash" => "Backslash",
        "semicolon" => "Semicolon",
        "colon" => "Colon",
        "bracketleft" => "OpenBracket",
        "bracketright" => "CloseBracket",
        "apostrophe" => "Quote",
        "grave" => "Backtick",
        "bar" => "Pipe",
        "question" => "Questionmark",
        "exclam" => "Exclamationmark",
        other => match other.strip_prefix("KP_") {
            Some(d) if d.len() == 1 => return egui::Key::from_name(d),
            _ => other,
        },
    };
    egui::Key::from_name(n)
}

/// Turns names like `eglGetProcAddress` into GL function addresses.
fn gl_loader() -> Result<impl Fn(&str) -> *const std::ffi::c_void, String> {
    type GetProc = unsafe extern "C" fn(*const std::ffi::c_char) -> *const std::ffi::c_void;
    let wayland = gdk::Display::default().map(|d| d.type_().name().contains("Wayland")).unwrap_or(false);
    let open = |names: &[&str]| names.iter().find_map(|n| unsafe { libloading::Library::new(n).ok() });
    let (lib, sym): (Option<libloading::Library>, &[u8]) =
        if wayland { (open(&["libEGL.so.1", "libEGL.so"]), b"eglGetProcAddress\0") } else { (open(&["libGL.so.1", "libGL.so"]), b"glXGetProcAddressARB\0") };
    let lib: &'static libloading::Library = Box::leak(Box::new(lib.ok_or("bibliothèque OpenGL introuvable")?));
    let get: GetProc = unsafe { *lib.get::<GetProc>(sym).map_err(|e| e.to_string())? };
    let direct: Option<&'static libloading::Library> = open(&["libGL.so.1", "libOpenGL.so.0", "libGLESv2.so.2"]).map(|l| &*Box::leak(Box::new(l)));
    Ok(move |name: &str| {
        let c = std::ffi::CString::new(name).unwrap_or_default();
        let p = unsafe { get(c.as_ptr()) };
        if !p.is_null() {
            return p;
        }
        direct.and_then(|l| unsafe { l.get::<*const std::ffi::c_void>(c.as_bytes_with_nul()).ok().map(|s| *s) }).unwrap_or(std::ptr::null())
    })
}

/// Ask for the next frame in `delay` (keeps the earliest request).
fn schedule(area: &gtk::GLArea, next: &Rc<Cell<Option<Instant>>>, delay: Duration) {
    if delay.is_zero() {
        area.queue_render();
        return;
    }
    if delay > Duration::from_secs(3600) {
        return;
    }
    let at = Instant::now() + delay;
    if next.get().is_some_and(|n| n <= at) {
        return;
    }
    next.set(Some(at));
    let (area, next) = (area.clone(), next.clone());
    glib::timeout_add_local_once(delay, move || {
        if next.get() == Some(at) {
            next.set(None);
            area.queue_render();
        }
    });
}

pub fn run(urls: Vec<String>) -> Result<(), String> {
    glib::set_prgname(Some("sanaga"));
    glib::set_application_name("Sanaga");
    gtk::init().map_err(|e| e.to_string())?;
    crate::desktop::set_native();

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Sanaga");
    window.set_default_size(1280, 800);
    window.set_size_request(560, 380);
    window.set_decorated(false);
    {
        let icon = crate::icons::window_icon(64);
        let bytes = glib::Bytes::from_owned(icon.rgba.clone());
        let pix = gtk::gdk_pixbuf::Pixbuf::from_bytes(&bytes, gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, icon.width as i32, icon.height as i32, icon.width as i32 * 4);
        window.set_icon(Some(&pix));
    }
    let overlay = gtk::Overlay::new();
    let base = gtk::EventBox::new();
    let area = gtk::GLArea::new();
    area.set_has_alpha(false);
    area.set_auto_render(false);
    base.add(&area);
    base.set_can_focus(true);
    base.add_events(
        gdk::EventMask::POINTER_MOTION_MASK
            | gdk::EventMask::BUTTON_PRESS_MASK
            | gdk::EventMask::BUTTON_RELEASE_MASK
            | gdk::EventMask::SCROLL_MASK
            | gdk::EventMask::SMOOTH_SCROLL_MASK
            | gdk::EventMask::KEY_PRESS_MASK
            | gdk::EventMask::KEY_RELEASE_MASK
            | gdk::EventMask::LEAVE_NOTIFY_MASK
            | gdk::EventMask::FOCUS_CHANGE_MASK,
    );
    overlay.add(&base);
    window.add(&overlay);

    // the swipe arrow (Chrome-like round button at the page's edge)
    let swipe_hint = gtk::Label::new(Some("←"));
    swipe_hint.set_widget_name("sanaga-swipe");
    swipe_hint.set_valign(gtk::Align::Center);
    swipe_hint.set_no_show_all(true);
    {
        let css = gtk::CssProvider::new();
        let _ = css.load_from_data(
            b"#sanaga-swipe { background: rgba(255,255,255,0.97); color: #1a73e8; border-radius: 999px; padding: 6px 15px 9px 15px; font-size: 24px; font-weight: bold; box-shadow: 0 2px 8px rgba(0,0,0,0.25); }
              #sanaga-swipe.ready { background: #1a73e8; color: white; }",
        );
        swipe_hint.style_context().add_provider(&css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
    // no window of its own: clicks and scrolling go through it to the page
    let device_frame = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    device_frame.set_app_paintable(true);
    device_frame.set_halign(gtk::Align::Start);
    device_frame.set_valign(gtk::Align::Start);
    device_frame.set_no_show_all(true);
    overlay.add_overlay(&device_frame);
    overlay.set_overlay_pass_through(&device_frame, true);
    overlay.add_overlay(&swipe_hint);
    let host_rc = Rc::new(Host {
        overlay: overlay.clone(),
        base: base.clone(),
        area: area.clone(),
        window: window.clone(),
        insets: Cell::new([0, 88, 0, 0]),
        swipe_hint,
        device_frame: device_frame.clone(),
        device_pic: RefCell::new(None),
        nav: Cell::new(0),
        shot: RefCell::new(None),
        tip_shown: RefCell::new(None),
    });
    HOST.with(|h| *h.borrow_mut() = Some(host_rc.clone()));
    {
        let h = host_rc.clone();
        device_frame.connect_draw(move |a, cr| {
            if std::env::var_os("SANAGA_DEBUG_FRAME").is_some() {
                eprintln!("device frame drawn (picture: {})", h.device_pic.borrow().is_some());
            }
            if let Some(s) = h.device_pic.borrow().as_ref() {
                let scale = a.scale_factor().max(1) as f64;
                cr.scale(1.0 / scale, 1.0 / scale);
                let _ = cr.set_source_surface(s, 0.0, 0.0);
                let _ = cr.paint();
            }
            glib::Propagation::Stop
        });
    }

    let ctx = egui::Context::default();
    let app = Sanaga::new(&ctx, urls, None);
    let gui = Rc::new(RefCell::new(Gui { app, ctx: ctx.clone(), gl: None, painter: None, start: Instant::now(), cursor: egui::CursorIcon::Default, text_focus: false }));
    let input = Rc::new(RefCell::new(Input { focused: true, ..Default::default() }));
    let next = Rc::new(Cell::new(None::<Instant>));

    // repaint requests (also from other threads: network, favicons...)
    #[allow(deprecated)]
    let (tx, rx) = glib::MainContext::channel::<Duration>(glib::Priority::DEFAULT);
    ctx.set_request_repaint_callback(move |info| {
        let _ = tx.send(info.delay);
    });
    {
        let (area, next) = (area.clone(), next.clone());
        #[allow(deprecated)]
        rx.attach(None, move |delay| {
            schedule(&area, &next, delay);
            glib::ControlFlow::Continue
        });
    }

    // text input (accents, compose key, input methods)
    let im = gtk::IMMulticontext::new();
    {
        let (input, area) = (input.clone(), area.clone());
        im.connect_commit(move |_, text| {
            input.borrow_mut().events.push(Event::Text(text.to_string()));
            area.queue_render();
        });
    }
    {
        let im = im.clone();
        base.connect_realize(move |b| im.set_client_window(b.window().as_ref()));
    }

    // ---- GL ----
    {
        let gui = gui.clone();
        area.connect_realize(move |area| {
            if std::env::var_os("SANAGA_DEBUG_GL").is_some() {
                eprintln!("GL area realize");
            }
            area.make_current();
            if let Some(e) = area.error() {
                eprintln!("OpenGL indisponible : {e}");
                return;
            }
            let loader = match gl_loader() {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("OpenGL indisponible : {e}");
                    return;
                }
            };
            let gl = Arc::new(unsafe { glow::Context::from_loader_function(loader) });
            if std::env::var_os("SANAGA_DEBUG_GL").is_some() {
                use glow::HasContext;
                unsafe {
                    eprintln!("GL {} | {} | GLSL {} | es={}", gl.get_parameter_string(glow::VERSION), gl.get_parameter_string(glow::RENDERER), gl.get_parameter_string(glow::SHADING_LANGUAGE_VERSION), area.uses_es());
                }
            }
            match egui_glow::Painter::new(gl.clone(), "", None, true) {
                Ok(p) => {
                    let mut g = gui.borrow_mut();
                    g.gl = Some(gl);
                    g.painter = Some(p);
                }
                Err(e) => eprintln!("OpenGL indisponible : {e}"),
            }
        });
    }
    {
        let gui = gui.clone();
        area.connect_unrealize(move |area| {
            if std::env::var_os("SANAGA_DEBUG_GL").is_some() {
                eprintln!("GL area unrealize");
            }
            area.make_current();
            if let Some(mut p) = gui.borrow_mut().painter.take() {
                p.destroy();
            }
        });
    }
    {
        let (gui, input, next, host_rc, im) = (gui.clone(), input.clone(), next.clone(), host_rc.clone(), im.clone());
        area.connect_render(move |area, _| {
            let Ok(mut g) = gui.try_borrow_mut() else { return glib::Propagation::Stop };
            paint(&mut g, area, &input, &next, &host_rc, &im);
            glib::Propagation::Stop
        });
    }

    // ---- input ----
    {
        let (input, area) = (input.clone(), area.clone());
        base.connect_motion_notify_event(move |_, ev| {
            let (x, y) = ev.position();
            let mut i = input.borrow_mut();
            i.modifiers = modifiers_of(ev.state());
            i.events.push(Event::PointerMoved(pos2(x as f32, y as f32)));
            area.queue_render();
            glib::Propagation::Proceed
        });
    }
    let button_of = |b: u32| match b {
        1 => Some(PointerButton::Primary),
        2 => Some(PointerButton::Middle),
        3 => Some(PointerButton::Secondary),
        8 => Some(PointerButton::Extra1),
        9 => Some(PointerButton::Extra2),
        _ => None,
    };
    {
        let (input, area) = (input.clone(), area.clone());
        base.connect_button_press_event(move |b, ev| {
            if ev.event_type() != gdk::EventType::ButtonPress {
                return glib::Propagation::Stop; // egui counts double clicks itself
            }
            if !b.has_focus() {
                b.grab_focus();
            }
            let (x, y) = ev.position();
            let (rx, ry) = ev.root();
            let mut i = input.borrow_mut();
            i.modifiers = modifiers_of(ev.state());
            i.press = (ev.button(), rx, ry, ev.time());
            if let Some(button) = button_of(ev.button()) {
                let modifiers = i.modifiers;
                i.events.push(Event::PointerButton { pos: pos2(x as f32, y as f32), button, pressed: true, modifiers });
            }
            area.queue_render();
            glib::Propagation::Stop
        });
    }
    {
        let (input, area) = (input.clone(), area.clone());
        base.connect_button_release_event(move |_, ev| {
            let (x, y) = ev.position();
            let mut i = input.borrow_mut();
            i.modifiers = modifiers_of(ev.state());
            if let Some(button) = button_of(ev.button()) {
                let modifiers = i.modifiers;
                i.events.push(Event::PointerButton { pos: pos2(x as f32, y as f32), button, pressed: false, modifiers });
            }
            area.queue_render();
            glib::Propagation::Stop
        });
    }
    {
        let (input, area) = (input.clone(), area.clone());
        base.connect_scroll_event(move |_, ev| {
            let delta = match ev.direction() {
                gdk::ScrollDirection::Up => vec2(0.0, 1.0),
                gdk::ScrollDirection::Down => vec2(0.0, -1.0),
                gdk::ScrollDirection::Left => vec2(1.0, 0.0),
                gdk::ScrollDirection::Right => vec2(-1.0, 0.0),
                _ => {
                    let (dx, dy) = ev.delta();
                    vec2(-dx as f32, -dy as f32)
                }
            };
            let mut i = input.borrow_mut();
            i.modifiers = modifiers_of(ev.state());
            let modifiers = i.modifiers;
            i.events.push(Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta, phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Unknown, modifiers });
            area.queue_render();
            glib::Propagation::Stop
        });
    }
    {
        let (input, area) = (input.clone(), area.clone());
        base.connect_leave_notify_event(move |_, _| {
            input.borrow_mut().events.push(Event::PointerGone);
            area.queue_render();
            glib::Propagation::Proceed
        });
    }
    {
        let (input, area, im) = (input.clone(), area.clone(), im.clone());
        base.connect_key_press_event(move |_, ev| {
            let modifiers = modifiers_of(ev.state());
            let kv = ev.keyval();
            let key = key_of(kv);
            {
                let mut i = input.borrow_mut();
                i.modifiers = modifiers;
                if modifiers.command && !modifiers.alt {
                    match key {
                        Some(egui::Key::C) => i.events.push(Event::Copy),
                        Some(egui::Key::X) => i.events.push(Event::Cut),
                        _ => {}
                    }
                }
                if let Some(key) = key {
                    i.events.push(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
                }
            }
            if modifiers.command && !modifiers.alt && key == Some(egui::Key::V) {
                let (input, area) = (input.clone(), area.clone());
                gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).request_text(move |_, t| {
                    if let Some(t) = t {
                        input.borrow_mut().events.push(Event::Paste(t.replace("\r\n", "\n")));
                        area.queue_render();
                    }
                });
            } else if !modifiers.command && !modifiers.alt && !im.filter_keypress(ev) {
                if let Some(c) = kv.to_unicode().filter(|c| !c.is_control()) {
                    input.borrow_mut().events.push(Event::Text(c.to_string()));
                }
            }
            area.queue_render();
            glib::Propagation::Stop
        });
    }
    {
        let (input, area, im) = (input.clone(), area.clone(), im.clone());
        base.connect_key_release_event(move |_, ev| {
            im.filter_keypress(ev);
            let modifiers = modifiers_of(ev.state());
            let mut i = input.borrow_mut();
            i.modifiers = modifiers;
            if let Some(key) = key_of(ev.keyval()) {
                i.events.push(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
            }
            area.queue_render();
            glib::Propagation::Stop
        });
    }
    {
        let im = im.clone();
        base.connect_focus_in_event(move |_, _| {
            im.focus_in();
            glib::Propagation::Proceed
        });
    }
    {
        let (input, area, im) = (input.clone(), area.clone(), im.clone());
        base.connect_focus_out_event(move |_, _| {
            im.focus_out();
            let mut i = input.borrow_mut();
            i.chrome_lost_focus = true;
            i.modifiers = Modifiers::default();
            area.queue_render();
            glib::Propagation::Proceed
        });
    }
    {
        let (input, area) = (input.clone(), area.clone());
        window.connect_window_state_event(move |_, ev| {
            let s = ev.new_window_state();
            let mut i = input.borrow_mut();
            i.maximized = s.contains(gdk::WindowState::MAXIMIZED);
            i.fullscreen = s.contains(gdk::WindowState::FULLSCREEN);
            i.focused = s.contains(gdk::WindowState::FOCUSED);
            let f = i.focused;
            i.events.push(Event::WindowFocused(f));
            area.queue_render();
            glib::Propagation::Proceed
        });
    }
    {
        let gui = gui.clone();
        window.connect_delete_event(move |_, _| {
            if let Ok(mut g) = gui.try_borrow_mut() {
                g.app.on_close();
            }
            gtk::main_quit();
            glib::Propagation::Proceed
        });
    }

    window.show_all();
    base.grab_focus();
    gtk::main();
    HOST.with(|h| *h.borrow_mut() = None);
    Ok(())
}

fn paint(g: &mut Gui, area: &gtk::GLArea, input: &Rc<RefCell<Input>>, next: &Rc<Cell<Option<Instant>>>, host: &Rc<Host>, im: &gtk::IMMulticontext) {
    if g.painter.is_none() {
        return;
    }
    let scale = area.scale_factor().max(1) as f32;
    let (w, h) = (area.allocated_width().max(1), area.allocated_height().max(1));
    let zoom = g.ctx.zoom_factor();
    let mut raw = egui::RawInput::default();
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(w as f32, h as f32) / zoom);
    raw.screen_rect = Some(screen);
    raw.time = Some(g.start.elapsed().as_secs_f64());
    let lost_focus;
    {
        let mut i = input.borrow_mut();
        raw.events = std::mem::take(&mut i.events);
        if (zoom - 1.0).abs() > f32::EPSILON {
            for e in &mut raw.events {
                match e {
                    Event::PointerMoved(p) => *p = (p.to_vec2() / zoom).to_pos2(),
                    Event::PointerButton { pos, .. } => *pos = (pos.to_vec2() / zoom).to_pos2(),
                    _ => {}
                }
            }
        }
        if i.modifiers != i.sent_modifiers {
            i.sent_modifiers = i.modifiers;
            raw.events.insert(0, Event::ModifiersChanged(i.modifiers));
        }
        raw.focused = i.focused;
        lost_focus = std::mem::take(&mut i.chrome_lost_focus);
        let vp = raw.viewports.entry(ViewportId::ROOT).or_default();
        vp.native_pixels_per_point = Some(scale);
        vp.focused = Some(i.focused);
        vp.maximized = Some(i.maximized);
        vp.fullscreen = Some(i.fullscreen);
        vp.inner_rect = Some(screen);
    }
    if let Some(p) = &g.painter {
        raw.max_texture_side = Some(p.max_texture_side());
    }
    if lost_focus {
        // the page has the keyboard now: Sanaga's text fields let go of it
        if let Some(id) = g.ctx.memory(|m| m.focused()) {
            g.ctx.memory_mut(|m| m.surrender_focus(id));
        }
    }
    // test clicks: one press or release per frame, so each is seen
    INJECT.with(|q| {
        let mut q = q.borrow_mut();
        while let Some(e) = (!q.is_empty()).then(|| q.remove(0)) {
            let stop = matches!(e, Event::PointerButton { .. } | Event::Key { pressed: false, .. });
            raw.events.push(e);
            if stop {
                break;
            }
        }
        if !q.is_empty() {
            area.queue_render();
        }
    });
    INJECT_NOW.with(|q| raw.events.append(&mut q.borrow_mut()));
    TOOLTIP.with(|t| *t.borrow_mut() = None);
    let ctx = g.ctx.clone();
    let app = &mut g.app;
    let full = ctx.run_ui(raw, |ui| app.frame_ui(ui));
    // hover help: a system tooltip, above the page
    let tip = TOOLTIP.with(|t| t.borrow_mut().take());
    if *host.tip_shown.borrow() != tip {
        host.base.set_tooltip_text(tip.as_deref());
        if tip.is_some() {
            host.base.trigger_tooltip_query();
        }
        *host.tip_shown.borrow_mut() = tip;
    }

    // The keyboard follows: a text field of Sanaga's interface took it (address
    // bar, search...) -> to the interface; it let go of it (Enter, Escape) ->
    // back to the page shown.
    let text_focus = ctx.memory(|m| m.focused().is_some());
    if text_focus && !host.base.has_focus() {
        host.base.grab_focus();
    } else if !text_focus && g.text_focus && host.base.has_focus() {
        if let Some(page) = g.app.shown_page() {
            page.grab_focus();
        }
    }
    g.text_focus = text_focus;

    // what the interface asks of the window
    let out = &full.platform_output;
    if out.cursor_icon != g.cursor {
        g.cursor = out.cursor_icon;
        if let (Some(win), Some(d)) = (host.base.window(), gdk::Display::default()) {
            let c = gdk::Cursor::from_name(&d, cursor_name(out.cursor_icon));
            win.set_cursor(c.as_ref());
        }
    }
    for c in &out.commands {
        match c {
            egui::OutputCommand::CopyText(t) => gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(t),
            egui::OutputCommand::OpenUrl(u) => {
                let _ = std::process::Command::new("xdg-open").arg(&u.url).spawn();
            }
            _ => {}
        }
    }
    if let Some(ime) = &out.ime {
        let r = ime.cursor_rect;
        im.set_cursor_location(&gdk::Rectangle::new((r.left() * zoom) as i32, (r.top() * zoom) as i32, (r.width() * zoom).max(1.0) as i32, (r.height() * zoom).max(1.0) as i32));
    }
    let (mut close, mut repaint) = (false, Duration::MAX);
    if let Some(vo) = full.viewport_output.get(&ViewportId::ROOT) {
        repaint = vo.repaint_delay;
        for c in &vo.commands {
            let win = &host.window;
            match c {
                ViewportCommand::Close => close = true,
                ViewportCommand::Title(t) => win.set_title(t),
                ViewportCommand::StartDrag => {
                    let (b, x, y, t) = input.borrow().press;
                    win.begin_move_drag(b.max(1) as i32, x as i32, y as i32, t);
                }
                ViewportCommand::BeginResize(dir) => {
                    use egui::ResizeDirection as D;
                    let edge = match dir {
                        D::North => gdk::WindowEdge::North,
                        D::South => gdk::WindowEdge::South,
                        D::East => gdk::WindowEdge::East,
                        D::West => gdk::WindowEdge::West,
                        D::NorthEast => gdk::WindowEdge::NorthEast,
                        D::NorthWest => gdk::WindowEdge::NorthWest,
                        D::SouthEast => gdk::WindowEdge::SouthEast,
                        D::SouthWest => gdk::WindowEdge::SouthWest,
                    };
                    let (b, x, y, t) = input.borrow().press;
                    win.begin_resize_drag(edge, b.max(1) as i32, x as i32, y as i32, t);
                }
                ViewportCommand::Maximized(true) => win.maximize(),
                ViewportCommand::Maximized(false) => win.unmaximize(),
                ViewportCommand::Minimized(true) => win.iconify(),
                ViewportCommand::Fullscreen(true) => win.fullscreen(),
                ViewportCommand::Fullscreen(false) => win.unfullscreen(),
                ViewportCommand::Focus => win.present(),
                _ => {}
            }
        }
    }

    // draw
    let mut full = full;
    let prims = ctx.tessellate(std::mem::take(&mut full.shapes), full.pixels_per_point);
    let size_px = [(w as f32 * scale) as u32, (h as f32 * scale) as u32];
    if let Some(p) = g.painter.as_mut() {
        p.clear(size_px, [0.0, 0.0, 0.0, 1.0]);
        p.paint_and_update_textures(size_px, full.pixels_per_point, &prims, &mut full.textures_delta);
    }
    if let (Some(gl), true) = (&g.gl, std::env::var_os("SANAGA_DEBUG_GL").is_some()) {
        use glow::HasContext;
        let e = unsafe { gl.get_error() };
        if e != 0 {
            eprintln!("GL error 0x{e:x}");
        }
    }
    if let Some(gl) = &g.gl {
        // the window is opaque: blended text left see-through pixels behind
        use glow::HasContext;
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.color_mask(false, false, false, true);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.color_mask(true, true, true, true);
        }
    }
    if let (Some(gl), Ok(path)) = (&g.gl, std::env::var("SANAGA_DEBUG_GLSHOT")) {
        // test aid: what Sanaga itself drew (before GTK puts the window together)
        if g.app.frame_count() == std::env::var("SANAGA_DEBUG_GLSHOT_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(40) {
            use glow::HasContext;
            let (pw, ph) = (size_px[0] as usize, size_px[1] as usize);
            let mut px = vec![0u8; pw * ph * 4];
            unsafe {
                gl.read_pixels(0, 0, pw as i32, ph as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut px)));
            }
            let mut flipped = Vec::with_capacity(px.len());
            for row in px.chunks_exact(pw * 4).rev() {
                flipped.extend_from_slice(row);
            }
            let _ = image::save_buffer(&path, &flipped, pw as u32, ph as u32, image::ColorType::Rgba8);
            eprintln!("gl shot {path}");
        }
    }
    if close {
        g.app.on_close();
        gtk::main_quit();
        return;
    }
    schedule(area, next, repaint);
    if let Some(path) = host.shot.borrow_mut().take() {
        let win = host.window.clone();
        glib::idle_add_local_once(move || save_shot(&win, &path));
    }
}

fn cursor_name(c: egui::CursorIcon) -> &'static str {
    use egui::CursorIcon as C;
    match c {
        C::PointingHand => "pointer",
        C::Text => "text",
        C::VerticalText => "vertical-text",
        C::Wait => "wait",
        C::Progress => "progress",
        C::Crosshair => "crosshair",
        C::Help => "help",
        C::ContextMenu => "context-menu",
        C::Cell => "cell",
        C::Alias => "alias",
        C::Copy => "copy",
        C::Move => "move",
        C::NoDrop => "no-drop",
        C::NotAllowed => "not-allowed",
        C::Grab => "grab",
        C::Grabbing => "grabbing",
        C::AllScroll => "all-scroll",
        C::ResizeHorizontal | C::ResizeColumn => "col-resize",
        C::ResizeVertical | C::ResizeRow => "row-resize",
        C::ResizeEast => "e-resize",
        C::ResizeWest => "w-resize",
        C::ResizeNorth => "n-resize",
        C::ResizeSouth => "s-resize",
        C::ResizeNorthEast => "ne-resize",
        C::ResizeNorthWest => "nw-resize",
        C::ResizeSouthEast => "se-resize",
        C::ResizeSouthWest => "sw-resize",
        C::ResizeNeSw => "nesw-resize",
        C::ResizeNwSe => "nwse-resize",
        C::ZoomIn => "zoom-in",
        C::ZoomOut => "zoom-out",
        C::None => "none",
        _ => "default",
    }
}
