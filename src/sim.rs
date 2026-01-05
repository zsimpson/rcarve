use crate::im::Lum16Im;
use crate::region_tree::CutBand;
use crate::toolpath::ToolPath;
use crate::desc::Thou;

/// The goal of this module is to simualte the effect of toolpaths on a heightmap image.
/// The toolpaths are assumed in the correct order. The Toolpaths are in pixel X/Y and thou Z.
/// A fast function applies a rounde end to each side of the line and fills the interior.

pub fn make_circular_pixel_iz(tool_radius_pix: usize) -> Vec<usize> {
    let mut pixel_iz: Vec<usize> = Vec::new();
    let r_sq = (tool_radius_pix as isize) * (tool_radius_pix as isize);
    for dy in -(tool_radius_pix as isize)..=(tool_radius_pix as isize) {
        for dx in -(tool_radius_pix as isize)..=(tool_radius_pix as isize) {
            if dx * dx + dy * dy <= r_sq {
                let offset = (dy as isize) * (2 * tool_radius_pix as isize + 1) + (dx as isize)
                    + (tool_radius_pix as isize)
                    + (tool_radius_pix as isize) * (2 * tool_radius_pix as isize + 1);
                pixel_iz.push(offset as usize);
            }
        }
    }
    pixel_iz
}


/// Draw a line with rounded ends into a Lum16Im, interpolating the height values along the line.
/// Clip the line to the image bounds before starting.
/// Only set the pixel value if the new value is lower (deeper cut).
/// To make this very fast we create a LUT of circular end-caps for the tool radius which
/// is a vector of pixel offsets relative to the tool center.
/// We splat those offsets at each end of the line, and then fill in the rectangle between the two ends
/// using a scanline approach where we use Bresenham's line algorithm and on mostly-vertical
/// lines we draw horizontal spans, and on mostly-horizontal lines we draw vertical spans.
pub fn draw_line_path_rounded_ends(
    tool_circle_lut: &Vec<usize>,
    im: &mut Lum16Im,
    start: (usize, usize, Thou),
    end: (usize, usize, Thou),
    tool_radius_pix: usize,
) {
    let w = im.w;
    let h = im.h;
    if w == 0 || h == 0 {
        return;
    }

    let (sx, sy, sz) = start;
    let (ex, ey, ez) = end;

    let kernel_w = 2 * tool_radius_pix + 1;

    let to_u16 = |z: Thou| -> u16 {
        let zi = z.0 as i64;
        if zi <= 0 {
            0
        } else if zi >= (u16::MAX as i64) {
            u16::MAX
        } else {
            zi as u16
        }
    };

    let set_min = |im: &mut Lum16Im, x: usize, y: usize, v: u16| {
        let idx = y * im.s + x;
        let cur = im.arr[idx];
        if v < cur {
            im.arr[idx] = v;
        }
    };

    let splat_circle = |im: &mut Lum16Im, cx: usize, cy: usize, v: u16| {
        let cx_i = cx as isize;
        let cy_i = cy as isize;
        let r_i = tool_radius_pix as isize;
        for &k in tool_circle_lut {
            let kx = (k % kernel_w) as isize;
            let ky = (k / kernel_w) as isize;
            let dx = kx - r_i;
            let dy = ky - r_i;
            let x = cx_i + dx;
            let y = cy_i + dy;
            if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                set_min(im, x as usize, y as usize, v);
            }
        }
    };

    // Always splat rounded ends.
    splat_circle(im, sx, sy, to_u16(sz));
    splat_circle(im, ex, ey, to_u16(ez));

    // Fill the interior by walking the centerline with Bresenham and drawing a span
    // perpendicular to the major axis.
    let mut x0 = sx as i32;
    let mut y0 = sy as i32;
    let x1 = ex as i32;
    let y1 = ey as i32;

    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx_step = if x0 < x1 { 1 } else { -1 };
    let sy_step = if y0 < y1 { 1 } else { -1 };

    let steps = dx.max(dy).max(1) as f64;
    let z0 = sz.0 as f64;
    let z1f = ez.0 as f64;

    let mut i_step = 0_i32;

    if dx >= dy {
        // Mostly-horizontal: step in X, draw vertical spans.
        let mut err = dx / 2;
        loop {
            let t = (i_step as f64) / steps;
            let z = (z0 + (z1f - z0) * t).round() as i32;
            let zv = to_u16(Thou(z));

            let x = x0;
            let y = y0;
            if x >= 0 && y >= 0 {
                let x_u = x as usize;
                if x_u < w {
                    let y_min = (y - tool_radius_pix as i32).max(0) as usize;
                    let y_max = (y + tool_radius_pix as i32).min((h - 1) as i32) as usize;
                    for yy in y_min..=y_max {
                        set_min(im, x_u, yy, zv);
                    }
                }
            }

            if x0 == x1 {
                break;
            }
            x0 += sx_step;
            err -= dy;
            if err < 0 {
                y0 += sy_step;
                err += dx;
            }
            i_step += 1;
        }
    } else {
        // Mostly-vertical: step in Y, draw horizontal spans.
        let mut err = dy / 2;
        loop {
            let t = (i_step as f64) / steps;
            let z = (z0 + (z1f - z0) * t).round() as i32;
            let zv = to_u16(Thou(z));

            let x = x0;
            let y = y0;
            if x >= 0 && y >= 0 {
                let y_u = y as usize;
                if y_u < h {
                    let x_min = (x - tool_radius_pix as i32).max(0) as usize;
                    let x_max = (x + tool_radius_pix as i32).min((w - 1) as i32) as usize;
                    for xx in x_min..=x_max {
                        set_min(im, xx, y_u, zv);
                    }
                }
            }

            if y0 == y1 {
                break;
            }
            y0 += sy_step;
            err -= dx;
            if err < 0 {
                x0 += sx_step;
                err += dy;
            }
            i_step += 1;
        }
    }
}

