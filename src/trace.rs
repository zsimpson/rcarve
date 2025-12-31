#[allow(dead_code)]

use std::collections::HashMap;

pub const CONTOUR_ID_MAX: i32 = i32::MAX;
pub const CONTOUR_ID_MIN: i32 = i32::MIN;

#[derive(Clone, Copy, Debug)]
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
}

/// Minimal image wrapper matching your style (`w,h,s,arr`).
/// Assumes 1-channel i32 pixels.
pub struct ImI32 {
    pub w: usize,
    pub h: usize,
    pub s: usize,     // stride in elements
    pub arr: Vec<i32> // length >= s*h
}

impl ImI32 {
    #[inline]
    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.s + x
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
pub fn contours_find_by_suzuki_abe_i32(im: &mut ImI32) -> Vec<Contour> {
    let w = im.w;
    let h = im.h;
    assert!(w >= 2 && h >= 2, "need at least a 1-pixel border");
    assert!(im.s >= w, "stride must be >= width");
    assert!(im.arr.len() >= im.s * h);

    let w1 = w - 1;
    let h1 = h - 1;

    // 8-neighborhood LUTs (same ordering as your C).
    const DIR_TO_DELT_CW: [(i32, i32); 8] = [
        ( 0,  1), // 0
        ( 1,  1), // 1
        ( 1,  0), // 2
        ( 1, -1), // 3
        ( 0, -1), // 4
        (-1, -1), // 5
        (-1,  0), // 6
        (-1,  1), // 7
    ];

    const DELT_PLUS_1_TO_DIR_CW: [i32; 9] = [
        // dy = -1, dx = -1,0,1
        5, 6, 7,
        // dy =  0, dx = -1,0,1 (0 impossible)
        4, -1, 0,
        // dy =  1, dx = -1,0,1
        3, 2, 1,
    ];

    const DIR_TO_DELT_CCW: [(i32, i32); 8] = [
        ( 0,  1), // 0
        (-1,  1), // 1
        (-1,  0), // 2
        (-1, -1), // 3
        ( 0, -1), // 4
        ( 1, -1), // 5
        ( 1,  0), // 6
        ( 1,  1), // 7
    ];

    const DELT_PLUS_1_TO_DIR_CCW: [i32; 9] = [
        // dy = -1
        3, 2, 1,
        // dy = 0
        4, -1, 0,
        // dy = 1
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
        let left = im.idx(0, y);
        let right = im.idx(w1, y);
        im.arr[left] = 0;
        im.arr[right] = 0;
    }
    for x in 0..w {
        let top = im.idx(x, 0);
        let bot = im.idx(x, h1);
        im.arr[top] = 0;
        im.arr[bot] = 0;
    }

    // Normalize interior to {0,1}
    for y in 1..h1 {
        for x in 1..w1 {
            let i = im.idx(x, y);
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

            let f0 = im.arr[im.idx(x0, y0)];
            // These are ((2)) in the paper.
            let mut y2: i32 = 0;
            let mut x2: i32 = 0;

            let mut is_hole = false;

            // (1a)
            if f0 == 1 && im.arr[im.idx(x0 - 1, y0)] == 0 {
                is_hole = false;
                curr_id += 1;
                y2 = y0 as i32;
                x2 = (x0 as i32) - 1;
            }
            // (1b)
            else if f0 >= 1 && im.arr[im.idx(x0 + 1, y0)] == 0 {
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
                debug_assert!((-1..=1).contains(&dy) && (-1..=1).contains(&dx) && !(dy == 0 && dx == 0));
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
                    if im.arr[im.idx(ux, uy)] != 0 {
                        y1 = ny;
                        x1 = nx;
                        d_found = Some(d);
                        break;
                    }
                }

                if d_found.is_none() {
                    // singleton pixel
                    im.arr[im.idx(x0, y0)] = -curr_id;
                    skip_to_4 = true;
                }

                if !skip_to_4 {
                    // (3.2) ((2))=((1)); ((3))=((0))
                    y2 = y1; x2 = x1;
                    let mut y3: i32 = y0 as i32;
                    let mut x3: i32 = x0 as i32;
                    let start = Iv2 { x: x3, y: y3 };

                    loop {
                        // record point ((3))
                        contours[new_index].points.push(Iv2 { x: x3, y: y3 });

                        // (3.3) counter-clockwise search for ((4)), starting from next element after ((2))
                        let dy = y2 - y3;
                        let dx = x2 - x3;
                        debug_assert!((-1..=1).contains(&dy) && (-1..=1).contains(&dx) && !(dy == 0 && dx == 0));
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
                            if im.arr[im.idx(ux, uy)] != 0 {
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
                        let idx3 = im.idx(ux3, uy3);

                        if east_was_examined {
                            let east = im.arr[im.idx((ux3 + 1), uy3)];
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
                        y2 = y3; x2 = x3;
                        y3 = y4; x3 = x4;
                    }

                    // Repeat the initial pixel
                    contours[new_index].points.push(start);
                }
            }

            // (4) update last_id
            if im.arr[im.idx(x0, y0)] != 1 {
                last_id = im.arr[im.idx(x0, y0)].abs();
            }
        }
    }

    contours
}

