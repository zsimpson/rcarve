#[allow(dead_code)]
use std::collections::HashMap;

use crate::im::Im;

pub const CONTOUR_ID_MAX: i32 = i32::MAX;
pub const CONTOUR_ID_MIN: i32 = i32::MIN;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Iv2 {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug)]
pub struct Contour {
    pub id: i32,
    pub is_hole: bool,
    pub parent: Option<usize>, // index into `contours`
    pub points: Vec<Iv2>,
}

impl Contour {
    fn new(id: i32, is_hole: bool) -> Self {
        Self {
            id,
            is_hole,
            parent: None,
            points: Vec::new(),
        }
    }

    fn point_segment_dist_sq(p: Iv2, a: Iv2, b: Iv2) -> f64 {
        // Squared distance from point p to segment a-b.
        let px = p.x as f64;
        let py = p.y as f64;
        let ax = a.x as f64;
        let ay = a.y as f64;
        let bx = b.x as f64;
        let by = b.y as f64;

        let abx = bx - ax;
        let aby = by - ay;

        if abx == 0.0 && aby == 0.0 {
            let dx = px - ax;
            let dy = py - ay;
            return dx * dx + dy * dy;
        }

        let apx = px - ax;
        let apy = py - ay;
        let denom = abx * abx + aby * aby;
        let t = (apx * abx + apy * aby) / denom;

        let (dx, dy) = if t >= 1.0 {
            (px - bx, py - by)
        } else if t > 0.0 {
            let projx = ax + t * abx;
            let projy = ay + t * aby;
            (px - projx, py - projy)
        } else {
            (px - ax, py - ay)
        };

        dx * dx + dy * dy
    }

    fn rdp_rec(
        points: &[Iv2],
        tolerance_sq: f64,
        start_i: usize,
        end_i: usize,
        out: &mut Vec<Iv2>,
    ) {
        debug_assert!(start_i < end_i);
        debug_assert!(end_i < points.len());

        let start = points[start_i];
        let end = points[end_i];

        let mut max_dist_sq = 0.0f64;
        let mut index: Option<usize> = None;

        for i in (start_i + 1)..end_i {
            let d_sq = Self::point_segment_dist_sq(points[i], start, end);
            if d_sq > max_dist_sq {
                max_dist_sq = d_sq;
                index = Some(i);
            }
        }

        if max_dist_sq > tolerance_sq {
            let mid = index.expect("midpoint must exist if max_dist_sq > 0");
            // Note: we only ever push end points in the base case, so midpoint won't duplicate.
            Self::rdp_rec(points, tolerance_sq, start_i, mid, out);
            Self::rdp_rec(points, tolerance_sq, mid, end_i, out);
        } else {
            out.push(end);
        }
    }

    /// Simplify this contour using the Ramer–Douglas–Peucker algorithm.
    ///
    /// `tolerance` is in the same units as the contour coordinates (pixels).
    ///
    /// If the contour is "closed" (last point equals first point), this treats it as a ring by
    /// simplifying the path without the duplicated final point, then re-closing it.
    pub fn simplify_by_rdp(&self, tolerance: f64) -> Contour {
        if self.points.len() <= 2 {
            return Contour {
                id: self.id,
                is_hole: self.is_hole,
                parent: self.parent,
                points: self.points.clone(),
            };
        }

        let tolerance_sq = tolerance.max(0.0) * tolerance.max(0.0);

        let is_closed =
            self.points.len() >= 2 && self.points[0] == self.points[self.points.len() - 1];

        let source: &[Iv2] = if is_closed {
            // Drop the repeated closing point for simplification.
            &self.points[..self.points.len() - 1]
        } else {
            &self.points
        };

        if source.len() <= 2 {
            return Contour {
                id: self.id,
                is_hole: self.is_hole,
                parent: self.parent,
                points: self.points.clone(),
            };
        }

        let mut simplified: Vec<Iv2> = Vec::with_capacity(source.len());
        simplified.push(source[0]);
        Self::rdp_rec(source, tolerance_sq, 0, source.len() - 1, &mut simplified);

        if is_closed {
            // Ensure closure (avoid double-close if it already ended up closed).
            if simplified.last().copied() != Some(simplified[0]) {
                simplified.push(simplified[0]);
            }
        }

        Contour {
            id: self.id,
            is_hole: self.is_hole,
            parent: self.parent,
            points: simplified,
        }
    }
}

