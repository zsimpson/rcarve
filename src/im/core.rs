#![allow(dead_code)]

use std::marker::PhantomData;

use crate::im::roi;

#[derive(Debug, Clone)]
pub struct Im<T, const N_CH: usize, S = ()> {
    pub w: usize,
    pub h: usize,
    pub s: usize, // stride in elements (w * N_CH)
    pub arr: Vec<T>,
    _phantom: PhantomData<S>,
}

impl<T: PartialEq, const N_CH: usize, S> PartialEq for Im<T, N_CH, S> {
    fn eq(&self, other: &Self) -> bool {
        self.w == other.w && self.h == other.h && self.s == other.s && self.arr == other.arr
    }
}

impl<T: Eq, const N_CH: usize, S> Eq for Im<T, N_CH, S> {}

#[derive(Clone, Copy, Debug)]
pub struct Binary;

#[derive(Clone, Copy, Debug)]
pub struct Grayscale;

#[derive(Clone, Copy, Debug)]
pub struct Rgba;

pub type MaskIm = Im<u8, 1, Binary>;
pub type Lum8Im = Im<u8, 1, Grayscale>;
pub type Lum16Im = Im<u16, 1, Grayscale>;
pub type Lum32Im = Im<u32, 1, Grayscale>;
pub type RGBAIm = Im<u8, 4, Rgba>;

/// Minimal trait for working with 1-channel images generically.
///
/// This is intentionally small so simulation / drawing code can operate on any
/// `Im<T, 1, S>` regardless of its semantic tag `S`.
pub trait Im1Mut<T> {
    /// Stride in elements (for 1-channel, this is just the image width).
    fn stride(&self) -> usize;

    /// Mutable access to the underlying linear buffer.
    fn arr_mut(&mut self) -> &mut [T];
}

impl<T, S> Im1Mut<T> for Im<T, 1, S> {
    #[inline]
    fn stride(&self) -> usize {
        self.s
    }

    #[inline]
    fn arr_mut(&mut self) -> &mut [T] {
        &mut self.arr
    }
}

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
    /// Change the semantic tag type parameter `S` without touching pixel data.
    ///
    /// This is useful when you want to treat the same underlying image buffer as a
    /// different semantic type (e.g. `Im<u16, 1>` -> `Im<u16, 1, RegionI>`).
    #[inline]
    pub fn retag<S2>(self) -> Im<T, N_CH, S2> {
        Im {
            w: self.w,
            h: self.h,
            s: self.s,
            arr: self.arr,
            _phantom: PhantomData,
        }
    }

    #[inline(always)]
    pub fn get_or_default(&self, x: usize, y: usize, ch: usize, default: T) -> T
    where
        T: Copy,
    {
        self.arr
            .get(y * self.s + x * N_CH + ch)
            .copied()
            .unwrap_or(default)
    }

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

// Drawing helpers for 1-channel images.
// -----------------------------------------------------------------------------

impl<T: Copy, S> Im<T, 1, S> {
    /// Draw a 1-pixel outline along the *ROI border* (top/bottom/left/right) with `value`.
    ///
    /// ROI uses left/top inclusive and right/bottom exclusive bounds.
    pub fn one_pixel_border_along_roi(&mut self, roi: roi::ROI, value: T) -> &mut Self {
        if self.w == 0 || self.h == 0 {
            return self;
        }

        // Clamp ROI to image bounds (right/bottom are exclusive).
        let l = roi.l.min(self.w);
        let t = roi.t.min(self.h);
        let r = roi.r.min(self.w);
        let b = roi.b.min(self.h);

        if r <= l || b <= t {
            return self;
        }

        let y_top = t;
        let y_bot = b - 1;
        let x_left = l;
        let x_right = r - 1;

        // Top/bottom edges
        for x in l..r {
            unsafe {
                *self.get_unchecked_mut(x, y_top, 0) = value;
                *self.get_unchecked_mut(x, y_bot, 0) = value;
            }
        }

        // Left/right edges
        for y in t..b {
            unsafe {
                *self.get_unchecked_mut(x_left, y, 0) = value;
                *self.get_unchecked_mut(x_right, y, 0) = value;
            }
        }

        self
    }

