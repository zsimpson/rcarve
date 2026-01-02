#![allow(dead_code)]

use std::marker::PhantomData;

#[derive(Debug, Clone)]
pub struct Im<T, const N_CH: usize, S = ()> {
    pub w: usize,
    pub h: usize,
    pub s: usize, // stride in elements (w * N_CH)
    pub arr: Vec<T>,
    _phantom: PhantomData<S>,
}

pub struct Binary;
pub struct Grayscale;
pub struct Rgba;

pub type MaskIm = Im<u8, 1, Binary>;
pub type Lum8Im = Im<u8, 1, Grayscale>;
pub type Lum16Im = Im<u16, 1, Grayscale>;
pub type RGBAIm = Im<u8, 4, Rgba>;

// Constructor
// -----------------------------------------------------------------------------
impl<T: Copy + Default, const N_CH: usize, S> Im<T, N_CH, S> {
    pub fn new(w: usize, h: usize) -> Self {
        let s = w * N_CH;
        let arr = vec![T::default(); s * h];
        Self {
            w,
            h,
            s,
            arr,
            _phantom: PhantomData,
        }
    }
}

impl<T, const N_CH: usize, S> Im<T, N_CH, S> {
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, x: usize, y: usize, ch: usize) -> &T {
        unsafe { self.arr.get_unchecked(y * self.s + x * N_CH + ch) }
    }

    #[inline(always)]
    pub unsafe fn get_unchecked_mut(&mut self, x: usize, y: usize, ch: usize) -> &mut T {
        unsafe { self.arr.get_unchecked_mut(y * self.s + x * N_CH + ch) }
    }

    /// Convert a linear sample index (as used by `pixels`/`pixels_mut`) into `(x, y, ch)`.
    #[inline]
    pub fn idx_to_xyc(&self, i: usize) -> (usize, usize, usize) {
        let y = i / self.s;
        let rem = i - y * self.s;
        let x = rem / N_CH;
        let ch = rem - x * N_CH;
        (x, y, ch)
    }

    /// Iterate over all channel samples in-place.
    ///
    /// The index `i` is a linear index into `self.arr` (i.e. includes channels).
    /// This is designed for quick in-place transforms and supports chaining.
    #[inline]
    pub fn pixels<F>(&mut self, mut f: F) -> &mut Self
    where
        F: FnMut(&mut T, usize),
    {
        for (i, v) in self.arr.iter_mut().enumerate() {
            f(v, i);
        }
        self
    }

    /// Mutable iterator over all channel samples.
    ///
    /// Yields `(i, v)` where `i` is the linear index into `self.arr`.
    #[inline]
    pub fn pixels_mut(&mut self) -> std::iter::Enumerate<std::slice::IterMut<'_, T>> {
        self.arr.iter_mut().enumerate()
    }
}

// Debug image viewer (feature-gated).
// -----------------------------------------------------------------------------

impl<S> Im<u8, 1, S> {
    /// Open a debug UI window showing this image.
    ///
    /// Enabled by default.
    ///
    /// To compile without the UI dependency: `cargo run --no-default-features --features cli_only`.
    pub fn debug_im(&self, title: &str) {
        #[cfg(all(feature = "debug_ui", not(feature = "cli_only")))]
        {
            if let Err(e) = super::debug_ui::show_u8_1(self, title) {
                println!("debug_im: {e}");
            }
            return;
        }
        #[cfg(not(all(feature = "debug_ui", not(feature = "cli_only"))))]
        {
            let _ = title;
            println!("debug_im: disabled (build without `--features cli_only`) ");
        }
    }
}

impl<S> Im<u8, 4, S> {
    /// Open a debug UI window showing this image.
    ///
    /// Enabled by default.
    ///
    /// To compile without the UI dependency: `cargo run --no-default-features --features cli_only`.
    pub fn debug_im(&self, title: &str) {
        #[cfg(all(feature = "debug_ui", not(feature = "cli_only")))]
        {
            if let Err(e) = super::debug_ui::show_u8_4(self, title) {
                println!("debug_im: {e}");
            }
            return;
        }
        #[cfg(not(all(feature = "debug_ui", not(feature = "cli_only"))))]
        {
            let _ = title;
            println!("debug_im: disabled (build without `--features cli_only`) ");
        }
    }
}

impl<S> Im<u16, 1, S> {
    /// Open a debug UI window showing this image.
    ///
    /// Display is downscaled for viewing (uses the high 8 bits), but hover readout
    /// shows the full `u16` value.
    pub fn debug_im(&self, title: &str) {
        #[cfg(all(feature = "debug_ui", not(feature = "cli_only")))]
        {
            if let Err(e) = super::debug_ui::show_u16_1(self, title) {
                println!("debug_im: {e}");
            }
            return;
        }
        #[cfg(not(all(feature = "debug_ui", not(feature = "cli_only"))))]
        {
            let _ = title;
            println!("debug_im: disabled (build without `--features cli_only`) ");
        }
    }
}

// Convenience APIs that don't depend on external crates.
// -----------------------------------------------------------------------------

impl<S> Im<i32, 1, S> {
    pub fn to_mask_im(&self) -> MaskIm {
        let mut mask_im = MaskIm::new(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                let v = unsafe { *self.get_unchecked(x, y, 0) };
                let m: u8 = if v != 0 { 255 } else { 0 };
                unsafe {
                    *mask_im.get_unchecked_mut(x, y, 0) = m;
                }
            }
        }
        mask_im
    }
}

pub fn copy_mask_im_to_rgba_im(src: &MaskIm, dst: &mut RGBAIm, r: u8, g: u8, b: u8) {
    assert_eq!(src.w, dst.w, "width mismatch");
    assert_eq!(src.h, dst.h, "height mismatch");

    for y in 0..src.h {
        for x in 0..src.w {
            let m = unsafe { *src.get_unchecked(x, y, 0) };
            let rgba = if m != 0 {
                [r, g, b, 255_u8]
            } else {
                [0_u8, 0_u8, 0_u8, 255_u8]
            };
            for ch in 0..4 {
                unsafe {
                    *dst.get_unchecked_mut(x, y, ch) = rgba[ch];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_inplace_transform_runs_in_order() {
        let mut im = Im::<u8, 1>::new(3, 1);
        im.arr.copy_from_slice(&[10, 20, 250]);

        im.pixels(|v, _i| {
            *v = (*v as u16 * 20).min(255) as u8;
        });

        assert_eq!(im.arr, vec![200, 255, 255]);
    }
}
