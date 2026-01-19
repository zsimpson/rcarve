use rcarve::debug_ui;
use rcarve::desc::{Guid, PlyDesc, Thou, parse_comp_json};
use rcarve::im::Lum16Im;
use rcarve::im::label::{LabelInfo, label_im};
use rcarve::region_tree;
use rcarve::sim::sim_toolpaths;
use rcarve::toolpath::{break_long_toolpaths, create_toolpaths_from_region_tree, sort_toolpaths, cull_empty_toolpaths};
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
                "top_thou": 720,
                "hidden": false,
                "is_floor": false,
                "mpoly": [
                    {
                        "exterior": [30,30, 150,30, 150,150, 30,150],
                        "holes": []
                    }
                ]
            },
            "PD_HOLE": {
                "owner_layer_guid": "LD_HOLE",
                "guid": "PD_HOLE",
                "top_thou": 500,
                "hidden": true,
                "is_floor": false,
                "mpoly": [
                    {
                        "exterior": [0, 0, 500,0, 500,500, 0,500],
                        "holes": [
                            [200,200, 300,200, 300,300, 200,300]
                        ]
                    }
                ]
            },
            "FLOOR_PLY_DESC": {
                "owner_layer_guid": "FLOOR_LAYER_DESC",
                "guid": "FLOOR_PLY_DESC",
                "top_thou": 100,
                "hidden": false,
                "is_floor": true,
                "mpoly": [
                    {
                        "exterior": [0, 0, 500,0, 500,500, 0,500],
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
            },
            "LD_HOLE": {
                "guid": "LD_HOLE",
                "hidden": false,
                "is_frame": false
            },
            "FLOOR_LAYER_DESC": {
                "guid": "FLOOR_LAYER_DESC",
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
            { "top_thou": 1000, "bot_thou": 800, "cut_pass": "rough" },
            { "top_thou": 800, "bot_thou": 600, "cut_pass": "rough" },
            { "top_thou": 600, "bot_thou": 400, "cut_pass": "rough" },
            { "top_thou": 400, "bot_thou": 200, "cut_pass": "rough" },
            { "top_thou": 200, "bot_thou": 0, "cut_pass": "rough" },

            { "top_thou": 1000, "bot_thou": 900, "cut_pass": "refine" },
            { "top_thou": 900, "bot_thou": 800, "cut_pass": "refine" },
            { "top_thou": 800, "bot_thou": 700, "cut_pass": "refine" },
            { "top_thou": 700, "bot_thou": 600, "cut_pass": "refine" },
            { "top_thou": 600, "bot_thou": 500, "cut_pass": "refine" },
            { "top_thou": 500, "bot_thou": 400, "cut_pass": "refine" },
            { "top_thou": 400, "bot_thou": 300, "cut_pass": "refine" },
            { "top_thou": 300, "bot_thou": 200, "cut_pass": "refine" },
            { "top_thou": 200, "bot_thou": 100, "cut_pass": "refine" },
            { "top_thou": 100, "bot_thou": 0, "cut_pass": "refine" }
        ]
    }
"#;

fn main() {
    // Debug UI collector (global). These calls are intended to stay in-place and become no-ops
    // in production builds by disabling the `debug_ui` feature.
    debug_ui::init("rcarve debug");

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

    let mut ply_im: region_tree::PlyIm = region_tree::PlyIm::new(w, h);

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

    debug_ui::add_ply_im("ply_im", &ply_im);

    let (region_im_raw, region_infos): (rcarve::im::Im<u16, 1>, Vec<LabelInfo>) = label_im(&ply_im);
    let region_im: region_tree::RegionIm = region_im_raw.retag::<region_tree::RegionI>();

    // debug_ui::add_region_im("region_im", &region_im);

    // TODO: Real tools
    let tool_i = 0;
    // TODO use real initial heightmap
    let bulk_top_thou: Thou = Thou(1000);
    // TODO convert to a max length into pixels
    let max_segment_len_pix = 100_usize;

    let mut sim_im = Lum16Im::new(w, h);
    sim_im.arr.fill(bulk_top_thou.0 as u16);
    let before_sim_im = sim_im.clone();

    // Rough
    let rough_toolpaths = {
        let rough_cut_bands = region_tree::create_cut_bands(
            "rough",
            &ply_im,
            &comp_desc.bands,
            &region_im,
            &region_infos,
            &sorted_ply_descs,
        );

        println!("Rough cut bands:");
        region_tree::debug_print_cut_bands(&rough_cut_bands);

        let rough_region_root = region_tree::create_region_tree(&rough_cut_bands, &region_infos);

        // TODO un hard-code these and use real tool settings
        let rough_tool_dia_pix = 20_usize;
        let rough_step_size_pix = (rough_tool_dia_pix.saturating_mul(4) / 5).max(1);
        let rough_margin_pix = 5_usize;
        let rough_pride_thou = Thou(0);
        let rough_perimeter_step_size_pix = (rough_tool_dia_pix.saturating_mul(4) / 5).max(1);

        let mut rough_toolpaths = create_toolpaths_from_region_tree(
            "rough",
            &rough_region_root,
            &rough_cut_bands,
            tool_i,
            rough_tool_dia_pix,
            rough_step_size_pix,
            rough_margin_pix,
            rough_pride_thou,
            &ply_im,
            &region_im,
            &region_infos,
            0,
            rough_perimeter_step_size_pix,
            true,
            None,
        );

        sort_toolpaths(&mut rough_toolpaths, &rough_region_root);
        break_long_toolpaths(&mut rough_toolpaths, max_segment_len_pix);
        rough_toolpaths
    };

    println!();

    // let refine_toolpaths = [];

    // Refine
    let refine_toolpaths = {
        let refine_cut_bands = region_tree::create_cut_bands(
            "refine",
            &ply_im,
            &comp_desc.bands,
            &region_im,
            &region_infos,
            &sorted_ply_descs,
        );

        println!("Refine cut bands:");
        region_tree::debug_print_cut_bands(&refine_cut_bands);

        let refine_region_root = region_tree::create_region_tree(&refine_cut_bands, &region_infos);

        let refine_tool_dia_pix = 10_usize;
        let refine_step_size_pix = (refine_tool_dia_pix.saturating_mul(4) / 5).max(1);
        let refine_margin_pix = 0_usize;
        let refine_pride_thou = Thou(0);
        let refine_perimeter_step_size_pix = (refine_tool_dia_pix.saturating_mul(4) / 5).max(1);

        // Can be increased if needed to clear tight spots.
        // This is a choice between pride on rough. If there's pride added
        // on rough the detail needs to surface in which case you only need
        // one perimeter on refine. However if you allow rough to go to the
        // bottom then you need at least as many perimeters as the ratio of
        // the rough to refine tool diameters.
        let n_perimeters = 2;

        let mut refine_toolpaths = create_toolpaths_from_region_tree(
            "refine",
            &refine_region_root,
            &refine_cut_bands,
            tool_i,
            refine_tool_dia_pix,
            refine_step_size_pix,
            refine_margin_pix,
            refine_pride_thou,
            &ply_im,
            &region_im,
            &region_infos,
            n_perimeters,  
            refine_perimeter_step_size_pix,
            false,
            None,
        );

        sort_toolpaths(&mut refine_toolpaths, &refine_region_root);
        break_long_toolpaths(&mut refine_toolpaths, max_segment_len_pix);
        refine_toolpaths
    };

    let mut all_toolpaths = rough_toolpaths;
    all_toolpaths.extend(refine_toolpaths);

    // The toolspaths need to be mutable because the the sim function
    // annotates them with cut information.
    sim_toolpaths(&mut sim_im, &mut all_toolpaths);
    cull_empty_toolpaths(&mut all_toolpaths);

    debug_ui::add_lum16("sim_after", &sim_im);
    debug_ui::add_toolpath_movie("sim toolpath movie", &before_sim_im, &all_toolpaths, 20);
    debug_ui::show();
}
