use rcarve::desc::{PlyDesc, PolyDesc, parse_comp_json};
use rcarve::im::Im;
use rcarve::im::label::{LabelInfo, label_im};
use rcarve::mat3::Mat3;
use rcarve::mpoly::{IntPath, IntPoint, MPoly};

const TEST_JSON: &str = r#"
    {
        "version": 3,
        "guid": "JGYYJQBHTX",
        "dim_desc": {
            "bulk_d_inch": 0.75,
            "bulk_w_inch": 4,
            "bulk_h_inch": 4,
            "padding_inch": 0,
            "frame_inch": 0.5
        },
        "ply_desc_by_guid": {
            "HZWKZRTQJV": {
                "owner_layer_guid": "R7Y9XP4VNB",
                "guid": "HZWKZRTQJV",
                "top_thou": 950,
                "hidden": false,
                "is_floor": false,
                "mpoly": [
                    {
                        "exterior": [0,0, 100,0, 100,100, 0,100],
                        "holes": [
                            [20,20, 80,20, 80,80, 20,80]
                        ]
                    }
                ]
            },
            "ZWKKED69NS": {
                "owner_layer_guid": "R7Y9XP4VNB",
                "guid": "ZWKKED69NS",
                "top_thou": 850,
                "hidden": false,
                "is_floor": false,
                "mpoly": [
                    {
                        "exterior": [25,40, 35,40, 35,60, 25,60],
                        "holes": []
                    },
                    {
                        "exterior": [50,40, 55,40, 55,60, 50,60],
                        "holes": []
                    }
                ]
            }
        },
        "layer_desc_by_guid": {
            "R7Y9XP4VNB": {
                "guid": "R7Y9XP4VNB",
                "hidden": false,
                "is_frame": false
            }
        },
        "carve_desc": {
            "grain_y": true,
            "rough_tool_guid": "EBES3PGSC3",
            "refine_tool_guid": "W5C7NZWAK4",
            "detail_tool_guid": null
        },
        "bands": [
            {
                "top_thou": 1000,
                "bot_thou": 900,
                "which": "refine"
            },
            {
                "top_thou": 900,
                "bot_thou": 800,
                "which": "refine"
            },
            {
                "top_thou": 1000,
                "bot_thou": 200,
                "which": "rough"
            }
        ]
    }
"#;

type LabelVal = u16;
type SepVal = u16;
type SepIm = Im<SepVal, 1>;

pub fn polydesc_to_mpoly(polydesc: &PolyDesc, ply_xform: &Mat3) -> MPoly {
    fn flat_verts_to_path(flat: &[i32], ply_xform: &Mat3) -> Option<IntPath> {
        // flat = [x0, y0, x1, y1, ...]
        // Need at least 3 points (6 ints).
        if flat.len() < 6 {
            return None;
        }

        let n = flat.len() - (flat.len() % 2);
        if n < 6 {
            return None;
        }

        let mut pts: Vec<IntPoint> = Vec::with_capacity(n / 2);
        for xy in flat[..n].chunks_exact(2) {
            let (x, y) = ply_xform.transform_point2(xy[0] as f64, xy[1] as f64);
            pts.push(IntPoint::from_scaled(x.round() as i64, y.round() as i64));
        }

        Some(IntPath::new(pts))
    }

    let mut paths: Vec<IntPath> = Vec::with_capacity(1 + polydesc.holes.len());
    if let Some(ext) = flat_verts_to_path(&polydesc.exterior, ply_xform) {
        paths.push(ext);
    }
    for hole in &polydesc.holes {
        if let Some(h) = flat_verts_to_path(hole, ply_xform) {
            paths.push(h);
        }
    }

    MPoly::new(paths)
}

fn main() {
    let roi_l = 0_usize;
    let roi_t = 0_usize;
    let roi_r = 100_usize;
    let roi_b = 100_usize;
    let w = (roi_r - roi_l) as usize;
    let h = (roi_b - roi_t) as usize;

    let mut sep_im = SepIm::new(w, h);

    let comp_desc = parse_comp_json(TEST_JSON).expect("Failed to parse comp JSON");
    // println!("Parsed CompDesc: {:?}", comp_desc);

    // Iter the ply_descs, Keep the plies that are not hidden
    // (including heckig if their owner layer_desc is not hidden)
    // and then sort by top_thou descending.
    let mut sorted_ply_decsc: Vec<&PlyDesc> = comp_desc
        .ply_desc_by_guid
        .values()
        .filter(|ply_desc| {
            if ply_desc.hidden {
                return false;
            }
            if let Some(layer) = comp_desc.layer_desc_by_guid.get(&ply_desc.owner_layer_guid) {
                return !layer.hidden;
            }
            true
        })
        .collect();

    sorted_ply_decsc.sort_by(|a, b| b.top_thou.cmp(&a.top_thou));

    // From bottom to top, raster each ply into the Im using its i as the value.
    // Each ply contains a Vec<PolyDesc>; raster each polygon into the same label.
    // Note that .rev() is last so the i corresponds to the original index.
    for (i, ply_desc) in sorted_ply_decsc.iter().enumerate().rev() {
        let value = (i as u16) + 1; // start from 1

        let ply_xform = Mat3::from_ply_mat(&ply_desc.ply_mat)
            .unwrap_or_default()
            .then_translate(-(roi_l as f64), -(roi_t as f64));

        for polydesc in &ply_desc.mpoly {
            let mpoly = polydesc_to_mpoly(polydesc, &ply_xform);
            if mpoly.is_empty() {
                continue;
            }

            mpoly.raster(&mut sep_im, |sep_im, x_start, x_end, y| {
                for x in x_start..x_end {
                    unsafe {
                        *sep_im.get_unchecked_mut(x as usize, y as usize, 0) = value;
                    }
                }
            });
        }
    }

    let (mut label_im, _label_infos): (Im<LabelVal, 1>, Vec<LabelInfo>) = label_im(&sep_im);

    label_im
        .pixels(|v, _i| {
            *v = ((*v as u32) * 20000).min(65535) as u16;
        })
        .save_png("_label_im.png")
        .expect("Failed to save PNG");

    sep_im
        .pixels(|v, _i| {
            *v = ((*v as u32) * 20000).min(65535) as u16;
        })
        .save_png("_sep_im.png")
        .expect("Failed to save PNG");
}
