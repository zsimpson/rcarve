use crate::im::{Im1Mut, Lum16Im};
use crate::toolpath::{IV3, ToolPath};

/// The goal of this module is to simulate the effect of toolpaths on a heightmap image.
/// The toolpaths are assumed in the correct order. The Toolpaths are in pixel X/Y and thou Z.

// Debug UI window (feature-gated).
// -----------------------------------------------------------------------------

#[cfg(all(feature = "debug_ui", not(feature = "cli_only")))]
pub mod debug_ui;

/// Return a list of signed pixel indices that form a circular shape centered at (0,0) given the stride .
pub fn circle_pixel_iz(radius_pix: usize, stride: usize) -> Vec<isize> {
    let mut pixel_iz = Vec::new();
    let r = radius_pix as isize;
    let r_sq = r * r;
    let s_isize = stride as isize;
    for y in -r..=r {
        for x in -r..=r {
            if x * x + y * y <= r_sq {
                let iz = y * s_isize + x;
                pixel_iz.push(iz);
            }
        }
    }
    pixel_iz
}

pub fn splat_pixel_iz_no_bounds(
    cen_x: usize,
    cen_y: usize,
    im: &mut Lum16Im,
    z: u16,
    pixel_iz: &[isize],
) {
    let stride = im.s;
    let center_i = (cen_y * stride + cen_x) as isize;
    let arr = im.arr_mut();
    let len_i = arr.len() as isize;

    for &di in pixel_iz {
        let i = center_i + di;
        debug_assert!(
            i >= 0,
            "splat_pixel_iz_no_bounds: negative index (center_i={center_i}, di={di})"
        );
        debug_assert!(
            i < len_i,
            "splat_pixel_iz_no_bounds: OOB index (i={i}, len={len_i})"
        );

        unsafe {
            let p = arr.get_unchecked_mut(i as usize);
            if z < *p {
                *p = z;
            }
        }
    }
}

pub fn splat_pixel_iz_bounded(
    cen_x: usize,
    cen_y: usize,
    im: &mut Lum16Im,
    z: u16,
    radius_pix: usize,
    pixel_iz: &[isize],
) {
    let stride = im.s;
    let w_usize = im.w;
    let h_usize = im.h;
    let w = w_usize as isize;
    let h = h_usize as isize;
    let cen_x_i = cen_x as isize;
    let cen_y_i = cen_y as isize;
    let r = radius_pix as isize;
    let arr = im.arr_mut();

    for &di in pixel_iz {
        // `di` was constructed as: di = dy * stride + dx, with dx,dy in [-radius_pix, radius_pix].
        // We must clip in pixel-space; computing x/y from a flattened index wraps at row boundaries.
        let mut dy = di / stride as isize;
        let mut dx = di - dy * stride as isize;

        // `di/stride` uses truncating division; adjust so that dx is within [-r, r].
        if dx < -r {
            dx += stride as isize;
            dy -= 1;
        } else if dx > r {
            dx -= stride as isize;
            dy += 1;
        }

        let x = cen_x_i + dx;
        let y = cen_y_i + dy;
        if x < 0 || x >= w || y < 0 || y >= h {
            continue;
        }

        let i = (y as usize) * w_usize + (x as usize);

        unsafe {
            let p = arr.get_unchecked_mut(i);
            if z < *p {
                *p = z;
            }
        }
    }
}

