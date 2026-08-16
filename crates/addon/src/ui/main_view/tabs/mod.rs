//! Sub-tab renderers dispatched by `render_main_content`.
//!
//! Each tab lives in its own file:
//! - `new_build` — scenario summary + comparison (role lives in the left menu)
//! - `improve` — iterate on an existing build
//! - `settings` — API keys, models, theme, cache
//! - `saveload` — saved-build list + reconstruction
//!
//! Helpers shared between sibling tabs (e.g. `render_optimization_progress`)
//! live in the parent `main_view` module and are re-exported here so each tab
//! can refer to them as `super::name` (one hop) instead of `super::super::name`.

pub(super) mod improve;
pub(super) mod new_build;
pub(super) mod saveload;
pub(super) mod settings;

pub(super) use super::{build_display, lock_panel, optimization, render_optimization_progress};
