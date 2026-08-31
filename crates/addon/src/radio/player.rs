//! Playback core: stream-download + icy-metadata + rodio on a dedicated
//! tokio runtime (1 worker), owned by this module behind a
//! `Mutex<Option<...>>` so `shutdown` can tear it down deterministically.
//!
//! Contract (filled by the playback workstream):
//! - Never touch rodio/cpal from the render thread; a dedicated audio-owner
//!   thread holds the `OutputStream` and is JOINED on shutdown.
//! - Tune-in guards: connect_timeout 10s, 15s cap on the header phase only,
//!   NO timeout on the stream body; prefetch 64 KiB; 512 KiB bounded ring.
//! - Status/now-playing updates go through `crate::state::with_state`.
//! - `shutdown` must return well inside `state::UNLOAD_JOIN_BUDGET`.

use super::RbStation;

/// Start playing `station`, replacing any current stream.
pub fn play(station: &RbStation) {
    let _ = station; // stub: playback workstream
}

/// Stop playback and release the audio device. Cheap to call when idle.
pub fn stop() {}

/// Keybind toggle: playing -> stop; otherwise re-tune the current/last station.
pub fn toggle() {}

/// Output gain from 0-100 percent; log taper applied inside.
pub fn set_volume(percent: u8) {
    let _ = percent; // stub: playback workstream
}

/// Unload teardown: stop sink -> join audio-owner thread -> drop reader ->
/// shut the tokio runtime down (bounded). Called from `on_unload` BEFORE
/// worker cancellation; must never block long or panic.
pub fn shutdown() {}