/// Render a triangle into im at a single Z height, without bounds checking.
pub fn triangle_no_bounds_single_z(
    a: (isize, isize),
    b: (isize, isize),
    c: (isize, isize),
    im: &mut Lum16Im,
    z: u16,
) {
    #[inline(always)]
    fn edge_setup(x0: i64, y0: i64, x1: i64, y1: i64, y_start: i64) -> (i64, i64) {
        debug_assert!(y0 != y1);
        debug_assert!(y0 < y1);
        debug_assert!(y_start >= y0);
        debug_assert!(y_start <= y1);

        let dy = y1 - y0;
        let dx = x1 - x0;
        let step_fp = (dx << 16) / dy;
        let x_start_fp = (x0 << 16) + step_fp * (y_start - y0);
        (x_start_fp, step_fp)
    }

    #[inline(always)]
    fn draw_span_no_bounds_single_z(
        arr: &mut [u16],
        stride: usize,
        y: usize,
        x0_fp: i64,
        x1_fp: i64,
        z: u16,
    ) {
        let (mut left_fp, mut right_fp) = (x0_fp, x1_fp);
        if left_fp > right_fp {
            std::mem::swap(&mut left_fp, &mut right_fp);
        }

        // Inclusive span: [ceil(left), floor(right)].
        let xl = (left_fp + 0xFFFF) >> 16;
        let xr = right_fp >> 16;
        if xl > xr {
            return;
        }

        debug_assert!(xl >= 0);
        debug_assert!(xr >= 0);
        let row_start = y * stride;
        let mut i = row_start + (xl as usize);
        let end_i = row_start + (xr as usize);
        while i <= end_i {
            unsafe {
                let p = arr.get_unchecked_mut(i);
                if z < *p {
                    *p = z;
                }
            }
            i += 1;
        }
    }

    let stride = im.s;
    let arr = im.arr_mut();

    // Sort vertices by y, then by x for stability.
    let mut v = [a, b, c];
    v.sort_unstable_by(|p, q| p.1.cmp(&q.1).then(p.0.cmp(&q.0)));
    let (x0, y0) = (v[0].0 as i64, v[0].1 as i64);
    let (x1, y1) = (v[1].0 as i64, v[1].1 as i64);
    let (x2, y2) = (v[2].0 as i64, v[2].1 as i64);

    debug_assert!(y0 <= y1 && y1 <= y2);
    if y0 == y2 {
        // Degenerate (flat) triangle: just draw the horizontal span on that scanline.
        let y = y0 as usize;
        let min_x = x0.min(x1).min(x2);
        let max_x = x0.max(x1).max(x2);
        draw_span_no_bounds_single_z(arr, stride, y, min_x << 16, max_x << 16, z);
        return;
    }

    // Decide which side the long edge (v0->v2) is on, by comparing its x at y1 to x1.
    let long_left = if y1 == y0 {
        // Top is flat; compare at y0+1 (any y in the lower half works).
        let y_probe = y0 + 1;
        let x_long_probe_fp = (x0 << 16) + ((x2 - x0) << 16) * (y_probe - y0) / (y2 - y0);
        x_long_probe_fp < (x1 << 16)
    } else {
        let x_long_at_y1_fp = (x0 << 16) + ((x2 - x0) << 16) * (y1 - y0) / (y2 - y0);
        x_long_at_y1_fp < (x1 << 16)
    };

    // Top half: y in [y0, y1) using edges (v0->v1) and (v0->v2).
    if y0 < y1 {
        let (x_long_fp, long_step_fp) = edge_setup(x0, y0, x2, y2, y0);
        let (x_short_fp, short_step_fp) = edge_setup(x0, y0, x1, y1, y0);

        let (mut x_left_fp, left_step_fp, mut x_right_fp, right_step_fp) = if long_left {
            (x_long_fp, long_step_fp, x_short_fp, short_step_fp)
        } else {
            (x_short_fp, short_step_fp, x_long_fp, long_step_fp)
        };

        let mut y = y0;
        while y < y1 {
            draw_span_no_bounds_single_z(arr, stride, y as usize, x_left_fp, x_right_fp, z);
            x_left_fp += left_step_fp;
            x_right_fp += right_step_fp;
            y += 1;
        }
    }

    // Bottom half: y in [y1, y2] using edges (v1->v2) and (v0->v2).
    if y1 < y2 {
        let (x_long_fp, long_step_fp) = edge_setup(x0, y0, x2, y2, y1);
        let (x_short_fp, short_step_fp) = edge_setup(x1, y1, x2, y2, y1);

        let (mut x_left_fp, left_step_fp, mut x_right_fp, right_step_fp) = if long_left {
            (x_long_fp, long_step_fp, x_short_fp, short_step_fp)
        } else {
            (x_short_fp, short_step_fp, x_long_fp, long_step_fp)
        };

        let mut y = y1;
        while y <= y2 {
            draw_span_no_bounds_single_z(arr, stride, y as usize, x_left_fp, x_right_fp, z);
            x_left_fp += left_step_fp;
            x_right_fp += right_step_fp;
            y += 1;
        }
    }
}

