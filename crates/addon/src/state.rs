use std::sync::Mutex;

static STATE: Mutex<Option<AddonState>> = Mutex::new(None);

pub struct AddonState {
    pub window_visible: bool,
}

fn lock_state() -> std::sync::MutexGuard<'static, Option<AddonState>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn init() {
    *lock_state() = Some(AddonState {
        window_visible: false,
    });
}

pub fn toggle_window() {
    if let Some(state) = lock_state().as_mut() {
        state.window_visible = !state.window_visible;
    }
}

pub fn is_window_visible() -> bool {
    lock_state()
        .as_ref()
        .map(|s| s.window_visible)
        .unwrap_or(false)
}
