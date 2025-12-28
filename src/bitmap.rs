#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Bitmap<T> {
    pub w: usize,
    pub h: usize,
    pub s: usize, // stride: elements per row
    pub arr: Vec<T>,
}

impl<T: Copy + Default> Bitmap<T> {
    pub fn new(w: usize, h: usize) -> Self {
        let s = w;
        let arr = vec![T::default(); s * h];
        Self { w, h, s, arr }
    }
}

impl<T> Bitmap<T> {
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, x: usize, y: usize) -> &T {
        unsafe { self.arr.get_unchecked(y * self.s + x) }
    }

    #[inline(always)]
    pub unsafe fn get_unchecked_mut(&mut self, x: usize, y: usize) -> &mut T {
        unsafe { self.arr.get_unchecked_mut(y * self.s + x) }
    }
}
