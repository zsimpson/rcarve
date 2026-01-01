use crate::im::Im;
use clipper2::{EndType, JoinType, One, Path, Paths, Point};

pub type IntPoint = Point<One>;
pub type IntPath = Path<One>;
pub type IntPaths = Paths<One>;

#[derive(Clone, Debug)]
pub struct MPoly {
    paths: IntPaths,
}

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

impl MPoly {
    pub fn new(paths: Vec<IntPath>) -> Self {
        Self {
            paths: IntPaths::new(paths),
        }
    }

    pub fn from_paths(paths: IntPaths) -> Self {
        Self { paths }
    }

    pub fn paths(&self) -> &IntPaths {
        &self.paths
    }

    pub fn into_paths(self) -> IntPaths {
        self.paths
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &IntPath> {
        self.paths.iter()
    }

    pub fn inflate(&self, delta: f64, join: JoinType, end: EndType, miter_limit: f64) -> Self {
        Self {
            paths: self.paths.inflate(delta, join, end, miter_limit),
        }
    }

    pub fn simplify(&self, epsilon: f64, preserve_collinear: bool) -> Self {
        Self {
            paths: self.paths.simplify(epsilon, preserve_collinear),
        }
    }

    pub fn raster<
        T: Copy + Default,
        const N_CH: usize,
        F: FnMut(&mut Im<T, N_CH>, i32, i32, i32),
    >(
        &self,
        im: &mut Im<T, N_CH>,
        mut callback: F,
    ) {
        let rings: Vec<Vec<[i32; 2]>> = self.paths.iter().map(coords_from_path).collect();
        fill_poly_v2i_n(
            0,
            0,
            im.w as i32,
            im.h as i32,
            &rings,
            &mut |x_start: i32, x_end: i32, y: i32| callback(im, x_start, x_end, y),
        );
    }

    pub fn raster_edges<
        T: Copy + Default,
        const N_CH: usize,
        F: FnMut(&mut Im<T, N_CH>, i32, i32),
    >(
        &self,
        im: &mut Im<T, N_CH>,
        mut callback: F,
    ) {
        for path in self.paths.iter() {
            let coords = coords_from_path(path);
            let n = coords.len();
            if n < 2 {
                continue;
            }

            for i in 0..n {
                let p0 = coords[i];
                let p1 = coords[(i + 1) % n];

                // Bresenham's line algorithm
                let dx = (p1[0] - p0[0]).abs();
                let dy = -(p1[1] - p0[1]).abs();
                let sx = if p0[0] < p1[0] { 1 } else { -1 };
                let sy = if p0[1] < p1[1] { 1 } else { -1 };
                let mut err = dx + dy;
                let mut x = p0[0];
                let mut y = p0[1];

                loop {
                    if x >= 0 && x < im.w as i32 && y >= 0 && y < im.h as i32 {
                        callback(im, x, y);
                    }
                    if x == p1[0] && y == p1[1] {
                        break;
                    }
                    let e2 = 2 * err;
                    if e2 >= dy {
                        err += dy;
                        x += sx;
                    }
                    if e2 <= dx {
                        err += dx;
                        y += sy;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;

    use image::RgbaImage;

    use crate::im::RGBAIm;

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

    fn circle_coords(n: usize, cx: f64, cy: f64, r: f64) -> Vec<[i32; 2]> {
        assert!(n >= 3, "circle needs at least 3 vertices");
        assert!(r.is_finite() && r > 0.0, "radius must be finite and > 0");
        assert!(cx.is_finite() && cy.is_finite(), "center must be finite");

        let step = TAU / (n as f64);
        (0..n)
            .map(|i| {
                let a = (i as f64) * step;
                let x = cx + r * a.cos();
                let y = cy + r * a.sin();
                [x.round() as i32, y.round() as i32]
            })
            .collect()
    }

    fn save_rgba_im(im: &RGBAIm, out_path: &str) {
        let w = im.w;
        let h = im.h;
        let img = RgbaImage::from_raw(w as u32, h as u32, im.arr.clone())
            .expect("invalid RGBA image buffer");
        img.save(out_path)
            .unwrap_or_else(|e| panic!("failed to save {out_path}: {e}"));
    }

    fn fixture_with_holes(dx: i32, dy: i32) -> MPoly {
        let shift = |coords: Vec<[i32; 2]>| -> Vec<[i32; 2]> {
            coords.into_iter().map(|[x, y]| [x + dx, y + dy]).collect()
        };

        // Outer square, plus 2 inset holes.
        let outer: IntPath = ipath(shift(vec![[10, 10], [90, 10], [90, 90], [10, 90]]));
        let hole0: IntPath = ipath(shift(vec![[20, 20], [20, 40], [40, 40], [40, 20]]));
        let hole1: IntPath = ipath(shift(vec![[45, 45], [50, 80], [80, 80], [80, 50]]));
        MPoly::new(vec![outer, hole0, hole1])
    }

    fn raster_red(mpoly: &MPoly, im: &mut RGBAIm) {
        mpoly.raster(im, |im, x_start, x_end, y| {
            for x in x_start..x_end {
                unsafe {
                    *im.get_unchecked_mut(x as usize, y as usize, 0) = 255;
                    *im.get_unchecked_mut(x as usize, y as usize, 1) = 0;
                    *im.get_unchecked_mut(x as usize, y as usize, 2) = 0;
                    *im.get_unchecked_mut(x as usize, y as usize, 3) = 255;
                }
            }
        });
    }

    #[test]
    fn erode_no_hole() {
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
        let mut im = RGBAIm::new(150, 150);

        // Plot onto the im with red
        {
            mpoly.raster(&mut im, |im, x_start, x_end, y| {
                for x in x_start..x_end {
                    unsafe {
                        *im.get_unchecked_mut(x as usize, y as usize, 0) = 255;
                        *im.get_unchecked_mut(x as usize, y as usize, 1) = 0;
                        *im.get_unchecked_mut(x as usize, y as usize, 2) = 0;
                        *im.get_unchecked_mut(x as usize, y as usize, 3) = 255;
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
            dilated.raster(&mut im, |im, x_start, x_end, y| {
                for x in x_start..x_end {
                    unsafe {
                        *im.get_unchecked_mut(x as usize, y as usize, 1) = 255;
                    }
                }
            });
        }

        save_rgba_im(&im, "./test_data/_erode_no_hole.png");
    }

    #[test]
    fn erode_with_hole() {
        let mpoly: MPoly = fixture_with_holes(0, 0);

        // Allocate RGBA 8-bit im
        let mut im = RGBAIm::new(120, 120);

        // Plot onto the im with red
        {
            raster_red(&mpoly, &mut im);
        }

        // Dilation (positive delta). With the hole being CW, it should shrink.
        let dilated = mpoly
            .inflate(-6.0, JoinType::Square, EndType::Polygon, 2.0)
            .simplify(0.001, false);

        // Plot dilated polygon onto the im by setting only the green channel.
        {
            dilated.raster(&mut im, |im, x_start, x_end, y| {
                for x in x_start..x_end {
                    unsafe {
                        *im.get_unchecked_mut(x as usize, y as usize, 1) = 255;
                    }
                }
            });
        }

        save_rgba_im(&im, "./test_data/_erode_with_hole.png");
    }

    #[test]
    fn raster_clips_when_geometry_outside_image() {
        // Offset geometry so it extends beyond all four edges of the smaller image.
        // This should not panic, and the in-bounds pixels should match a clipped
        // region of a larger render.
        let mpoly: MPoly = fixture_with_holes(-15, -15);

        let mut big_rgba_im = RGBAIm::new(120, 120);
        let mut small_rgba_im = RGBAIm::new(60, 60);

        raster_red(&mpoly, &mut big_rgba_im);
        raster_red(&mpoly, &mut small_rgba_im);

        big_rgba_im
            .save_png("./test_data/_raster_clips_big.png")
            .unwrap();
        small_rgba_im
            .save_png("./test_data/_raster_clips_small.png")
            .unwrap();

        // Ensure the test is meaningful (we actually rasterized something in the small image).
        let any_red = small_rgba_im.arr.iter().step_by(4).any(|&r| r != 0);
        assert!(any_red, "expected some in-bounds pixels to be filled");

        // Compare small image against the top-left crop of the big image.
        for y in 0..small_rgba_im.h {
            for x in 0..small_rgba_im.w {
                let small_idx = y * small_rgba_im.s + x * 4;
                let big_idx = y * big_rgba_im.s + x * 4;
                assert_eq!(
                    &small_rgba_im.arr[small_idx..small_idx + 4],
                    &big_rgba_im.arr[big_idx..big_idx + 4],
                    "pixel mismatch at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn erode_small_hole() {
        let verts = circle_coords(16, 20.0, 20.0, 10.0);
        let mpoly: MPoly = MPoly::new(vec![ipath(verts)]);

        // Allocate RGBA 8-bit im
        let mut im = RGBAIm::new(120, 120);
        mpoly.raster_edges(&mut im, |im, x, y| unsafe {
            *im.get_unchecked_mut(x as usize, y as usize, 0) = 255;
            *im.get_unchecked_mut(x as usize, y as usize, 1) = 0;
            *im.get_unchecked_mut(x as usize, y as usize, 2) = 0;
            *im.get_unchecked_mut(x as usize, y as usize, 3) = 255;
        });

        let eroded = mpoly
            .inflate(-6.0, JoinType::Round, EndType::Polygon, 2.0)
            .simplify(0.001, false);

        assert!(!eroded.is_empty());

        // Test that eroding more than the radius removes the polygon.
        let eroded = mpoly
            .inflate(-12.0, JoinType::Round, EndType::Polygon, 2.0)
            .simplify(0.001, false);

        assert!(eroded.is_empty());
    }
}