/// Port of your Suzuki–Abe contour tracing.
///
/// Preconditions (same as your C assumptions):
/// - `im` must already be binary-ish (0 vs nonzero), but we normalize interior to {0,1}.
/// - We write borders and IDs into the image in-place.
/// - Image must have at least a 1-pixel margin available (we will force the outermost border to 0).
///
/// Returns a Vec of contours; parent references are indices into that Vec.
pub fn contours_by_suzuki_abe(im: &mut Im<i32, 1>) -> Vec<Contour> {
    let w = im.w;
    let h = im.h;
    assert!(w >= 2 && h >= 2, "need at least a 1-pixel border");
    assert!(im.s >= w, "stride must be >= width");
    assert!(im.arr.len() >= im.s * h);

    #[inline]
    fn idx(stride: usize, x: usize, y: usize) -> usize {
        y * stride + x
    }

    let w1 = w - 1;
    let h1 = h - 1;

    // 8-neighborhood LUTs (same ordering as your C).
    const DIR_TO_DELT_CW: [(i32, i32); 8] = [
        (0, 1),   // 0
        (1, 1),   // 1
        (1, 0),   // 2
        (1, -1),  // 3
        (0, -1),  // 4
        (-1, -1), // 5
        (-1, 0),  // 6
        (-1, 1),  // 7
    ];

    const DELT_PLUS_1_TO_DIR_CW: [i32; 9] = [
        // dy = -1, dx = -1,0,1
        5, 6, 7, // dy =  0, dx = -1,0,1 (0 impossible)
        4, -1, 0, // dy =  1, dx = -1,0,1
        3, 2, 1,
    ];

    const DIR_TO_DELT_CCW: [(i32, i32); 8] = [
        (0, 1),   // 0
        (-1, 1),  // 1
        (-1, 0),  // 2
        (-1, -1), // 3
        (0, -1),  // 4
        (1, -1),  // 5
        (1, 0),   // 6
        (1, 1),   // 7
    ];

    const DELT_PLUS_1_TO_DIR_CCW: [i32; 9] = [
        // dy = -1
        3, 2, 1, // dy = 0
        4, -1, 0, // dy = 1
        5, 6, 7,
    ];

    #[inline]
    fn delt_to_dir_cw(dy: i32, dx: i32) -> i32 {
        DELT_PLUS_1_TO_DIR_CW[((dy + 1) * 3 + (dx + 1)) as usize]
    }
    #[inline]
    fn delt_to_dir_ccw(dy: i32, dx: i32) -> i32 {
        DELT_PLUS_1_TO_DIR_CCW[((dy + 1) * 3 + (dx + 1)) as usize]
    }

    // (Border of zeros)
    for y in 0..h {
        let left = idx(im.s, 0, y);
        let right = idx(im.s, w1, y);
        im.arr[left] = 0;
        im.arr[right] = 0;
    }
    for x in 0..w {
        let top = idx(im.s, x, 0);
        let bot = idx(im.s, x, h1);
        im.arr[top] = 0;
        im.arr[bot] = 0;
    }

    // Normalize interior to {0,1}
    for y in 1..h1 {
        for x in 1..w1 {
            let i = idx(im.s, x, y);
            im.arr[i] = if im.arr[i] == 0 { 0 } else { 1 };
        }
    }

    let mut contours: Vec<Contour> = Vec::new();
    let mut id_to_index: HashMap<i32, usize> = HashMap::new();

    let mut curr_id: i32 = 1;

    // (0) raster scan
    for y0 in 1..h1 {
        let mut last_id: i32 = 1;

        for x0 in 1..w1 {
            let mut skip_to_4 = false;

            let f0 = im.arr[idx(im.s, x0, y0)];
            // These are ((2)) in the paper.
            let mut y2: i32 = 0;
            let mut x2: i32 = 0;

            let mut is_hole = false;

            // (1a)
            if f0 == 1 && im.arr[idx(im.s, x0 - 1, y0)] == 0 {
                is_hole = false;
                curr_id += 1;
                y2 = y0 as i32;
                x2 = (x0 as i32) - 1;
            }
            // (1b)
            else if f0 >= 1 && im.arr[idx(im.s, x0 + 1, y0)] == 0 {
                is_hole = true;
                curr_id += 1;
                y2 = y0 as i32;
                x2 = (x0 as i32) + 1;
                if f0 > 1 {
                    last_id = f0;
                }
            }
            // (1c)
            else {
                skip_to_4 = true;
            }

            assert!(curr_id <= CONTOUR_ID_MAX && curr_id >= CONTOUR_ID_MIN);

            if !skip_to_4 {
                // (2) decide parent using Table 1
                let new_index = contours.len();
                contours.push(Contour::new(curr_id, is_hole));
                id_to_index.insert(curr_id, new_index);

                let last_idx_opt = id_to_index.get(&last_id).copied();

                if let Some(last_idx) = last_idx_opt {
                    let last_is_hole = contours[last_idx].is_hole;
                    let last_parent = contours[last_idx].parent;

                    let parent = if last_is_hole {
                        if is_hole {
                            // hole inside hole -> parent's parent
                            last_parent
                        } else {
                            // contour inside hole
                            Some(last_idx)
                        }
                    } else {
                        if is_hole {
                            // hole inside contour
                            Some(last_idx)
                        } else {
                            // contour next to contour
                            last_parent
                        }
                    };
                    contours[new_index].parent = parent;
                }

                // (3.1) find ((1)) by clockwise search around ((0)) starting from ((2))
                let (mut y1, mut x1) = (0i32, 0i32);

                let dy = y2 - (y0 as i32);
                let dx = x2 - (x0 as i32);
                debug_assert!(
                    (-1..=1).contains(&dy) && (-1..=1).contains(&dx) && !(dy == 0 && dx == 0)
                );
                let dir0 = delt_to_dir_cw(dy, dx);
                debug_assert!((0..8).contains(&dir0));

                let mut d_found = None;
                for d in 0..8 {
                    let dird = ((dir0 + d + 8) % 8) as usize;
                    let (ddy, ddx) = DIR_TO_DELT_CW[dird];
                    let ny = (y0 as i32) + ddy;
                    let nx = (x0 as i32) + ddx;
                    let uy = ny as usize;
                    let ux = nx as usize;
                    debug_assert!(uy < h && ux < w);
                    if im.arr[idx(im.s, ux, uy)] != 0 {
                        y1 = ny;
                        x1 = nx;
                        d_found = Some(d);
                        break;
                    }
                }

                if d_found.is_none() {
                    // singleton pixel
                    im.arr[idx(im.s, x0, y0)] = -curr_id;
                    skip_to_4 = true;
                }

                if !skip_to_4 {
                    // (3.2) ((2))=((1)); ((3))=((0))
                    y2 = y1;
                    x2 = x1;
                    let mut y3: i32 = y0 as i32;
                    let mut x3: i32 = x0 as i32;
                    let start = Iv2 { x: x3, y: y3 };

                    loop {
                        // record point ((3))
                        contours[new_index].points.push(Iv2 { x: x3, y: y3 });

                        // (3.3) counter-clockwise search for ((4)), starting from next element after ((2))
                        let dy = y2 - y3;
                        let dx = x2 - x3;
                        debug_assert!(
                            (-1..=1).contains(&dy)
                                && (-1..=1).contains(&dx)
                                && !(dy == 0 && dx == 0)
                        );
                        let dir0 = delt_to_dir_ccw(dy, dx);
                        debug_assert!((0..8).contains(&dir0));

                        let mut east_was_examined = false;
                        let (mut y4, mut x4) = (0i32, 0i32);

                        let mut found = false;
                        for d in 0..8 {
                            let dird = ((dir0 + d + 1 + 8) % 8) as usize; // +1 to start on the "next"
                            let (ddy, ddx) = DIR_TO_DELT_CCW[dird];
                            if ddy == 0 && ddx == 1 {
                                east_was_examined = true;
                            }
                            let ny = y3 + ddy;
                            let nx = x3 + ddx;
                            let uy = ny as usize;
                            let ux = nx as usize;
                            debug_assert!(uy < h && ux < w);
                            if im.arr[idx(im.s, ux, uy)] != 0 {
                                y4 = ny;
                                x4 = nx;
                                found = true;
                                break;
                            }
                        }
                        assert!(found, "Non-zero pixel failed. Should be impossible");

                        // (3.4a/3.4b) label f((3))
                        let ux3 = x3 as usize;
                        let uy3 = y3 as usize;
                        let idx3 = idx(im.s, ux3, uy3);

                        if east_was_examined {
                            let east = im.arr[idx(im.s, (ux3 + 1), uy3)];
                            if east == 0 {
                                im.arr[idx3] = -curr_id;
                            } else if im.arr[idx3] == 1 {
                                im.arr[idx3] = curr_id;
                            }
                        } else if im.arr[idx3] == 1 {
                            im.arr[idx3] = curr_id;
                        }

                        // (3.5) termination check: ((4))==((0)) and ((3))==((1))
                        if y4 == (y0 as i32) && x4 == (x0 as i32) && y3 == y1 && x3 == x1 {
                            break;
                        }

                        // advance: ((2))=((3)), ((3))=((4))
                        y2 = y3;
                        x2 = x3;
                        y3 = y4;
                        x3 = x4;
                    }

                    // Repeat the initial pixel
                    contours[new_index].points.push(start);
                }
            }

            // (4) update last_id
            if im.arr[idx(im.s, x0, y0)] != 1 {
                last_id = im.arr[idx(im.s, x0, y0)].abs();
            }
        }
    }

    contours
}

