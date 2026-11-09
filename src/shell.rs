//! Sanaga's window: one native GTK 4 window (Wayland or X11).
//!
//! Sanaga's interface (tabs, toolbar, menus, internal pages) is drawn with the
//! graphics card in a GL area filling the window, and each web page is a
//! normal GTK part of the same window, placed over the page area. GTK 4
//! composes both on the graphics card in the same frame: resizing stays
//! smooth, and a page's animations are not copied by the processor any more.

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
    area: gtk::GLArea,
    window: gtk::Window,
    insets: Cell<[i32; 4]>,
    /// Arrow shown while swiping back / forward over a page.
    swipe_hint: gtk::Label,
    /// The mobile simulator's drawn device, above the page (clicks go through).
    device_frame: gtk::DrawingArea,
    device_pic: Rc<RefCell<Option<gtk::cairo::ImageSurface>>>,
    /// Back (-1) or forward (1) asked by a gesture or a mouse button.
    nav: Cell<i32>,
    shot: RefCell<Option<String>>,
    tip_shown: RefCell<Option<String>>,
    /// When the next frame is already asked for.
    next: Rc<Cell<Option<Instant>>>,
    /// A frame of the interface is asked for (it runs once GTK is idle).
    wanted: Cell<bool>,
    /// Waiting for the screen's next refresh.
    ticking: Cell<bool>,
    /// The window's number (0 = the first window).
    slot: usize,
}

/// One open window: its parts and its interface.
struct Win {
    host: Rc<Host>,
    gui: std::rc::Weak<RefCell<Gui>>,
    input: Rc<RefCell<Input>>,
    im: gtk::IMMulticontext,
}

/// Asks a window for a frame of its interface. The interface runs when GTK
/// is idle, never while GTK draws (GTK 4 does not allow adding or moving a
/// page while drawing), then GTK draws what it made.
fn wake(h: &Host) {
    if h.wanted.replace(true) {
        return;
    }
    let slot = h.slot;
    glib::idle_add_local_once(move || run_window(slot));
}

/// What the input handlers hold: the GL area, and its window to wake.
#[derive(Clone)]
struct Wake {
    area: gtk::GLArea,
    host: std::rc::Weak<Host>,
}
impl Wake {
    fn queue_render(&self) {
        if let Some(h) = self.host.upgrade() {
            wake(&h);
        }
    }
}
impl std::ops::Deref for Wake {
    type Target = gtk::GLArea;
    fn deref(&self) -> &gtk::GLArea {
        &self.area
    }
}

/// One frame of a window's interface.
fn run_window(slot: usize) {
    let found = WINDOWS.with(|w| w.borrow().iter().find(|x| x.host.slot == slot).map(|x| (x.host.clone(), x.gui.upgrade(), x.input.clone(), x.im.clone())));
    let Some((host, Some(gui), input, im)) = found else { return };
    host.wanted.set(false);
    let Ok(mut g) = gui.try_borrow_mut() else {
        // (busy: a dialog of this window is open) try again shortly
        let h = Rc::downgrade(&host);
        glib::timeout_add_local_once(Duration::from_millis(30), move || {
            if let Some(h) = h.upgrade() {
                wake(&h);
            }
        });
        return;
    };
    // what the interface asks of "the window" now applies to this one
    HOST.with(|h| *h.borrow_mut() = Some(host.clone()));
    if crate::page::ACTIVE_WINDOW.load(std::sync::atomic::Ordering::Relaxed) == host.slot {
        let arrived: Vec<String> = INCOMING.with(|i| i.borrow_mut().drain(..).collect());
        if !arrived.is_empty() {
            g.app.open_from_outside(arrived);
        }
    }
    let area = host.area.clone();
    update(&mut g, &area, &input, &host, &im);
    // Ctrl+N, a tab moved to a new window
    if let Some(urls) = g.app.take_window_request() {
        glib::idle_add_local_once(move || {
            open_window(urls);
        });
    }
}

/// What the interface made, waiting for GTK to draw it.
struct Frame {
    prims: Vec<egui::ClippedPrimitive>,
    ppp: f32,
    size: [i32; 2],
}

thread_local! {
    /// The window being drawn now (what the interface's requests apply to).
    static HOST: RefCell<Option<Rc<Host>>> = const { RefCell::new(None) };
    /// All open windows.
    static WINDOWS: RefCell<Vec<Win>> = const { RefCell::new(Vec::new()) };
    /// The number the next window gets.
    static NEXT_SLOT: Cell<usize> = const { Cell::new(1) };
    /// Addresses sent by other launches of Sanaga, for the window used last.
    static INCOMING: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Hover help asked during this frame.
    static TOOLTIP: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Input made up by tests (clicks...).
    static INJECT: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static INJECT_NOW: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    /// The main loop (quitting it ends Sanaga).
    static MAIN: RefCell<Option<glib::MainLoop>> = const { RefCell::new(None) };
    /// `--screenshot FILE`: the job for the window, and quitting once the picture is saved.
    static SHOT_JOB: RefCell<Option<crate::app::ScreenshotJob>> = const { RefCell::new(None) };
    static QUIT_AFTER_SHOT: Cell<bool> = const { Cell::new(false) };
}

/// `--screenshot`: save a picture of the window, then quit.
pub fn shot_and_quit(path: String) {
    QUIT_AFTER_SHOT.with(|q| q.set(true));
    request_shot(path);
}

pub fn set_tooltip(text: String) {
    TOOLTIP.with(|t| *t.borrow_mut() = Some(text));
}

/// Test aid: a mouse click at (x, y), pressed then released.
#[cfg_attr(not(debug_assertions), allow(dead_code))] // (test aids: debug builds)
pub fn gtk_click(x: f64, y: f64, button: u32) {
    let b = match button {
        3 => PointerButton::Secondary,
        2 => PointerButton::Middle,
        _ => PointerButton::Primary,
    };
    let p = pos2(x as f32, y as f32);
    inject(vec![
        Event::PointerMoved(p),
        Event::PointerButton { pos: p, button: b, pressed: true, modifiers: Modifiers::default() },
        Event::PointerButton { pos: p, button: b, pressed: false, modifiers: Modifiers::default() },
    ]);
}

/// Test aid: like `inject`, optionally all in one frame.
#[cfg_attr(not(debug_assertions), allow(dead_code))] // (test aids: debug builds)
pub fn inject_together(together: bool, events: Vec<Event>) {
    if together {
        INJECT_NOW.with(|i| i.borrow_mut().extend(events));
        if let Some(h) = host() {
            wake(&h);
        }
    } else {
        inject(events);
    }
}

