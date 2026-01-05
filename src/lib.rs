// Library crate root.
//
// This crate is used both as a binary (src/main.rs) and as a library.
// Keeping modules here prevents "dead_code" warnings for public APIs that are
// intentionally exported for downstream crates.

pub mod im;
pub mod desc;
pub mod region_tree;
pub mod toolpath;
pub mod trace;
pub mod mpoly;
pub mod dilate_im;
pub mod mat3;
pub mod sim;

#[cfg(test)]
pub mod test_helpers;
