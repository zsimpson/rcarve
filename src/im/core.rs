#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Im<T, const N_CH: usize> {
    pub w: usize,
    pub h: usize,
    pub s: usize, // stride in elements (w * N_CH)
    pub arr: Vec<T>,
}

// Constructor
// -----------------------------------------------------------------------------
impl<T: Copy + Default, const N_CH: usize> Im<T, N_CH> {
    pub fn new(w: usize, h: usize) -> Self {
        let s = w * N_CH;
        let arr = vec![T::default(); s * h];
        Self { w, h, s, arr }
    }
}

impl<T, const N_CH: usize> Im<T, N_CH> {
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, x: usize, y: usize, ch: usize) -> &T {
        unsafe { self.arr.get_unchecked(y * self.s + x * N_CH + ch) }
    }

    #[inline(always)]
    pub unsafe fn get_unchecked_mut(&mut self, x: usize, y: usize, ch: usize) -> &mut T {
        unsafe { self.arr.get_unchecked_mut(y * self.s + x * N_CH + ch) }
    }
}

// Pixel-wise arithmetic helpers (no external crates).
// -----------------------------------------------------------------------------

pub trait MulClampMax: Copy {
    fn mul_clamp_max(self, rhs: Self) -> Self;
}

macro_rules! impl_mul_clamp_max_int {
    ($($t:ty),* $(,)?) => {
        $(
            impl MulClampMax for $t {
                #[inline(always)]
                fn mul_clamp_max(self, rhs: Self) -> Self {
                    self.checked_mul(rhs).unwrap_or(<$t>::MAX)
                }
            }
        )*
    };
}

impl_mul_clamp_max_int!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);

impl<T: MulClampMax, const N_CH: usize> Im<T, N_CH> {
    pub fn mul_const_clamp_max_inplace(&mut self, k: T) -> &mut Self {
        for v in &mut self.arr {
            *v = (*v).mul_clamp_max(k);
        }
        self
    }

    pub fn mul_const_clamp_max(&self, k: T) -> Self {
        let mut out = self.clone();
        out.mul_const_clamp_max_inplace(k);
        out
    }
}

// Convenience APIs that don't depend on external crates.
// -----------------------------------------------------------------------------

impl Im<i32, 1> {
    pub fn to_mask_im(&self) -> Im<u8, 1> {
        let mut mask_im = Im::<u8, 1>::new(self.w, self.h);
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

pub type RGBAIm = Im<u8, 4>;
pub type MaskIm = Im<u8, 1>;
pub type Lum8Im = Im<u8, 1>;
pub type Lum16Im = Im<u16, 1>;

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
    fn mul_const_clamp_max_u8_clamps_on_overflow() {
        let mut im = Im::<u8, 1>::new(2, 1);
        im.arr[0] = 200;
        im.arr[1] = 10;

        let out = im.mul_const_clamp_max(2);
        assert_eq!(out.arr, vec![255, 20]);
    }
}