/// Test aid: input events added to the next frames.
#[cfg_attr(not(debug_assertions), allow(dead_code))] // (test aids: debug builds)
pub fn inject(events: Vec<Event>) {
    INJECT.with(|i| i.borrow_mut().extend(events));
    if let Some(h) = host() {
        wake(&h);
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
pub fn attach(view: &webkit6::WebView) {
    let Some(h) = host() else { return };
    view.set_halign(gtk::Align::Fill);
    view.set_valign(gtk::Align::Fill);
    apply_insets(view, h.insets.get());
    h.overlay.add_overlay(view);
    raise_overlays(&h);
    watch_gestures(view, Rc::downgrade(&h));
}

/// The simulator's frame and the swipe arrow stay above the pages (GTK draws
/// the overlays in the order they were added).
fn raise_overlays(h: &Host) {
    for w in [h.device_frame.upcast_ref::<gtk::Widget>(), h.swipe_hint.upcast_ref::<gtk::Widget>()] {
        if w.next_sibling().is_some() {
            h.overlay.remove_overlay(w);
            h.overlay.add_overlay(w);
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
            if a.margin_start() != x || a.margin_top() != y || a.content_width() != w || a.content_height() != hh {
                a.set_margin_start(x);
                a.set_margin_top(y);
                a.set_content_width(w);
                a.set_content_height(hh);
            }
            *h.device_pic.borrow_mut() = Some(surf);
            a.queue_draw();
            if !a.is_visible() {
                a.set_visible(true);
                raise_overlays(&h);
            }
        }
        None => {
            if a.is_visible() {
                a.set_visible(false);
            }
            h.device_pic.borrow_mut().take();
        }
    }
}

fn children(w: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    let mut c = w.first_child();
    while let Some(x) = c {
        c = x.next_sibling();
        out.push(x);
    }
    out
}

/// Test aid: where each part of the window really is.
#[cfg_attr(not(debug_assertions), allow(dead_code))] // (test aids: debug builds)
pub fn debug_allocations() {
    let Some(h) = host() else { return };
    for c in children(&h.overlay) {
        eprintln!(
            "alloc {} visible={} w={} h={} margins l={} t={} r={} b={}",
            c.type_().name(),
            c.is_visible(),
            c.width(),
            c.height(),
            c.margin_start(),
            c.margin_top(),
            c.margin_end(),
            c.margin_bottom()
        );
    }
}

/// A gesture or a mouse button asked to go back (-1) or forward (1).
pub fn take_nav() -> i32 {
    host().map(|h| h.nav.replace(0)).unwrap_or(0)
}

fn ask_nav(h: &Host, dir: i32) {
    h.nav.set(dir);
    wake(h);
}

/// Two fingers sliding sideways on a touchpad go back / forward (Chrome,
/// Firefox); the mouse's side buttons too.
fn watch_gestures(view: &webkit6::WebView, host: std::rc::Weak<Host>) {
    let click = gtk::GestureClick::new();
    click.set_button(0);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let h = host.clone();
    click.connect_pressed(move |g, _, _, _| {
        let Some(h) = h.upgrade() else { return };
        match g.current_button() {
            8 => ask_nav(&h, -1),
            9 => ask_nav(&h, 1),
            _ => return,
        }
        g.set_state(gtk::EventSequenceState::Claimed);
    });
    view.add_controller(click);
    // (sum sideways, sum up-down, decided: Some(true) = a sideways swipe)
    let state: Rc<Cell<(f64, f64, Option<bool>)>> = Rc::new(Cell::new((0.0, 0.0, None)));
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    let (st, h) = (state.clone(), host.clone());
    scroll.connect_scroll(move |c, dx, dy| {
        let Some(h) = h.upgrade() else { return glib::Propagation::Proceed };
        if c.unit() != gdk::ScrollUnit::Surface {
            return glib::Propagation::Proceed; // a mouse wheel, not a touchpad
        }
        let (mut sx, mut sy, mut decided) = st.get();
        sx += dx;
        sy += dy;
        if decided.is_none() && sx.abs() + sy.abs() > 3.0 {
            decided = Some(sx.abs() > 2.5 * sy.abs());
        }
        st.set((sx, sy, decided));
        if decided == Some(true) {
            show_swipe(&h, if sx < 0.0 { -1 } else { 1 }, (sx.abs() / SWIPE_GO).min(1.0));
        }
        glib::Propagation::Proceed
    });
    scroll.connect_scroll_end(move |_| {
        let Some(h) = host.upgrade() else { return };
        let (sx, _, decided) = state.get();
        if decided == Some(true) && sx.abs() >= SWIPE_GO {
            ask_nav(&h, if sx < 0.0 { -1 } else { 1 });
        }
        state.set((0.0, 0.0, None));
        show_swipe(&h, 0, 0.0);
    });
    view.add_controller(scroll);
}

const SWIPE_GO: f64 = 22.0;

/// Test aid: the swipe arrow, and the navigation a finished swipe asks for.
#[cfg_attr(not(debug_assertions), allow(dead_code))] // (test aids: debug builds)
pub fn test_swipe(dir: i32, progress: f64, finish: bool) {
    let Some(h) = host() else { return };
    show_swipe(&h, dir, progress);
    if finish {
        show_swipe(&h, 0, 0.0);
        ask_nav(&h, dir);
    }
}

/// The arrow at the page's edge while swiping (`dir` 0 hides it).
fn show_swipe(h: &Host, dir: i32, progress: f64) {
    let l = &h.swipe_hint;
    if dir == 0 || progress < 0.15 {
        l.set_visible(false);
        return;
    }
    let [left, _, right, _] = h.insets.get();
    l.set_text(if dir < 0 { "←" } else { "→" });
    l.set_halign(if dir < 0 { gtk::Align::Start } else { gtk::Align::End });
    let slide = (progress * 40.0) as i32;
    l.set_margin_start(left + 8 + if dir < 0 { slide } else { 0 });
    l.set_margin_end(right + 8 + if dir > 0 { slide } else { 0 });
    l.set_opacity(0.35 + 0.65 * progress);
    if progress >= 1.0 {
        l.add_css_class("ready");
    } else {
        l.remove_css_class("ready");
    }
    l.set_visible(true);
}

/// The page area, as distances from the window's left, top, right and bottom
/// edges. Pages keep these distances by themselves when the window changes size.
pub fn set_page_area(insets: [i32; 4]) {
    let Some(h) = host() else { return };
    if h.insets.get() == insets {
        return;
    }
    h.insets.set(insets);
    for c in children(&h.overlay) {
        if c != *h.area.upcast_ref::<gtk::Widget>() && c != *h.swipe_hint.upcast_ref::<gtk::Widget>() && c != *h.device_frame.upcast_ref::<gtk::Widget>() {
            apply_insets(&c, insets);
        }
    }
}

/// A small window of its own: a link preview, or a floating video kept above
/// the others (`pin`). `open_label`: a button that opens what it shows in a
/// tab of the last window used. True when the window opened.
#[allow(clippy::too_many_arguments)]
pub fn float_window(engine: Option<&crate::page::Engine>, title: &str, url: Option<&str>, html: Option<(&str, &str)>, w: i32, h: i32, open_label: Option<&str>, pin: bool) -> bool {
    use webkit6::prelude::*;
    if host().is_none() {
        return false;
    }
    let view = crate::page::plain_view(engine);
    let win = gtk::Window::new();
    win.set_title(Some(title));
    win.set_default_size(w, h);
    win.set_icon_name(Some("sanaga"));
    let header = gtk::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some(title))));
    if let Some(label) = open_label {
        let b = gtk::Button::with_label(label);
        b.add_css_class("suggested-action");
        let (v, wn) = (view.clone(), win.clone());
        b.connect_clicked(move |_| {
            if let Some(u) = v.uri() {
                open_in_last_window(u.to_string());
            }
            wn.close();
        });
        header.pack_end(&b);
    }
    win.set_titlebar(Some(&header));
    win.set_child(Some(&view));
    match (url, html) {
        // (asked for by the person: a link preview, a floating video)
        (Some(u), _) => crate::page::load_by_person(&view, u),
        (None, Some((page, base))) => view.load_html(page, Some(base)),
        _ => return false,
    }
    let v = view.clone();
    win.connect_close_request(move |_| {
        v.terminate_web_process();
        glib::Propagation::Proceed
    });
    win.present();
    crate::desktop::float_window(title, w, h, pin);
    true
}

