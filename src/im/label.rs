use super::core::Im;
use std::collections::HashMap;

/// Flood-fill a connected component in a single-channel image.
fn flood_im<SrcT, TarT, S>(
    src_im: &Im<SrcT, 1, S>,
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LabelInfo {
    pub size: usize,
    pub start_x: usize,
    pub start_y: usize,
}

/// Label a single channel image's connected components.
pub fn label_im<SrcT, TarT, S>(src_im: &Im<SrcT, 1, S>) -> (Im<TarT, 1>, Vec<LabelInfo>)
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
    let mut group_info: Vec<LabelInfo> = vec![LabelInfo::default()];

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

pub type NeighborMap = Vec<HashMap<usize, usize>>;

/// Build a per-label neighbor map from a labeled image.
///
/// For each pair of distinct, non-background labels `(a, b)`, the count is the
/// number of pixels along the shared border between the two regions.
///
/// This is computed by first counting, for each direction, how many pixels in
/// region `a` are 4-connected to at least one pixel in region `b`, then taking
/// the symmetric value:
///
/// $$\text{shared}(a,b) = \min(\text{touch}(a\to b),\ \text{touch}(b\to a))$$
///
/// Pixels on the outer image edge are still counted normally; the only thing
/// that does *not* count is the implicit neighbor "outside" the image.
///
/// The returned vector is indexed by label id, matching `group_info` from
/// [`label_im`]. Index 0 is reserved/background and will always be empty.
pub fn neighbor_map_from_labels<T, S>(
    label_im: &Im<T, 1, S>,
    group_info: &[LabelInfo],
) -> NeighborMap
where
    T: Copy + Default + PartialEq + TryInto<usize>,
{
    let w = label_im.w;
    let h = label_im.h;
    let s = label_im.s;

    let mut neighbors: NeighborMap = vec![HashMap::new(); group_info.len()];

    // Empty or 1D images can't have any 4-connected cross-label borders.
    if w == 0 || h == 0 || w == 1 || h == 1 {
        return neighbors;
    }

    let bg = T::default();

    for y in 0..h {
        let row = y * s;
        for x in 0..w {
            let a = label_im.arr[row + x];
            if a == bg {
                continue;
            }

            let a_id: usize = a
                .try_into()
                .unwrap_or_else(|_| panic!("label value did not convert to usize"));
            if a_id == 0 || a_id >= neighbors.len() {
                continue;
            }

            // Collect unique neighboring label ids (max 4) for this pixel.
            let mut n_ids: [usize; 4] = [0; 4];
            let mut n_len = 0usize;
            let mut consider = |b: T| {
                if b == bg || b == a {
                    return;
                }
                let b_id: usize = b
                    .try_into()
                    .unwrap_or_else(|_| panic!("label value did not convert to usize"));
                if b_id == 0 || b_id >= neighbors.len() {
                    return;
                }
                for i in 0..n_len {
                    if n_ids[i] == b_id {
                        return;
                    }
                }
                n_ids[n_len] = b_id;
                n_len += 1;
            };

            if x + 1 < w {
                consider(label_im.arr[row + x + 1]);
            }
            if x > 0 {
                consider(label_im.arr[row + x - 1]);
            }
            if y + 1 < h {
                consider(label_im.arr[(y + 1) * s + x]);
            }
            if y > 0 {
                consider(label_im.arr[(y - 1) * s + x]);
            }

            for i in 0..n_len {
                *neighbors[a_id].entry(n_ids[i]).or_insert(0) += 1;
            }
        }
    }

    // Symmetrize counts so the map represents a shared-border measure.
    for a in 1..neighbors.len() {
        let keys: Vec<usize> = neighbors[a].keys().copied().collect();
        for b in keys {
            if b <= a || b >= neighbors.len() {
                continue;
            }

            let ab = neighbors[a].get(&b).copied().unwrap_or(0);
            let ba = neighbors[b].get(&a).copied().unwrap_or(0);
            let shared = ab.min(ba);

            if shared == 0 {
                neighbors[a].remove(&b);
                neighbors[b].remove(&a);
            } else {
                neighbors[a].insert(b, shared);
                neighbors[b].insert(a, shared);
            }
        }
    }

    neighbors
}

// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn labels_from_ascii(grid: &str) -> Im<u16, 1> {
        let rows: Vec<&str> = grid
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        let h = rows.len();
        assert!(h > 0, "grid must have at least one non-empty row");
        let w = rows[0].len();
        assert!(w > 0, "grid rows must be non-empty");
        for r in &rows {
            assert_eq!(r.len(), w, "all rows must have equal length");
        }

        let mut im = Im::<u16, 1>::new(w, h);
        for (y, row) in rows.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                let v = ch
                    .to_digit(10)
                    .unwrap_or_else(|| panic!("invalid label char '{ch}', expected digit"))
                    as u16;
                im.arr[y * im.s + x] = v;
            }
        }
        im
    }

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
    fn neighbor_map_counts_shared_borders_and_ignores_outer_edge() {
        // 5x5 label image. The outer edge is 0 (background), so it's not part of any region.
        // Labels 1 and 2 touch along a 3-pixel vertical seam fully in the interior.
        // Shared borders between (1,2): 3
        let labels = labels_from_ascii(
            r#"
                00000
                01120
                01120
                01120
                00000
            "#,
        );

        // group_info only needs correct length/indexing.
        let group_info = vec![LabelInfo::default(); 3];

        let neigh = neighbor_map_from_labels(&labels, &group_info);
        assert_eq!(neigh.len(), 3);
        assert_eq!(neigh[0].len(), 0);
        assert_eq!(neigh[1].get(&2).copied(), Some(3));
        assert_eq!(neigh[2].get(&1).copied(), Some(3));
    }

    #[test]
    fn neighbor_map_counts_boundary_pixels_when_one_surrounds_another() {
        // 5x5 label image where region 1 surrounds region 2.
        // Each region has 8 boundary pixels touching the other.
        let labels = labels_from_ascii(
            r#"
                11111
                12221
                12221
                12221
                11111
            "#,
        );

        let group_info = vec![LabelInfo::default(); 3];
        let neigh = neighbor_map_from_labels(&labels, &group_info);
        assert_eq!(neigh[1].get(&2).copied(), Some(8));
        assert_eq!(neigh[2].get(&1).copied(), Some(8));
    }

    #[test]
    fn foo() {
        let labels = labels_from_ascii(
            r#"
                11311
                12221
                12221
                12221
                11111
            "#,
        );

        let group_info = vec![LabelInfo::default(); 4];
        let neigh = neighbor_map_from_labels(&labels, &group_info);
        println!("{:?}", neigh);
        assert_eq!(neigh[1].get(&2).copied(), Some(7));
        assert_eq!(neigh[1].get(&3).copied(), Some(1));
        assert_eq!(neigh[2].get(&1).copied(), Some(7));
        assert_eq!(neigh[2].get(&3).copied(), Some(1));
        assert_eq!(neigh[3].get(&1).copied(), Some(1));
        assert_eq!(neigh[3].get(&2).copied(), Some(1));
    }    
}
