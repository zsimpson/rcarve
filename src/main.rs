use rcarve::debug_ui;
use rcarve::desc::{CompDesc, Guid, PlyDesc, Thou, ToolDesc, Units, parse_comp_json};
use rcarve::dilate_im::im_dilate;
use rcarve::im::label::{LabelInfo, label_im};
use rcarve::im::{Lum16Im, MaskIm, ROI};
use rcarve::mpoly::{IntPath, IntPoint, MPoly};
use rcarve::region_tree;
use rcarve::sim;
use rcarve::toolpath;

use std::collections::HashMap;
use std::fs;
use std::io::BufWriter;
use std::sync::Arc;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;

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

fn tool_dia_inch(tool_desc: &ToolDesc) -> f64 {
    match tool_desc.units {
        Units::Inch => tool_desc.diameter,
        Units::Mm => tool_desc.diameter / 25.4,
    }
}

#[derive(Debug, Clone, Serialize)]
struct SingleToolOut {
    tool_guid: String,
    tool_i: usize,
    tool_dia_pix: usize,
    tool_dia_inch: f64,
    ppi: usize,
    tile_n: usize,
    toolpaths: Vec<ToolpathOut>,
}

#[derive(Debug, Clone, Serialize)]
struct ToolpathOut {
    is_cut: bool,
    cuts: [u64; 2],
    points: Vec<i32>,
    tile_i: usize,
}

