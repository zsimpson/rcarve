use crate::desc::Thou;
use crate::im::Lum16Im;

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
        let x_lft = (xf + x_lftf).floor() as usize;
        let x_rgt = (xf + x_rgtf).ceil() as usize;

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

/// Draw a vertical parallelogram shape into a Lum16Im along a center-line
/// given by (x0f, y0f) to (x1f, y1f), with left and right extents.
///
/// This variant clamps spans to the image bounds once per scanline and then
/// uses unchecked indexing in the inner loop.
fn parallelogram_vert_bounded_single_z(
    im: &mut Lum16Im,
    x0f: f64,
    y0f: f64,
    x1f: f64,
    y1f: f64,
    x_lftf: f64,
    x_rgtf: f64,
    z: u16,
) {
    let w = im.w as isize;
    let h = im.h as isize;
    let stride = im.s as isize;
    let arr = &mut im.arr;

    let x_stepf = (x1f - x0f) / (y1f - y0f);

    let mut xf = x0f;
    let mut y = y0f.round() as isize;
    let mut y1 = y1f.round() as isize;

    if y1 <= 0 || y >= h {
        return;
    }

    if y < 0 {
        xf += x_stepf * ((0 - y) as f64);
        y = 0;
    }
    if y1 > h {
        y1 = h;
    }

    while y < y1 {
        let i_at_row = y * stride;

        let mut x_lft = (xf + x_lftf).floor() as isize;
        let mut x_rgt = (xf + x_rgtf).ceil() as isize;

        if x_rgt >= 0 && x_lft < w {
            if x_lft < 0 {
                x_lft = 0;
            }
            if x_rgt >= w {
                x_rgt = w - 1;
            }

            let mut i = (i_at_row + x_lft) as usize;
            let rgt_i = (i_at_row + x_rgt) as usize;
            while i <= rgt_i {
                unsafe {
                    let p = arr.get_unchecked_mut(i);
                    if z < *p {
                        *p = z;
                    }
                }
                i += 1;
            }
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
        let y_top = (yf + y_topf).floor() as usize;
        let y_bot = (yf + t_botf).ceil() as usize;

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

/// Draw a horizontal parallelogram shape into a Lum16Im along a center-line
/// given by (x0f, y0f) to (x1f, y1f), with left and right extents.
///
/// This variant clamps spans to the image bounds once per scanline and then
/// uses unchecked indexing in the inner loop.
fn parallelogram_horz_bounded_single_z(
    im: &mut Lum16Im,
    x0f: f64,
    y0f: f64,
    x1f: f64,
    y1f: f64,
    y_topf: f64,
    t_botf: f64,
    z: u16,
) {
    let w = im.w as isize;
    let h = im.h as isize;
    let stride = im.s as isize;
    let arr = &mut im.arr;

    let y_stepf = (y1f - y0f) / (x1f - x0f);

    let mut yf = y0f;
    let mut x = x0f.round() as isize;
    let mut x1 = x1f.round() as isize;

    if x1 <= 0 || x >= w {
        return;
    }

    if x < 0 {
        yf += y_stepf * ((0 - x) as f64);
        x = 0;
    }
    if x1 > w {
        x1 = w;
    }

    while x < x1 {
        let i_at_col = x;

        let mut y_top = (yf + y_topf).floor() as isize;
        let mut y_bot = (yf + t_botf).ceil() as isize;

        if y_bot >= 0 && y_top < h {
            if y_top < 0 {
                y_top = 0;
            }
            if y_bot >= h {
                y_bot = h - 1;
            }

            let mut i = (i_at_col + stride * y_top) as usize;
            let bot_i = (i_at_col + stride * y_bot) as usize;
            while i <= bot_i {
                unsafe {
                    let p = arr.get_unchecked_mut(i);
                    if z < *p {
                        *p = z;
                    }
                }
                i += stride as usize;
            }
        }

        yf += y_stepf;
        x += 1;
    }
}

pub(crate) fn draw_parallelogram_vert_no_bounds_single_z(
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

pub(crate) fn draw_parallelogram_vert_bounded_single_z(
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

    parallelogram_vert_bounded_single_z(
        im,
        x0f - otside_xof,
        y0f - otside_yof,
        x0f + inside_xof,
        y0f + inside_yof,
        top_lft,
        top_rgt,
        z_u16,
    );

    parallelogram_vert_bounded_single_z(
        im,
        x0f + inside_xof,
        y0f + inside_yof,
        x1f - otside_xof,
        y1f - otside_yof,
        -spnf,
        spnf,
        z_u16,
    );

    parallelogram_vert_bounded_single_z(
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

pub(crate) fn draw_parallelogram_horz_no_bounds_single_z(
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

pub(crate) fn draw_parallelogram_horz_bounded_single_z(
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

    parallelogram_horz_bounded_single_z(
        im,
        x0f - otside_xof,
        y0f - otside_yof,
        x0f + inside_xof,
        y0f + inside_yof,
        top_lft,
        top_rgt,
        z_u16,
    );

    parallelogram_horz_bounded_single_z(
        im,
        x0f + inside_xof,
        y0f + inside_yof,
        x1f - otside_xof,
        y1f - otside_yof,
        -spnf,
        spnf,
        z_u16,
    );

    parallelogram_horz_bounded_single_z(
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
