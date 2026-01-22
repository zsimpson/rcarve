use rcarve::debug_ui;
use rcarve::desc::{CompDesc, Guid, PlyDesc, Thou, ToolDesc, Units, parse_comp_json};
use rcarve::dilate_im::im_dilate;
use rcarve::im::label::{LabelInfo, label_im};
use rcarve::im::{Lum16Im, MaskIm, ROI};
use rcarve::mpoly::{IntPath, IntPoint, MPoly};
use rcarve::region_tree;
use rcarve::sim;
use rcarve::toolpath;

fn tool_i_and_dia_pix(tool_descs: &[ToolDesc], tool_guid: &Guid, ppi: usize) -> (usize, usize) {
    let (tool_i, tool_desc) = tool_descs
        .iter()
        .enumerate()
        .find(|(_, td)| &td.guid == tool_guid)
        .unwrap_or_else(|| {
            panic!(
                "tool_guid {} not found in tool_descs (len={})",
                tool_guid,
                tool_descs.len()
            )
        });

    let tool_dia_in = match tool_desc.units {
        Units::Inch => tool_desc.diameter,
        Units::Mm => tool_desc.diameter / 25.4,
    };
    let tool_dia_pix = ((tool_dia_in * ppi as f64).round() as usize).max(1);
    (tool_i, tool_dia_pix)
}

#[allow(dead_code)]
const TEST_JSON: &str = r#"
    {
        "version": 3,
        "guid": "JGYYJQBHTX",
        "dim_desc": {
            "bulk_d_inch": 1.0,
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
                "ply_mat": [0.002, 0.0, 0.0, 0.002, 0.0, 0.0],
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
                "ply_mat": [0.002, 0.0, 0.0, 0.002, 0.0, 0.0],
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
                "ply_mat": [0.002, 0.0, 0.0, 0.002, 0.0, 0.0],
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
                "ply_mat": [0.002, 0.0, 0.0, 0.002, 0.0, 0.0],
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
        ],
        "tool_descs": [
            {
                "guid": "EBES3PGSC3",
                "units": "inch",
                "kind": "endmill",
                "diameter": 0.25,
                "length": 0.5
            },
            {
                "guid": "W5C7NZWAK4",
                "units": "inch",
                "kind": "endmill",
                "diameter": 0.125,
                "length": 0.25
            },
            {
                "guid": "BZ76A81UGA",
                "units": "inch",
                "kind": "endmill",
                "diameter": 0.063,
                "length": 0.125
            }
        ],
        "carve_desc": {
            "grain_y": true,
            "rough_tool_guid": "EBES3PGSC3",
            "refine_tool_guid": "W5C7NZWAK4",
            "detail_tool_guid": null
        }
    }
"#;

/// Create the thou-valued Product Im by layering the plies with dilation
fn make_prod_im(
    w: usize,
    h: usize,
    sorted_ply_descs: &[PlyDesc],
    ply_im: &region_tree::PlyIm,
    tool_dia_pix: usize,
    top_thou: Thou,
    roi: ROI,
) -> Lum16Im {
    // Build prod view at the refine tool_dia_pix scale
    // For each play from bottom to top
    let mut ply_mask_im = MaskIm::new(w, h);
    let mut dil_ply_mask_im = MaskIm::new(w, h);
    let mut prod_im = Lum16Im::new(w, h);

    for (ply_i, ply_desc) in sorted_ply_descs.iter().enumerate().skip(1) {
        ply_mask_im.arr.fill(0);
        dil_ply_mask_im.arr.fill(0);

        // Set the ply_mask_im to 255 where ply_im is >= ply_i
        for y in 0..h {
            for x in 0..w {
                let v = unsafe { *ply_im.get_unchecked(x, y, 0) };
                unsafe {
                    *ply_mask_im.get_unchecked_mut(x, y, 0) =
                        if v as usize >= ply_i { 255_u8 } else { 0_u8 }
                }
            }
        }

        im_dilate(&ply_mask_im, &mut dil_ply_mask_im, tool_dia_pix);

        // Invert dil_ply_mask_im in place
        dil_ply_mask_im.invert();

        im_dilate(&dil_ply_mask_im, &mut ply_mask_im, tool_dia_pix);

        // The image in the dil_ply_mask_im is now black where we want to write
        // the ply into the prod_im
        for y in 0..h {
            for x in 0..w {
                let v = unsafe { *ply_mask_im.get_unchecked(x, y, 0) };
                unsafe {
                    *prod_im.get_unchecked_mut(x, y, 0) = if v == 0 {
                        ply_desc.top_thou.0 as u16
                    } else {
                        *prod_im.get_unchecked(x, y, 0)
                    }
                }
            }
        }
    }

    prod_im.one_pixel_border_on_image_edges_over_roi_span(roi, top_thou.0 as u16);

    prod_im
}

