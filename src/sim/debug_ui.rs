// Thin wrapper module.
//
// The actual UI implementation lives in crate::debug_ui.
// This file exists to preserve the existing `sim::debug_ui::*` call sites.

pub use crate::debug_ui::show_toolpath_movie;