/// Render a triangle into im at a single Z height, clipping spans to image bounds.
///
/// This is a scanline rasterizer: it walks y from ymin..ymax and fills contiguous x spans.
pub fn triangle_with_bounds_single_z(
    a: (isize, isize),
    b: (isize, isize),
    c: (isize, isize),
    im: &mut Lum16Im,
    z: u16,
) {
    #[inline(always)]
    fn edge_setup(x0: i64, y0: i64, x1: i64, y1: i64, y_start: i64) -> (i64, i64) {
        debug_assert!(y0 != y1);
        debug_assert!(y0 < y1);
        debug_assert!(y_start >= y0);
        debug_assert!(y_start <= y1);

        let dy = y1 - y0;
        let dx = x1 - x0;
        let step_fp = (dx << 16) / dy;
        let x_start_fp = (x0 << 16) + step_fp * (y_start - y0);
        (x_start_fp, step_fp)
    }

    #[inline(always)]
    fn draw_span_bounded_single_z(
        arr: &mut [u16],
        stride: usize,
        y: usize,
        w: i64,
        x0_fp: i64,
        x1_fp: i64,
        z: u16,
    ) {
        let (mut left_fp, mut right_fp) = (x0_fp, x1_fp);
        if left_fp > right_fp {
            std::mem::swap(&mut left_fp, &mut right_fp);
        }

        // Inclusive span: [ceil(left), floor(right)].
        let mut xl = (left_fp + 0xFFFF) >> 16;
        let mut xr = right_fp >> 16;
        if xl > xr {
            return;
        }

        if xr < 0 || xl >= w {
            return;
        }

        if xl < 0 {
            xl = 0;
        }
        if xr >= w {
            xr = w - 1;
        }
        if xl > xr {
            return;
        }

        let row_start = y * stride;
        let mut i = row_start + (xl as usize);
        let end_i = row_start + (xr as usize);
        while i <= end_i {
            unsafe {
                let p = arr.get_unchecked_mut(i);
                if z < *p {
                    *p = z;
                }
            }
            i += 1;
        }
    }

    let w = im.w as i64;
    let h = im.h as i64;
    if w <= 0 || h <= 0 {
        return;
    }

    let stride = im.s;
    let arr = im.arr_mut();

    // Sort vertices by y, then by x for stability.
    let mut v = [a, b, c];
    v.sort_unstable_by(|p, q| p.1.cmp(&q.1).then(p.0.cmp(&q.0)));
    let (x0, y0) = (v[0].0 as i64, v[0].1 as i64);
    let (x1, y1) = (v[1].0 as i64, v[1].1 as i64);
    let (x2, y2) = (v[2].0 as i64, v[2].1 as i64);

    debug_assert!(y0 <= y1 && y1 <= y2);
    if y0 == y2 {
        // Degenerate (flat) triangle: just draw the horizontal span on that scanline.
        if y0 < 0 || y0 >= h {
            return;
        }
        let y = y0 as usize;
        let min_x = x0.min(x1).min(x2);
        let max_x = x0.max(x1).max(x2);
        draw_span_bounded_single_z(arr, stride, y, w, min_x << 16, max_x << 16, z);
        return;
    }

    // Decide which side the long edge (v0->v2) is on, by comparing its x at y1 to x1.
    let long_left = if y1 == y0 {
        let y_probe = y0 + 1;
        let x_long_probe_fp = (x0 << 16) + ((x2 - x0) << 16) * (y_probe - y0) / (y2 - y0);
        x_long_probe_fp < (x1 << 16)
    } else {
        let x_long_at_y1_fp = (x0 << 16) + ((x2 - x0) << 16) * (y1 - y0) / (y2 - y0);
        x_long_at_y1_fp < (x1 << 16)
    };

    // Top half: y in [y0, y1) using edges (v0->v1) and (v0->v2).
    if y0 < y1 {
        let y_start = y0.max(0);
        let y_end_excl = y1.min(h);
        if y_start < y_end_excl {
            let (x_long_fp, long_step_fp) = edge_setup(x0, y0, x2, y2, y_start);
            let (x_short_fp, short_step_fp) = edge_setup(x0, y0, x1, y1, y_start);

            let (mut x_left_fp, left_step_fp, mut x_right_fp, right_step_fp) = if long_left {
                (x_long_fp, long_step_fp, x_short_fp, short_step_fp)
            } else {
                (x_short_fp, short_step_fp, x_long_fp, long_step_fp)
            };

            let mut y = y_start;
            while y < y_end_excl {
                draw_span_bounded_single_z(arr, stride, y as usize, w, x_left_fp, x_right_fp, z);
                x_left_fp += left_step_fp;
                x_right_fp += right_step_fp;
                y += 1;
            }
        }
    }

    // Bottom half: y in [y1, y2] using edges (v1->v2) and (v0->v2).
    if y1 < y2 {
        let y_start = y1.max(0);
        let y_end_incl = y2.min(h - 1);
        if y_start <= y_end_incl {
            let (x_long_fp, long_step_fp) = edge_setup(x0, y0, x2, y2, y_start);
            let (x_short_fp, short_step_fp) = edge_setup(x1, y1, x2, y2, y_start);

            let (mut x_left_fp, left_step_fp, mut x_right_fp, right_step_fp) = if long_left {
                (x_long_fp, long_step_fp, x_short_fp, short_step_fp)
            } else {
                (x_short_fp, short_step_fp, x_long_fp, long_step_fp)
            };

            let mut y = y_start;
            while y <= y_end_incl {
                draw_span_bounded_single_z(arr, stride, y as usize, w, x_left_fp, x_right_fp, z);
                x_left_fp += left_step_fp;
                x_right_fp += right_step_fp;
                y += 1;
            }
        }
    }
}

