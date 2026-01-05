use crate::cut_stack::PlyIm;
use crate::desc::{BandDesc, Guid, PlyDesc, Thou};
use crate::im::core::Im;
use crate::im::label::ROI;
use crate::im::MaskIm;

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