/// The open windows: number and title (the sidebar lists them).
pub fn window_list() -> Vec<(usize, String)> {
    WINDOWS.with(|w| w.borrow().iter().map(|x| (x.host.slot, x.host.window.title().map(|t| t.to_string()).unwrap_or_default())).collect())
}

/// A picture of a part of the window as GTK last drew it (a web page: what
/// is on screen, even while its page is busy or frozen by an alert).
pub fn widget_texture(w: &impl IsA<gtk::Widget>) -> Option<gdk::Texture> {
    let w = w.as_ref();
    if !w.is_visible() || w.width() < 2 || w.height() < 2 {
        return None;
    }
    let parent = w.parent()?;
    let snap = gtk::Snapshot::new();
    parent.snapshot_child(w, &snap);
    let node = snap.to_node()?;
    let renderer = w.native()?.renderer()?;
    Some(renderer.render_texture(&node, None))
}

/// Is Ctrl held down right now? (The keyboard's real state, even while a
/// page had the keyboard.)
pub fn ctrl_held() -> Option<bool> {
    gdk::Display::default()
        .and_then(|d| d.default_seat())
        .and_then(|s| s.keyboard())
        .map(|k| k.modifier_state().contains(gdk::ModifierType::CONTROL_MASK))
}

/// Give the keyboard to Sanaga's interface (away from the page).
pub fn focus_interface() {
    if let Some(h) = host() {
        h.area.grab_focus();
    }
}

/// The title of the window being drawn now.
pub fn window_title() -> String {
    host().and_then(|h| h.window.title()).map(|t| t.to_string()).unwrap_or_default()
}

/// Bring window `slot` to the front.
pub fn present_window(slot: usize) {
    if let Some(h) = WINDOWS.with(|w| w.borrow().iter().find(|x| x.host.slot == slot).map(|x| x.host.clone())) {
        h.window.present();
    }
}

/// Open an address in a new tab of the window used last.
pub fn open_in_last_window(url: String) {
    INCOMING.with(|i| i.borrow_mut().push(url));
    if let Some(h) = last_used_window() {
        h.window.present();
        wake(&h);
    }
}

/// Save a picture of the whole window (test aid).
pub fn request_shot(path: String) {
    if let Some(h) = host() {
        *h.shot.borrow_mut() = Some(path);
        wake(&h);
    }
}

fn save_shot(window: &gtk::Window, path: &str) {
    let (w, h) = (window.width().max(1), window.height().max(1));
    let Some(content) = window.child() else { return };
    let snap = gtk::Snapshot::new();
    window.snapshot_child(&content, &snap);
    let mut node = snap.to_node();
    if node.is_none() {
        // (screen asleep: GTK has not laid the window out since a change)
        content.allocate(w, h, -1, None);
        let snap = gtk::Snapshot::new();
        window.snapshot_child(&content, &snap);
        node = snap.to_node();
    }
    let renderer = window.renderer();
    let (Some(node), Some(renderer)) = (node.as_ref(), renderer) else {
        eprintln!("live shot impossible ({w}x{h}, picture {}, drawing {})", node.is_some(), window.renderer().is_some());
        return;
    };
    let tex = renderer.render_texture(node, None);
    if let Some(dir) = std::path::Path::new(path).parent().filter(|d| !d.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(dir);
    }
    let quit = QUIT_AFTER_SHOT.with(|q| q.get());
    match tex.save_to_png(path) {
        Ok(()) if quit => eprintln!("Capture enregistrée : {path}"),
        Ok(()) => eprintln!("live shot {path}"),
        Err(e) => eprintln!("Capture impossible ({path}) : {e}"),
    }
    if quit {
        quit_main();
    }
}

/// Ask a file of the person (blocking until they answer): the system's file
/// dialog (GTK 4 only has an asynchronous one, so a small loop waits).
pub fn choose_file(title: &str, patterns: &[&str], filter_name: &str, save_as: Option<&str>) -> Option<std::path::PathBuf> {
    let dialog = gtk::FileDialog::builder().title(title).modal(true).build();
    if !patterns.is_empty() {
        let filter = gtk::FileFilter::new();
        for p in patterns {
            filter.add_pattern(p);
        }
        filter.set_name(Some(filter_name));
        let list = gtk::gio::ListStore::new::<gtk::FileFilter>();
        list.append(&filter);
        dialog.set_filters(Some(&list));
    }
    if let Some(name) = save_as {
        dialog.set_initial_name(Some(name));
    }
    let parent = host().map(|h| h.window.clone());
    let result: Rc<RefCell<Option<std::path::PathBuf>>> = Rc::new(RefCell::new(None));
    let wait = glib::MainLoop::new(None, false);
    let (r, w) = (result.clone(), wait.clone());
    let done = move |res: Result<gtk::gio::File, glib::Error>| {
        *r.borrow_mut() = res.ok().and_then(|f| f.path());
        w.quit();
    };
    if save_as.is_some() {
        dialog.save(parent.as_ref(), None::<&gtk::gio::Cancellable>, done);
    } else {
        dialog.open(parent.as_ref(), None::<&gtk::gio::Cancellable>, done);
    }
    wait.run();
    result.take()
}

/// What the input handlers collect between two frames.
#[derive(Default)]
struct Input {
    events: Vec<Event>,
    modifiers: Modifiers,
    sent_modifiers: Modifiers,
    focused: bool,
    /// Last mouse press: button, position in the window, time, device (to
    /// start a window move or resize).
    press: (u32, f64, f64, u32),
    device: Option<gdk::Device>,
    /// The keyboard went from Sanaga's interface to a page.
    chrome_lost_focus: bool,
}

