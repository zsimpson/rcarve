use rcarve::desc::{parse_comp_json, PolyDesc};
use rcarve::mpoly::{IntPath, IntPoint, MPoly};
// use std::collections::HashSet;

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
                        "exterior": [40,40, 60,40, 60,60, 40,60],
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

use rcarve::im::Im;
pub type Sep16Im = Im<u16, 1>;

pub fn polydesc_to_mpoly(polydesc: &PolyDesc) -> MPoly {
    fn flat_verts_to_path(flat: &[i32]) -> Option<IntPath> {
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
            let x = xy[0] as i64;
            let y = xy[1] as i64;
            pts.push(IntPoint::from_scaled(x, y));
        }

        Some(IntPath::new(pts))
    }

    let mut paths: Vec<IntPath> = Vec::with_capacity(1 + polydesc.holes.len());
    if let Some(ext) = flat_verts_to_path(&polydesc.exterior) {
        paths.push(ext);
    }
    for hole in &polydesc.holes {
        if let Some(h) = flat_verts_to_path(hole) {
            paths.push(h);
        }
    }

    MPoly::new(paths)
}


fn main() {
    let comp_desc = parse_comp_json(TEST_JSON).expect("Failed to parse comp JSON");
    println!("Parsed CompDesc: {:?}", comp_desc);

    // Iter the ply_descs, Keep the plies that are not hidden
    // (including heckig if their owner layer_desc is not hidden)
    // and then sort by top_thou descending.
    let mut sorted_plies: Vec<_> = comp_desc
        .ply_desc_by_guid
        .values()
        .filter(|ply| {
            if ply.hidden {
                return false;
            }
            if let Some(layer) = comp_desc.layer_desc_by_guid.get(&ply.owner_layer_guid) {
                return !layer.hidden;
            }
            true
        })
        .collect();

    sorted_plies.sort_by(|a, b| b.top_thou.cmp(&a.top_thou));

    // New an Im that is 100x100 
    let mut im = Sep16Im::new(100, 100);

    // Raster each ply into the Im using its i as the value.
    // Each ply contains a Vec<PolyDesc>; raster each polygon into the same label.
    for (i, ply) in sorted_plies.iter().enumerate() {
        let value = (i as u16) + 1; // start from 1

        for polydesc in &ply.mpoly {
            let mpoly = polydesc_to_mpoly(polydesc);
            if mpoly.is_empty() {
                continue;
            }

            mpoly.raster(&mut im, |im, x_start, x_end, y| {
                for x in x_start..x_end {
                    unsafe {
                        *im.get_unchecked_mut(x as usize, y as usize, 0) = value;
                    }
                }
            });
        }
    }

    im
        .mul_const_clamp_max_inplace(20000)
        .save_png("_test_output.png").expect("Failed to save PNG");
}