/// Simulate toolpaths into a `Lum16Im` representing the result.
///
/// Toolpath points are in pixel X/Y and thou Z, and are assumed to already be ordered.
pub fn sim_toolpaths(
    im: &mut Lum16Im,
    toolpaths: &Vec<ToolPath>,
    _cut_bands: &Vec<CutBand>,
    _w: usize,
    _h: usize,
) {
    for path in toolpaths {
        let tool_radius_pix = path.tool_dia_pix / 2;
        let tool_circle_lut = make_circular_pixel_iz(tool_radius_pix);

        for win in path.points.windows(2) {
            let a = win[0];
            let b = win[1];

            if a.x < 0 || a.y < 0 || b.x < 0 || b.y < 0 {
                continue;
            }
            let ax = (a.x as usize).min(im.w.saturating_sub(1));
            let ay = (a.y as usize).min(im.h.saturating_sub(1));
            let bx = (b.x as usize).min(im.w.saturating_sub(1));
            let by = (b.y as usize).min(im.h.saturating_sub(1));

            draw_line_path_rounded_ends(
                &tool_circle_lut,
                im,
                (ax, ay, Thou(a.z)),
                (bx, by, Thou(b.z)),
                tool_radius_pix,
            );
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_line_capsule_sets_expected_pixels() {
        let mut im = Lum16Im::new(20, 20);
        im.arr.fill(1000);

        let r = 1;
        let lut = make_circular_pixel_iz(r);

        draw_line_path_rounded_ends(
            &lut,
            &mut im,
            (5, 5, Thou(200)),
            (10, 5, Thou(200)),
            r,
        );

        let at = |x: usize, y: usize| -> u16 { im.arr[y * im.s + x] };

        // Inside capsule.
        assert_eq!(at(4, 5), 200);
        assert_eq!(at(5, 4), 200);
        assert_eq!(at(8, 6), 200);
        assert_eq!(at(11, 5), 200);

        // Outside capsule.
        assert_eq!(at(3, 5), 1000);
        assert_eq!(at(12, 5), 1000);
        assert_eq!(at(4, 4), 1000);
        assert_eq!(at(11, 4), 1000);
    }

    #[test]
    fn draw_line_interpolates_z_along_centerline() {
        let mut im = Lum16Im::new(30, 10);
        im.arr.fill(1000);

        let r = 0;
        let lut = make_circular_pixel_iz(r);

        // Horizontal line 5px long (x=5..10). With steps=5, x=7 is t=2/5 => 220.
        draw_line_path_rounded_ends(
            &lut,
            &mut im,
            (5, 5, Thou(300)),
            (10, 5, Thou(100)),
            r,
        );

        let at = |x: usize, y: usize| -> u16 { im.arr[y * im.s + x] };
        assert_eq!(at(5, 5), 300);
        assert_eq!(at(10, 5), 100);
        assert_eq!(at(7, 5), 220);
    }
}