/// Screen readers (Orca…) read Sanaga's interface through AT-SPI: egui
/// describes what it draws (AccessKit), this adapter publishes it, and the
/// reader's actions (press a button, move the focus) come back as input.
struct A11y {
    adapter: accesskit_unix::Adapter,
    /// A screen reader asked for the interface.
    on: Arc<std::sync::atomic::AtomicBool>,
    /// Its actions, for the next frame.
    actions: Arc<std::sync::Mutex<Vec<accesskit::ActionRequest>>>,
}

struct A11yActivate {
    on: Arc<std::sync::atomic::AtomicBool>,
    ctx: egui::Context,
}
impl accesskit::ActivationHandler for A11yActivate {
    fn request_initial_tree(&mut self) -> Option<accesskit::TreeUpdate> {
        self.on.store(true, std::sync::atomic::Ordering::Relaxed);
        self.ctx.request_repaint();
        None
    }
}
struct A11yAction {
    actions: Arc<std::sync::Mutex<Vec<accesskit::ActionRequest>>>,
    ctx: egui::Context,
}
impl accesskit::ActionHandler for A11yAction {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        if let Ok(mut q) = self.actions.lock() {
            q.push(request);
        }
        self.ctx.request_repaint();
    }
}
struct A11yDeactivate {
    on: Arc<std::sync::atomic::AtomicBool>,
}
impl accesskit::DeactivationHandler for A11yDeactivate {
    fn deactivate_accessibility(&mut self) {
        self.on.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

fn new_a11y(ctx: &egui::Context) -> A11y {
    let on = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let actions = Arc::new(std::sync::Mutex::new(Vec::new()));
    let adapter = accesskit_unix::Adapter::new(
        A11yActivate { on: on.clone(), ctx: ctx.clone() },
        A11yAction { actions: actions.clone(), ctx: ctx.clone() },
        A11yDeactivate { on: on.clone() },
    );
    A11y { adapter, on, actions }
}

struct Gui {
    a11y: A11y,
    app: Sanaga,
    ctx: egui::Context,
    gl: Option<Arc<glow::Context>>,
    painter: Option<egui_glow::Painter>,
    start: Instant,
    cursor: egui::CursorIcon,
    text_focus: bool,
    /// The last frame made (drawn again if GTK asks), and the picture changes
    /// not sent to the graphics card yet.
    frame: Option<Frame>,
    delta: egui::TexturesDelta,
}

fn modifiers_of(state: gdk::ModifierType) -> Modifiers {
    let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
    Modifiers {
        alt: state.contains(gdk::ModifierType::ALT_MASK),
        ctrl,
        shift: state.contains(gdk::ModifierType::SHIFT_MASK),
        mac_cmd: false,
        command: ctrl,
    }
}

fn key_of(kv: gdk::Key) -> Option<egui::Key> {
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
fn schedule(host: &Rc<Host>, delay: Duration) {
    if delay.is_zero() {
        // an animation: the next frame comes with the screen's next refresh
        // (or a little later if the window is not on screen: hidden, screen off)
        if host.wanted.replace(true) {
            return;
        }
        let slot = host.slot;
        if !host.ticking.replace(true) {
            host.area.add_tick_callback(move |_, _| {
                if let Some(h) = WINDOWS.with(|w| w.borrow().iter().find(|x| x.host.slot == slot).map(|x| x.host.clone())) {
                    h.ticking.set(false);
                    if h.wanted.get() {
                        run_window(slot);
                    }
                }
                glib::ControlFlow::Break
            });
        }
        glib::timeout_add_local_once(Duration::from_millis(200), move || {
            if WINDOWS.with(|w| w.borrow().iter().any(|x| x.host.slot == slot && x.host.wanted.get())) {
                run_window(slot);
            }
        });
        return;
    }
    let next = &host.next;
    if delay > Duration::from_secs(3600) {
        return;
    }
    let at = Instant::now() + delay;
    if next.get().is_some_and(|n| n <= at) {
        return;
    }
    next.set(Some(at));
    let h = Rc::downgrade(host);
    glib::timeout_add_local_once(delay, move || {
        if let Some(h) = h.upgrade() {
            if h.next.get() == Some(at) {
                h.next.set(None);
                wake(&h);
            }
        }
    });
}

/// A window's interface asked for a frame (from any thread, through the
/// main loop).
fn frame_for(slot: usize, delay: Duration) {
    let h = WINDOWS.with(|w| w.borrow().iter().find(|x| x.host.slot == slot).map(|x| x.host.clone()));
    if let Some(h) = h {
        schedule(&h, delay);
    }
}

/// Where the running Sanaga listens for addresses opened from elsewhere (one
/// per data folder and profile): in $XDG_RUNTIME_DIR only (the user's own
/// closed folder). Without it, every launch runs on its own.
fn instance_socket(urls: &[String]) -> Option<std::path::PathBuf> {
    use std::hash::{Hash, Hasher};
    let profile = urls.iter().find_map(|u| u.strip_prefix("profil:")).unwrap_or("");
    let mut h = std::collections::hash_map::DefaultHasher::new();
    crate::store::Store::root_dir().hash(&mut h);
    profile.hash(&mut h);
    Some(crate::store::runtime_dir()?.join(format!("sanaga-{:016x}.sock", h.finish())))
}

// (message limits and checks: see guard.rs, testable without GTK)
use crate::guard::{accept_forwarded, allow_message, read_bounded, MAX_LINES, MAX_MESSAGE, MAX_READERS, READ_DEADLINE};

thread_local! {
    /// When the last messages from other launches arrived (rate limit).
    static RECENT_MESSAGES: RefCell<std::collections::VecDeque<Instant>> = const { RefCell::new(std::collections::VecDeque::new()) };
}

/// Connections being read now (each on its own short-lived thread).
static READERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Is the other end of the socket this same user (SO_PEERCRED)?
#[cfg(target_os = "linux")]
fn same_user(stream: &std::os::unix::net::UnixStream) -> bool {
    use std::os::unix::io::AsRawFd;
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: a valid socket, and a buffer of `len` bytes for the answer.
    let rc = unsafe { libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&mut cred as *mut libc::ucred).cast(), &mut len) };
    // SAFETY: getuid has no preconditions and cannot fail.
    rc == 0 && len as usize == std::mem::size_of::<libc::ucred>() && cred.uid == unsafe { libc::getuid() }
}

#[cfg(not(target_os = "linux"))]
fn same_user(stream: &std::os::unix::net::UnixStream) -> bool {
    use std::os::unix::io::AsRawFd;
    let (mut uid, mut gid) = (0, 0);
    // SAFETY: a valid socket and two places for the answer.
    let rc = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    rc == 0 && uid == unsafe { libc::getuid() }
}

/// Sanaga already runs with this data: hand it the addresses (it opens them
/// in new tabs and comes to the front) instead of starting a second copy that
/// would write the same files.
fn forward_to_running(sock: &std::path::Path, urls: &[String]) -> bool {
    use std::io::Write;
    let Ok(mut s) = std::os::unix::net::UnixStream::connect(sock) else { return false };
    let list: Vec<&str> = urls.iter().map(|u| u.as_str()).filter(|u| !u.starts_with("profil:")).collect();
    s.write_all(list.join("\n").as_bytes()).is_ok()
}

/// Addresses sent by another launch of Sanaga (on the main loop): only what
/// `accept_forwarded` lets through.
fn incoming(text: String, profile: Vec<String>) {
    // a few messages at a time
    if !RECENT_MESSAGES.with(|r| allow_message(&mut r.borrow_mut(), Instant::now())) {
        return;
    }
    let lines: Vec<String> = text.lines().filter_map(accept_forwarded).take(MAX_LINES).collect();
    if lines.is_empty() {
        return;
    }
    if lines.iter().any(|l| l == "nouvelle-fenetre:") {
        // `sanaga --new-window`: a window of its own
        let mut w: Vec<String> = lines.into_iter().filter(|l| l != "nouvelle-fenetre:").collect();
        w.extend(profile);
        open_window(w);
    } else {
        INCOMING.with(|i| i.borrow_mut().extend(lines));
        if let Some(h) = last_used_window() {
            h.window.present();
            wake(&h);
        }
    }
}

fn quit_main() {
    MAIN.with(|m| {
        if let Some(l) = m.borrow().as_ref() {
            l.quit();
        }
    });
}

/// `sanaga --screenshot FILE`: the normal window draws its fixed scene for a
/// moment, its picture is saved, and Sanaga quits. (Through GTK, it works
/// even when the window is not on screen, e.g. the screen is asleep.)
pub fn run_screenshot(urls: Vec<String>, job: crate::app::ScreenshotJob) -> Result<(), String> {
    let path = job.path.to_string_lossy().to_string();
    SHOT_JOB.with(|s| *s.borrow_mut() = Some(job));
    // whatever happens, a picture within the minute
    let urls: Vec<String> = urls.into_iter().filter(|u| !u.starts_with("webapp:")).collect();
    run_inner(urls, Some(path))
}

pub fn run(urls: Vec<String>) -> Result<(), String> {
    run_inner(urls, None)
}

fn run_inner(urls: Vec<String>, shot: Option<String>) -> Result<(), String> {
    // one Sanaga per data folder and profile (web apps run on their own; a
    // screenshot never talks to a running Sanaga)
    let sock = instance_socket(&urls);
    let single = shot.is_none() && !urls.iter().any(|u| u.starts_with("webapp:")) && sock.is_some();
    if let Some(sock) = sock.as_ref().filter(|_| single) {
        if forward_to_running(sock, &urls) {
            return Ok(());
        }
    }
    glib::set_prgname(Some("sanaga"));
    glib::set_application_name("Sanaga");
    gtk::init().map_err(|e| e.to_string())?;
    gtk::Window::set_default_icon_name("sanaga");
    crate::desktop::set_native();
    if let Some(display) = gdk::Display::default() {
        let css = gtk::CssProvider::new();
        css.load_from_string(
            "#sanaga-swipe { background: rgba(255,255,255,0.97); color: #1a73e8; border-radius: 999px; padding: 6px 15px 9px 15px; font-size: 24px; font-weight: bold; box-shadow: 0 2px 8px rgba(0,0,0,0.25); }
             #sanaga-swipe.ready { background: #1a73e8; color: white; }",
        );
        gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    // addresses sent by other launches of Sanaga (links opened from other apps)
    if let Some(sock) = sock.as_ref().filter(|_| single) {
        let _ = std::fs::remove_file(sock); // left by a Sanaga that did not close cleanly
        if let Ok(listener) = std::os::unix::net::UnixListener::bind(sock) {
            use std::os::unix::fs::PermissionsExt;
            // only this user (the folder is closed already)
            let _ = std::fs::set_permissions(sock, std::fs::Permissions::from_mode(0o600));
            let profile: Vec<String> = urls.iter().filter(|u| u.starts_with("profil:")).cloned().collect();
            // (accepting never waits for a peer: a slow or silent one can't
            // hold up the others)
            let _ = listener.set_nonblocking(true);
            std::thread::Builder::new()
                .name("sanaga-instance".into())
                .spawn(move || loop {
                    let stream = match listener.accept() {
                        Ok((s, _)) => s,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(50));
                            continue;
                        }
                        Err(_) => {
                            std::thread::sleep(Duration::from_millis(200));
                            continue;
                        }
                    };
                    // the same user only, a few connections at a time
                    if !same_user(&stream) || READERS.load(std::sync::atomic::Ordering::Relaxed) >= MAX_READERS {
                        continue;
                    }
                    let _ = stream.set_nonblocking(false);
                    READERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let profile = profile.clone();
                    let started = std::thread::Builder::new().name("sanaga-instance-read".into()).spawn(move || {
                        // a small message, sent at once: a slow peer is dropped
                        let message = read_bounded(&stream, MAX_MESSAGE, READ_DEADLINE);
                        READERS.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        if let Some(buf) = message {
                            let text = String::from_utf8_lossy(&buf).to_string();
                            glib::MainContext::default().invoke(move || incoming(text, profile));
                        }
                    });
                    if started.is_err() {
                        READERS.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                    }
                })
                .ok();
        }
    }

