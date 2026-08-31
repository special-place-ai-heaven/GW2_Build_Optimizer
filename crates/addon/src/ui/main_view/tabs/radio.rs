//! Radio tab — search radio-browser.info, play icecast streams in the
//! background, favorites, now-playing, choya DJ in the corner.

use crate::state::AddonState;
use gw2_core::i18n::t;
use nexus::imgui::Ui;

pub(in crate::ui::main_view) fn render_radio_tab(ui: &Ui, state: &mut AddonState) {
    // stub: UI workstream
    let _ = state;
    ui.text(t("tab.radio"));
}
