use crate::im::{Im1Mut, Lum16Im};
// use crate::region_tree::CutBand;
// use crate::toolpath::ToolPath;
use crate::desc::Thou;


/// The goal of this module is to simulate the effect of toolpaths on a heightmap image.
/// The toolpaths are assumed in the correct order. The Toolpaths are in pixel X/Y and thou Z.

/// Return a list of signed pixel indices that form a circular shape centered at (0,0) given the stride .
pub fn circle_pixel_iz(radius_pix:usize, stride: usize) -> Vec<isize> {
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
        debug_assert!(i >= 0, "splat_pixel_iz_no_bounds: negative index (center_i={center_i}, di={di})");
        debug_assert!(i < len_i, "splat_pixel_iz_no_bounds: OOB index (i={i}, len={len_i})");

        unsafe {
            let p = arr.get_unchecked_mut(i as usize);
            if z < *p {
                *p = z;
            }
        }
    }
}


fn draw_parallelogram_vertical_no_bounds_single_z(
    im: &mut Lum16Im,
    srt: (usize, usize, Thou),
    end: (usize, usize, Thou),
    tool_radius_pix: usize,
) {
    let rf = tool_radius_pix as f64;
    let z_u16 = srt.2.0 as u16;
    let stride = im.s;
    let arr = &mut im.arr;
    debug_assert!(srt.2 == end.2);

    let sx = srt.0;
    let sy = srt.1;
    let sxf = sx as f64 + 0.5;
    let syf = sy as f64 + 0.5;

    let ex = end.0;
    let ey = end.1;
    let exf = ex as f64 + 0.5;
    let eyf = ey as f64 + 0.5;

    let dxf = exf - sxf;
    let dyf = eyf - syf;
    debug_assert!(dyf > 0.0);
    let d_magf = (dxf * dxf + dyf * dyf).sqrt();

    let x_spanf = rf * d_magf / dyf;
    let y_offsf = (x_spanf * x_spanf - rf * rf).sqrt() * dyf / d_magf;
    let y_srtf = syf + y_offsf;
    let y_srt = y_srtf.round() as usize;
    let y_end = (syf + dyf - y_offsf).round() as usize;
    let x_stepf = dxf / dyf;

    // TODO: Fill in the top parallelogram cap


    let mut y = y_srt;
    let mut xf = sxf + x_stepf * y_offsf;
    while y < y_end {
        let i_at_row = y * stride;
        let x_min = (xf - x_spanf).ceil() as usize;
        let x_max = (xf + x_spanf).floor() as usize;

        let mut i = i_at_row + x_min;
        let rgt_i = i_at_row + x_max;
        while i <= rgt_i {
            unsafe {
                let p = arr.get_unchecked_mut(i);
                if z_u16 < *p {
                    *p = z_u16;
                }
            }
            i += 1;
        }

        xf += x_stepf;
        y += 1;
    }
}


/// Draw a line with rounded ends into a Lum16Im, interpolating the height values along the line.
/// Clip the line to the image bounds before starting.
/// Only set the pixel value if the new value is lower (deeper cut).
pub fn draw_toolpath_single_depth(
    im: &mut Lum16Im,
    start: (usize, usize, Thou),
    end: (usize, usize, Thou),
    tool_radius_pix: usize,
) {
    draw_parallelogram_vertical_no_bounds_single_z(im, start, end, tool_radius_pix);
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