    if std::env::var_os("SANAGA_DEBUG_FRAMES").is_some() {
        glib::timeout_add_local(Duration::from_secs(1), || {
            let n = WINDOWS.with(|w| w.borrow().len());
            let busy = WINDOWS.with(|w| w.borrow().iter().map(|x| x.gui.upgrade().map(|g| g.try_borrow_mut().is_err())).collect::<Vec<_>>());
            eprintln!("tick windows={n} busy={busy:?}");
            glib::ControlFlow::Continue
        });
    }
    let main = glib::MainLoop::new(None, false);
    MAIN.with(|m| *m.borrow_mut() = Some(main.clone()));
    if let Some(path) = shot {
        glib::timeout_add_local_once(Duration::from_secs(45), move || shot_and_quit(path));
    }
    let urls: Vec<String> = urls.into_iter().filter(|u| u != "nouvelle-fenetre:").collect();
    let Some(first) = open_window(urls) else { return Err("fenêtre indisponible".into()) };
    // the other windows of the last session
    let extra = first.borrow_mut().app.take_extra_sessions();
    let profile = first.borrow().app.profile_marker();
    for tabs in extra {
        let mut w = tabs;
        w.extend(profile.clone());
        open_window(w);
    }
    drop(first);
    main.run();
    if let Some(sock) = sock.as_ref().filter(|_| single) {
        let _ = std::fs::remove_file(sock);
    }
    WINDOWS.with(|w| w.borrow_mut().clear());
    HOST.with(|h| *h.borrow_mut() = None);
    MAIN.with(|m| *m.borrow_mut() = None);
    Ok(())
}

