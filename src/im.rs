#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Im<T> {
    pub w: usize,
    pub h: usize,
    pub s: usize, // stride: elements per row
    pub arr: Vec<T>,
}

impl<T: Copy + Default> Im<T> {
    pub fn new(w: usize, h: usize) -> Self {
        let s = w;
        let arr = vec![T::default(); s * h];
        Self { w, h, s, arr }
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

pub fn label_im<SrcT, TarT>(src_im: &Im<SrcT>, dst_im: &mut Im<TarT>) -> Vec<LabelInfo>
where
    SrcT: Copy + Default + PartialEq,
    TarT: Copy + Default + PartialEq + TryFrom<usize>,
{
    assert_eq!(src_im.w, dst_im.w, "src/dst width mismatch");
    assert_eq!(src_im.h, dst_im.h, "src/dst height mismatch");

    let w = src_im.w;
    let h = src_im.h;

    // Mirror the JS behavior: allocate/clear destination labels to 0.
    let dst_default = TarT::default();
    dst_im.arr.fill(dst_default);

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
            let filled = flood_im(src_im, dst_im, x, y, label_val);

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

    group_info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flood_im_fills_connected_component() {
        const DIM: usize = 5;

        // DIM x DIM image with a 2x2 block of 7s in top-left, and a separate single 7.
        // `Im::new` initializes all pixels to `T::default()`; for `u8` that's 0.
        let mut src = Im::<u8>::new(DIM, DIM);
        let idx = |x: usize, y: usize| -> usize { y * DIM + x };

        src.arr[idx(0, 0)] = 7;
        src.arr[idx(1, 0)] = 7;
        src.arr[idx(0, 1)] = 7;
        src.arr[idx(1, 1)] = 7;
        src.arr[idx(DIM - 1, DIM - 1)] = 7;

        let mut dst = Im::<u16>::new(DIM, DIM);

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
        let mut src = Im::<u8>::new(DIM, DIM);
        src.arr[idx(1, 1)] = 7;
        src.arr[idx(2, 1)] = 7;
        src.arr[idx(1, 2)] = 7;
        src.arr[idx(2, 2)] = 7;
        src.arr[idx(4, 0)] = 9;
        src.arr[idx(5, 0)] = 9;

        let mut dst = Im::<u16>::new(DIM, DIM);
        let groups = label_im(&src, &mut dst);

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