fn make_diff_im(sim_im: &Lum16Im, prod_im: &Lum16Im) -> MaskIm {
    // Subtract prod from sim and anything remaining is a artifact that needs to be cleaned up
    // Any pixel in sim that has a difference; we only keep it if it is not adjacent to a pixel
    // of the same value in the prod im. This avoids thin edges that are just slightly out of alignment.
    const NEIGHBOR_OFFSETS_8: [(isize, isize); 8] = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ];

    let w = sim_im.w;
    let h = sim_im.h;

    let mut diff_mask_im = MaskIm::new(w, h);
    // diff_mask_im.arr.fill(0);
    for y in 0..h {
        for x in 0..w {
            let sim_v = unsafe { *sim_im.get_unchecked(x, y, 0) };
            let prod_v = unsafe { *prod_im.get_unchecked(x, y, 0) };
            let diff_v = if sim_v != prod_v { sim_v } else { 0_u16 };
            if diff_v != 0 {
                // Check neighbors in prod_im
                let mut adjacent_same = false;
                for &(dx, dy) in NEIGHBOR_OFFSETS_8.iter() {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx >= 0 && nx < w as isize && ny >= 0 && ny < h as isize {
                        let n_prod_v =
                            unsafe { *prod_im.get_unchecked(nx as usize, ny as usize, 0) };
                        if n_prod_v >= sim_v {
                            adjacent_same = true;
                            break;
                        }
                    }
                }
                if !adjacent_same {
                    unsafe {
                        *diff_mask_im.get_unchecked_mut(x, y, 0) = 255;
                    }
                }
            }
        }
    }

    diff_mask_im
}