/// The window used last (it gets the links opened from other applications).
fn last_used_window() -> Option<Rc<Host>> {
    let active = crate::page::ACTIVE_WINDOW.load(std::sync::atomic::Ordering::Relaxed);
    WINDOWS.with(|w| {
        let w = w.borrow();
        w.iter().find(|x| x.host.slot == active).or_else(|| w.first()).map(|x| x.host.clone())
    })
}

/// Close one window: its tabs are saved; if other windows stay open, they are
/// not reopened next time (as in Chrome). The last window quits Sanaga.
fn close_window(host: &Rc<Host>, gui: &RefCell<Gui>) {
    let others = WINDOWS.with(|w| w.borrow().iter().filter(|x| !Rc::ptr_eq(&x.host, host)).count());
    if let Ok(mut g) = gui.try_borrow_mut() {
        g.app.on_close();
        if others > 0 {
            g.app.close_window();
        }
    }
    WINDOWS.with(|w| w.borrow_mut().retain(|x| !Rc::ptr_eq(&x.host, host)));
    if HOST.with(|h| h.borrow().as_ref().is_some_and(|c| Rc::ptr_eq(c, host))) {
        let next = WINDOWS.with(|w| w.borrow().first().map(|x| x.host.clone()));
        HOST.with(|h| *h.borrow_mut() = next);
    }
    if others == 0 {
        quit_main();
    }
}

/// Quit Sanaga: every window saves its tabs, then the program ends.
fn quit_all() {
    let all: Vec<Rc<RefCell<Gui>>> = WINDOWS.with(|w| w.borrow().iter().filter_map(|x| x.gui.upgrade()).collect());
    for g in all {
        if let Ok(mut g) = g.try_borrow_mut() {
            g.app.on_close();
        }
    }
    quit_main();
}

fn button_of(b: u32) -> Option<PointerButton> {
    match b {
        1 => Some(PointerButton::Primary),
        2 => Some(PointerButton::Middle),
        3 => Some(PointerButton::Secondary),
        8 => Some(PointerButton::Extra1),
        9 => Some(PointerButton::Extra2),
        _ => None,
    }
}

