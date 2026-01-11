use rcarve::im::Lum16Im;
use rcarve::region_tree::{PlyIm, RegionI, RegionIm, create_cut_bands, create_region_tree, debug_print_cut_bands};
use rcarve::desc::{Guid, PlyDesc, Thou, parse_comp_json};
use rcarve::im::label::{LabelInfo, label_im};
use rcarve::toolpath::create_toolpaths_from_region_tree;
use rcarve::sim::sim_toolpaths;
// use rcarve::sim::{circle_pixel_iz, draw_toolpath_single_depth};

#[allow(dead_code)]
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
                "top_thou": 850,
                "hidden": false,
                "is_floor": false,
                "mpoly": [
                    {
                        "exterior": [100,100, 400,100, 400,400, 100,400],
                        "holes": [
                            [200,200, 300,200, 300,300, 200,300]
                        ]
                    }
                ]
            },
            "ZWKKED69NS": {
                "owner_layer_guid": "R7Y9XP4VNB",
                "guid": "ZWKKED69NS",
                "top_thou": 500,
                "hidden": false,
                "is_floor": false,
                "mpoly": [
                    {
                        "exterior": [10,10, 50,10, 50,50, 10,50],
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
                "top_thou": 800,
                "bot_thou": 0,
                "cut_pass": "refine"
            },
            {
                "top_thou": 1000,
                "bot_thou": 800,
                "cut_pass": "rough"
            },
            {
                "top_thou": 800,
                "bot_thou": 0,
                "cut_pass": "rough"
            }
        ]
    }
"#;

fn main() {
    let roi_l = 0_usize;
    let roi_t = 0_usize;
    let roi_r = 500_usize;
    let roi_b = 500_usize;
    let w = (roi_r - roi_l) as usize;
    let h = (roi_b - roi_t) as usize;

    let comp_desc = parse_comp_json(TEST_JSON).expect("Failed to parse comp JSON");
    // println!("Parsed CompDesc: {:?}", comp_desc);

    // Keep plies that are not hidden (and whose layer is not hidden),
    // then sort bottom-to-top so higher `top_thou` get higher ply indices.
    let mut sorted_ply_descs: Vec<PlyDesc> = comp_desc
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
        .cloned()
        .collect();

    sorted_ply_descs.sort_by(|a, b| a.top_thou.cmp(&b.top_thou));

    // Fiddle with plies for debugging.
    // Set the ply_mat on every ply
    for ply_desc in &mut sorted_ply_descs {
        ply_desc.ply_mat = vec![2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
    }

    // Prepend a dummy ply for background (ply_i = 0).
    // `create_cut_bands` expects this exact shape.
    sorted_ply_descs.insert(
        0,
        PlyDesc {
            owner_layer_guid: Guid("".to_string()),
            guid: Guid("".to_string()),
            top_thou: Thou(0),
            hidden: true,
            is_floor: false,
            ply_mat: vec![2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
            mpoly: Vec::new(),
        },
    );

    let mut ply_im: PlyIm = PlyIm::new(w, h);

    // From bottom to top, raster each ply into the image using its index as the value.
    // Higher plies overwrite lower ones.
    for (ply_i, ply_desc) in sorted_ply_descs.iter().enumerate().skip(1) {
        for mpoly in &ply_desc.mpoly {
            let mpoly = mpoly.translated(-(roi_l as i64), -(roi_t as i64));
            if mpoly.is_empty() {
                continue;
            }

            mpoly.raster(&mut ply_im, |ply_im, x_start, x_end, y| {
                for x in x_start..x_end {
                    unsafe {
                        *ply_im.get_unchecked_mut(x as usize, y as usize, 0) = ply_i as u16;
                    }
                }
            });
        }
    }

    // ply_im.debug_im("ply_im");

    let (region_im_raw, region_infos): (rcarve::im::Im<u16, 1>, Vec<LabelInfo>) = label_im(&ply_im);
    let region_im: RegionIm = region_im_raw.retag::<RegionI>();

    // Print ROI/pixel/neighbors info (skip index 0).
    for (label_id, info) in region_infos.iter().enumerate().skip(1) {
        println!(
            "Label {}: size={}, start=({},{}), roi=({},{})->({},{}) px_count={} neigh_count={}",
            label_id,
            info.size,
            info.start_x,
            info.start_y,
            info.roi.l,
            info.roi.t,
            info.roi.r,
            info.roi.b,
            info.pixel_iz.len(),
            info.neighbors.len()
        );
    }

    // region_im.debug_im("region_im");

    let cut_bands = create_cut_bands(
        "rough",
        &ply_im,
        &comp_desc.bands,
        &region_im,
        &region_infos,
        &sorted_ply_descs,
    );

    debug_print_cut_bands(&cut_bands);

    let region_root = create_region_tree(&cut_bands, &region_infos);

    // TODO un hard-code the tool radius
    let tool_dia_pix = 20_usize; // Pixels

    // Raster step size: int(80% of tool radius), clamped to at least 1.
    let step_size_pix = (tool_dia_pix.saturating_mul(4) / 5).max(1);

    let tool_i = 0; // TODO

    let surface_toolpaths = create_toolpaths_from_region_tree(
        &region_root,
        &cut_bands,
        tool_i,
        tool_dia_pix,
        step_size_pix,
        &ply_im,
        &region_im,
        &region_infos,
        true,
        true,
        None,
    );

    let mut sim_im = Lum16Im::new(w, h);
    sim_im.arr.fill(1000_u16); // TODO real initial heightmap

    let base_im = sim_im.clone();

    sim_toolpaths(
        &mut sim_im,
        &surface_toolpaths,
    );

    sim_im.debug_im("sim_im");

    #[cfg(all(feature = "debug_ui", not(feature = "cli_only")))]
    {
        if let Err(e) = rcarve::sim::debug_ui::show_toolpath_movie(
            &base_im,
            &surface_toolpaths,
            "sim toolpath movie",
        ) {
            println!("show_toolpath_movie: {e}");
        }
    }
}
