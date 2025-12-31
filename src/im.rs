#![allow(dead_code)]

use image::ImageResult;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Im<T, const N_CH: usize> {
    pub w: usize,
    pub h: usize,
    pub s: usize, // stride in elements (w * N_CH)
    pub arr: Vec<T>,
}

// Helpers for i32 PNG packing/unpacking
// ------------------------------------------------------------------------------
fn dim_mismatch_err() -> image::ImageError {
    image::ImageError::Parameter(image::error::ParameterError::from_kind(
        image::error::ParameterErrorKind::DimensionMismatch,
    ))
}

fn pack_i32_as_rgba8(pixels: &[i32]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(pixels.len() * 4);
    for v in pixels {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn unpack_rgba8_as_i32(raw_rgba: &[u8]) -> Result<Vec<i32>, image::ImageError> {
    if raw_rgba.len() % 4 != 0 {
        return Err(dim_mismatch_err());
    }

    let mut out: Vec<i32> = Vec::with_capacity(raw_rgba.len() / 4);
    for px in raw_rgba.chunks_exact(4) {
        out.push(i32::from_le_bytes([px[0], px[1], px[2], px[3]]));
    }
    Ok(out)
}

// Constructor
// ------------------------------------------------------------------------------
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

// PNG I/O
// ------------------------------------------------------------------------------
impl Im<u8, 1> {
    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> ImageResult<()> {
        let img = image::GrayImage::from_raw(self.w as u32, self.h as u32, self.arr.clone())
            .ok_or_else(|| {
                image::ImageError::Parameter(image::error::ParameterError::from_kind(
                    image::error::ParameterErrorKind::DimensionMismatch,
                ))
            })?;

        img.save_with_format(path, image::ImageFormat::Png)
    }
}

impl Im<u8, 4> {
    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> ImageResult<()> {
        let img = image::RgbaImage::from_raw(self.w as u32, self.h as u32, self.arr.clone())
            .ok_or_else(|| {
                image::ImageError::Parameter(image::error::ParameterError::from_kind(
                    image::error::ParameterErrorKind::DimensionMismatch,
                ))
            })?;

        img.save_with_format(path, image::ImageFormat::Png)
    }
}

impl Im<u16, 1> {
    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> ImageResult<()> {
        let img = image::ImageBuffer::<image::Luma<u16>, _>::from_raw(
            self.w as u32,
            self.h as u32,
            self.arr.clone(),
        )
        .ok_or_else(|| {
            image::ImageError::Parameter(image::error::ParameterError::from_kind(
                image::error::ParameterErrorKind::DimensionMismatch,
            ))
        })?;

        img.save_with_format(path, image::ImageFormat::Png)
    }
}

impl Im<u16, 4> {
    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> ImageResult<()> {
        let img = image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(
            self.w as u32,
            self.h as u32,
            self.arr.clone(),
        )
        .ok_or_else(|| {
            image::ImageError::Parameter(image::error::ParameterError::from_kind(
                image::error::ParameterErrorKind::DimensionMismatch,
            ))
        })?;

        img.save_with_format(path, image::ImageFormat::Png)
    }
}

impl Im<i32, 1> {
    // PNG doesn't support 32-bit single-channel integer pixels, so we losslessly
    // round-trip by packing each i32 into RGBA8 (little-endian bytes).
    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> ImageResult<()> {
        let raw = pack_i32_as_rgba8(&self.arr);

        let img = image::RgbaImage::from_raw(self.w as u32, self.h as u32, raw)
            .ok_or_else(dim_mismatch_err)?;

        img.save_with_format(path, image::ImageFormat::Png)
    }

    pub fn load_png<P: AsRef<Path>>(path: P) -> ImageResult<Self> {
        let img = image::open(path)?.into_rgba8();
        let w = img.width() as usize;
        let h = img.height() as usize;
        let raw = img.into_raw();

        if raw.len() != w * h * 4 {
            return Err(dim_mismatch_err());
        }

        let arr = unpack_rgba8_as_i32(&raw)?;
        Ok(Self { w, h, s: w, arr })
    }

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

// Labeling
// =============================================================================

/// Flood-fill a connected component in a single-channel image.
fn flood_im<SrcT, TarT>(
    src_im: &Im<SrcT, 1>,
    dst_im: &mut Im<TarT, 1>,
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

        let px = unsafe { *src_im.get_unchecked(x, y, 0) };
        if px != group_val {
            continue;
        }

        unsafe {
            *dst_im.get_unchecked_mut(x, y, 0) = fill_val;
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

/// Label a single channel image's connected components.
pub fn label_im<SrcT, TarT>(src_im: &Im<SrcT, 1>) -> (Im<TarT, 1>, Vec<LabelInfo>)
where
    SrcT: Copy + Default + PartialEq,
    TarT: Copy + Default + PartialEq + TryFrom<usize>,
{
    let w = src_im.w;
    let h = src_im.h;

    let mut dst_im: Im<TarT, 1> = Im::<TarT, 1>::new(w, h);

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

// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flood_im_fills_connected_component() {
        const DIM: usize = 5;

        // DIM x DIM image with a 2x2 block of 7s in top-left, and a separate single 7.
        // `Im::new` initializes all pixels to `T::default()`; for `u8` that's 0.
        let mut src = Im::<u8, 1>::new(DIM, DIM);
        let idx = |x: usize, y: usize| -> usize { y * DIM + x };

        src.arr[idx(0, 0)] = 7;
        src.arr[idx(1, 0)] = 7;
        src.arr[idx(0, 1)] = 7;
        src.arr[idx(1, 1)] = 7;
        src.arr[idx(DIM - 1, DIM - 1)] = 7;

        let mut dst = Im::<u16, 1>::new(DIM, DIM);

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
        let mut src = Im::<u8, 1>::new(DIM, DIM);
        src.arr[idx(1, 1)] = 7;
        src.arr[idx(2, 1)] = 7;
        src.arr[idx(1, 2)] = 7;
        src.arr[idx(2, 2)] = 7;
        src.arr[idx(4, 0)] = 9;
        src.arr[idx(5, 0)] = 9;

        let (dst, groups): (Im<u16, 1>, Vec<LabelInfo>) = label_im(&src);

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

    #[test]
    fn i32_png_pack_is_lossless() {
        let src: [i32; 6] = [0, 1, -1, 123456789, i32::MIN, i32::MAX];
        let packed = pack_i32_as_rgba8(&src);
        let unpacked = unpack_rgba8_as_i32(&packed).unwrap();
        assert_eq!(unpacked, src);
    }

    #[test]
    fn can_new_i32_im() {
        let im = Im::<i32, 1>::new(3, 2);
        assert_eq!(im.w, 3);
        assert_eq!(im.h, 2);
        assert_eq!(im.s, 3);
        assert_eq!(im.arr.len(), 3 * 2);
        assert!(im.arr.iter().all(|&v| v == 0));
    }
}