#[inline]
fn point_near_bounds(p: IV3, radius_pix: usize, w: usize, h: usize) -> bool {
    let r = radius_pix as i32;
    p.x < r || p.y < r || p.x.saturating_add(r) >= w as i32 || p.y.saturating_add(r) >= h as i32
}
/// Draw a line with rounded ends into a Lum16Im, interpolating the height values along the line.
/// Clip the line to the image bounds before starting.
/// Only set the pixel value if the new value is lower (deeper cut).
pub fn draw_toolpath_segment_single_depth(
    im: &mut Lum16Im,
    p0: IV3,
    p1: IV3,
    radius_pix: usize,
    circle_pixel_iz: &[isize],
) {
    debug_assert!(p0.z == p1.z);
    let z_u16 = p0.z.clamp(0, u16::MAX as i32) as u16;

    // let dx = p1.0 as isize - p0.0 as isize;
    // let dy = p1.1 as isize - p0.1 as isize;
    // let tmp;
    // let mut q0 = p0;
    // let mut q1 = p1;

    let use_bounded = point_near_bounds(p0, radius_pix, im.w, im.h)
        || point_near_bounds(p1, radius_pix, im.w, im.h);

    let rf = radius_pix as f64;

    let p0x = p0.x as f64;
    let p0y = p0.y as f64;
    let p1x = p1.x as f64;
    let p1y = p1.y as f64;

    let px = p0x - p1x;
    let py = p0y - p1y;
    let p_mag = (px * px + py * py).sqrt();
    if p_mag == 0.0 {
        return;
    }
    let nx = px / p_mag;
    let ny = py / p_mag;
    let qx = -ny * rf;
    let qy = nx * rf;

    let a = ((p0x - qx).round() as isize, (p0y - qy).round() as isize);
    let b = ((p0x + qx).round() as isize, (p0y + qy).round() as isize);
    let c = ((p1x + qx).round() as isize, (p1y + qy).round() as isize);
    let d = ((p1x - qx).round() as isize, (p1y - qy).round() as isize);

    if use_bounded {
        triangle_with_bounds_single_z(a, b, c, im, z_u16);
        triangle_with_bounds_single_z(a, c, d, im, z_u16);
    } else {
        triangle_no_bounds_single_z(a, b, c, im, z_u16);
        triangle_no_bounds_single_z(a, c, d, im, z_u16);
    }

    let p0x_usize = p0.x as usize;
    let p0y_usize = p0.y as usize;
    let p1x_usize = p1.x as usize;
    let p1y_usize = p1.y as usize;

    if use_bounded {
        splat_pixel_iz_bounded(p0x_usize, p0y_usize, im, z_u16, radius_pix, &circle_pixel_iz);
    } else {
        splat_pixel_iz_no_bounds(p0x_usize, p0y_usize, im, z_u16, &circle_pixel_iz);
    }

    if use_bounded {
        splat_pixel_iz_bounded(p1x_usize, p1y_usize, im, z_u16, radius_pix, &circle_pixel_iz);
    } else {
        splat_pixel_iz_no_bounds(p1x_usize, p1y_usize, im, z_u16, &circle_pixel_iz);
    }
}

/// Simulate toolpaths into a `Lum16Im` representing the result.
/// Toolpath points are in pixel X/Y and thou Z, and are assumed to already be ordered.
pub fn sim_toolpaths(im: &mut Lum16Im, toolpaths: &[ToolPath], tool_dia_pix: usize) {
    if toolpaths.is_empty() {
        return;
    }

    // TODO: Multi tool. For now assume the first is the only.

    let radius_pix = tool_dia_pix / 2; //toolpaths[0].tool_dia_pix / 2;
    let circle_pixel_iz = circle_pixel_iz(radius_pix, im.s);
    // let z_thou = Thou(toolpaths[0].points[0].z);

    for toolpath in toolpaths {
        // Traverse consecutive point pairs.
        for seg in toolpath.points.windows(2) {
            let p0 = seg[0];
            let p1 = seg[1];

            draw_toolpath_segment_single_depth(
                im,
                p0,
                p1,
                radius_pix,
                &circle_pixel_iz,
            );
        }
    }
}
