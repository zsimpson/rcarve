use crate::region_tree::PlyIm;
use crate::desc::{BandDesc, Guid, PlyDesc, Thou};
use crate::im::core::Im;
use crate::im::ROI;
use crate::im::MaskIm;
use crate::toolpath::ToolPath;

pub fn ply_im_from_ascii(grid: &str) -> PlyIm {
    let rows: Vec<&str> = grid
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    let h = rows.len();
    assert!(h > 0, "grid must have at least one non-empty row");
    let w = rows[0].len();
    assert!(w > 0, "grid rows must be non-empty");
    for r in &rows {
        assert_eq!(r.len(), w, "all rows must have equal length");
    }

    let mut ply_im = PlyIm::new(w, h);
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let v = ch
                .to_digit(10)
                .unwrap_or_else(|| panic!("invalid label char '{ch}', expected digit"))
                as u16;
            ply_im.arr[y * ply_im.s + x] = v;
        }
    }
    ply_im
}

pub fn stub_ply_desc(guid: &str, top_thou: i32, hidden: bool) -> PlyDesc {
    PlyDesc {
        owner_layer_guid: Guid("layer0".to_string()),
        guid: Guid(guid.to_string()),
        top_thou: Thou(top_thou),
        hidden,
        is_floor: false,
        ply_mat: vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        mpoly: Vec::new(),
    }
}

pub fn stub_band_desc(top_thou: i32, bot_thou: i32, cut_pass: &str) -> BandDesc {
    BandDesc {
        top_thou: Thou(top_thou),
        bot_thou: Thou(bot_thou),
        cut_pass: cut_pass.to_string(),
    }
}

pub fn im_u16_to_ascii<S>(im: &Im<u16, 1, S>) -> String {
    let mut out = String::new();
    for y in 0..im.h {
        for x in 0..im.w {
            let v = im.arr[y * im.s + x];
            let ch = match v {
                0..=9 => (b'0' + (v as u8)) as char,
                10..=35 => (b'A' + ((v as u8) - 10)) as char,
                _ => '*',
            };
            out.push(ch);
        }
        out.push('\n');
    }
    out
}

pub fn mask_to_ascii(mask: &MaskIm, roi: Option<&ROI>) -> String {
    let mut out = String::new();

    let roi = roi.map(|r| {
        let l = r.l.min(mask.w);
        let t = r.t.min(mask.h);
        let r_ex = r.r.min(mask.w);
        let b_ex = r.b.min(mask.h);
        (l, t, r_ex, b_ex)
    });

    for y in 0..mask.h {
        for x in 0..mask.w {
            let v = mask.arr[y * mask.s + x];

            let is_roi_edge = if let Some((l, t, r_ex, b_ex)) = roi {
                if r_ex > l && b_ex > t {
                    let on_x_edge = x == l || x + 1 == r_ex;
                    let on_y_edge = y == t || y + 1 == b_ex;
                    x >= l && x < r_ex && y >= t && y < b_ex && (on_x_edge || on_y_edge)
                } else {
                    false
                }
            } else {
                false
            };

            out.push(if is_roi_edge { '*' } else if v > 0 { '#' } else { '.' });
        }
        out.push('\n');
    }
    out
}

pub fn toolpaths_to_ascii(paths: &[ToolPath], w: usize, h: usize) -> String {
    let mut grid: Vec<Vec<char>> = vec![vec!['.'; w]; h];

    let mut plot = |x: i32, y: i32, ch: char| {
        if x < 0 || y < 0 {
            return;
        }
        let x = x as usize;
        let y = y as usize;
        if y < h && x < w {
            grid[y][x] = ch;
        }
    };

    for (path_i, path) in paths.iter().enumerate() {
        let ch = (b'0' + ((path_i % 10) as u8)) as char;
        if path.points.is_empty() {
            continue;
        }

        // Draw each segment, inclusive of both endpoints.
        for seg_i in 0..path.points.len().saturating_sub(1) {
            let a = path.points[seg_i];
            let b = path.points[seg_i + 1];

            // Bresenham line rasterization.
            let mut x0 = a.x;
            let mut y0 = a.y;
            let x1 = b.x;
            let y1 = b.y;

            let dx = (x1 - x0).abs();
            let sx = if x0 < x1 { 1 } else { -1 };
            let dy = -(y1 - y0).abs();
            let sy = if y0 < y1 { 1 } else { -1 };
            let mut err = dx + dy;

            loop {
                plot(x0, y0, ch);
                if x0 == x1 && y0 == y1 {
                    break;
                }
                let e2 = 2 * err;
                if e2 >= dy {
                    err += dy;
                    x0 += sx;
                }
                if e2 <= dx {
                    err += dx;
                    y0 += sy;
                }
            }
        }

        // Single-point paths (or just to ensure last point is always plotted).
        if let Some(last) = path.points.last() {
            plot(last.x, last.y, ch);
        }
    }

    let mut out = String::new();
    for y in 0..h {
        for x in 0..w {
            out.push(grid[y][x]);
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolpath::IV3;

    #[test]
    fn toolpaths_to_ascii_renders_digits_by_index() {
        let paths = vec![
            ToolPath {
                points: vec![IV3 { x: 3, y: 1, z: 0 }, IV3 { x: 6, y: 1, z: 0 }],
                closed: false,
                tool_dia_pix: 5,
                tool_i: 0,
                tile_i: 0,
                tree_node_id: 0,
                cuts: vec![Default::default(); 2],
                is_traverse: false,
                is_raster: false,
            },
            ToolPath {
                points: vec![IV3 { x: 12, y: 1, z: 0 }, IV3 { x: 17, y: 1, z: 0 }],
                closed: false,
                tool_dia_pix: 5,
                tool_i: 0,
                tile_i: 0,
                tree_node_id: 0,
                cuts: vec![Default::default(); 2],
                is_traverse: false,
                is_raster: false,
            },
            ToolPath {
                points: vec![IV3 { x: 6, y: 2, z: 0 }, IV3 { x: 10, y: 2, z: 0 }],
                closed: false,
                tool_dia_pix: 5,
                tool_i: 0,
                tile_i: 0,
                tree_node_id: 0,
                cuts: vec![Default::default(); 2],
                is_traverse: false,
                is_raster: false,
            },
        ];

        let ascii = toolpaths_to_ascii(&paths, 20, 3);
        assert_eq!(
            ascii,
            "....................\n...0000.....111111..\n......22222.........\n"
        );
    }
}

