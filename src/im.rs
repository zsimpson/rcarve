#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Im<T> {
    pub w: usize,
    pub h: usize,
    pub s: usize, // stride: elements per row
    pub n_ch: usize,
    pub arr: Vec<T>,
}

impl<T: Copy + Default> Im<T> {
    pub fn new(w: usize, h: usize, n_ch: usize) -> Self {
        let s = w * n_ch;
        let arr = vec![T::default(); s * h];
        Self { w, h, s, n_ch, arr }
    }
}

impl<T> Im<T> {
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, x: usize, y: usize) -> &T {
        unsafe { self.arr.get_unchecked(y * self.s + x) }
    }

    #[inline(always)]
    pub unsafe fn get_unchecked_mut(&mut self, x: usize, y: usize) -> &mut T {
        unsafe { self.arr.get_unchecked_mut(y * self.s + x) }
    }
}

use std::io::ErrorKind;
use std::io::Error;
use std::path::Path;
use image::ImageResult;
use image::ImageError;

impl Im<u8> {
    fn invalid_input(msg: impl Into<String>) -> ImageError {
        ImageError::IoError(Error::new(ErrorKind::InvalidInput, msg.into()))
    }

    /// Load PNG from `path` into an `Im<u8>`.
    /// - `n_ch == 1`: produce a single-channel grayscale image (Luma)
    /// - `n_ch == 4`: produce an RGBA image (4 channels)
    pub fn new_from_png<P: AsRef<Path>>(path: P, n_ch: usize) -> ImageResult<Self> {
        let dynimg = image::open(path)?;
        let w = dynimg.width() as usize;
        let h = dynimg.height() as usize;

        if n_ch != 1 && n_ch != 4 {
            return Err(Self::invalid_input(format!(
                "Im<u8>::new_from_png supports n_ch=1 or n_ch=4 (got n_ch={})",
                n_ch
            )));
        }

        let mut im = Im::<u8>::new(w, h, n_ch);

        if n_ch == 1 {
            let luma = dynimg.to_luma8();
            for y in 0..h {
                for x in 0..w {
                    let v = luma.get_pixel(x as u32, y as u32)[0];
                    im.arr[y * im.s + x] = v;
                }
            }
        } else {
            let rgba = dynimg.to_rgba8();
            for y in 0..h {
                for x in 0..w {
                    let p = rgba.get_pixel(x as u32, y as u32).0;
                    let base = y * im.s + x * 4;
                    im.arr[base] = p[0];
                    im.arr[base + 1] = p[1];
                    im.arr[base + 2] = p[2];
                    im.arr[base + 3] = p[3];
                }
            }
        }

        Ok(im)
    }

    fn to_rgba8_bytes(&self) -> ImageResult<Vec<u8>> {
        if self.n_ch != 1 && self.n_ch != 4 {
            return Err(Self::invalid_input(format!(
                "Im<u8>::save_png supports n_ch=1 or n_ch=4 (got n_ch={})",
                self.n_ch
            )));
        }

        let expected_row = self
            .w
            .checked_mul(self.n_ch)
            .ok_or_else(|| Self::invalid_input("w*n_ch overflow"))?;
        if self.s < expected_row {
            return Err(Self::invalid_input(format!(
                "stride too small: s={} but w*n_ch={}",
                self.s, expected_row
            )));
        }
        let min_len = self
            .s
            .checked_mul(self.h)
            .ok_or_else(|| Self::invalid_input("s*h overflow"))?;
        if self.arr.len() < min_len {
            return Err(Self::invalid_input(format!(
                "buffer too small: len={} but need at least s*h={}",
                self.arr.len(),
                min_len
            )));
        }

        let mut raw: Vec<u8> = Vec::with_capacity(self.w * self.h * 4);

        for y in 0..self.h {
            let row0 = y * self.s;
            for x in 0..self.w {
                let base = row0 + x * self.n_ch;
                if self.n_ch == 1 {
                    let v = self.arr[base];
                    raw.extend_from_slice(&[v, v, v, 255]);
                } else {
                    raw.extend_from_slice(&[
                        self.arr[base],
                        self.arr[base + 1],
                        self.arr[base + 2],
                        self.arr[base + 3],
                    ]);
                }
            }
        }

        Ok(raw)
    }

    /// Save as a PNG file.
    ///
    /// Encoding rules:
    /// - `n_ch == 1`: grayscale input expanded to RGBA (`R=G=B=v`, `A=255`)
    /// - `n_ch == 4`: assumed already RGBA packed as `[R, G, B, A]` per pixel
    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> ImageResult<()> {
        let raw = self.to_rgba8_bytes()?;
        let img = image::RgbaImage::from_raw(self.w as u32, self.h as u32, raw)
            .ok_or_else(|| Self::invalid_input("invalid RGBA buffer"))?;
        img.save_with_format(path, image::ImageFormat::Png)
    }
}

impl Im<u16> {
    fn invalid_input(msg: impl Into<String>) -> ImageError {
        ImageError::IoError(Error::new(ErrorKind::InvalidInput, msg.into()))
    }

