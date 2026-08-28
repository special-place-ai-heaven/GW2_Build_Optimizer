//! Windows clipboard. ImGui's `set_clipboard_text` stays inside the overlay
//! and never reaches GW2's Paste Build Template.

use std::os::raw::c_void;
use std::sync::Mutex;

const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 0x0002;

#[link(name = "user32")]
extern "system" {
    fn OpenClipboard(owner: *mut c_void) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, mem: *mut c_void) -> *mut c_void;
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
    fn GlobalLock(mem: *mut c_void) -> *mut c_void;
    fn GlobalUnlock(mem: *mut c_void) -> i32;
    fn GlobalFree(mem: *mut c_void) -> *mut c_void;
}

/// What one attempt at the clipboard did.
enum Attempt {
    Copied,
    /// Another process held the clipboard. Worth retrying.
    Busy,
    /// The copy cannot happen: allocation refused, or the clipboard rejected it.
    Failed,
}

/// A copy that lost the race for the clipboard, waiting for a later frame.
struct Pending {
    /// NUL-terminated UTF-16, already encoded.
    wide: Vec<u16>,
    tries: u8,
}

/// Total attempts at the clipboard, the immediate one included — the same three
/// the sleeping loop used to make, now spread over three frames (~35 ms at 60
/// fps) instead of 60 ms of stalled draw calls. Long enough for a clipboard
/// manager to let go, short enough that the text never lands much later on top
/// of something the player copied since.
const MAX_TRIES: u8 = 3;

/// At most one queued copy: a second click means the player wants the newer
/// text, so it replaces the older one rather than queueing behind it.
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

/// Poison-tolerant lock. Nothing panics while this guard is held, but a
/// poisoned mutex must not disable copying for the rest of the session.
fn pending() -> std::sync::MutexGuard<'static, Option<Pending>> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Put `text` on the Windows clipboard.
///
/// Returns true when the text is on the clipboard **or** queued for the next
/// frames — another process (clipboard manager, remote-desktop sync) can hold
/// the clipboard for a few milliseconds, and the caller's "Copied!" feedback
/// would be wrong to call that a failure. False means the copy cannot happen
/// at all.
///
/// Never sleeps: every call site is on the render thread, which is the game's
/// only draw pass, so the retry rides later frames instead — see [`pump`].
pub fn copy_text(text: &str) -> bool {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    // SAFETY: `wide` is a live, NUL-terminated UTF-16 buffer for the whole call.
    match unsafe { set_clipboard(&wide) } {
        Attempt::Copied => {
            *pending() = None;
            true
        }
        Attempt::Busy => {
            *pending() = Some(Pending { wide, tries: 1 });
            true
        }
        Attempt::Failed => {
            *pending() = None;
            false
        }
    }
}

/// Retry a copy that lost the race, once. Called at the top of every overlay
/// frame from [`crate::ui::render`], with `STATE` **not** held.
pub fn pump() {
    let Some(mut queued) = pending().take() else {
        return;
    };
    // SAFETY: same contract as `copy_text` — `queued.wide` is NUL-terminated.
    if let Attempt::Busy = unsafe { set_clipboard(&queued.wide) } {
        queued.tries += 1;
        if queued.tries < MAX_TRIES {
            // A copy queued while this frame was retrying is newer: keep it.
            let mut slot = pending();
            if slot.is_none() {
                *slot = Some(queued);
            }
        }
    }
}

/// One attempt to publish `wide` (NUL-terminated UTF-16). The caller owns the
/// retry policy; this never sleeps and never loops.
///
/// # Safety
/// `wide` must be a valid, NUL-terminated UTF-16 buffer.
unsafe fn set_clipboard(wide: &[u16]) -> Attempt {
    if OpenClipboard(std::ptr::null_mut()) == 0 {
        return Attempt::Busy;
    }
    let bytes = std::mem::size_of_val(wide);
    let _ = EmptyClipboard();
    let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
    if handle.is_null() {
        let _ = CloseClipboard();
        return Attempt::Failed;
    }
    let ptr = GlobalLock(handle);
    if ptr.is_null() {
        let _ = GlobalFree(handle);
        let _ = CloseClipboard();
        return Attempt::Failed;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr().cast::<u8>(), ptr.cast::<u8>(), bytes);
    let _ = GlobalUnlock(handle);
    let ok = !SetClipboardData(CF_UNICODETEXT, handle).is_null();
    if !ok {
        let _ = GlobalFree(handle);
    }
    let _ = CloseClipboard();
    if ok {
        Attempt::Copied
    } else {
        Attempt::Failed
    }
}
