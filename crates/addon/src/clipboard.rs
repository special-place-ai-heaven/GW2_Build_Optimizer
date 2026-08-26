//! Windows clipboard. ImGui's `set_clipboard_text` stays inside the overlay
//! and never reaches GW2's Paste Build Template.

use std::os::raw::c_void;

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

pub fn copy_text(text: &str) -> bool {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let bytes = wide.len() * 2;
    unsafe {
        // Another process briefly holding the clipboard (clipboard managers,
        // remote-desktop sync) makes OpenClipboard fail transiently — retry
        // instead of silently dropping the user's copy.
        let mut opened = 0;
        while OpenClipboard(std::ptr::null_mut()) == 0 {
            opened += 1;
            if opened >= 3 {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = EmptyClipboard();
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if handle.is_null() {
            let _ = CloseClipboard();
            return false;
        }
        let ptr = GlobalLock(handle);
        if ptr.is_null() {
            let _ = GlobalFree(handle);
            let _ = CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr().cast::<u8>(), ptr.cast::<u8>(), bytes);
        let _ = GlobalUnlock(handle);
        let ok = !SetClipboardData(CF_UNICODETEXT, handle).is_null();
        if !ok {
            let _ = GlobalFree(handle);
        }
        let _ = CloseClipboard();
        ok
    }
}
