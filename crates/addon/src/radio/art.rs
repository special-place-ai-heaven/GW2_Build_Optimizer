//! Choya DJ corner sprite + radio garnish (ON AIR badge, EQ bars) from the
//! `choya_radio.png` atlas (1536x1024 RGBA, embedded).
//!
//! Contract (filled by the UI workstream): self-contained — own embedded
//! texture helper (mirroring `theme::embedded_tex`), own rect tables, drawn
//! into the Radio tab corner via draw-list blits keyed off `RadioStatus`
//! (sleep+zzz when idle/stopped, dance while connecting, idle bob + mix
//! bursts + ON AIR while playing).

use crate::state::AddonState;
use nexus::imgui::Ui;

/// Draw the DJ choya in the tab corner, state-driven.
pub fn draw_corner_choya(ui: &Ui, state: &AddonState) {
    let _ = (ui, state); // stub: UI workstream
}
