use std::sync::Mutex;

static STATE: Mutex<Option<AddonState>> = Mutex::new(None);

pub struct AddonState {
    pub window_visible: bool,
}

pub fn init() {
    let mut lock = STATE.lock().unwrap();
    *lock = Some(AddonState {
        window_visible: false,
    });
}

pub fn toggle_window() {
    if let Some(state) = STATE.lock().unwrap().as_mut() {
        state.window_visible = !state.window_visible;
    }
}

pub fn is_window_visible() -> bool {
    STATE
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.window_visible)
        .unwrap_or(false)
}
