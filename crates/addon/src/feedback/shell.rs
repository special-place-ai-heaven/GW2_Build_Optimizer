//! Open a URL in the player's default browser via `ShellExecuteW`.
//!
//! Raw `extern "system"` declaration, no `windows-sys` / `open` crate — same
//! precedent as `GetUserDefaultUILanguage` in `crates/core/src/i18n.rs`.

/// Open a URL with the system's default handler (ShellExecuteW "open").
/// Returns false when Windows refuses.
///
/// Only `http://` / `https://` URLs are passed on: `ShellExecuteW("open", …)`
/// happily launches local programs and `file:` paths, and the URL may come
/// from a server-supplied taxonomy document.
pub fn open_url(url: &str) -> bool {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return false;
    }
    launch(url)
}

#[cfg(windows)]
fn launch(url: &str) -> bool {
    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: *mut core::ffi::c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show_cmd: i32,
        ) -> isize;
    }

    /// NUL-terminated UTF-16 for the Win32 W-API.
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    const SW_SHOWNORMAL: i32 = 1;

    let operation = wide("open");
    let file = wide(url);
    // SAFETY: both buffers are NUL-terminated and outlive the call; the
    // remaining pointers are documented-nullable (no owner window, no
    // parameters, default directory). ShellExecuteW does not retain them.
    let result = unsafe {
        ShellExecuteW(
            core::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            core::ptr::null(),
            core::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Per the Win32 docs the return is an HINSTANCE-shaped error code: > 32 is success.
    result > 32
}

#[cfg(not(windows))]
fn launch(_url: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_schemes() {
        // Guard path only: none of these may reach ShellExecuteW.
        assert!(!open_url("file:///C:/x"));
        assert!(!open_url("cmd.exe"));
        assert!(!open_url(""));
        assert!(!open_url("javascript:alert(1)"));
        assert!(!open_url("HTTP://not-lowercase.example"));
        assert!(!open_url(" https://leading-space.example"));
    }
}
