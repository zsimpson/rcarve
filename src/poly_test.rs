use clipper2::{EndType, JoinType, One, Path, Paths, Point};
type IntPoint = Point<One>;
type IntPath = Path<One>;
type IntPaths = Paths<One>;

// - callback: Takes the x, y coords and x-span (x_end is not inclusive),
//   note that `x_end` will always be greater than `x`.
fn fill_poly_v2i_n<F: FnMut(i32, i32, i32)>(
    xmin: i32,
    ymin: i32,
    xmax: i32,
    ymax: i32,
    coords: &Vec<[i32; 2]>,
    callback: &mut F,
) {
    /* Originally by Darel Rex Finley, 2007.
     * Optimized by Campbell Barton, 2016 to keep sorted intersections. */

    /*
     * Note: all the index lookups here could be made unsafe
     * (as in, we know they won't fail).
     */

    // only because we use this with int values frequently, avoids casting every time.
    let coords_len: i32 = coords.len() as i32;

    let mut span_y: Vec<[i32; 2]> = Vec::with_capacity(coords.len());

    {
        let mut i_prev: i32 = coords_len - 1;
        let mut i_curr: i32 = 0;
        let mut co_prev = &coords[i_prev as usize];
        for co_curr in coords {
            if co_prev[1] != co_curr[1] {
                // Any segments entirely above or below the area of interest can be skipped.
                if (std::cmp::min(co_prev[1], co_curr[1]) >= ymax)
                    || (std::cmp::max(co_prev[1], co_curr[1]) < ymin)
                {
                    continue;
                }

                span_y.push(if co_prev[1] < co_curr[1] {
                    [i_prev, i_curr]
                } else {
                    [i_curr, i_prev]
                });
            }
            i_prev = i_curr;
            i_curr += 1;
            co_prev = co_curr;
        }
    }

    // sort edge-segments on y, then x axis
    span_y.sort_by(|a, b| {
        let co_a = &coords[a[0] as usize];
        let co_b = &coords[b[0] as usize];
        let mut ord = co_a[1].cmp(&co_b[1]);
        if ord == std::cmp::Ordering::Equal {
            ord = co_a[0].cmp(&co_b[0]);
        }
        if ord == std::cmp::Ordering::Equal {
            // co_a & co_b are identical, use the line closest to the x-min
            let co = co_a; // could be co_b too.
            let co_a = &coords[a[1] as usize];
            let co_b = &coords[b[1] as usize];
            ord = 0.cmp(
                &(((co_b[0] - co[0]) * (co_a[1] - co[1]))
                    - ((co_a[0] - co[0]) * (co_b[1] - co[1]))),
            );
        }
        ord
    });

    // Used to store x intersections for the current y axis ('pixel_y')
    struct NodeX {
        span_y_index: usize,
        // 'x' pixel value for the current 'pixel_y'.
        x: i32,
    }
    let mut node_x: Vec<NodeX> = Vec::with_capacity(coords.len() + 1);
    let mut span_y_index: usize = 0;

    if span_y.len() != 0 && coords[span_y[0][0] as usize][1] < ymin {
        while (span_y_index < span_y.len()) && (coords[span_y[span_y_index][0] as usize][1] < ymin)
        {
            assert!(
                coords[span_y[span_y_index][0] as usize][1] < coords[span_y[span_y_index][1] as usize][1]
            );
            if coords[span_y[span_y_index][1] as usize][1] >= ymin {
                node_x.push(NodeX {
                    span_y_index: span_y_index,
                    x: -1,
                });
            }
            span_y_index += 1;
        }
    }

    // Loop through the rows of the image.
    for pixel_y in ymin..ymax {
        let mut is_sorted = true;
        let mut do_remove = false;
        {
            let mut x_ix_prev = i32::min_value();
            for n in &mut node_x {
                let s = &span_y[n.span_y_index];
                let co_prev = &coords[s[0] as usize];
                let co_curr = &coords[s[1] as usize];

                assert!(co_prev[1] < pixel_y && co_curr[1] >= pixel_y);
                let x = (co_prev[0] - co_curr[0]) as f64;
                let y = (co_prev[1] - co_curr[1]) as f64;
                let y_px = (pixel_y - co_curr[1]) as f64;
                let x_ix = ((co_curr[0] as f64) + ((y_px / y) * x)).round() as i32;
                n.x = x_ix;

                if is_sorted && (x_ix_prev > x_ix) {
                    is_sorted = false;
                }
                if do_remove == false && co_curr[1] == pixel_y {
                    do_remove = true;
                }
                x_ix_prev = x_ix;
            }
        }
        // Theres no reason this will ever be larger
        assert!(node_x.len() <= coords.len() + 1);

        // Sort the nodes, via a simple "bubble" sort.
        if is_sorted == false {
            let node_x_end = node_x.len() - 1;
            let mut i: usize = 0;
            while i < node_x_end {
                if node_x[i].x > node_x[i + 1].x {
                    node_x.swap(i, i + 1);
                    if i != 0 {
                        i -= 1;
                    }
                } else {
                    i += 1;
                }
            }
        }

        // Fill the pixels between node pairs.
        {
            // TODO, use `node_x.step_by(2)`. When its in stable
            let mut i = 0;
            while i < node_x.len() {
                let mut x_src = node_x[i].x;
                let mut x_dst = node_x[i + 1].x;

                if x_src >= xmax {
                    break;
                }

                if x_dst > xmin {
                    if x_src < xmin {
                        x_src = xmin;
                    }
                    if x_dst > xmax {
                        x_dst = xmax;
                    }

                    // Single call per x-span.
                    if x_src < x_dst {
                        callback(x_src - xmin, x_dst - xmin, pixel_y - ymin);
                    }
                }
                i += 2;
            }
        }

        // Clear finalized nodes in one pass, only when needed
        // (avoids excessive array-resizing).
        if do_remove {
            let mut i_dst: usize = 0;
            for i_src in 0..node_x.len() {
                let s = &span_y[node_x[i_src].span_y_index];
                let co = &coords[s[1] as usize];
                if co[1] != pixel_y {
                    if i_dst != i_src {
                        // x is initialized for the next pixel_y (no need to adjust here)
                        node_x[i_dst].span_y_index = node_x[i_src].span_y_index;
                    }
                    i_dst += 1;
                }
            }
            node_x.truncate(i_dst);
        }

        // Scan for new events
        {
            while span_y_index < span_y.len()
                && coords[span_y[span_y_index][0] as usize][1] == pixel_y
            {
                // Note, node_x these are just added at the end,
                // not ideal but sorting once will resolve.

                // x is initialized for the next pixel_y
                node_x.push(NodeX {
                    span_y_index: span_y_index,
                    x: -1,
                });
                span_y_index += 1;
            }
        }
    }
}