fn toolpath_to_toolpath_out(tp: &toolpath::ToolPath) -> ToolpathOut {
    let mut pixels_changed: u64 = 0;
    let mut depth_sum_thou: u64 = 0;
    for c in &tp.cuts {
        pixels_changed += c.pixels_changed;
        depth_sum_thou += c.depth_sum_thou;
    }

    let mut points: Vec<i32> = Vec::with_capacity(tp.points.len().saturating_mul(3));
    for p in &tp.points {
        points.push(p.x);
        points.push(p.y);
        points.push(p.z);
    }

    ToolpathOut {
        is_cut: !tp.is_traverse,
        cuts: [pixels_changed, depth_sum_thou],
        points,
        tile_i: tp.tile_i,
    }
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

fn translate_toolpaths_in_place(toolpaths: &mut [toolpath::ToolPath], dx: i32, dy: i32) {
    if dx == 0 && dy == 0 {
        return;
    }

    for tp in toolpaths.iter_mut() {
        for p in tp.points.iter_mut() {
            p.x = p.x.checked_add(dx).expect("toolpath x overflow");
            p.y = p.y.checked_add(dy).expect("toolpath y overflow");
        }
    }
}

fn regroup_toolpaths_by_tool(mut toolpaths: Vec<toolpath::ToolPath>) -> HashMap<usize, Vec<toolpath::ToolPath>> {
    // if toolpaths.is_empty() {
    //     return toolpaths;
    // }

    // Group toolpaths by tool, and order tools from largest -> smallest.
    // Preserve the relative order of toolpaths within each tool group.
    let mut toolpaths_by_tool_i: HashMap<usize, Vec<toolpath::ToolPath>> = HashMap::new();

    for tp in toolpaths.drain(..) {
        // tool_dia_by_tool_i
        //     .entry(tp.tool_i)
        //     .and_modify(|d| *d = (*d).max(tp.tool_dia_pix))
        //     .or_insert(tp.tool_dia_pix);
        toolpaths_by_tool_i.entry(tp.tool_i).or_default().push(tp);
    }

    // let mut tools: Vec<(usize, usize)> = tool_dia_by_tool_i
    //     .iter()
    //     .map(|(&tool_i, &tool_dia_pix)| (tool_dia_pix, tool_i))
    //     .collect();

    // Largest diameter first; stable-ish tie-break by tool index.
    // tools.sort_by(|(dia_a, tool_a), (dia_b, tool_b)| dia_b.cmp(dia_a).then_with(|| tool_a.cmp(tool_b)));

    // let mut out: Vec<toolpath::ToolPath> = Vec::new();
    // for (_tool_dia_pix, tool_i) in tools {
    //     if let Some(mut tps) = toolpaths_by_tool_i.remove(&tool_i) {
    //         out.append(&mut tps);
    //     }
    // }
    // out
    toolpaths_by_tool_i
}

fn carve_rois_in_pool(
    comp_desc: Arc<CompDesc>,
    global_roi: ROI,
    tile_rois: Vec<ROI>,
    ppi: usize,
    n_workers: usize,
) -> Vec<toolpath::ToolPath> {
    if tile_rois.is_empty() {
        return Vec::new();
    }

    let n_workers = n_workers.max(1).min(tile_rois.len());
    
    // Jobs are (tile_index, ROI); send `None` as a stop signal.
    let (job_tx, job_rx) = mpsc::channel::<Option<(usize, ROI)>>();
    let job_rx = Arc::new(Mutex::new(job_rx));
    let (res_tx, res_rx) = mpsc::channel::<(usize, Vec<toolpath::ToolPath>)>();

    for _ in 0..n_workers {
        let comp_desc = Arc::clone(&comp_desc);
        let job_rx = Arc::clone(&job_rx);
        let res_tx = res_tx.clone();
        std::thread::spawn(move || loop {
            let msg = {
                let rx = job_rx.lock().expect("job_rx poisoned");
                rx.recv()
            };

            match msg {
                Ok(Some((tile_i, tile_roi))) => {
                    let mut toolpaths = carve_roi(&comp_desc, global_roi, tile_roi, ppi);
                    for tp in toolpaths.iter_mut() {
                        tp.tile_i = tile_i;
                    }
                    let _ = res_tx.send((tile_i, toolpaths));
                }
                Ok(None) | Err(_) => break,
            }
        });
    }

    // Enqueue all jobs.
    for (tile_i, roi) in tile_rois.iter().copied().enumerate() {
        job_tx
            .send(Some((tile_i, roi)))
            .expect("failed to enqueue job");
    }
    // Tell workers to stop.
    for _ in 0..n_workers {
        let _ = job_tx.send(None);
    }
    drop(job_tx);

    // Collect exactly one result per tile ROI, preserving row-major tile order.
    let mut by_tile: Vec<Vec<toolpath::ToolPath>> = vec![Vec::new(); tile_rois.len()];
    for _ in 0..tile_rois.len() {
        let (tile_i, toolpaths) = res_rx.recv().expect("worker result channel closed");
        if tile_i >= by_tile.len() {
            panic!("worker returned invalid tile index {tile_i}");
        }
        by_tile[tile_i] = toolpaths;
    }

    let mut all_toolpaths: Vec<toolpath::ToolPath> = Vec::new();
    for mut tp in by_tile {
        all_toolpaths.append(&mut tp);
    }
    all_toolpaths
}

fn carve_roi(comp_desc: &CompDesc, global_roi: ROI, roi: ROI, ppi: usize) -> Vec<toolpath::ToolPath> {

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

    // Add a dummy ply for the global frame only (not per-tile).
    // The frame is "uncut" stock, so it has the same top_thou as the initial bulk thickness.
    let frame_px = (comp_desc.dim_desc.frame_inch * ppi as f64).round() as i64;
    if frame_px > 0 {
        let fp = frame_px as usize;
        if fp * 2 < global_roi.w() && fp * 2 < global_roi.h() {
            let l = global_roi.l as i64;
            let t = global_roi.t as i64;
            let r = global_roi.r as i64;
            let b = global_roi.b as i64;
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

    let (region_im_raw, region_infos): (rcarve::im::Im<u16, 1>, Vec<LabelInfo>) = label_im(&ply_im);
    let region_im: region_tree::RegionIm = region_im_raw.retag::<region_tree::RegionI>();

    // debug_ui::add_region_im("region_im", &region_im);

    let max_segment_len_inch = 4.0_f64;
    let max_segment_len_pix = ((max_segment_len_inch * ppi as f64).round() as usize).max(1);

    let mut sim_im = Lum16Im::new(w, h);
    sim_im.arr.fill(bulk_top_thou.0 as u16);

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

        rough_toolpaths
    };

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

        refine_toolpaths
    };

    // Differencer: Compare the results of the idealized product image from the current sim and add tool paths for the differences.
    let local_roi = ROI {
        l: 0,
        t: 0,
        r: w,
        b: h,
    };
    let prod_im = make_prod_im(
        w,
        h,
        &sorted_ply_descs,
        &ply_im,
        refine_tool_dia_pix,
        bulk_top_thou,
        local_roi,
    );

    let diff_mask_im = make_diff_im(&sim_im, &prod_im);

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
            0,
            (refine_tool_dia_pix.saturating_mul(2) / 5).max(1),
            true,
            None,
        );

        toolpath::sort_toolpaths(&mut diff_refine_toolpaths, &refine_region_root);
        toolpath::break_long_toolpaths(&mut diff_refine_toolpaths, max_segment_len_pix);
        sim::sim_toolpaths(&mut sim_im, &mut diff_refine_toolpaths, None);
        toolpath::cull_empty_toolpaths(&mut diff_refine_toolpaths);

        diff_refine_toolpaths
    };

    let mut all_toolpaths = rough_toolpaths;
    all_toolpaths.extend(refine_toolpaths);
    all_toolpaths.extend(diff_refine_toolpaths);

    // Convert ROI-local pixel coords to global pixel coords.
    let dx: i32 = roi.l.try_into().expect("roi.l too large for i32");
    let dy: i32 = roi.t.try_into().expect("roi.t too large for i32");
    translate_toolpaths_in_place(&mut all_toolpaths, dx, dy);

    all_toolpaths
}

