use crate::im::{Im1Mut, Lum16Im};
// use crate::region_tree::CutBand;
// use crate::toolpath::ToolPath;
use crate::desc::Thou;
use crate::parallelogram::{
    draw_parallelogram_horz_no_bounds_single_z,
    draw_parallelogram_horz_bounded_single_z,
    draw_parallelogram_vert_no_bounds_single_z,
    draw_parallelogram_vert_bounded_single_z,
};

/// The goal of this module is to simulate the effect of toolpaths on a heightmap image.
/// The toolpaths are assumed in the correct order. The Toolpaths are in pixel X/Y and thou Z.

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

#[inline]
fn point_near_bounds(p: (usize, usize, Thou), radius_pix: usize, w: usize, h: usize) -> bool {
    p.0 < radius_pix
        || p.1 < radius_pix
        || p.0.saturating_add(radius_pix) >= w
        || p.1.saturating_add(radius_pix) >= h
}
/// Draw a line with rounded ends into a Lum16Im, interpolating the height values along the line.
/// Clip the line to the image bounds before starting.
/// Only set the pixel value if the new value is lower (deeper cut).
pub fn draw_toolpath_single_depth(
    im: &mut Lum16Im,
    p0: (usize, usize, Thou),
    p1: (usize, usize, Thou),
    radius_pix: usize,
    circle_pixel_iz: Vec<isize>,
) {
    debug_assert!(p0.2 == p1.2);
    let z_u16 = p0.2.0 as u16;

    let dx = p1.0 as isize - p0.0 as isize;
    let dy = p1.1 as isize - p0.1 as isize;
    let tmp;
    let mut q0 = p0;
    let mut q1 = p1;

    let use_bounded = point_near_bounds(q0, radius_pix, im.w, im.h)
        || point_near_bounds(q1, radius_pix, im.w, im.h);

    if dx != 0 || dy != 0 {
        if dx.abs() >= dy.abs() {
            // Mostly horizontal line
            if dx < 0 {
                // Swap to make left-to-right
                tmp = q0;
                q0 = q1;
                q1 = tmp;
            }
            if use_bounded {
                draw_parallelogram_horz_bounded_single_z(im, q0, q1, radius_pix);
            } else {
                draw_parallelogram_horz_no_bounds_single_z(im, q0, q1, radius_pix);
            }
        }
        else {
            // Mostly vertical line
            if use_bounded {
                draw_parallelogram_vert_bounded_single_z(im, q0, q1, radius_pix);
            } else {
                draw_parallelogram_vert_no_bounds_single_z(im, q0, q1, radius_pix);
            }
        }

        if use_bounded {
            splat_pixel_iz_bounded(q0.0, q0.1, im, z_u16, radius_pix, &circle_pixel_iz);
        } else {
            splat_pixel_iz_no_bounds(q0.0, q0.1, im, z_u16, &circle_pixel_iz);
        }
    }
    if use_bounded {
        splat_pixel_iz_bounded(q1.0, q1.1, im, z_u16, radius_pix, &circle_pixel_iz);
    } else {
        splat_pixel_iz_no_bounds(q1.0, q1.1, im, z_u16, &circle_pixel_iz);
    }
}

// Simulate toolpaths into a `Lum16Im` representing the result.
//
// Toolpath points are in pixel X/Y and thou Z, and are assumed to already be ordered.
// pub fn sim_toolpaths(
//     _im: &mut Lum16Im,
//     _toolpaths: &Vec<ToolPath>,
//     _cut_bands: &Vec<CutBand>,
//     _w: usize,
//     _h: usize,
// ) {
// }