/// Create a 2D multipolygon represented as two paths:
/// - an outer square perimeter
/// - an inner square hole (opposite winding)
pub fn make_square_with_hole() -> IntPaths {
    fn p(x: i64, y: i64) -> IntPoint {
        IntPoint::from_scaled(x, y)
    }

    // Outer: counter-clockwise winding (positive signed area when Y increases upward).
    #[rustfmt::skip]
    let outer: IntPath = IntPath::new(vec![
        p(0, 0),
        p(10, 0),
        p(10, 10),
        p(0, 10),
    ]);

    // Hole: clockwise winding (negative signed area), so a positive dilation shrinks the hole.
    #[rustfmt::skip]
    let hole: IntPath = IntPath::new(vec![
        p(3, 3),
        p(3, 7),
        p(7, 7),
        p(7, 3),
    ]);

    IntPaths::new(vec![outer, hole])
}

// #[allow(dead_code)]
// pub type Bitmap<T> = crate::bitmap::Bitmap<T>;

#[cfg(test)]
mod tests {
    use std::cmp::{max, min};

    use crate::bitmap::Bitmap;
    use image::RgbaImage;

    use super::*;

    fn p(x: i64, y: i64) -> IntPoint {
        IntPoint::from_scaled(x, y)
    }

    fn coords_from_path(path: &IntPath) -> Vec<[i32; 2]> {
        path.iter()
            .map(|pt| [pt.x_scaled() as i32, pt.y_scaled() as i32])
            .collect()
    }