fn carve_roi(comp_desc: CompDesc, roi: ROI, ppi: usize) {
    // Debug UI collector (global). These calls are intended to stay in-place and become no-ops
    // in production builds by disabling the `debug_ui` feature.
    debug_ui::init("rcarve debug");

    let w = (roi.r - roi.l) as usize;
    let h = (roi.b - roi.t) as usize;

    let bulk_top_thou = Thou((comp_desc.dim_desc.bulk_d_inch * 1000.0).round() as i32);

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

    // Add a dummy ply for the frame. The frame is "uncut" stock, so it has the same top_thou
    // as the initial bulk thickness.
    let frame_px = (comp_desc.dim_desc.frame_inch * ppi as f64).round() as i64;
    if frame_px > 0 {
        let fp = frame_px as usize;
        if fp * 2 < w && fp * 2 < h {
            let l = roi.l as i64;
            let t = roi.t as i64;
            let r = roi.r as i64;
            let b = roi.b as i64;
            let outer = IntPath::new(vec![
                IntPoint::from_scaled(l, t),
                IntPoint::from_scaled(r, t),
                IntPoint::from_scaled(r, b),
                IntPoint::from_scaled(l, b),
            ]);
            let inner = IntPath::new(vec![
                IntPoint::from_scaled(l + fp as i64, t + fp as i64),
                IntPoint::from_scaled(r - fp as i64, t + fp as i64),
                IntPoint::from_scaled(r - fp as i64, b - fp as i64),
                IntPoint::from_scaled(l + fp as i64, b - fp as i64),
            ]);
            let frame_mpoly = MPoly::new(vec![outer, inner]);

            sorted_ply_descs.push(PlyDesc {
                owner_layer_guid: Guid("".to_string()),
                guid: Guid("__FRAME__".to_string()),
                top_thou: bulk_top_thou,
                hidden: true,
                is_floor: false,
                ply_mat: vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                mpoly: vec![frame_mpoly],
            });
        }
    }

    let mut ply_im: region_tree::PlyIm = region_tree::PlyIm::new(w, h);

    // From bottom to top, raster each ply into the image using its index as the value.
    // Higher plies overwrite lower ones.
    for (ply_i, ply_desc) in sorted_ply_descs.iter().enumerate().skip(1) {
        for mpoly in &ply_desc.mpoly {
            let mpoly = mpoly.translated(-(roi.l as i64), -(roi.t as i64));
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

    let max_segment_len_inch = 4.0_f64;
    let max_segment_len_pix = ((max_segment_len_inch * ppi as f64).round() as usize).max(1);

    let mut sim_im = Lum16Im::new(w, h);
    sim_im.arr.fill(bulk_top_thou.0 as u16);
    let before_sim_im = sim_im.clone();

    // Rough setup
    let rough_cut_bands = region_tree::create_cut_bands(
        "rough",
        &ply_im,
        &comp_desc.bands,
        &region_im,
        &region_infos,
        &sorted_ply_descs,
    );
    let rough_tool_guid = comp_desc
        .carve_desc
        .rough_tool_guid
        .as_ref()
        .expect("No rough tool guid in carve_desc");
    let (rough_tool_i, rough_tool_dia_pix) =
        tool_i_and_dia_pix(&comp_desc.tool_descs, rough_tool_guid, ppi);
    let rough_region_root = region_tree::create_region_tree(&rough_cut_bands, &region_infos);
    let rough_margin_pix = rough_tool_dia_pix.saturating_mul(2) / 5;
    let rough_pride_thou = Thou(0);

    // Refine setup
    let refine_cut_bands = region_tree::create_cut_bands(
        "refine",
        &ply_im,
        &comp_desc.bands,
        &region_im,
        &region_infos,
        &sorted_ply_descs,
    );
    let refine_region_root = region_tree::create_region_tree(&refine_cut_bands, &region_infos);
    let refine_tool_guid = comp_desc
        .carve_desc
        .refine_tool_guid
        .as_ref()
        .expect("No refine tool guid in carve_desc");
    let (refine_tool_i, refine_tool_dia_pix) =
        tool_i_and_dia_pix(&comp_desc.tool_descs, refine_tool_guid, ppi);
    
    // TODO: I need two modes on gen_surcaces. One for all surfaces (rough)
    // and another for just the final surfaces (refine if rough pride > 0)
    // let refine_gen_surfaces = rough_pride_thou.0 > 0;

    // Rough create
    let rough_toolpaths = {
        let mut rough_toolpaths = toolpath::create_toolpaths_from_region_tree(
            "rough",
            &rough_region_root,
            &rough_cut_bands,
            rough_tool_i,
            rough_tool_dia_pix,
            (rough_tool_dia_pix.saturating_mul(4) / 5).max(1),
            rough_margin_pix,
            rough_pride_thou,
            &ply_im,
            &region_im,
            None,
            &region_infos,
            0,
            (rough_tool_dia_pix.saturating_mul(4) / 5).max(1),
            true,
            None,
        );

        toolpath::sort_toolpaths(&mut rough_toolpaths, &rough_region_root);
        toolpath::break_long_toolpaths(&mut rough_toolpaths, max_segment_len_pix);
        sim::sim_toolpaths(&mut sim_im, &mut rough_toolpaths, None);
        toolpath::cull_empty_toolpaths(&mut rough_toolpaths);
        let mut rough_traverse_sim_im = before_sim_im.clone();
        toolpath::add_traverse_toolpaths(&mut rough_traverse_sim_im, &mut rough_toolpaths);

        rough_toolpaths
    };

    println!();

    let before_sim_im = sim_im.clone();

    // Refine create
    let refine_toolpaths = {
        let mut refine_toolpaths = toolpath::create_toolpaths_from_region_tree(
            "refine",
            &refine_region_root,
            &refine_cut_bands,
            refine_tool_i,
            refine_tool_dia_pix,
            (refine_tool_dia_pix.saturating_mul(4) / 5).max(1),
            0_usize,
            Thou(0),
            &ply_im,
            &region_im,
            None,
            &region_infos,
            3,
            (refine_tool_dia_pix.saturating_mul(4) / 5).max(1),
            false,
            None,
        );

        toolpath::sort_toolpaths(&mut refine_toolpaths, &refine_region_root);
        toolpath::break_long_toolpaths(&mut refine_toolpaths, max_segment_len_pix);
        sim::sim_toolpaths(&mut sim_im, &mut refine_toolpaths, None);
        toolpath::cull_empty_toolpaths(&mut refine_toolpaths);
        let mut refine_traverse_sim_im = before_sim_im.clone();
        toolpath::add_traverse_toolpaths(&mut refine_traverse_sim_im, &mut refine_toolpaths);

        refine_toolpaths
    };

    // Differencer: Compare the results of the idealized product image from the current sim and add tool paths for the differences.
    let prod_im = make_prod_im(
        w,
        h,
        &sorted_ply_descs,
        &ply_im,
        refine_tool_dia_pix,
        bulk_top_thou,
        roi,
    );

    let diff_mask_im = make_diff_im(&sim_im, &prod_im);

    let mut before_sim_im = sim_im.clone();

    // Run the refine toolpaths again with the diff_mask to try to clean up the diff areas
    let diff_refine_toolpaths = {
        let mut diff_refine_toolpaths = toolpath::create_toolpaths_from_region_tree(
            "refine",
            &refine_region_root,
            &refine_cut_bands,
            refine_tool_i,
            refine_tool_dia_pix,
            (refine_tool_dia_pix.saturating_mul(2) / 5).max(1),
            0_usize,
            Thou(0),
            &ply_im,
            &region_im,
            Some(&diff_mask_im),
            &region_infos,
            1,
            (refine_tool_dia_pix.saturating_mul(2) / 5).max(1),
            false,
            None,
        );

        toolpath::sort_toolpaths(&mut diff_refine_toolpaths, &refine_region_root);
        toolpath::break_long_toolpaths(&mut diff_refine_toolpaths, max_segment_len_pix);
        sim::sim_toolpaths(&mut sim_im, &mut diff_refine_toolpaths, None);
        toolpath::cull_empty_toolpaths(&mut diff_refine_toolpaths);
        let mut refine_traverse_sim_im = before_sim_im.clone();
        toolpath::add_traverse_toolpaths(&mut refine_traverse_sim_im, &mut diff_refine_toolpaths);

        diff_refine_toolpaths
    };

    debug_ui::add_lum16("prod_im", &prod_im);
    debug_ui::add_lum16("sim_im", &sim_im);
    debug_ui::add_mask_im("diff_im", &diff_mask_im);

    let mut all_toolpaths = rough_toolpaths;
    all_toolpaths.extend(refine_toolpaths);
    all_toolpaths.extend(diff_refine_toolpaths);

    before_sim_im.arr.fill(bulk_top_thou.0 as u16);
    debug_ui::add_toolpath_movie("sim toolpath movie", &before_sim_im, &all_toolpaths);
    debug_ui::show();
}

fn main() {
    // Pixels per inch used for conversions between inches and pixels.
    let ppi: usize = 100_usize;

    let comp_desc = parse_comp_json(TEST_JSON).expect("Failed to parse comp JSON");

    let total_w_inch =
        comp_desc.dim_desc.bulk_w_inch + 2.0 * comp_desc.dim_desc.frame_inch;
    let total_h_inch =
        comp_desc.dim_desc.bulk_h_inch + 2.0 * comp_desc.dim_desc.frame_inch;

    // Convert normalized/real-unit geometry into pixel space.
    let scale = (
        comp_desc.dim_desc.bulk_w_inch * ppi as f64,
        comp_desc.dim_desc.bulk_h_inch * ppi as f64,
    );
    let frame_px = (comp_desc.dim_desc.frame_inch * ppi as f64).round() as i64;
    let translation = (frame_px, frame_px);
    let comp_desc = comp_desc.with_adjusted_mpolys(translation, scale);
    // println!("Parsed CompDesc: {:?}", comp_desc);

    let roi = ROI {
        l: 0,
        t: 0,
        r: ppi * total_w_inch as usize,
        b: ppi * total_h_inch as usize,
    };
    carve_roi(comp_desc, roi, ppi);
}