/// Open a Sanaga window. `urls` as on the command line; the first window
/// restores the last session, the others start with the given addresses (or
/// the home page).
fn open_window(urls: Vec<String>) -> Option<Rc<RefCell<Gui>>> {
    let first = WINDOWS.with(|w| w.borrow().is_empty()) && !urls.iter().any(|u| u.starts_with("fenetre:"));
    let slot = if first {
        0
    } else {
        let n = NEXT_SLOT.with(|n| n.get());
        NEXT_SLOT.with(|c| c.set(n + 1));
        n
    };
    let mut urls = urls;
    if slot > 0 && !urls.iter().any(|u| u.starts_with("fenetre:")) {
        urls.push(format!("fenetre:{slot}"));
    }
    let urls: Vec<String> = urls.into_iter().map(|u| if u.starts_with("fenetre:") { format!("fenetre:{slot}") } else { u }).collect();

    let window = gtk::Window::new();
    window.set_title(Some("Sanaga"));
    window.set_default_size(1280, 800);
    window.set_size_request(560, 380);
    window.set_decorated(false);
    window.set_icon_name(Some("sanaga"));
    let overlay = gtk::Overlay::new();
    let area = gtk::GLArea::new();
    area.set_has_depth_buffer(false);
    area.set_auto_render(false);
    area.set_focusable(true);
    area.set_hexpand(true);
    area.set_vexpand(true);
    overlay.set_child(Some(&area));
    window.set_child(Some(&overlay));

    // the swipe arrow (Chrome-like round button at the page's edge)
    let swipe_hint = gtk::Label::new(Some("←"));
    swipe_hint.set_widget_name("sanaga-swipe");
    swipe_hint.set_valign(gtk::Align::Center);
    swipe_hint.set_visible(false);
    swipe_hint.set_can_target(false);
    // clicks and scrolling go through it to the page
    let device_frame = gtk::DrawingArea::new();
    device_frame.set_halign(gtk::Align::Start);
    device_frame.set_valign(gtk::Align::Start);
    device_frame.set_visible(false);
    device_frame.set_can_target(false);
    let device_pic: Rc<RefCell<Option<gtk::cairo::ImageSurface>>> = Rc::new(RefCell::new(None));
    {
        let pic = device_pic.clone();
        device_frame.set_draw_func(move |a, cr, _, _| {
            if let Some(s) = pic.borrow().as_ref() {
                let scale = a.scale_factor().max(1) as f64;
                cr.scale(1.0 / scale, 1.0 / scale);
                let _ = cr.set_source_surface(s, 0.0, 0.0);
                let _ = cr.paint();
            }
        });
    }
    overlay.add_overlay(&device_frame);
    overlay.add_overlay(&swipe_hint);
    let host_rc = Rc::new(Host {
        overlay: overlay.clone(),
        area: area.clone(),
        window: window.clone(),
        insets: Cell::new([0, 88, 0, 0]),
        swipe_hint,
        device_frame,
        device_pic,
        nav: Cell::new(0),
        shot: RefCell::new(None),
        tip_shown: RefCell::new(None),
        next: Rc::new(Cell::new(None)),
        wanted: Cell::new(false),
        ticking: Cell::new(false),
        slot,
    });
    // (the window being made is the one the interface talks to while it starts)
    HOST.with(|h| *h.borrow_mut() = Some(host_rc.clone()));

    let ctx = egui::Context::default();
    // repaint requests (also from other threads: network, favicons...)
    ctx.set_request_repaint_callback(move |info| {
        let delay = info.delay;
        glib::MainContext::default().invoke(move || frame_for(slot, delay));
    });
    let app = Sanaga::new(&ctx, urls, SHOT_JOB.with(|s| s.borrow_mut().take()));
    let gui = Rc::new(RefCell::new(Gui { a11y: new_a11y(&ctx), app, ctx: ctx.clone(), gl: None, painter: None, start: Instant::now(), cursor: egui::CursorIcon::Default, text_focus: false, frame: None, delta: Default::default() }));
    crate::page::ACTIVE_WINDOW.store(slot, std::sync::atomic::Ordering::Relaxed);
    let input = Rc::new(RefCell::new(Input { focused: true, ..Default::default() }));

    // text input (accents, compose key, input methods)
    let im = gtk::IMMulticontext::new();
    WINDOWS.with(|w| w.borrow_mut().push(Win { host: host_rc.clone(), gui: Rc::downgrade(&gui), input: input.clone(), im: im.clone() }));
    let waker = Wake { area: area.clone(), host: Rc::downgrade(&host_rc) };
    im.set_client_widget(Some(&area));
    {
        let (input, area) = (input.clone(), waker.clone());
        im.connect_commit(move |_, text| {
            input.borrow_mut().events.push(Event::Text(text.to_string()));
            area.queue_render();
        });
    }

    // ---- GL ----
    {
        let (gui, w) = (gui.clone(), waker.clone());
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
                    w.queue_render();
                }
                Err(e) => eprintln!("OpenGL indisponible : {e}"),
            }
        });
    }
    {
        let gui = gui.clone();
        area.connect_unrealize(move |area| {
            area.make_current();
            if let Ok(mut g) = gui.try_borrow_mut() {
                if let Some(mut p) = g.painter.take() {
                    p.destroy();
                }
            }
        });
    }
    {
        let (gui, w) = (gui.clone(), waker.clone());
        area.connect_render(move |area, _| {
            let Ok(mut g) = gui.try_borrow_mut() else { return glib::Propagation::Stop };
            draw(&mut g, area);
            // the window changed size: a new frame for the new size
            let size = [area.width().max(1), area.height().max(1)];
            if g.frame.as_ref().is_some_and(|f| f.size != size) {
                w.queue_render();
            }
            glib::Propagation::Stop
        });
    }

    // ---- input ----
    let input0 = input.clone();
    {
        let motion = gtk::EventControllerMotion::new();
        let (input, a) = (input0.clone(), waker.clone());
        motion.connect_motion(move |c, x, y| {
            let mut i = input.borrow_mut();
            i.modifiers = modifiers_of(c.current_event_state());
            i.events.push(Event::PointerMoved(pos2(x as f32, y as f32)));
            a.queue_render();
        });
        let (input, a) = (input0.clone(), waker.clone());
        motion.connect_leave(move |_| {
            input.borrow_mut().events.push(Event::PointerGone);
            a.queue_render();
        });
        area.add_controller(motion);
    }
    {
        let click = gtk::GestureClick::new();
        click.set_button(0);
        let (input, a) = (input0.clone(), waker.clone());
        click.connect_pressed(move |g, _, x, y| {
            if !a.has_focus() {
                a.grab_focus();
            }
            let mut i = input.borrow_mut();
            i.modifiers = modifiers_of(g.current_event_state());
            let b = g.current_button();
            i.press = (b, x, y, g.current_event_time());
            i.device = g.current_event_device();
            if let Some(button) = button_of(b) {
                let modifiers = i.modifiers;
                i.events.push(Event::PointerButton { pos: pos2(x as f32, y as f32), button, pressed: true, modifiers });
            }
            a.queue_render();
        });
        let (input, a) = (input0.clone(), waker.clone());
        click.connect_released(move |g, _, x, y| {
            let mut i = input.borrow_mut();
            i.modifiers = modifiers_of(g.current_event_state());
            if let Some(button) = button_of(g.current_button()) {
                let modifiers = i.modifiers;
                i.events.push(Event::PointerButton { pos: pos2(x as f32, y as f32), button, pressed: false, modifiers });
            }
            a.queue_render();
        });
        area.add_controller(click);
    }
    {
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        let (input, a) = (input0.clone(), waker.clone());
        scroll.connect_scroll(move |c, dx, dy| {
            // a mouse wheel moves by lines, a touchpad by points
            let (unit, delta) = if c.unit() == gdk::ScrollUnit::Wheel { (egui::MouseWheelUnit::Line, vec2(-dx as f32, -dy as f32)) } else { (egui::MouseWheelUnit::Point, vec2(-dx as f32, -dy as f32) * 1.0) };
            let mut i = input.borrow_mut();
            i.modifiers = modifiers_of(c.current_event_state());
            let modifiers = i.modifiers;
            i.events.push(Event::MouseWheel { unit, delta, phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Unknown, modifiers });
            a.queue_render();
            glib::Propagation::Stop
        });
        area.add_controller(scroll);
    }
    {
        let keys = gtk::EventControllerKey::new();
        keys.set_im_context(Some(&im));
        let (input, a) = (input0.clone(), waker.clone());
        keys.connect_key_pressed(move |_, kv, _, state| {
            let modifiers = modifiers_of(state);
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
                let (input, a2) = (input.clone(), a.clone());
                a.clipboard().read_text_async(None::<&gtk::gio::Cancellable>, move |res| {
                    if let Ok(Some(t)) = res {
                        input.borrow_mut().events.push(Event::Paste(t.replace("\r\n", "\n")));
                        a2.queue_render();
                    }
                });
            }
            a.queue_render();
            glib::Propagation::Stop
        });
        let (input, a) = (input0.clone(), waker.clone());
        keys.connect_key_released(move |_, kv, _, state| {
            // (GTK gives the state from before this key: a modifier let go
            // is still in it)
            let mut modifiers = modifiers_of(state);
            match kv.name().as_deref() {
                Some("Control_L" | "Control_R") => {
                    modifiers.ctrl = false;
                    modifiers.command = false;
                }
                Some("Shift_L" | "Shift_R") => modifiers.shift = false,
                Some("Alt_L" | "Alt_R" | "Meta_L" | "Meta_R") => modifiers.alt = false,
                _ => {}
            }
            let mut i = input.borrow_mut();
            i.modifiers = modifiers;
            if let Some(key) = key_of(kv) {
                i.events.push(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
            }
            a.queue_render();
        });
        area.add_controller(keys);
    }
    {
        let focus = gtk::EventControllerFocus::new();
        let im2 = im.clone();
        focus.connect_enter(move |_| im2.focus_in());
        let (input, a, im2) = (input0.clone(), waker.clone(), im.clone());
        focus.connect_leave(move |_| {
            im2.focus_out();
            let mut i = input.borrow_mut();
            i.chrome_lost_focus = true;
            i.modifiers = Modifiers::default();
            a.queue_render();
        });
        area.add_controller(focus);
    }
    {
        let (input, a) = (input0.clone(), waker.clone());
        window.connect_is_active_notify(move |w| {
            let f = w.is_active();
            let mut i = input.borrow_mut();
            i.focused = f;
            if f {
                crate::page::ACTIVE_WINDOW.store(slot, std::sync::atomic::Ordering::Relaxed);
            }
            i.events.push(Event::WindowFocused(f));
            a.queue_render();
        });
    }
    {
        let (gui, host) = (gui.clone(), host_rc.clone());
        window.connect_close_request(move |_| {
            close_window(&host, &gui);
            glib::Propagation::Proceed
        });
    }

    window.present();
    area.grab_focus();
    waker.queue_render();
    Some(gui)
}

