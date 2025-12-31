use crate::im::MaskIm;

// -----------------------------------------------------------------------------
// Window-based dilation op (like WinDilationOp)
// -----------------------------------------------------------------------------
#[derive(Clone, Debug)]
struct WinDilationOp {
    dia_pix: usize,
    offsets: Vec<isize>,
}

impl WinDilationOp {
    fn new(dia_pix: usize, im_pitch: usize) -> Self {
        assert!(dia_pix > 0 && dia_pix <= im_pitch);

        let r_pix = (dia_pix / 2) as isize;
        let r2_pix = r_pix * r_pix;

        // Upper bound (square), we’ll shrink by only pushing points in the disk.
        let mut offsets = Vec::with_capacity((dia_pix + 1) * (dia_pix + 1));

        let pitch = im_pitch as isize;
        for y in -r_pix..=r_pix {
            for x in -r_pix..=r_pix {
                if x * x + y * y <= r2_pix {
                    offsets.push(y * pitch + x);
                }
            }
        }

        Self {
            dia_pix,
            offsets,
        }
    }
}

// -----------------------------------------------------------------------------
// Window dilation core (full-image)
// -----------------------------------------------------------------------------
fn im_dilate_win_with_op(src: &MaskIm, dst: &mut MaskIm, op: &WinDilationOp) {
    assert_eq!(src.w, dst.w);
    assert_eq!(src.h, dst.h);
    assert_eq!(src.s, src.w); // for N_CH=1, stride == w
    assert_eq!(dst.s, dst.w);
    assert!(op.dia_pix > 0 && op.dia_pix <= src.w && op.dia_pix <= src.h);

    let w = src.w;
    let h = src.h;
    let wh = w * h;

    let src_arr: &[u8] = &src.arr;
    let dst_arr: &mut [u8] = &mut dst.arr;

    let set_val: u8 = 255;

    // This matches your C "edge = dia/2 + 1"
    let edge = op.dia_pix / 2 + 1;

    // Helper to index flat arrays
    #[inline(always)]
    fn idx(w: usize, x: usize, y: usize) -> usize {
        y * w + x
    }

    // -------------------------
    // Core region (no bounds checks on neighborhood)
    // -------------------------
    // This leaves enough margin so (curr + offset) stays in-bounds.
    let l0 = edge;
    let r0 = w.saturating_sub(edge);
    let t0 = edge;
    let b0 = h.saturating_sub(edge);

    // If image is too small for a core region, skip it (edges will handle everything).
    if l0 < r0 && t0 < b0 {
        for y in t0..b0 {
            let mut curr = idx(w, l0, y);
            for _x in l0..r0 {
                if src_arr[curr] == 0 {
                    let mut found = false;
                    for &off in &op.offsets {
                        // curr is in [0,wh), and core margin ensures neighbor is in-bounds.
                        let k = (curr as isize + off) as usize;
                        if src_arr[k] > 0 {
                            found = true;
                            break;
                        }
                    }
                    if found {
                        dst_arr[curr] = set_val;
                    } else {
                        dst_arr[curr] = 0;
                    }
                } else {
                    dst_arr[curr] = set_val;
                }
                curr += 1;
            }
        }
    }

    // -------------------------
    // Edges (bounds checks)
    // -------------------------
    // This is a faithful translation of the 4-case region logic.
    // We compute (l0,r0,t0,b0) = the edge strip to write,
    // and (l1,r1,t1,b1) = the region within which neighbor samples are allowed.
    let cases = 4;
    for case_i in 0..cases {
        let (l0e, r0e, t0e, b0e, l1e, r1e, t1e, b1e) = match case_i {
            // top strip
            0 => {
                let l0 = 0;
                let r0 = w;
                let t0 = 0;
                let b0 = edge.min(h);
                let l1 = l0;
                let r1 = r0;
                let t1 = t0;
                let b1 = (b0 + edge).min(h);
                (l0, r0, t0, b0, l1, r1, t1, b1)
            }
            // bottom strip
            1 => {
                let l0 = 0;
                let r0 = w;
                let t0 = h.saturating_sub(edge);
                let b0 = h;
                let l1 = l0;
                let r1 = r0;
                let t1 = t0.saturating_sub(edge);
                let b1 = b0;
                (l0, r0, t0, b0, l1, r1, t1, b1)
            }
            // left strip
            2 => {
                let l0 = 0;
                let r0 = edge.min(w);
                let t0 = 0;
                let b0 = h;
                let l1 = l0;
                let r1 = (r0 + edge).min(w);
                let t1 = t0;
                let b1 = b0;
                (l0, r0, t0, b0, l1, r1, t1, b1)
            }
            // right strip
            _ => {
                let l0 = w.saturating_sub(edge);
                let r0 = w;
                let t0 = 0;
                let b0 = h;
                let l1 = l0.saturating_sub(edge);
                let r1 = r0;
                let t1 = t0;
                let b1 = b0;
                (l0, r0, t0, b0, l1, r1, t1, b1)
            }
        };

        // Skip empty regions
        if l0e >= r0e || t0e >= b0e {
            continue;
        }

        for y in t0e..b0e {
            let mut curr = idx(w, l0e, y);
            for x in l0e..r0e {
                let _ = x; // just for symmetry with C; curr drives index
                if src_arr[curr] == 0 {
                    let mut found = false;
                    for &off in &op.offsets {
                        let k_signed = curr as isize + off;
                        if k_signed < 0 || k_signed >= wh as isize {
                            continue;
                        }
                        let k = k_signed as usize;
                        let kx = k % w;
                        let ky = k / w;

                        if kx < l1e || kx >= r1e || ky < t1e || ky >= b1e {
                            continue;
                        }
                        if src_arr[k] > 0 {
                            found = true;
                            break;
                        }
                    }
                    if found {
                        dst_arr[curr] = set_val;
                    } else {
                        dst_arr[curr] = 0;
                    }
                } else {
                    dst_arr[curr] = set_val;
                }
                curr += 1;
            }
        }
    }
}