    /// Load PNG from `path` into an `Im<u16>`.
    /// - `n_ch == 1`: expects the PNG to store 16-bit values split into R (low)
    ///   and G (high) channels (or a normal 8-bit image saved with that packing).
    pub fn new_from_png<P: AsRef<Path>>(path: P, n_ch: usize) -> ImageResult<Self> {
        let dynimg = image::open(path)?;
        let w = dynimg.width() as usize;
        let h = dynimg.height() as usize;

        if n_ch != 1 {
            return Err(Self::invalid_input(format!(
                "Im<u16>::new_from_png supports only n_ch=1 (got n_ch={})",
                n_ch
            )));
        }

        let mut im = Im::<u16>::new(w, h, n_ch);

        // Read as RGBA8 and reconstruct u16 as (G<<8) | R where R=low, G=high
        let rgba = dynimg.to_rgba8();
        for y in 0..h {
            for x in 0..w {
                let p = rgba.get_pixel(x as u32, y as u32).0;
                let lo = p[0] as u16;
                let hi = p[1] as u16;
                let v = (hi << 8) | lo;
                im.arr[y * im.s + x] = v;
            }
        }

        Ok(im)
    }

    fn to_rgba8_bytes(&self) -> ImageResult<Vec<u8>> {
        if self.n_ch != 1 {
            return Err(Self::invalid_input(format!(
                "Im<u16>::save_png supports only n_ch=1 (got n_ch={})",
                self.n_ch
            )));
        }

        let expected_row = self.w;
        if self.s < expected_row {
            return Err(Self::invalid_input(format!(
                "stride too small: s={} but w={}",
                self.s, expected_row
            )));
        }
        let min_len = self
            .s
            .checked_mul(self.h)
            .ok_or_else(|| Self::invalid_input("s*h overflow"))?;
        if self.arr.len() < min_len {
            return Err(Self::invalid_input(format!(
                "buffer too small: len={} but need at least s*h={}",
                self.arr.len(),
                min_len
            )));
        }

        let mut raw: Vec<u8> = Vec::with_capacity(self.w * self.h * 4);

        for y in 0..self.h {
            let row0 = y * self.s;
            for x in 0..self.w {
                let v = self.arr[row0 + x];
                // Split the 16-bit value over R and G.
                // R = low byte, G = high byte, B = 0, A = 255.
                let lo = (v & 0x00FF) as u8;
                let hi = (v >> 8) as u8;
                raw.extend_from_slice(&[lo, hi, 0, 255]);
            }
        }

        Ok(raw)
    }

    /// Save as a PNG file.
    ///
    /// Encoding rules:
    /// - `n_ch == 1`: value split over R and G (`R=low8`, `G=high8`, `B=0`, `A=255`)
    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> ImageResult<()> {
        let raw = self.to_rgba8_bytes()?;
        let img = image::RgbaImage::from_raw(self.w as u32, self.h as u32, raw)
            .ok_or_else(|| Self::invalid_input("invalid RGBA buffer"))?;
        img.save_with_format(path, image::ImageFormat::Png)
    }

    
}