    /// Draw a 1-pixel border on the *image edges* (x=0/x=w-1/y=0/y=h-1), but only
    /// over the span covered by `roi`.
    pub fn one_pixel_border_on_image_edges_over_roi_span(
        &mut self,
        roi: roi::ROI,
        value: T,
    ) -> &mut Self {
        if self.w == 0 || self.h == 0 {
            return self;
        }

        let l = roi.l.min(self.w);
        let t = roi.t.min(self.h);
        let r = roi.r.min(self.w);
        let b = roi.b.min(self.h);
        if r <= l || b <= t {
            return self;
        }

        let x0 = 0usize;
        let x1 = self.w - 1;
        let y0 = 0usize;
        let y1 = self.h - 1;

        for y in t..b {
            unsafe {
                *self.get_unchecked_mut(x0, y, 0) = value;
                *self.get_unchecked_mut(x1, y, 0) = value;
            }
        }

        for x in l..r {
            unsafe {
                *self.get_unchecked_mut(x, y0, 0) = value;
                *self.get_unchecked_mut(x, y1, 0) = value;
            }
        }

        self
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

pub fn copy_mask_im_to_lum32_im(src: &MaskIm, dst: &mut Lum32Im) {
    assert_eq!(src.w, dst.w, "width mismatch");
    assert_eq!(src.h, dst.h, "height mismatch");

    for y in 0..src.h {
        for x in 0..src.w {
            let v = unsafe { *src.get_unchecked(x, y, 0) };
            unsafe {
                *dst.get_unchecked_mut(x, y, 0) = v as u32;
            }
        }
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

impl Im<u8, 1, Binary> {
    pub fn invert(&mut self) -> &mut Self {
        for y in 0..self.h {
            for x in 0..self.w {
                let v = unsafe { *self.get_unchecked(x, y, 0) };
                let inv: u8 = if v == 0 { 255 } else { 0 };
                unsafe {
                    *self.get_unchecked_mut(x, y, 0) = inv;
                }
            }
        }
        self
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

    #[test]
    fn mask_im_inverted_flips_zero_and_nonzero() {
        let mut m = MaskIm::new(3, 1);
        m.arr.copy_from_slice(&[0, 1, 255]);
        m.invert();
        assert_eq!(m.arr, vec![255, 0, 0]);
    }

    #[test]
    fn mask_im_one_pixel_border_along_roi_draws_roi_outline() {
        let mut m = MaskIm::new(5, 4);

        // ROI: x in [1,4), y in [1,3)
        m.one_pixel_border_along_roi(
            roi::ROI {
                l: 1,
                t: 1,
                r: 4,
                b: 3,
            },
            255,
        );

        // Top edge (y=1): x=1..3
        assert_eq!(m.get_or_default(1, 1, 0, 0), 255);
        assert_eq!(m.get_or_default(2, 1, 0, 0), 255);
        assert_eq!(m.get_or_default(3, 1, 0, 0), 255);

        // Bottom edge (y=2): x=1..3
        assert_eq!(m.get_or_default(1, 2, 0, 0), 255);
        assert_eq!(m.get_or_default(2, 2, 0, 0), 255);
        assert_eq!(m.get_or_default(3, 2, 0, 0), 255);

        // Outside ROI should remain 0
        assert_eq!(m.get_or_default(0, 0, 0, 0), 0);
        assert_eq!(m.get_or_default(4, 3, 0, 0), 0);
    }

    #[test]
    fn mask_im_one_pixel_border_on_image_edges_over_roi_span_touches_only_image_edges() {
        let mut m = MaskIm::new(5, 4);
        m.one_pixel_border_on_image_edges_over_roi_span(
            roi::ROI {
                l: 1,
                t: 1,
                r: 4,
                b: 3,
            },
            255,
        );

        // Image edges should be set over ROI's span.
        assert_eq!(m.get_or_default(0, 1, 0, 0), 255);
        assert_eq!(m.get_or_default(4, 2, 0, 0), 255);
        assert_eq!(m.get_or_default(1, 0, 0, 0), 255);
        assert_eq!(m.get_or_default(3, 3, 0, 0), 255);

        // Interior pixels should remain untouched.
        assert_eq!(m.get_or_default(2, 1, 0, 0), 0);
        assert_eq!(m.get_or_default(2, 2, 0, 0), 0);
    }
}