fn im_dilate_win(src: &MaskIm, dst: &mut MaskIm, dia_pix: usize) {
    assert_eq!(src.w, dst.w);
    assert_eq!(src.h, dst.h);
    assert!(dia_pix > 0 && dia_pix <= src.w && dia_pix <= src.h);

    let op = WinDilationOp::new(dia_pix, src.w);
    im_dilate_win_with_op(src, dst, &op);
}

// -----------------------------------------------------------------------------
// EDT dilation (Felzenszwalb & Huttenlocher exact 1D squared EDT)
// -----------------------------------------------------------------------------
fn edt_1d(f_in: &[i32], d_out: &mut [i32], v: &mut [usize], z: &mut [i32]) {
    // f_in, d_out length == n
    let n = f_in.len();
    assert_eq!(d_out.len(), n);
    assert!(v.len() >= n);
    assert!(z.len() >= n + 1);

    let mut k: usize = 0;
    v[0] = 0;
    z[0] = i32::MIN;
    z[1] = i32::MAX;

    for q in 1..n {
        let mut s: i32;
        loop {
            let p = v[k];
            // s = ((f[q]+q^2)-(f[p]+p^2)) / (2*(q-p))
            let fq = f_in[q] + (q * q) as i32;
            let fp = f_in[p] + (p * p) as i32;
            let num = fq - fp;
            let den = (2 * (q - p)) as i32;
            s = num / den;

            if s <= z[k] {
                if k == 0 {
                    // cannot decrement further; break to avoid underflow
                    break;
                }
                k -= 1;
            } else {
                break;
            }
        }

        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = i32::MAX;
    }

    k = 0;
    for q in 0..n {
        while z[k + 1] < q as i32 {
            k += 1;
        }
        let dx = q as i32 - v[k] as i32;
        d_out[q] = f_in[v[k]] + dx * dx;
    }
}

fn im_dilate_edt(src: &MaskIm, dst: &mut MaskIm, dia_pix: usize) {
    assert_eq!(src.w, dst.w);
    assert_eq!(src.h, dst.h);
    assert!(dia_pix <= src.w && dia_pix <= src.h);

    let w = src.w;
    let h = src.h;
    let wh = w * h;

    let inf: i32 = i32::MAX / 4;

    let src_arr: &[u8] = &src.arr;
    let dst_arr: &mut [u8] = &mut dst.arr;

    // dt: 0 where src!=0 else inf
    let mut dt = vec![0i32; wh];
    for i in 0..wh {
        dt[i] = if src_arr[i] != 0 { 0 } else { inf };
    }

    let scratch_len = w.max(h);
    let mut scratch_in = vec![0i32; scratch_len];
    let mut scratch_out = vec![0i32; scratch_len];
    let mut v = vec![0usize; scratch_len];
    let mut z = vec![0i32; scratch_len + 1];

    // horizontal pass
    for y in 0..h {
        let row = &dt[y * w..y * w + w];
        scratch_in[..w].copy_from_slice(row);
        edt_1d(&scratch_in[..w], &mut scratch_out[..w], &mut v[..w], &mut z[..w + 1]);
        dt[y * w..y * w + w].copy_from_slice(&scratch_out[..w]);
    }

    // vertical pass
    for x in 0..w {
        for y in 0..h {
            scratch_in[y] = dt[y * w + x];
        }
        edt_1d(&scratch_in[..h], &mut scratch_out[..h], &mut v[..h], &mut z[..h + 1]);
        for y in 0..h {
            dt[y * w + x] = scratch_out[y];
        }
    }

    let radius = (dia_pix / 2) as i32;
    let radius_sq = radius * radius;

    for i in 0..wh {
        dst_arr[i] = if dt[i] <= radius_sq { 255 } else { 0 };
    }
}