// COnvert the toolpaths per tool into gcode lines
// Each is_cut will be a G1 move, each traverse a G0 move
fn to_gcode(json: &SingleToolOut) -> String {
    // Convention:
    // - X/Y are inches in absolute coordinates derived from pixels via `ppi`.
    // - Z is inches derived from `thou` via /1000.0.
    //   (This assumes `z` in the toolpaths is a height-above-zero value in thou.)
    const CLEARANCE_Z_INCH: f64 = 0.1;
    const FEED_IPM: f64 = 60.0;
    const PLUNGE_IPM: f64 = 30.0;

    let ppi_f = json.ppi as f64;

    // Compute a safe Z in thou (exact integer units) so later comparisons are exact.
    let mut max_z_thou: i32 = 0;
    for tp in &json.toolpaths {
        for xyz in tp.points.chunks_exact(3) {
            max_z_thou = max_z_thou.max(xyz[2]);
        }
    }
    let clearance_thou: i32 = (CLEARANCE_Z_INCH * 1000.0).round() as i32;
    let safe_z_thou: i32 = max_z_thou.saturating_add(clearance_thou);

    #[derive(Debug, Clone, Copy, Default)]
    struct ModalState {
        x_pix: Option<i32>,
        y_pix: Option<i32>,
        z_thou: Option<i32>,
        last_feed_ipm: Option<f64>,
    }

    fn fmt_xy_inch(pix: i32, ppi: f64) -> f64 {
        (pix as f64) / ppi
    }

    fn fmt_z_inch(thou: i32) -> f64 {
        (thou as f64) / 1000.0
    }

    fn push_g0(out: &mut String, st: &mut ModalState, x_pix: Option<i32>, y_pix: Option<i32>, z_thou: Option<i32>, ppi_f: f64) {
        let mut any = false;
        out.push_str("G0");

        if let Some(x) = x_pix {
            if st.x_pix.map_or(true, |px| px != x) {
                out.push_str(&format!(" X{:.4}", fmt_xy_inch(x, ppi_f)));
                st.x_pix = Some(x);
                any = true;
            }
        }
        if let Some(y) = y_pix {
            if st.y_pix.map_or(true, |py| py != y) {
                out.push_str(&format!(" Y{:.4}", fmt_xy_inch(y, ppi_f)));
                st.y_pix = Some(y);
                any = true;
            }
        }
        if let Some(z) = z_thou {
            if st.z_thou.map_or(true, |pz| pz != z) {
                out.push_str(&format!(" Z{:.4}", fmt_z_inch(z)));
                st.z_thou = Some(z);
                any = true;
            }
        }

        if any {
            out.push('\n');
        } else {
            // No axis changed; roll back the prefix.
            out.truncate(out.len().saturating_sub(2));
        }
    }

    fn push_g1(out: &mut String, st: &mut ModalState, x_pix: Option<i32>, y_pix: Option<i32>, z_thou: Option<i32>, feed_ipm: Option<f64>, ppi_f: f64) {
        let mut any_axis = false;
        let mut any_feed = false;

        out.push_str("G1");

        if let Some(x) = x_pix {
            if st.x_pix.map_or(true, |px| px != x) {
                out.push_str(&format!(" X{:.4}", fmt_xy_inch(x, ppi_f)));
                st.x_pix = Some(x);
                any_axis = true;
            }
        }
        if let Some(y) = y_pix {
            if st.y_pix.map_or(true, |py| py != y) {
                out.push_str(&format!(" Y{:.4}", fmt_xy_inch(y, ppi_f)));
                st.y_pix = Some(y);
                any_axis = true;
            }
        }
        if let Some(z) = z_thou {
            if st.z_thou.map_or(true, |pz| pz != z) {
                out.push_str(&format!(" Z{:.4}", fmt_z_inch(z)));
                st.z_thou = Some(z);
                any_axis = true;
            }
        }

        if let Some(f) = feed_ipm {
            // Emit F when it differs from the last F we emitted.
            if st.last_feed_ipm.map_or(true, |lf| (lf - f).abs() > 1e-9) {
                out.push_str(&format!(" F{:.1}", f));
                st.last_feed_ipm = Some(f);
                any_feed = true;
            }
        }

        if any_axis || any_feed {
            out.push('\n');
        } else {
            out.truncate(out.len().saturating_sub(2));
        }
    }

    fn push_comment(out: &mut String, s: &str) {
        // Use `()`-style comments for compatibility with many CNC controllers.
        // Replace any ')' to avoid prematurely closing the comment.
        let safe = s.replace(')', "]");
        out.push('(');
        out.push_str(&safe);
        out.push_str(")\n");
    }

    let mut out = String::new();
    push_comment(&mut out, "rcarve toolpaths");
    push_comment(
        &mut out,
        &format!(
            "tool_guid={} tool_i={} tool_dia_inch={:.6} tool_dia_pix={} ppi={}",
            json.tool_guid, json.tool_i, json.tool_dia_inch, json.tool_dia_pix, json.ppi
        ),
    );

    // TODO: Softcode
    out.push_str("G20 (Unit is inches)\n");
    out.push_str(&format!("G0 Z${} (Unit is inches)\n", CLEARANCE_Z_INCH));
    out.push_str("M03 (Spindle on)\n");

    let mut st = ModalState::default();
    // Start with a retract to safe Z (Z-only).
    push_g0(&mut out, &mut st, None, None, Some(safe_z_thou), ppi_f);

    let mut last_cut_tile_i: Option<usize> = None;

    for (tp_i, tp) in json.toolpaths.iter().enumerate() {
        let mut pts = tp.points.chunks_exact(3);
        let Some(first) = pts.next() else {
            continue;
        };

        let x0_pix: i32 = first[0];
        let y0_pix: i32 = first[1];
        let z0_thou: i32 = first[2];

        if tp.is_cut {
            if last_cut_tile_i != Some(tp.tile_i) {
                let tile_1 = tp.tile_i.saturating_add(1);
                let tile_n = json.tile_n.max(tile_1);
                push_comment(
                    &mut out,
                    &format!("============================= TILE {tile_1} of {tile_n} ============================="),
                );
                last_cut_tile_i = Some(tp.tile_i);
            }
            push_comment(&mut out, &format!("tp[{tp_i}] cuts=[{}, {}]", tp.cuts[0], tp.cuts[1]));

            // Always retract to safe Z before any XY reposition.
            push_g0(&mut out, &mut st, None, None, Some(safe_z_thou), ppi_f);
            // Reposition in XY at safe Z (omit Z if already at safe).
            push_g0(&mut out, &mut st, Some(x0_pix), Some(y0_pix), None, ppi_f);
            // Plunge (Z-only) at plunge feed.
            push_g1(&mut out, &mut st, None, None, Some(z0_thou), Some(PLUNGE_IPM), ppi_f);

            // Follow the polyline at cut feed.
            for xyz in pts {
                let x_pix: i32 = xyz[0];
                let y_pix: i32 = xyz[1];
                let z_thou: i32 = xyz[2];
                push_g1(
                    &mut out,
                    &mut st,
                    Some(x_pix),
                    Some(y_pix),
                    Some(z_thou),
                    Some(FEED_IPM),
                    ppi_f,
                );
            }
        } else {
            push_comment(
                &mut out,
                &format!("---- tp[{tp_i}] traverse"),
            );
            // Enforce a conservative traverse policy:
            // retract to safe Z, traverse in XY at constant safe Z, and let the next cut handle plunging.

            // Retract (Z-only).
            push_g0(&mut out, &mut st, None, None, Some(safe_z_thou), ppi_f);
            // Traverse to the first point in XY (no Z changes during traverse).
            push_g0(&mut out, &mut st, Some(x0_pix), Some(y0_pix), None, ppi_f);
            for xyz in pts {
                let x_pix: i32 = xyz[0];
                let y_pix: i32 = xyz[1];
                push_g0(&mut out, &mut st, Some(x_pix), Some(y_pix), None, ppi_f);
            }
        }
    }

    // Final retract.
    push_g0(&mut out, &mut st, None, None, Some(safe_z_thou), ppi_f);
    out.push_str("M2\n");
    out
}