    #[test]
    fn draw_poly_from_path() {
        let coords: Vec<[i32; 2]> = vec![
            [10, 10],
            [30, 10],
            [30, 60],
            [60, 60],
            [60, 50],
            [80, 50],
            [60, 100],
            [10, 100],
        ];

        // Allocate RGBA 8-bit bitmap 150x150 pixels
        let w: usize = 150;
        let h: usize = 150;
        let mut bitmap = Bitmap::<[u8; 4]>::new(w, h);

        // Plot onto the bitmap with red
        {
            fill_poly_v2i_n(
                0, 0, w as i32, h as i32, &coords,
                &mut |x_start: i32, x_end: i32, y: i32| {
                    let x_start = max(0, x_start);
                    let x_end = min(w as i32, x_end);
                    let y = max(0, min(h as i32 - 1, y));
                    for x in x_start..x_end {
                        unsafe {
                            *bitmap.get_unchecked_mut(x as usize, y as usize) = [255, 0, 0, 255];
                        }
                    }
                },
            );

        }

        // Make a Paths from the coords above
        let path: IntPath = IntPath::new(
            coords
                .iter()
                .map(|c| p(c[0] as i64, c[1] as i64))
                .collect(),
        );

        let mpoly: IntPaths = IntPaths::new(vec![path]);

        // Dilation (positive delta). With the hole being CW, it should shrink.
        let dilated = mpoly
            .inflate(-3.0, JoinType::Square, EndType::Polygon, 2.0)
            .simplify(0.001, false);

        assert!(!dilated.is_empty(), "expected dilated polygon(s)");

        // Convert the dilated result back to rasterizer coords (use the first path).
        // If you want to rasterize multiple disjoint paths, loop over `0..dilated.len()`.
        let dilated_coords: Vec<[i32; 2]> = coords_from_path(
            dilated
                .first()
                .expect("dilated unexpectedly had no first path"),
        );

        // Print the dilated coords for debugging
        // println!("Dilated polygon coords:");
        // for c in &dilated_coords {
        //     println!("  [{}, {}],", c[0], c[1]);
        // }

        // Plot dilated polygon onto the bitmap by setting only the green channel.
        {
            fill_poly_v2i_n(
                0, 0, w as i32, h as i32, &dilated_coords,
                &mut |x_start: i32, x_end: i32, y: i32| {
                    let x_start = max(0, x_start);
                    let x_end = min(w as i32, x_end);
                    let y = max(0, min(h as i32 - 1, y));
                    println!("  p[{}, {}, {}],", x_start, x_end, y);
                    for x in x_start..x_end {
                        unsafe {
                            (*bitmap.get_unchecked_mut(x as usize, y as usize))[1] = 255;
                        }
                    }
                },
            );
        }

        // Save the bitmap to a file for visual inspection
        let out_path = "./test_data/_poly_draw.png";
        let mut raw: Vec<u8> = Vec::with_capacity(w * h * 4);
        for px in &bitmap.arr {
            raw.extend_from_slice(px);
        }

        let img =
            RgbaImage::from_raw(w as u32, h as u32, raw).expect("invalid RGBA image buffer");
        img.save(out_path)
            .unwrap_or_else(|e| panic!("failed to save {out_path}: {e}"));

    }

    #[test]
    fn dilate_square_with_hole() {
        let multi_poly = make_square_with_hole();
        assert_eq!(multi_poly.len(), 2);

        let outer = multi_poly.get(0).expect("missing outer path");
        let hole = multi_poly.get(1).expect("missing hole path");
        assert!(outer.signed_area() > 0.0, "outer should be CCW");
        assert!(hole.signed_area() < 0.0, "hole should be CW");

        // Dilation (positive delta). With the hole being CW, it should shrink.
        let dilated = multi_poly
            .inflate(1.0, JoinType::Square, EndType::Polygon, 2.0)
            .simplify(0.001, false);

        assert!(!dilated.is_empty());
        assert!(dilated.contains_points());
    }

    #[test]
    fn erode_outer_and_expand_hole_by_2px() {
        // Outer: 20x20, hole: 8x8 centered, so erosion by 2px remains non-empty.
        let outer: IntPath = IntPath::new(vec![p(0, 0), p(20, 0), p(20, 20), p(0, 20)]);
        let hole: IntPath = IntPath::new(vec![p(3, 3), p(3, 14), p(14, 14), p(14, 3)]);
        let poly: IntPaths = IntPaths::new(vec![outer, hole]);

        // Negative delta = erosion:
        // - outer boundary moves inward
        // - hole boundary moves outward
        let out = poly
            .inflate(-2.0, JoinType::Square, EndType::Polygon, 2.0)
            .simplify(0.001, false);

        assert_eq!(out.len(), 1, "expected 1 polygon after erosion");

        // TODO
        // Rasterize original (red) + eroded (green, additive) so overlap is yellow.
    }
}
