use crate::im::{Im1Mut, Lum16Im};
// use crate::region_tree::CutBand;
// use crate::toolpath::ToolPath;
use crate::desc::Thou;
use crate::parallelogram::{
    draw_parallelogram_horz_no_bounds_single_z,
    draw_parallelogram_vert_no_bounds_single_z,
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

    if dx != 0 || dy != 0 {
        if dx.abs() >= dy.abs() {
            // Mostly horizontal line
            if dx < 0 {
                // Swap to make left-to-right
                tmp = q0;
                q0 = q1;
                q1 = tmp;
            }
            draw_parallelogram_horz_no_bounds_single_z(im, q0, q1, radius_pix);
        }
        else {
            // Mostly vertical line
            draw_parallelogram_vert_no_bounds_single_z(im, q0, q1, radius_pix);
        }

        splat_pixel_iz_no_bounds(q0.0, q0.1, im, z_u16, &circle_pixel_iz);
    }
    splat_pixel_iz_no_bounds(q1.0, q1.1, im, z_u16, &circle_pixel_iz);
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
