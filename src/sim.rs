use crate::im::{Im1Mut, Lum16Im};
// use crate::region_tree::CutBand;
// use crate::toolpath::ToolPath;
use crate::desc::Thou;

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

pub fn splat_pixel_iz_no_bounds<TIm, T>(
    cen_x: usize,
    cen_y: usize,
    im: &mut TIm,
    z: T,
    pixel_iz: &[isize],
) where
    TIm: Im1Mut<T>,
    T: Copy + Ord,
{
    let stride = im.stride();
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

/// Draw a vertical parallelogram shape into a Lum16Im along a center-line
/// given by (x0f, y0f) to (x1f, y1f), with left and right extents
fn parallelogram_vert_no_bounds_single_z(
    im: &mut Lum16Im,
    x0f: f64,
    y0f: f64,
    x1f: f64,
    y1f: f64,
    x_lftf: f64,
    x_rgtf: f64,
    z: u16,
) {
    let stride = im.s;
    let arr = &mut im.arr;
    let x_stepf = (x1f - x0f) / (y1f - y0f);

    let mut xf = x0f;
    let mut y = y0f.round() as usize;
    let y1 = y1f.round() as usize;
    while y < y1 {
        let i_at_row = y * stride;
        let x_lft = (xf + x_lftf).ceil() as usize;
        let x_rgt = (xf + x_rgtf).floor() as usize;

        let mut i = i_at_row + x_lft;
        let rgt_i = i_at_row + x_rgt;
        while i <= rgt_i {
            unsafe {
                let p = arr.get_unchecked_mut(i);
                if z < *p {
                    *p = z;
                }
            }
            i += 1;
        }

        xf += x_stepf;
        y += 1;
    }
}

/// Draw a horizontal parallelogram shape into a Lum16Im along a center-line
/// given by (x0f, y0f) to (x1f, y1f), with left and right extents
fn parallelogram_horz_no_bounds_single_z(
    im: &mut Lum16Im,
    x0f: f64,
    y0f: f64,
    x1f: f64,
    y1f: f64,
    y_topf: f64,
    t_botf: f64,
    z: u16,
) {
    let stride = im.s;
    let arr = &mut im.arr;
    let y_stepf = (y1f - y0f) / (x1f - x0f);

    let mut yf = y0f;
    let mut x = x0f.round() as usize;
    let x1 = x1f.round() as usize;
    while x < x1 {
        let i_at_col = x;
        let y_top = (yf + y_topf).ceil() as usize;
        let y_bot = (yf + t_botf).floor() as usize;

        let mut i = i_at_col + stride * y_top;
        let bot_i = i_at_col + stride * y_bot;
        while i <= bot_i {
            unsafe {
                let p = arr.get_unchecked_mut(i);
                if z < *p {
                    *p = z;
                }
            }
            i += stride;
        }

        yf += y_stepf;
        x += 1;
    }
}

fn draw_parallelogram_vert_no_bounds_single_z(
    im: &mut Lum16Im,
    p0: (usize, usize, Thou),
    p1: (usize, usize, Thou),
    radius_pix: usize,
) {
    let rf = radius_pix as f64;
    let z_u16 = p0.2.0 as u16;
    debug_assert!(p0.2 == p1.2);

    let x0 = p0.0;
    let y0 = p0.1;
    let x0f = x0 as f64 + 0.5;
    let y0f = y0 as f64 + 0.5;

    let x1 = p1.0;
    let y1 = p1.1;
    let x1f = x1 as f64 + 0.5;
    let y1f = y1 as f64 + 0.5;

    let dxf = x1f - x0f;
    let dyf = y1f - y0f;
    debug_assert!(dyf > 0.0);
    let d_magf = (dxf * dxf + dyf * dyf).sqrt();

    let x_normf = dxf / d_magf;
    let y_normf = dyf / d_magf;

    let offf = rf * dxf / dyf;
    let spnf = rf * d_magf / dyf;
    // let side = if dxf >= 0.0 { 1.0 } else { -1.0 };

    let mut inside_offf = offf;
    if d_magf < 2.0 * offf {
        inside_offf = offf.min(d_magf - offf);
    }

    let top_lft;
    let top_rgt;
    let bot_lft;
    let bot_rgt;
    let inside_xof;
    let inside_yof;
    let otside_xof;
    let otside_yof;
    if dxf > 0.0 {
        // Right side
        top_lft = 0.0;
        top_rgt = spnf;
        bot_lft = -spnf;
        bot_rgt = 0.0;
        otside_xof = offf * x_normf;
        otside_yof = offf * y_normf;
        inside_xof = inside_offf * x_normf;
        inside_yof = inside_offf * y_normf;
    } else {
        // Left side
        // Swap which half gets filled versus the dxf>0 case.
        top_lft = -spnf;
        top_rgt = 0.0;
        bot_lft = 0.0;
        bot_rgt = spnf;
        otside_xof = -offf * x_normf;
        otside_yof = -offf * y_normf;
        inside_xof = -inside_offf * x_normf;
        inside_yof = -inside_offf * y_normf;
    }

    parallelogram_vert_no_bounds_single_z(
        im,
        x0f - otside_xof,
        y0f - otside_yof,
        x0f + inside_xof,
        y0f + inside_yof,
        top_lft,
        top_rgt,
        z_u16,
    );

    parallelogram_vert_no_bounds_single_z(
        im,
        x0f + inside_xof,
        y0f + inside_yof,
        x1f - otside_xof,
        y1f - otside_yof,
        -spnf,
        spnf,
        z_u16,
    );

    parallelogram_vert_no_bounds_single_z(
        im,
        x1f - inside_xof,
        y1f - inside_yof,
        x1f + otside_xof,
        y1f + otside_yof,
        bot_lft,
        bot_rgt,
        z_u16,
    );
}


fn draw_parallelogram_horz_no_bounds_single_z(
    im: &mut Lum16Im,
    p0: (usize, usize, Thou),
    p1: (usize, usize, Thou),
    radius_pix: usize,
) {
    let rf = radius_pix as f64;
    let z_u16 = p0.2.0 as u16;
    debug_assert!(p0.2 == p1.2);

    let x0 = p0.0;
    let y0 = p0.1;
    let x0f = x0 as f64 + 0.5;
    let y0f = y0 as f64 + 0.5;

    let x1 = p1.0;
    let y1 = p1.1;
    let x1f = x1 as f64 + 0.5;
    let y1f = y1 as f64 + 0.5;

    let dxf = x1f - x0f;
    let dyf = y1f - y0f;
    debug_assert!(dxf > 0.0);
    let d_magf = (dxf * dxf + dyf * dyf).sqrt();

    let x_normf = dxf / d_magf;
    let y_normf = dyf / d_magf;

    let offf = rf * dyf / dxf;
    let spnf = rf * d_magf / dxf;

    let mut inside_offf = offf;
    if d_magf < 2.0 * offf {
        inside_offf = offf.min(d_magf - offf);
    }

    // TODO rename
    let top_lft;
    let top_rgt;
    let bot_lft;
    let bot_rgt;
    let inside_xof;
    let inside_yof;
    let otside_xof;
    let otside_yof;
    if dyf > 0.0 {
        // Bottom side
        top_lft = 0.0;
        top_rgt = spnf;
        bot_lft = -spnf;
        bot_rgt = 0.0;
        otside_xof = offf * x_normf;
        otside_yof = offf * y_normf;
        inside_xof = inside_offf * x_normf;
        inside_yof = inside_offf * y_normf;
    } else {
        // Top side
        top_lft = -spnf;
        top_rgt = 0.0;
        bot_lft = 0.0;
        bot_rgt = spnf;
        otside_xof = -offf * x_normf;
        otside_yof = -offf * y_normf;
        inside_xof = -inside_offf * x_normf;
        inside_yof = -inside_offf * y_normf;
    }

    parallelogram_horz_no_bounds_single_z(
        im,
        x0f - otside_xof,
        y0f - otside_yof,
        x0f + inside_xof,
        y0f + inside_yof,
        top_lft,
        top_rgt,
        z_u16,
    );

    parallelogram_horz_no_bounds_single_z(
        im,
        x0f + inside_xof,
        y0f + inside_yof,
        x1f - otside_xof,
        y1f - otside_yof,
        -spnf,
        spnf,
        z_u16,
    );

    parallelogram_horz_no_bounds_single_z(
        im,
        x1f - inside_xof,
        y1f - inside_yof,
        x1f + otside_xof,
        y1f + otside_yof,
        bot_lft,
        bot_rgt,
        z_u16,
    );
}




/// Draw a line with rounded ends into a Lum16Im, interpolating the height values along the line.
/// Clip the line to the image bounds before starting.
/// Only set the pixel value if the new value is lower (deeper cut).
pub fn draw_toolpath_single_depth(
    im: &mut Lum16Im,
    start: (usize, usize, Thou),
    end: (usize, usize, Thou),
    tool_radius_pix: usize,
    circle_pixel_iz: Vec<isize>,
) {
    let dx = end.0 as isize - start.0 as isize;
    let dy = end.1 as isize - start.1 as isize;
    let tmp;

    let mut p0 = start;
    let mut p1 = end;

    if dx != 0 || dy != 0 {
        if dx.abs() >= dy.abs() {
            // Mostly horizontal line
            if dx < 0 {
                // Swap to make left-to-right
                tmp = p0;
                p0 = p1;
                p1 = tmp;
            }
            draw_parallelogram_horz_no_bounds_single_z(im, p0, p1, tool_radius_pix);
        }
        else {
            // Mostly vertical line
            draw_parallelogram_vert_no_bounds_single_z(im, p0, p1, tool_radius_pix);
        }

        splat_pixel_iz_no_bounds(p0.0, p0.1, im, 800 as u16, &circle_pixel_iz);
    }
    splat_pixel_iz_no_bounds(p1.0, p1.1, im, 800 as u16, &circle_pixel_iz);
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