pub fn flood_im<SrcT, TarT>(
    src_im: &Im<SrcT>,
    dst_im: &mut Im<TarT>,
    start_x: usize,
    start_y: usize,
    fill_val: TarT,
) -> usize
where
    SrcT: Copy + PartialEq,
    TarT: Copy,
{
    assert_eq!(src_im.w, dst_im.w, "src/dst width mismatch");
    assert_eq!(src_im.h, dst_im.h, "src/dst height mismatch");

    let w = src_im.w;
    let h = src_im.h;
    assert!(start_x < w && start_y < h, "start coords out of bounds");

    // Deliberately safe indexing here: if our bounds assumptions are wrong,
    // we want a clear panic rather than UB.
    let group_val = src_im.arr[start_y * src_im.s + start_x];

    let mut visited: Vec<u8> = vec![0; w * h];
    let mut stack: Vec<(usize, usize)> = Vec::with_capacity(w * h / 10 + 1024);
    stack.push((start_x, start_y));

    let mut filled = 0usize;
    while let Some((x, y)) = stack.pop() {
        let v_i = y * w + x;
        if visited[v_i] != 0 {
            continue;
        }
        visited[v_i] = 1;

        let px = unsafe { *src_im.get_unchecked(x, y) };
        if px != group_val {
            continue;
        }

        unsafe {
            *dst_im.get_unchecked_mut(x, y) = fill_val;
        }
        filled += 1;

        if y + 1 < h {
            let ny = y + 1;
            let n_i = ny * w + x;
            if visited[n_i] == 0 {
                stack.push((x, ny));
            }
        }
        if x + 1 < w {
            let nx = x + 1;
            let n_i = y * w + nx;
            if visited[n_i] == 0 {
                stack.push((nx, y));
            }
        }
        if y > 0 {
            let ny = y - 1;
            let n_i = ny * w + x;
            if visited[n_i] == 0 {
                stack.push((x, ny));
            }
        }
        if x > 0 {
            let nx = x - 1;
            let n_i = y * w + nx;
            if visited[n_i] == 0 {
                stack.push((nx, y));
            }
        }
    }

    filled
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LabelInfo {
    pub size: usize,
    pub start_x: usize,
    pub start_y: usize,
}

pub fn label_im<SrcT, TarT>(src_im: &Im<SrcT>) -> (Im<TarT>, Vec<LabelInfo>)
where
    SrcT: Copy + Default + PartialEq,
    TarT: Copy + Default + PartialEq + TryFrom<usize>,
{
    let w = src_im.w;
    let h = src_im.h;

    let mut dst_im: Im<TarT> = Im::<TarT>::new(w, h, 1);

    // Mirror the JS behavior: allocate/clear destination labels to 0.
    let dst_default = TarT::default();

    let src_bg = SrcT::default();

    // group_info is indexed by group id (and [0] is reserved, do not use it!).
    let mut group_info: Vec<LabelInfo> = vec![LabelInfo {
        size: 0,
        start_x: 0,
        start_y: 0,
    }];

    let mut group_i: usize = 1;
    for y in 0..h {
        for x in 0..w {
            let src_i = y * src_im.s + x;
            let dst_i = y * dst_im.s + x;

            if src_im.arr[src_i] == src_bg {
                // Background pixel
                continue;
            }
            if dst_im.arr[dst_i] != dst_default {
                // Already labeled
                continue;
            }

            let label_val: TarT = TarT::try_from(group_i)
                .ok()
                .unwrap_or_else(|| panic!("label value overflow at group_i={group_i}"));

            // Use flood_im to write this label into dst for the whole connected region.
            let filled = flood_im(src_im, &mut dst_im, x, y, label_val);

            // Ensure our table stays aligned with group ids.
            debug_assert_eq!(group_info.len(), group_i);
            group_info.push(LabelInfo {
                size: filled,
                start_x: x,
                start_y: y,
            });

            group_i += 1;
        }
    }

    (dst_im, group_info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flood_im_fills_connected_component() {
        const DIM: usize = 5;

        // DIM x DIM image with a 2x2 block of 7s in top-left, and a separate single 7.
        // `Im::new` initializes all pixels to `T::default()`; for `u8` that's 0.
        let mut src = Im::<u8>::new(DIM, DIM, 1);
        let idx = |x: usize, y: usize| -> usize { y * DIM + x };

        src.arr[idx(0, 0)] = 7;
        src.arr[idx(1, 0)] = 7;
        src.arr[idx(0, 1)] = 7;
        src.arr[idx(1, 1)] = 7;
        src.arr[idx(DIM - 1, DIM - 1)] = 7;

        let mut dst = Im::<u16>::new(DIM, DIM, 1);

        let filled = flood_im(&src, &mut dst, 0, 0, 1234u16);
        assert_eq!(filled, 4);

        // Filled component
        assert_eq!(dst.arr[idx(0, 0)], 1234);
        assert_eq!(dst.arr[idx(1, 0)], 1234);
        assert_eq!(dst.arr[idx(0, 1)], 1234);
        assert_eq!(dst.arr[idx(1, 1)], 1234);

        // Not connected, should remain default(0)
        assert_eq!(dst.arr[idx(DIM - 1, DIM - 1)], 0);

        // Background should remain default(0)
        assert_eq!(dst.arr[idx(2, 2)], 0);
    }

    #[test]
    fn label_im_finds_two_groups_and_returns_info() {
        const DIM: usize = 6;
        let idx = |x: usize, y: usize| -> usize { y * DIM + x };

        // Background is 0.
        // Group 1: value 7, a 2x2 block at (1,1)..(2,2) => size 4, start (1,1)
        // Group 2: value 9, a horizontal run at y=0, x=4..5 => size 2, start (4,0)
        let mut src = Im::<u8>::new(DIM, DIM, 1);
        src.arr[idx(1, 1)] = 7;
        src.arr[idx(2, 1)] = 7;
        src.arr[idx(1, 2)] = 7;
        src.arr[idx(2, 2)] = 7;
        src.arr[idx(4, 0)] = 9;
        src.arr[idx(5, 0)] = 9;

        let (dst, groups): (Im<u16>, Vec<LabelInfo>) = label_im(&src);

        // [0] is reserved.
        assert_eq!(groups.len(), 3);

        // Scan order is row-major (y then x), so the first group starts at (4,0).
        assert_eq!(
            groups[1],
            LabelInfo {
                size: 2,
                start_x: 4,
                start_y: 0
            }
        );
        assert_eq!(
            groups[2],
            LabelInfo {
                size: 4,
                start_x: 1,
                start_y: 1
            }
        );

        // Verify labels were written into dst with group ids.
        assert_eq!(dst.arr[idx(4, 0)], 1);
        assert_eq!(dst.arr[idx(5, 0)], 1);
        assert_eq!(dst.arr[idx(1, 1)], 2);
        assert_eq!(dst.arr[idx(2, 1)], 2);
        assert_eq!(dst.arr[idx(1, 2)], 2);
        assert_eq!(dst.arr[idx(2, 2)], 2);

        // Background remains 0.
        assert_eq!(dst.arr[idx(0, 0)], 0);
        assert_eq!(dst.arr[idx(3, 3)], 0);
    }
}
