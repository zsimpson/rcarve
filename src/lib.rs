// Library crate root.
//
// This crate is used both as a binary (src/main.rs) and as a library.
// Keeping modules here prevents "dead_code" warnings for public APIs that are
// intentionally exported for downstream crates.

pub mod bucket_vec;
pub mod debug_ui;
pub mod desc;
pub mod dilate_im;
pub mod im;
pub mod mat3;
pub mod mpoly;
pub mod region_tree;
pub mod sim;
pub mod toolpath;
pub mod trace;

#[cfg(test)]
pub mod test_helpers;