// -----------------------------------------------------------------------------
// Tuned method selection (your crossover table + dia<2 copy)
// -----------------------------------------------------------------------------
pub fn im_dilate(src: &MaskIm, dst: &mut MaskIm, dia_pix: usize) {
    assert_eq!(src.w, dst.w);
    assert_eq!(src.h, dst.h);
    assert!(dia_pix <= src.w && dia_pix <= src.h);

    let w = src.w;
    let h = src.h;
    let wh = w * h;

    if dia_pix < 2 {
        // below 2, that's a No-op: memcpy
        dst.arr[..wh].copy_from_slice(&src.arr[..wh]);
        return;
    }

    // (dim, dia) rows, identical to your C table
    const CROSSOVER_TABLE: &[(usize, usize)] = &[
        (512, 10),
        (496, 12),
        (480, 12),
        (464, 12),
        (448, 12),
        (432, 12),
        (416, 13),
        (400, 14),
        (384, 16),
        (368, 14),
        (352, 14),
        (336, 16),
        (320, 24),
        (304, 27),
        (288, 27),
        (272, 28),
        (256, 51),
        (240, 45),
        (224, 43),
        (208, 40),
        (192, 41),
        (176, 38),
        (160, 36),
        (144, 36),
        (128, 34),
        (0,   34),
    ];

    let dim = w.max(h);
    let mut use_win_method = false;

    for &(d, dia) in CROSSOVER_TABLE {
        if dim >= d {
            use_win_method = dia_pix < dia;
            break;
        }
    }

    if use_win_method {
        im_dilate_win(src, dst, dia_pix);
    } else {
        im_dilate_edt(src, dst, dia_pix);
    }
}

#[cfg(test)]
mod tests {
    use super::im_dilate;
    use crate::im::MaskIm;

    #[test]
    fn dilate_win_disk_radius_1_and_overwrites_dst() {
        let w = 7;
        let h = 7;
        let mut src = MaskIm::new(w, h);
        src.arr[3 * w + 3] = 255;

        let mut dst = MaskIm::new(w, h);
        dst.arr.fill(123);

        im_dilate(&src, &mut dst, 3);

        for y in 0..h {
            for x in 0..w {
                let dx = x as i32 - 3;
                let dy = y as i32 - 3;
                let expected = if dx * dx + dy * dy <= 1 { 255 } else { 0 };
                assert_eq!(dst.arr[y * w + x], expected, "mismatch at ({x},{y})");
            }
        }
    }

    #[test]
    fn dilate_edt_path_matches_expected_thresholds() {
        let w = 50;
        let h = 50;
        let mut src = MaskIm::new(w, h);
        src.arr[0] = 255;

        let mut dst = MaskIm::new(w, h);
        dst.arr.fill(77);

        // dia_pix=40 => radius=20, and for dim=50 the crossover table selects EDT.
        im_dilate(&src, &mut dst, 40);

        let at = |x: usize, y: usize| dst.arr[y * w + x];
        assert_eq!(at(0, 0), 255);
        assert_eq!(at(20, 0), 255);
        assert_eq!(at(21, 0), 0);
        assert_eq!(at(14, 14), 255);
        assert_eq!(at(15, 15), 0);
        assert_eq!(at(49, 49), 0);
    }

    #[test]
    fn dilate_dia_lt_2_is_copy() {
        let w = 9;
        let h = 3;
        let mut src = MaskIm::new(w, h);
        for i in 0..(w * h) {
            src.arr[i] = if i % 3 == 0 { 255 } else { 0 };
        }

        let mut dst = MaskIm::new(w, h);
        dst.arr.fill(42);

        im_dilate(&src, &mut dst, 1);
        assert_eq!(src.arr, dst.arr);
    }
}