fn fmt_verts(points: &[Iv2]) -> String {
    if points.is_empty() {
        return "<empty>".to_string();
    }

    let mut out = String::new();
    let mut n = 0usize;
    for p in points.iter().take(5) {
        if n > 0 {
            out.push(' ');
        }
        out.push('(');
        out.push_str(&p.x.to_string());
        out.push(',');
        out.push_str(&p.y.to_string());
        out.push(')');
        n += 1;
    }
    if points.len() > 5 {
        out.push_str(" ...");
    }
    out
}

fn dump_contour_line(c: &Contour, indent: usize) {
    if c.is_hole {
        println!(
            "{:indent$}id={}. HOLE. verts = {}",
            "",
            c.id,
            fmt_verts(&c.points),
            indent = indent
        );
    } else {
        println!(
            "{:indent$}id={}. verts = {}",
            "",
            c.id,
            fmt_verts(&c.points),
            indent = indent
        );
    }
}

impl Contour {
    /// Dump a single contour for debugging.
    pub fn dump(&self) {
        println!("Contour");
        dump_contour_line(self, 2);
    }

    pub fn draw_into_rgba_im_alternating_colors(
        &self,
        im: &mut Im<u8, 4>,
        r0: u8,
        g0: u8,
        b0: u8,
        r1: u8,
        g1: u8,
        b1: u8,
    ) {
        // Draw a line between each consecutive pair of points.
        // Color alternates by segment index (starting point index parity).

        #[inline]
        fn put_px(im: &mut Im<u8, 4>, x: i32, y: i32, r: u8, g: u8, b: u8) {
            if x < 0 || y < 0 {
                return;
            }
            let ux = x as usize;
            let uy = y as usize;
            if ux >= im.w || uy >= im.h {
                return;
            }
            let idx = uy * im.s + ux * 4;
            im.arr[idx + 0] = r;
            im.arr[idx + 1] = g;
            im.arr[idx + 2] = b;
            im.arr[idx + 3] = 255;
        }

        #[inline]
        fn draw_bresenham(im: &mut Im<u8, 4>, a: Iv2, b: Iv2, r: u8, g: u8, bb: u8) {
            // Classic Bresenham for all octants.
            let mut x0 = a.x;
            let mut y0 = a.y;
            let x1 = b.x;
            let y1 = b.y;

            let dx = (x1 - x0).abs();
            let sx = if x0 < x1 { 1 } else { -1 };
            let dy = -(y1 - y0).abs();
            let sy = if y0 < y1 { 1 } else { -1 };
            let mut err = dx + dy;

            loop {
                put_px(im, x0, y0, r, g, bb);
                if x0 == x1 && y0 == y1 {
                    break;
                }
                let e2 = 2 * err;
                if e2 >= dy {
                    err += dy;
                    x0 += sx;
                }
                if e2 <= dx {
                    err += dx;
                    y0 += sy;
                }
            }
        }

        if self.points.is_empty() {
            return;
        }

        // If there's only 1 point, just plot it.
        if self.points.len() == 1 {
            let p = self.points[0];
            put_px(im, p.x, p.y, r0, g0, b0);
            return;
        }

        for (i, seg) in self.points.windows(2).enumerate() {
            let a = seg[0];
            let b = seg[1];
            let (r, g, bcol) = if (i % 2) == 0 {
                (r0, g0, b0)
            } else {
                (r1, g1, b1)
            };
            draw_bresenham(im, a, b, r, g, bcol);
        }
    }
}

