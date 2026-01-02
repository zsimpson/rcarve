use rcarve::desc::{parse_comp_json, Guid, PlyDesc, Thou};
use rcarve::im::label::label_im;
use rcarve::im::Im;

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
                "cut_pass": "refine"
            },
            {
                "top_thou": 900,
                "bot_thou": 800,
                "cut_pass": "refine"
            },
            {
                "top_thou": 1000,
                "bot_thou": 200,
                "cut_pass": "rough"
            }
        ]
    }
"#;


fn main() {
    let roi_l = 0_usize;
    let roi_t = 0_usize;
    let roi_r = 100_usize;
    let roi_b = 100_usize;
    let w = (roi_r - roi_l) as usize;
    let h = (roi_b - roi_t) as usize;

    let mut ply_im: Im<u16, 1> = Im::new(w, h);

    let comp_desc = parse_comp_json(TEST_JSON).expect("Failed to parse comp JSON");
    // println!("Parsed CompDesc: {:?}", comp_desc);

    // Iter the ply_descs, Keep the plies that are not hidden
    // (including heckig if their owner layer_desc is not hidden)
    // and then sort by top_thou descending.
    let mut sorted_ply_descs: Vec<&PlyDesc> = comp_desc
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

    sorted_ply_descs.sort_by(|a, b| b.top_thou.cmp(&a.top_thou));

    // Prepend a dummy ply for background (value 0).
    // This simplifies the rasterization loop below.
    let dummy_ply = PlyDesc {
        owner_layer_guid: Guid("".to_string()),
        guid: Guid("".to_string()),
        top_thou: Thou(i32::MIN),
        hidden: false,
        is_floor: false,
        ply_mat: vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        mpoly: Vec::new(),
    };
    sorted_ply_descs.insert(0, &dummy_ply);

    // From bottom to top, raster each ply into the Im using its i as the value.
    // Each ply contains a Vec<PolyDesc>; raster each polygon into the same label.
    // Note that .rev() is last so the i corresponds to the original index.
    for (i, ply_desc) in sorted_ply_descs.iter().enumerate().rev() {
        if i == 0 {
            // Dummy ply for background; skip.
            continue;
        }
        for mpoly in &ply_desc.mpoly {
            let mpoly = mpoly.translated(-(roi_l as i64), -(roi_t as i64));
            if mpoly.is_empty() {
                continue;
            }

            mpoly.raster(&mut ply_im, |ply_im, x_start, x_end, y| {
                for x in x_start..x_end {
                    unsafe {
                        *ply_im.get_unchecked_mut(x as usize, y as usize, 0) = i as u16;
                    }
                }
            });
        }
    }

    ply_im.debug_im("ply_im");

    let (region_im, _regions_infos): (Im<u16, 1>, Vec<rcarve::im::label::LabelInfo>) = label_im(&ply_im);

    region_im.debug_im("region_im");


}
