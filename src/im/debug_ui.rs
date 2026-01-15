// Thin wrapper module.
//
// The actual UI implementation lives in crate::debug_ui.
// This file exists to preserve the existing `im::debug_ui::*` call sites.

pub use crate::debug_ui::{show_u16_1, show_u8_1, show_u8_4};