pub trait ContoursDebug {
    /// Extension trait so you can call `dump()` on `Vec<Contour>` (via deref to `[Contour]`).
    /// Dump contours for debugging.
    fn dump(&self);

    fn draw_into_rgba_im_alternating_colors(
        &self,
        im: &mut Im<u8, 4>,
        r0: u8,
        g0: u8,
        b0: u8,
        r1: u8,
        g1: u8,
        b1: u8,
    );
}

impl ContoursDebug for [Contour] {
    fn dump(&self) {
        println!("Contours");

        let mut holes_by_parent: Vec<Vec<usize>> = vec![Vec::new(); self.len()];
        let mut orphan_holes: Vec<usize> = Vec::new();

        for (i, c) in self.iter().enumerate() {
            if !c.is_hole {
                continue;
            }

            match c.parent {
                Some(p) if p < self.len() => holes_by_parent[p].push(i),
                _ => orphan_holes.push(i),
            }
        }

        for (i, c) in self.iter().enumerate() {
            if c.is_hole {
                continue;
            }

            dump_contour_line(c, 2);

            let holes = &holes_by_parent[i];
            for &hi in holes {
                let h = &self[hi];
                dump_contour_line(h, 6);
            }
        }

        for hi in orphan_holes {
            let h = &self[hi];
            dump_contour_line(h, 2);
        }
    }