fn update(g: &mut Gui, area: &gtk::GLArea, input: &Rc<RefCell<Input>>, host: &Rc<Host>, im: &gtk::IMMulticontext) {
    if g.painter.is_none() {
        return;
    }
    let scale = area.scale_factor().max(1) as f32;
    let (w, h) = (area.width().max(1), area.height().max(1));
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
        vp.maximized = Some(host.window.is_maximized());
        vp.fullscreen = Some(host.window.is_fullscreen());
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
            wake(host);
        }
    });
    INJECT_NOW.with(|q| raw.events.append(&mut q.borrow_mut()));
    // a screen reader is listening: egui describes the interface, and the
    // reader's actions become input
    if g.a11y.on.load(std::sync::atomic::Ordering::Relaxed) {
        g.ctx.enable_accesskit();
        if let Ok(mut q) = g.a11y.actions.lock() {
            raw.events.extend(q.drain(..).map(Event::AccessKitActionRequest));
        }
        g.a11y.adapter.update_window_focus_state(input.borrow().focused);
        let size = accesskit::Rect { x0: 0.0, y0: 0.0, x1: w as f64, y1: h as f64 };
        g.a11y.adapter.set_root_window_bounds(size, size);
    }
    TOOLTIP.with(|t| *t.borrow_mut() = None);
    let ctx = g.ctx.clone();
    let app = &mut g.app;
    let mut full = ctx.run_ui(raw, |ui| app.frame_ui(ui));
    if let Some(update) = full.platform_output.accesskit_update.take() {
        g.a11y.adapter.update_if_active(|| update);
    }
    // hover help: a system tooltip, above the page
    let tip = TOOLTIP.with(|t| t.borrow_mut().take());
    if *host.tip_shown.borrow() != tip {
        area.set_tooltip_text(tip.as_deref());
        if tip.is_some() {
            area.trigger_tooltip_query();
        }
        *host.tip_shown.borrow_mut() = tip;
    }

    // The keyboard follows: a text field of Sanaga's interface took it (address
    // bar, search...) -> to the interface; it let go of it (Enter, Escape) ->
    // back to the page shown.
    let text_focus = ctx.memory(|m| m.focused().is_some());
    if text_focus && !area.has_focus() {
        area.grab_focus();
    } else if !text_focus && g.text_focus && area.has_focus() {
        if let Some(page) = g.app.shown_page() {
            page.grab_focus();
        }
    }
    g.text_focus = text_focus;

    // what the interface asks of the window
    let out = &full.platform_output;
    if out.cursor_icon != g.cursor {
        g.cursor = out.cursor_icon;
        area.set_cursor_from_name(Some(cursor_name(out.cursor_icon)));
    }
    for c in &out.commands {
        match c {
            egui::OutputCommand::CopyText(t) => area.clipboard().set_text(t),
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
            let toplevel = win.surface().and_then(|s| s.downcast::<gdk::Toplevel>().ok());
            match c {
                ViewportCommand::Close => close = true,
                ViewportCommand::Title(t) => win.set_title(Some(t)),
                ViewportCommand::StartDrag => {
                    let i = input.borrow();
                    let (b, x, y, t) = i.press;
                    if let (Some(top), Some(dev)) = (&toplevel, &i.device) {
                        top.begin_move(dev, b.max(1) as i32, x, y, t);
                    }
                }
                ViewportCommand::BeginResize(dir) => {
                    use egui::ResizeDirection as D;
                    let edge = match dir {
                        D::North => gdk::SurfaceEdge::North,
                        D::South => gdk::SurfaceEdge::South,
                        D::East => gdk::SurfaceEdge::East,
                        D::West => gdk::SurfaceEdge::West,
                        D::NorthEast => gdk::SurfaceEdge::NorthEast,
                        D::NorthWest => gdk::SurfaceEdge::NorthWest,
                        D::SouthEast => gdk::SurfaceEdge::SouthEast,
                        D::SouthWest => gdk::SurfaceEdge::SouthWest,
                    };
                    let i = input.borrow();
                    let (b, x, y, t) = i.press;
                    if let Some(top) = &toplevel {
                        top.begin_resize(edge, i.device.as_ref(), b.max(1) as i32, x, y, t);
                    }
                }
                ViewportCommand::Maximized(true) => win.maximize(),
                ViewportCommand::Maximized(false) => win.unmaximize(),
                ViewportCommand::Minimized(true) => win.minimize(),
                ViewportCommand::Fullscreen(true) => win.fullscreen(),
                ViewportCommand::Fullscreen(false) => win.unfullscreen(),
                ViewportCommand::Focus => win.present(),
                _ => {}
            }
        }
    }

    // keep the picture for GTK, which draws it next
    let mut full = full;
    let prims = ctx.tessellate(std::mem::take(&mut full.shapes), full.pixels_per_point);
    g.delta.append(std::mem::take(&mut full.textures_delta));
    g.frame = Some(Frame { prims, ppp: full.pixels_per_point, size: [w, h] });
    area.queue_render();
    if close {
        if g.app.take_quit_all() {
            glib::idle_add_local_once(quit_all);
        } else {
            // close this window (as its close button does), once this frame is done
            let win = host.window.clone();
            glib::idle_add_local_once(move || win.close());
        }
        return;
    }
    if std::env::var_os("SANAGA_DEBUG_FRAMES").is_some() {
        eprintln!("frame t={:.2} next in {:?}", g.start.elapsed().as_secs_f64(), repaint);
    }
    schedule(host, repaint);
    if let Some(path) = host.shot.borrow_mut().take() {
        let win = host.window.clone();
        glib::idle_add_local_once(move || save_shot(&win, &path));
    }
}

/// GTK draws the window: Sanaga's interface goes into the GL area.
fn draw(g: &mut Gui, area: &gtk::GLArea) {
    if std::env::var_os("SANAGA_DEBUG_FRAMES").is_some() {
        eprintln!("draw {}x{} mapped={} has_frame={}", area.width(), area.height(), area.is_mapped(), g.frame.is_some());
    }
    let Some(frame) = g.frame.as_ref() else { return };
    let Some(p) = g.painter.as_mut() else { return };
    let scale = area.scale_factor().max(1) as f32;
    let (w, h) = (area.width().max(1), area.height().max(1));
    let size_px = [(w as f32 * scale) as u32, (h as f32 * scale) as u32];
    let mut delta = std::mem::take(&mut g.delta);
    p.clear(size_px, [0.0, 0.0, 0.0, 1.0]);
    p.paint_and_update_textures(size_px, frame.ppp, &frame.prims, &mut delta);
    if let Some(gl) = &g.gl {
        use glow::HasContext;
        if std::env::var_os("SANAGA_DEBUG_GL").is_some() {
            let e = unsafe { gl.get_error() };
            if e != 0 {
                eprintln!("GL error 0x{e:x}");
            }
        }
        // the window is opaque: blended text left see-through pixels behind
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.color_mask(false, false, false, true);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.color_mask(true, true, true, true);
        }
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
