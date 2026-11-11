//! See Cargo.toml. WebKit calls `webkit_web_process_extension_initialize`
//! when a page process starts; each time a page is made in it, the process
//! says who it is to that page's view in Sanaga (a "user message", WebKit's
//! own channel between a page process and the browser — pages can't use it).
//!
//! Who it is: when the process started and its number as it sees it. Inside
//! WebKit's sandbox a process only knows its number in the sandbox (often
//! 2), so its real number is found on Sanaga's side, from the start time
//! (the same seen from inside and outside) — see `net::processes_cached`.
//!
//! The few GLib and WebKit functions used are declared by hand: they are
//! already loaded in the page process, and Sanaga's build needs nothing more.

use std::ffi::{c_char, c_int, c_ulong, c_void, CString};
use std::ptr::null_mut;

extern "C" {
    fn g_signal_connect_data(instance: *mut c_void, signal: *const c_char, handler: Option<unsafe extern "C" fn()>, data: *mut c_void, destroy_data: *mut c_void, flags: c_int) -> c_ulong;
    fn g_variant_new_string(text: *const c_char) -> *mut c_void;
    fn webkit_user_message_new(name: *const c_char, parameters: *mut c_void) -> *mut c_void;
    fn webkit_web_page_send_message_to_view(page: *mut c_void, message: *mut c_void, cancellable: *mut c_void, callback: *mut c_void, user_data: *mut c_void);
}

/// "start time:process number", from /proc/self/stat (field 22: when the
/// process started, in clock ticks since the computer did).
fn who_am_i() -> Option<CString> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // (the name, in brackets, may hold spaces: count after it)
    let start = stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse::<u64>().ok()?;
    CString::new(format!("{start}:{}", std::process::id())).ok()
}

/// A page was made in this process: tell its view.
unsafe extern "C" fn page_created(_extension: *mut c_void, page: *mut c_void, _data: *mut c_void) {
    let Some(me) = who_am_i() else { return };
    // SAFETY: `page` is the WebKitWebPage WebKit hands to this signal; the
    // message takes the (floating) text, and WebKit takes the message.
    unsafe {
        let message = webkit_user_message_new(c"sanaga-process".as_ptr(), g_variant_new_string(me.as_ptr()));
        webkit_web_page_send_message_to_view(page, message, null_mut(), null_mut(), null_mut());
    }
}

/// Called by WebKit when a page process starts.
///
/// # Safety
/// `extension` must be the WebKitWebProcessExtension of this process (it is:
/// only WebKit calls this).
#[no_mangle]
pub unsafe extern "C" fn webkit_web_process_extension_initialize(extension: *mut c_void) {
    // SAFETY: a signal of that object, with the handler's real signature
    // (GLib stores handlers as `fn()`, and calls them with the signal's own).
    unsafe {
        let handler = std::mem::transmute::<unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void), unsafe extern "C" fn()>(page_created);
        g_signal_connect_data(extension, c"page-created".as_ptr(), Some(handler), null_mut(), null_mut(), 0);
    }
}