    fn draw_into_rgba_im_alternating_colors(
        &self,
        im: &mut Im<u8, 4>,
        r0: u8,
        g0: u8,
        b0: u8,
        r1: u8,
        g1: u8,
        b1: u8,
    ) {
        for contour in self {
            contour.draw_into_rgba_im_alternating_colors(im, r0, g0, b0, r1, g1, b1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::im;
    use crate::im::RGBAIm;

    fn fill_rect(im: &mut Im<i32, 1>, x0: usize, y0: usize, w: usize, h: usize, v: i32) {
        assert!(x0 + w <= im.w);
        assert!(y0 + h <= im.h);
        for y in y0..(y0 + h) {
            let row = y * im.s;
            for x in x0..(x0 + w) {
                im.arr[row + x] = v;
            }
        }
    }

    fn bbox(points: &[Iv2]) -> (i32, i32, i32, i32) {
        assert!(!points.is_empty());
        let mut min_x = points[0].x;
        let mut max_x = points[0].x;
        let mut min_y = points[0].y;
        let mut max_y = points[0].y;
        for p in points {
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
            min_y = min_y.min(p.y);
            max_y = max_y.max(p.y);
        }
        (min_x, min_y, max_x, max_y)
    }

    #[test]
    fn contours_by_suzuki_abe_finds_contours_and_flattens_hierarchy() {
        // Scene:
        // - black background
        // - one 75x75 white square
        // - two 20x50 black rectangles inside it (holes)
        // - one 10x10 white square inside each black rectangle
        // Expectation:
        // - outer square is a top-level contour
        // - the two holes have parent == outer square
        // - the two inner 10x10 squares are treated as *top-level* contours by this impl
        let mut im: Im<i32, 1> = Im::new(100, 100);

        let outer_x = 10_usize;
        let outer_y = 10_usize;
        let outer_w = 75_usize;
        let outer_h = 75_usize;
        fill_rect(&mut im, outer_x, outer_y, outer_w, outer_h, 1);

        let hole_y = outer_y + 10;
        let hole_h = 50_usize;
        let hole_w = 20_usize;

        let hole1_x = outer_x + 10;
        let hole2_x = outer_x + 45;
        fill_rect(&mut im, hole1_x, hole_y, hole_w, hole_h, 0);
        fill_rect(&mut im, hole2_x, hole_y, hole_w, hole_h, 0);

        let island_w = 10_usize;
        let island_h = 10_usize;
        let island_y = hole_y + 10;
        let island1_x = hole1_x + 5;
        let island2_x = hole2_x + 5;
        fill_rect(&mut im, island1_x, island_y, island_w, island_h, 1);
        fill_rect(&mut im, island2_x, island_y, island_w, island_h, 1);

        // im.to_mask_im().save_png("_test_suzuki_abe_parent_input.png");

        let contours = contours_by_suzuki_abe(&mut im);
        assert_eq!(contours.len(), 5, "expected 1 outer + 2 holes + 2 islands");

        // contours.dump();

        let outer_bbox = (
            outer_x as i32,
            outer_y as i32,
            (outer_x + outer_w - 1) as i32,
            (outer_y + outer_h - 1) as i32,
        );
        let island1_bbox = (
            island1_x as i32,
            island_y as i32,
            (island1_x + island_w - 1) as i32,
            (island_y + island_h - 1) as i32,
        );
        let island2_bbox = (
            island2_x as i32,
            island_y as i32,
            (island2_x + island_w - 1) as i32,
            (island_y + island_h - 1) as i32,
        );

        // Hole contours are traced on the white pixels surrounding the black rects, so their
        // bounding boxes expand by 1px around the hole interior.
        let hole1_contour_bbox = (
            (hole1_x as i32) - 1,
            (hole_y as i32) - 1,
            (hole1_x + hole_w) as i32,
            (hole_y + hole_h) as i32,
        );
        let hole2_contour_bbox = (
            (hole2_x as i32) - 1,
            (hole_y as i32) - 1,
            (hole2_x + hole_w) as i32,
            (hole_y + hole_h) as i32,
        );

        let mut outer_idx: Option<usize> = None;
        let mut hole_idxs: Vec<usize> = Vec::new();
        let mut island_idxs: Vec<usize> = Vec::new();

        for (i, c) in contours.iter().enumerate() {
            let bb = bbox(&c.points);
            if !c.is_hole && c.parent.is_none() && bb == outer_bbox {
                outer_idx = Some(i);
            }
            if c.is_hole && (bb == hole1_contour_bbox || bb == hole2_contour_bbox) {
                hole_idxs.push(i);
            }
            if !c.is_hole && c.parent.is_none() && (bb == island1_bbox || bb == island2_bbox) {
                island_idxs.push(i);
            }
        }

        let outer_idx = outer_idx.expect("outer contour not found");
        assert_eq!(hole_idxs.len(), 2, "expected two hole contours");
        assert_eq!(
            island_idxs.len(),
            2,
            "expected two independent top-level islands"
        );

        for &hi in &hole_idxs {
            assert_eq!(
                contours[hi].parent,
                Some(outer_idx),
                "hole should be parented to outer"
            );
        }
        for &ii in &island_idxs {
            assert_eq!(
                contours[ii].parent, None,
                "island should be top-level in this impl"
            );
        }

        let mut debug_im = RGBAIm::new(im.w, im.h);
        let mask_im = im.to_mask_im();
        im::copy_mask_im_to_rgba_im(&mask_im, &mut debug_im, 200, 200, 200);
        contours.draw_into_rgba_im_alternating_colors(&mut debug_im, 255, 0, 0, 0, 255, 0);
        debug_im.save_png("_test_suzuki_abe_contours_no_simplify.png");

        let mut debug_im = RGBAIm::new(im.w, im.h);
        let mask_im = im.to_mask_im();
        im::copy_mask_im_to_rgba_im(&mask_im, &mut debug_im, 200, 200, 200);
        for contour in &contours {
            let simplified = contour.simplify_by_rdp(0.9);
            simplified.draw_into_rgba_im_alternating_colors(&mut debug_im, 255, 0, 0, 0, 255, 0);
        }
        debug_im.save_png("_test_suzuki_abe_contours_with_simplify.png");

    }

    #[test]
    fn simplify_by_rdp_open_line_keeps_endpoints() {
        let c = Contour {
            id: 1,
            is_hole: false,
            parent: None,
            points: vec![Iv2 { x: 0, y: 0 }, Iv2 { x: 5, y: 0 }, Iv2 { x: 10, y: 0 }],
        };

        let s = c.simplify_by_rdp(0.0);
        assert_eq!(s.points.first().copied(), Some(Iv2 { x: 0, y: 0 }));
        assert_eq!(s.points.last().copied(), Some(Iv2 { x: 10, y: 0 }));
    }

    #[test]
    fn simplify_by_rdp_open_line_drops_near_collinear() {
        let c = Contour {
            id: 1,
            is_hole: false,
            parent: None,
            points: vec![Iv2 { x: 0, y: 0 }, Iv2 { x: 5, y: 1 }, Iv2 { x: 10, y: 0 }],
        };

        // With a tolerance larger than the peak's deviation (~1), it should simplify to endpoints.
        let s = c.simplify_by_rdp(2.0);
        assert_eq!(s.points, vec![Iv2 { x: 0, y: 0 }, Iv2 { x: 10, y: 0 }]);

        // With a tighter tolerance, it should keep the middle point.
        let s2 = c.simplify_by_rdp(0.5);
        assert_eq!(s2.points, c.points);
    }

    #[test]
    fn simplify_by_rdp_closed_contour_stays_closed() {
        let c = Contour {
            id: 1,
            is_hole: false,
            parent: None,
            points: vec![
                Iv2 { x: 0, y: 0 },
                Iv2 { x: 10, y: 0 },
                Iv2 { x: 10, y: 10 },
                Iv2 { x: 0, y: 10 },
                Iv2 { x: 0, y: 0 },
            ],
        };

        let s = c.simplify_by_rdp(0.1);
        assert_eq!(s.points.first().copied(), Some(Iv2 { x: 0, y: 0 }));
        assert_eq!(s.points.last().copied(), Some(Iv2 { x: 0, y: 0 }));
        assert!(s.points.len() >= 4);

        s.dump();
    }
}
