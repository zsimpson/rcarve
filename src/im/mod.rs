pub mod core;
#[allow(unused_imports)]
pub use core::{copy_mask_im_to_rgba_im, Im, Im1Mut, Lum16Im, Lum8Im, MaskIm, RGBAIm};

pub mod roi;
#[allow(unused_imports)]
pub use roi::ROI;

// Optional extras
// -----------------------------------------------------------------------------

#[cfg(feature = "im-io")]
pub mod io;

#[cfg(feature = "im-label")]
pub mod label;

#[cfg(feature = "im-label")]
#[allow(unused_imports)]
pub use label::{label_im, LabelInfo};

// Debug UI window
// -----------------------------------------------------------------------------

#[cfg(all(feature = "debug_ui", not(feature = "cli_only")))]
pub mod debug_ui;