fn main() {
    // Pixels per inch used for conversions between inches and pixels.
    let ppi: usize = 100_usize;

    let t0 = Instant::now();

    // Optional CLI args:
    // - N: grid size for NxN tiling
    // - workers: max worker threads (defaults to CPU count)
    // Example: `cargo run --release -- 4 8`
    // let grid_n: usize = std::env::args()
    //     .nth(1)
    //     .and_then(|s| s.parse::<usize>().ok())
    //     .unwrap_or(1)
    //     .max(1);
    let n_workers: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
        .max(1);

    // Debugging hack limit to one
    // let n_workers = 1;

    println!("Using {} worker threads", n_workers);
    
    // TODO compute a good grid_n dynamically
    let grid_n: usize = 4;

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

    // Debug UI collector (global). These calls are intended to stay in-place and become no-ops
    // in production builds by disabling the `debug_ui` feature.
    debug_ui::init("rcarve debug");

    let rough_tool_guid = comp_desc
        .carve_desc
        .rough_tool_guid
        .as_ref()
        .expect("No rough tool guid in carve_desc");
    let (_rough_tool_i, rough_tool_dia_pix) =
        tool_i_and_dia_pix(&comp_desc.tool_descs, rough_tool_guid, ppi);
    let overlap_pix = rough_tool_dia_pix;

    let bulk_top_thou = Thou((comp_desc.dim_desc.bulk_d_inch * 1000.0).round() as i32);

    let comp_desc = Arc::new(comp_desc);

    // Split the full ROI into an NxN grid in global pixel space, then pad each tile ROI
    // by `overlap_pix` on each side (clamped) to ensure overlap.
    let tile_w = (roi.w() + grid_n - 1) / grid_n;
    let tile_h = (roi.h() + grid_n - 1) / grid_n;

    let mut tile_rois: Vec<ROI> = Vec::with_capacity(grid_n * grid_n);
    for gy in 0..grid_n {
        for gx in 0..grid_n {
            let base_l = gx * tile_w;
            let base_t = gy * tile_h;
            let base_r = ((gx + 1) * tile_w).min(roi.r);
            let base_b = ((gy + 1) * tile_h).min(roi.b);
            if base_l >= base_r || base_t >= base_b {
                continue;
            }

            let tile_roi = ROI {
                l: base_l.saturating_sub(overlap_pix),
                t: base_t.saturating_sub(overlap_pix),
                r: (base_r + overlap_pix).min(roi.r),
                b: (base_b + overlap_pix).min(roi.b),
            };

            tile_rois.push(tile_roi);
        }
    }

    let tile_n: usize = tile_rois.len();

    let all_toolpaths = carve_rois_in_pool(Arc::clone(&comp_desc), roi, tile_rois, ppi, n_workers);

    let mut toolpaths_by_tool_i = regroup_toolpaths_by_tool(all_toolpaths);

    // Add traverse moves after merging, so transitions can span tile boundaries.
    let mut base_im = Lum16Im::new(roi.w(), roi.h());
    base_im.arr.fill(bulk_top_thou.0 as u16);
    let mut sim_im_for_traverse = base_im.clone();

    let n_total_toolpaths: usize = toolpaths_by_tool_i.values().map(|tps| tps.len()).sum();

    // Deterministic tool processing order (largest diameter first, then tool_i).
    let mut tools: Vec<(usize, usize)> = toolpaths_by_tool_i
        .iter()
        .map(|(&tool_i, toolpaths)| {
            let tool_dia_pix = toolpaths
                .first()
                .map(|tp| tp.tool_dia_pix)
                .expect("toolpaths should not be empty for tool_i");
            (tool_dia_pix, tool_i)
        })
        .collect();
    tools.sort_by(|(dia_a, tool_a), (dia_b, tool_b)| dia_b.cmp(dia_a).then_with(|| tool_a.cmp(tool_b)));

    // Each toolpath (except the last for a tool) has a traverse after it.
    // Total entries = sum_k (toolpaths_k + traverses_k) = sum_k (2*toolpaths_k - 1).
    let mut all_toolpaths = Vec::with_capacity(n_total_toolpaths * 2);
    for (tool_dia_pix, tool_i) in tools {
        let tool_desc = comp_desc
            .tool_descs
            .get(tool_i)
            .unwrap_or_else(|| panic!("tool_i {tool_i} out of range for tool_descs (len={})", comp_desc.tool_descs.len()));
        let tool_guid = tool_desc.guid.to_string();
        let tool_dia_inch = tool_dia_inch(tool_desc);
        let safe_tool_guid: String = tool_guid
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
            .collect();

        let mut toolpaths = toolpaths_by_tool_i
            .remove(&tool_i)
            .expect("tool_i should exist in map");

        let traverse_toolpaths = toolpath::add_traverse_toolpaths_one_tool(
            &mut sim_im_for_traverse,
            &mut toolpaths,
            tool_i,
            tool_dia_pix,
        );
        
        assert_eq!(toolpaths.len(), traverse_toolpaths.len());

        // Interleave: toolpath0, traverse0, toolpath1, traverse1, ..., toolpathN.
        let mut toolpaths_iter = toolpaths.into_iter();
        let mut traverses_iter = traverse_toolpaths.into_iter();
        let mut json_toolpaths: Vec<ToolpathOut> = Vec::new();
        while let Some(tp) = toolpaths_iter.next() {
            json_toolpaths.push(toolpath_to_toolpath_out(&tp));
            all_toolpaths.push(tp);
            if let Some(trav) = traverses_iter.next() {
                json_toolpaths.push(toolpath_to_toolpath_out(&trav));
                all_toolpaths.push(trav);
            }
        }

        // Save JSON toolpaths per tool for debugging
        let out_dir = std::path::Path::new("target/toolpaths");
        fs::create_dir_all(out_dir).expect("failed to create target/toolpaths");
        let out_path = out_dir.join(format!("tool_{tool_i}_{safe_tool_guid}.json"));
        let out = SingleToolOut {
            tool_guid,
            tool_i,
            tool_dia_pix,
            tool_dia_inch,
            ppi,
            tile_n,
            toolpaths: json_toolpaths,
        };
        let f = std::fs::File::create(&out_path)
            .unwrap_or_else(|e| panic!("failed to create {}: {e}", out_path.display()));
        let w = BufWriter::new(f);
        serde_json::to_writer_pretty(w, &out)
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", out_path.display()));

        // Save G-code per tool for debugging/visualization.
        // let gcode_dir = std::path::Path::new("target/gcode");
        // fs::create_dir_all(out_dir).expect("failed to create target/gcode");
        let gcode_path = out_dir.join(format!("tool_{tool_i}_{safe_tool_guid}.nc"));
        let gcode = to_gcode(&out);
        fs::write(&gcode_path, gcode)
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", gcode_path.display()));
    }

    println!("elapsed before movie: {:.3}s", t0.elapsed().as_secs_f64());

    debug_ui::add_toolpath_movie("sim toolpath movie", &base_im, &all_toolpaths);
    debug_ui::show();
}
