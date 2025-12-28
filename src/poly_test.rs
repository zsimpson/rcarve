use crate::im::Im;
use clipper2::{EndType, JoinType, One, Path, Paths, Point};
type IntPoint = Point<One>;
type IntPath = Path<One>;
type IntPaths = Paths<One>;
type MPoly = IntPaths;

// - callback: Takes the x, y coords and x-span (x_end is not inclusive),
//   note that `x_end` will always be greater than `x`.
fn fill_poly_v2i_n<F: FnMut(i32, i32, i32)>(
    xmin: i32,
    ymin: i32,
    xmax: i32,
    ymax: i32,
    rings: &[Vec<[i32; 2]>],
    callback: &mut F,
) {
    // Even-odd scanline fill across multiple rings.
    // This supports holes naturally: include the outer ring + the hole ring(s).
    // Note: this implementation is intentionally simple (not incremental).
    let mut x_intersections: Vec<i32> = Vec::new();

    for pixel_y in ymin..ymax {
        x_intersections.clear();

        for ring in rings {
            if ring.len() < 2 {
                continue;
            }

            let last = ring[ring.len() - 1];
            let mut x0 = last[0];
            let mut y0 = last[1];

            for &pt in ring {
                let x1 = pt[0];
                let y1 = pt[1];

                if y0 != y1 {
                    let y_min = std::cmp::min(y0, y1);
                    let y_max = std::cmp::max(y0, y1);

                    // Half-open range to avoid double-counting shared vertices.
                    if (pixel_y >= y_min) && (pixel_y < y_max) {
                        // Skip segments entirely outside vertical bounds of interest.
                        if (y_max >= ymin) && (y_min < ymax) {
                            let dy = (y1 - y0) as f64;
                            let t = ((pixel_y - y0) as f64) / dy;
                            let x = (x0 as f64) + (t * ((x1 - x0) as f64));
                            x_intersections.push(x.round() as i32);
                        }
                    }
                }

                x0 = x1;
                y0 = y1;
            }
        }

        if x_intersections.len() < 2 {
            continue;
        }

        x_intersections.sort_unstable();

        for pair in x_intersections.chunks_exact(2) {
            let mut x_src = pair[0];
            let mut x_dst = pair[1];

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
                if x_src < x_dst {
                    callback(x_src - xmin, x_dst - xmin, pixel_y - ymin);
                }
            }
        }
    }
}

fn coords_from_path(path: &IntPath) -> Vec<[i32; 2]> {
    path.iter()
        .map(|pt| [pt.x_scaled() as i32, pt.y_scaled() as i32])
        .collect()
}

pub fn raster_int_paths<T: Copy + Default, F: FnMut(&mut Im<T>, i32, i32, i32)>(
    im: &mut Im<T>,
    int_paths: &MPoly,
    mut callback: F,
) {
    let rings: Vec<Vec<[i32; 2]>> = int_paths.iter().map(coords_from_path).collect();
    fill_poly_v2i_n(
        0,
        0,
        im.w as i32,
        im.h as i32,
        &rings,
        &mut |x_start: i32, x_end: i32, y: i32| callback(im, x_start, x_end, y),
    );
}

#[cfg(test)]
mod tests {
    use image::RgbaImage;

    use super::*;

    fn p(x: i64, y: i64) -> IntPoint {
        IntPoint::from_scaled(x, y)
    }

    fn ipath(coords: Vec<[i32; 2]>) -> IntPath {
        IntPath::new(
            coords
                .into_iter()
                .map(|c| p(c[0] as i64, c[1] as i64))
                .collect(),
        )
    }

    fn save_rgba_im(im: &Im<[u8; 4]>, out_path: &str) {
        let w = im.w;
        let h = im.h;

        let mut raw: Vec<u8> = Vec::with_capacity(w * h * 4);
        for px in &im.arr {
            raw.extend_from_slice(px);
        }

        let img = RgbaImage::from_raw(w as u32, h as u32, raw).expect("invalid RGBA image buffer");
        img.save(out_path)
            .unwrap_or_else(|e| panic!("failed to save {out_path}: {e}"));
    }

    #[test]
    fn erode_no_hole() {
        // Make IntPaths from the coords above.
        let path: IntPath = ipath(vec![
            [10, 10],
            [30, 10],
            [30, 60],
            [60, 60],
            [60, 50],
            [80, 50],
            [60, 100],
            [10, 100],
        ]);

        let mpoly: MPoly = MPoly::new(vec![path]);

        // Allocate RGBA 8-bit im 150x150 pixels
        let mut im = Im::<[u8; 4]>::new(150, 150);

        // Plot onto the im with red
        {
            raster_int_paths(&mut im, &mpoly, |im, x_start, x_end, y| {
                for x in x_start..x_end {
                    unsafe {
                        *im.get_unchecked_mut(x as usize, y as usize) = [255, 0, 0, 255];
                    }
                }
            });
        }

        // Dilation (positive delta). With the hole being CW, it should shrink.
        let dilated = mpoly
            .inflate(-3.0, JoinType::Square, EndType::Polygon, 2.0)
            .simplify(0.001, false);

        assert!(!dilated.is_empty(), "expected dilated polygon(s)");

        // Plot dilated polygon onto the im by setting only the green channel.
        {
            raster_int_paths(&mut im, &dilated, |im, x_start, x_end, y| {
                // println!("  p[{}, {}, {}],", x_start, x_end, y);
                for x in x_start..x_end {
                    unsafe {
                        (*im.get_unchecked_mut(x as usize, y as usize))[1] = 255;
                    }
                }
            });
        }

        save_rgba_im(&im, "./test_data/_erode_no_hole.png");
    }

    #[test]
    fn erode_with_hole() {
        // Outer square, plus an inset square hole 10 units smaller on each side.
        // Outer uses CCW winding, hole uses CW winding.
        let outer: IntPath = ipath(vec![[10, 10], [90, 10], [90, 90], [10, 90]]);
        let hole0: IntPath = ipath(vec![[20, 20], [20, 40], [40, 40], [40, 20]]);
        let hole1: IntPath = ipath(vec![[45, 45], [50, 80], [80, 80], [80, 50]]);
        let mpoly: MPoly = MPoly::new(vec![outer, hole0, hole1]);

        // Allocate RGBA 8-bit im
        let mut im = Im::<[u8; 4]>::new(120, 120);

        // Plot onto the im with red
        {
            raster_int_paths(&mut im, &mpoly, |im, x_start, x_end, y| {
                for x in x_start..x_end {
                    unsafe {
                        *im.get_unchecked_mut(x as usize, y as usize) = [255, 0, 0, 255];
                    }
                }
            });
        }

        // Dilation (positive delta). With the hole being CW, it should shrink.
        let dilated = mpoly
            .inflate(-6.0, JoinType::Square, EndType::Polygon, 2.0)
            .simplify(0.001, false);

        // Plot dilated polygon onto the im by setting only the green channel.
        {
            raster_int_paths(&mut im, &dilated, |im, x_start, x_end, y| {
                // println!("  p[{}, {}, {}],", x_start, x_end, y);
                for x in x_start..x_end {
                    unsafe {
                        (*im.get_unchecked_mut(x as usize, y as usize))[1] = 255;
                    }
                }
            });
        }

        save_rgba_im(&im, "./test_data/_erode_with_hole.png");
    }
}
