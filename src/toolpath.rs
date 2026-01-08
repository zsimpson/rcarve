use crate::desc::Thou;
use crate::dilate_im::im_dilate;
use crate::im::MaskIm;
use crate::im::label::{LabelInfo, ROI};
use crate::region_tree::{CutBand, PlyIm, RegionIm, RegionNode, RegionRoot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IV3 {
    pub x: i32, // Pixels
    pub y: i32, // Pixels
    pub z: i32, // Thou
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPath {
    pub points: Vec<IV3>,
    pub tool_dia_pix: usize,
}

/// Given a cut mask image (1-channel, 8-bit), generate raster tool paths
/// that cover all the 'on' pixels in the mask. Starting at the top-left of the ROI,
/// raster left-to-right, creating a tool path for a contiguous run of 'on' pixels.
/// When an 'off' pixel is hit, end the current tool path (if any) and continue scanning.
/// Then step down by step_size_pix and repeat until the entire ROI is covered.
/// Each tool path is represented as a series of V3 points (X,Y,Z) where X,Y are in pixesls and
/// Z is the tool height (in Thou).
fn create_raster_surface_tool_paths_from_cut_mask(
    cut_mask_im: &MaskIm,
    roi: &ROI,
    tool_dia_pix: usize,
    tool_step_pix: usize,
    z_thou: Thou,
    // TODO: add orientation
) -> Vec<ToolPath> {
    let w = cut_mask_im.w;
    let h = cut_mask_im.h;
    if w == 0 || h == 0 {
        return Vec::new();
    }

    // Clamp ROI to image bounds (ROI right/bottom are exclusive).
    let mut l = roi.l.min(w);
    let mut t = roi.t.min(h);
    let mut r = roi.r.min(w);
    let mut b = roi.b.min(h);
    if l >= r || t >= b {
        return Vec::new();
    }

    // Ensure we never generate tool-center positions that would place the tool outside the image.
    let rad = (tool_dia_pix / 2 as usize)
        .min(w.saturating_sub(1))
        .min(h.saturating_sub(1));
    let max_x_excl = w.saturating_sub(rad);
    let max_y_excl = h.saturating_sub(rad);
    l = l.max(rad);
    t = t.max(rad);
    r = r.min(max_x_excl);
    b = b.min(max_y_excl);
    if l >= r || t >= b {
        return Vec::new();
    }

    let y_step = (tool_step_pix).max(1) as usize;

    let mut paths: Vec<ToolPath> = Vec::new();
    for y in (t..b).step_by(y_step) {
        let row = y * cut_mask_im.s;

        let mut run_start_x: Option<usize> = None;
        for x in l..r {
            let v = cut_mask_im.arr[row + x];
            if v != 0 {
                if run_start_x.is_none() {
                    run_start_x = Some(x);
                }
            } else if let Some(sx) = run_start_x.take() {
                let ex = x.saturating_sub(1);
                paths.push(ToolPath {
                    points: vec![
                        IV3 {
                            x: sx as i32,
                            y: y as i32,
                            z: z_thou.0,
                        },
                        IV3 {
                            x: ex as i32,
                            y: y as i32,
                            z: z_thou.0,
                        },
                    ],
                    tool_dia_pix
                });
            }
        }

        // Flush a run that reaches the scanline end.
        if let Some(sx) = run_start_x.take() {
            let ex = r.saturating_sub(1);
            paths.push(ToolPath {
                points: vec![
                    IV3 {
                        x: sx as i32,
                        y: y as i32,
                        z: z_thou.0,
                    },
                    IV3 {
                        x: ex as i32,
                        y: y as i32,
                        z: z_thou.0,
                    },
                ],
                tool_dia_pix,
            });
        }
    }

    paths
}

/// Given a RegionNode tree root, we traverse the tree and rasterize each node's regions
/// into a pixel image.
/// There's two working MaskIms:
///  * One is the curr_node_mask_im which holds the pixels of the current node. We copy it from the LabelInfo.pixel_iz,
///    then dilate it.
///  * The other is the above_mask. For that we expand the ROI by the tool_radius
///    and then copy any pixel above the current threshold inside that ROI into
///    the above mask. Then we dilate that as well and then we subtract the above_mask
///    from the curr_node_mask_im.
/// Then we convert these masks into clearing-paths by traversing the mask
/// and build a RLE representation of the mask along the standard scanlines.

pub fn create_surface_toolpaths_from_region_tree(
    region_root: &RegionRoot,
    cut_bands: &[CutBand],
    tool_dia_pix: usize,
    step_size_pix: usize,
    ply_im: &PlyIm,
    region_im: &RegionIm,
    region_infos: &[LabelInfo],
    mut on_region_masks: Option<
        &mut dyn FnMut(&RegionNode, (usize, usize, usize, usize), &MaskIm, &MaskIm, &MaskIm),
    >,
) -> Vec<ToolPath> {
    let w = region_im.w;
    let h = region_im.h;
    let mut cut_mask_im = MaskIm::new(w, h);
    let mut above_mask_im = MaskIm::new(w, h);
    let mut dil_above_mask_im = MaskIm::new(w, h);

    let mut paths: Vec<ToolPath> = Vec::new();

    // Recurse through the region tree
    fn recurse_region_tree(
        node: &RegionNode,
        cut_bands: &[CutBand],
        cut_mask_im: &mut MaskIm,
        above_mask_im: &mut MaskIm,
        dil_abv_mask_im: &mut MaskIm,
        tool_dia_pix: usize,
        step_size_pix: usize,
        ply_im: &PlyIm,
        region_infos: &[LabelInfo],
        _paths: &mut Vec<ToolPath>,
        on_region_masks: &mut Option<
            &mut dyn FnMut(&RegionNode, (usize, usize, usize, usize), &MaskIm, &MaskIm, &MaskIm),
        >,
    ) {
        match node {
            RegionNode::Floor { children, .. } => {
                for child in children {
                    recurse_region_tree(
                        child,
                        cut_bands,
                        cut_mask_im,
                        above_mask_im,
                        dil_abv_mask_im,
                        tool_dia_pix,
                        step_size_pix,
                        ply_im,
                        region_infos,
                        _paths,
                        on_region_masks,
                    );
                }
            }
            RegionNode::Cut {
                band_i,
                cut_plane_i,
                region_i,
            } => {
                let z_thou: Thou = cut_bands
                    .get(*band_i)
                    .and_then(|b| b.cut_planes.get(*cut_plane_i))
                    .map(|cp| cp.top_thou)
                    .unwrap_or(Thou(0));

                // Fill the two working images with 0.

                // TODO: Optimze by clearing on the ROI after the fact
                cut_mask_im.arr.fill(0);
                above_mask_im.arr.fill(0);
                dil_abv_mask_im.arr.fill(0);

                // Splat in the pixels for this region label.
                let label_i = region_i.0 as usize;
                if label_i == 0 || label_i >= region_infos.len() {
                    return;
                }
                let label_info = &region_infos[label_i];

                // Determine the ply threshold for this region.
                // Higher ply indices are higher Z (more material above).
                let start_i = label_info.start_y * ply_im.s + label_info.start_x;
                if start_i >= ply_im.arr.len() {
                    return;
                }
                let curr_ply_i = ply_im.arr[start_i];

                for &pix_i in &label_info.pixel_iz {
                    if pix_i < cut_mask_im.arr.len() {
                        cut_mask_im.arr[pix_i] = 255;
                    }
                }

                // Dilate the current region into tool-centerable space.
                im_dilate(cut_mask_im, dil_abv_mask_im, tool_dia_pix);
                std::mem::swap(cut_mask_im, dil_abv_mask_im);

                // Extract the above mask by expanding the ROI and copying any ply pixels that
                // are above the current region's ply threshold.
                let pad = tool_dia_pix;
                let l = label_info.roi.l.saturating_sub(pad);
                let t = label_info.roi.t.saturating_sub(pad);
                let r = (label_info.roi.r + pad).min(ply_im.w);
                let b = (label_info.roi.b + pad).min(ply_im.h);

                for y in t..b {
                    let row = y * ply_im.s;
                    for x in l..r {
                        let i = row + x;
                        if ply_im.arr[i] > curr_ply_i {
                            above_mask_im.arr[i] = 255;
                        }
                    }
                }

                // Dilate the above mask and subtract it from the current region mask.
                im_dilate(above_mask_im, dil_abv_mask_im, tool_dia_pix);
                for i in 0..cut_mask_im.arr.len() {
                    if dil_abv_mask_im.arr[i] > 0 {
                        cut_mask_im.arr[i] = 0;
                    }
                }

                let toolpaths = create_raster_surface_tool_paths_from_cut_mask(
                    cut_mask_im,
                    &ROI { l, t, r, b },
                    tool_dia_pix,
                    step_size_pix,
                    z_thou,
                );
                _paths.extend(toolpaths);

                // Optional debug/testing hook: after computing masks for a cut leaf.
                if let Some(cb) = on_region_masks.as_mut() {
                    (**cb)(
                        node,
                        (l, t, r, b),
                        cut_mask_im,
                        above_mask_im,
                        dil_abv_mask_im,
                    );
                }
            }
        }
    }

    for child in &region_root.children {
        recurse_region_tree(
            child,
            cut_bands,
            &mut cut_mask_im,
            &mut above_mask_im,
            &mut dil_above_mask_im,
            tool_dia_pix,
            step_size_pix,
            ply_im,
            region_infos,
            &mut paths,
            &mut on_region_masks,
        );
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::im::label::label_im;
    use crate::region_tree::{create_cut_bands, create_region_tree};
    use crate::test_helpers::{
        im_u16_to_ascii, mask_to_ascii, ply_im_from_ascii, stub_band_desc, stub_ply_desc,
        toolpaths_to_ascii,
    };

    fn count_cut_leaves(node: &crate::region_tree::RegionNode) -> usize {
        match node {
            crate::region_tree::RegionNode::Cut { .. } => 1,
            crate::region_tree::RegionNode::Floor { children, .. } => {
                children.iter().map(count_cut_leaves).sum()
            }
        }
    }

    #[test]
    fn surface_tool_path_generation_smoke_test() {
        // Build a non-trivial region tree (must contain Cut leaves) and ensure
        // toolpath generation runs without panicking.

        let ply_im = ply_im_from_ascii(
            r#"
                11111
                12221
                12321
                12221
                11111
            "#,
        );

        // Dummy + 3 real plies (values 1,2,3). We only need enough info to build cut bands/tree.
        let ply_descs = vec![
            stub_ply_desc("dummy", 0, true),
            stub_ply_desc("ply100", 100, false),
            stub_ply_desc("ply200", 200, false),
            stub_ply_desc("ply300", 300, false),
        ];

        let band_descs = vec![stub_band_desc(400, 0, "rough")];

        let (region_im_raw, region_infos) = label_im(&ply_im);
        let region_im: RegionIm = region_im_raw.retag::<crate::region_tree::RegionI>();

        let cut_bands = create_cut_bands(
            "rough",
            &ply_im,
            &band_descs,
            &region_im,
            &region_infos,
            &ply_descs,
        );

        let region_root = create_region_tree(&cut_bands, &region_infos);
        let total_cut_leaves: usize = region_root.children.iter().map(count_cut_leaves).sum();
        assert!(total_cut_leaves > 0, "test setup must produce cut leaves");

        let tool_dia_pix = 2_usize;
        let tool_step_pix = 1_usize;
        let paths = create_surface_toolpaths_from_region_tree(
            &region_root,
            &cut_bands,
            tool_dia_pix,
            tool_step_pix,
            &ply_im,
            &region_im,
            &region_infos,
            None,
        );

        assert!(!paths.is_empty(), "expected non-empty raster toolpaths");
        assert!(
            paths.iter().all(|p| p.points.len() >= 2),
            "each toolpath should have at least a start and end point"
        );
        assert!(
            paths
                .iter()
                .all(|p| p.points.iter().all(|pt| matches!(pt.z, 100 | 200 | 300))),
            "surface raster z should come from cut plane top_thou"
        );
    }

    #[test]
    fn raster_surface_toolpaths_basic_runs() {
        let mut m = MaskIm::new(6, 3);

        // y=0: ..###.
        for x in 2..5 {
            m.arr[0 * m.s + x] = 255;
        }

        // y=1: #.#..#
        m.arr[1 * m.s + 0] = 255;
        m.arr[1 * m.s + 2] = 255;
        m.arr[1 * m.s + 5] = 255;

        // y=2: (empty)

        let roi = ROI {
            l: 0,
            t: 0,
            r: 6,
            b: 3,
        };
        let paths = create_raster_surface_tool_paths_from_cut_mask(&m, &roi, 0, 1, Thou(123));

        // Expect 1 run on y=0 and 3 runs on y=1.
        assert_eq!(paths.len(), 4);

        assert_eq!(paths[0].points[0], IV3 { x: 2, y: 0, z: 123 });
        assert_eq!(paths[0].points[1], IV3 { x: 4, y: 0, z: 123 });

        assert_eq!(paths[1].points[0], IV3 { x: 0, y: 1, z: 123 });
        assert_eq!(paths[1].points[1], IV3 { x: 0, y: 1, z: 123 });

        assert_eq!(paths[2].points[0], IV3 { x: 2, y: 1, z: 123 });
        assert_eq!(paths[2].points[1], IV3 { x: 2, y: 1, z: 123 });

        assert_eq!(paths[3].points[0], IV3 { x: 5, y: 1, z: 123 });
        assert_eq!(paths[3].points[1], IV3 { x: 5, y: 1, z: 123 });
    }

    // #[test]
    // fn raster_surface_toolpaths_respects_step_and_radius() {
    //     let mut m = MaskIm::new(5, 5);

    //     // Fill a vertical line at x=0 for all rows; radius=1 should clamp it out.
    //     for y in 0..5 {
    //         m.arr[y * m.s + 0] = 255;
    //     }

    //     // Add a horizontal run at y=2 inside the safe region.
    //     for x in 1..4 {
    //         m.arr[2 * m.s + x] = 255;
    //     }

    //     let roi = ROI {
    //         l: 0,
    //         t: 0,
    //         r: 5,
    //         b: 5,
    //     };
    //     let paths = create_raster_surface_tool_paths_from_cut_mask(&m, &roi, 1, 2, Thou(50));

    //     // step=2 visits y=1,3 if clamped, but with radius=1 we scan y in [1,4) => y=1,3.
    //     // Our horizontal run is at y=2 and should be skipped; and x=0 is clamped out by radius.
    //     assert!(paths.is_empty());

    //     // Now step=1: y=2 is visited, but x=0 is still clamped out.
    //     let paths2 = create_raster_surface_tool_paths_from_cut_mask(&m, &roi, 1, 1, Thou(50));
    //     assert_eq!(paths2.len(), 1);
    //     assert_eq!(paths2[0].points[0], IV3 { x: 1, y: 2, z: 50 });
    //     assert_eq!(paths2[0].points[1], IV3 { x: 3, y: 2, z: 50 });
    // }

    #[test]
    fn surface_tool_path_generation_dump_better_image() {
        let ply_im = ply_im_from_ascii(
            r#"
                111111111111111111111111111111111111111111
                111111111111111111111111111111111111111111
                111111111111111111111111111111111111111111
                111111144444411111111111111111111111111111
                111111144444433333333333333333331111111111
                111111144444433333333333333333331111111111
                111111111333333333333333333333331111111111
                111111111333222222222222222233331111111111
                111111111333222211111112222233331111111111
                111111111333222211111112222233331111111111
                111111111333222222222222222233331111111111
                111111111333333333333333333333331111111111
                111111111333333333333333333333331111111111
                111111111111111111111111111111111111111111
                111111111111111111111111111111111111111111
                111111111111111111111111111111111111111111
            "#,
        );

        // Dummy + 4 real plies (values 1..4).
        let ply_descs = vec![
            stub_ply_desc("dummy", 0, true),
            stub_ply_desc("ply100", 100, false),
            stub_ply_desc("ply200", 200, false),
            stub_ply_desc("ply300", 300, false),
            stub_ply_desc("ply400", 400, false),
        ];
        let band_descs = vec![
            stub_band_desc(500, 350, "rough"),
            stub_band_desc(350, 0, "rough"),
        ];

        let (region_im_raw, region_infos) = label_im(&ply_im);
        let region_im: RegionIm = region_im_raw.retag::<crate::region_tree::RegionI>();

        let cut_bands = create_cut_bands(
            "rough",
            &ply_im,
            &band_descs,
            &region_im,
            &region_infos,
            &ply_descs,
        );
        let region_root = create_region_tree(&cut_bands, &region_infos);

        // print the z ranges of the cut bands
        for (i, band) in cut_bands.iter().enumerate() {
            let top = band.top_thou.0;
            let bot = band.bot_thou.0;
            println!("Cut band {i}: top_thou={}, bot_thou={}", top, bot);
        }

        let tool_dia_pix = 2_usize;
        let tool_step_pix = 2_usize;

        let mut node_results: Vec<(
            RegionNode,
            (usize, usize, usize, usize),
            MaskIm,
            MaskIm,
            MaskIm,
        )> = Vec::new();
        let snapshot = |src: &MaskIm| {
            let mut dst = MaskIm::new(src.w, src.h);
            dst.arr.copy_from_slice(&src.arr);
            dst
        };
        let mut on_region_masks = |node: &RegionNode,
                                   roi_pad: (usize, usize, usize, usize),
                                   cut_mask_im: &MaskIm,
                                   above_mask_im: &MaskIm,
                                   dil_abv_mask_im: &MaskIm| {
            if matches!(node, RegionNode::Cut { .. }) {
                node_results.push((
                    node.clone(),
                    roi_pad,
                    snapshot(cut_mask_im),
                    snapshot(above_mask_im),
                    snapshot(dil_abv_mask_im),
                ));
            }
        };

        // Primary call under test (should not panic).
        let _paths = create_surface_toolpaths_from_region_tree(
            &region_root,
            &cut_bands,
            tool_dia_pix,
            tool_step_pix,
            &ply_im,
            &region_im,
            &region_infos,
            Some(&mut on_region_masks),
        );

        // Dump ascii maps for visual inspection.
        println!("ply_im:\n{}", im_u16_to_ascii(&ply_im));
        println!("region_im:\n{}", im_u16_to_ascii(&region_im));

        assert_eq!(
            node_results.len(),
            region_root
                .children
                .iter()
                .map(count_cut_leaves)
                .sum::<usize>(),
            "expected one callback per cut leaf"
        );

        for (i, (region_node, (l, t, r, b), cut_m, above_m, dil_abv_m)) in
            node_results.iter().enumerate()
        {
            // Derive the ply value for this cut leaf via the region label's start_x/start_y.
            let (label_i, curr_ply_i) = match region_node {
                RegionNode::Cut { region_i, .. } => {
                    let label_i = region_i.0 as usize;
                    if label_i == 0 || label_i >= region_infos.len() {
                        (label_i, None)
                    } else {
                        let label_info = &region_infos[label_i];
                        let start_i = label_info.start_y * ply_im.s + label_info.start_x;
                        let curr_ply_v = ply_im.arr.get(start_i).copied();
                        (label_i, curr_ply_v)
                    }
                }
                _ => (0, None),
            };

            let mut at_mask = MaskIm::new(ply_im.w, ply_im.h);
            if label_i != 0 && label_i < region_infos.len() {
                let label_info = &region_infos[label_i];
                for &pix_i in &label_info.pixel_iz {
                    if pix_i < at_mask.arr.len() {
                        at_mask.arr[pix_i] = 255;
                    }
                }
            }

            println!(
                "--- Cut leaf {i} masks --------------------------------------------------------"
            );
            println!("cut[{i}] region_node: {:?}", region_node);
            println!("cut[{i}] label_i: {label_i}, curr_ply_i: {:?}", curr_ply_i);
            // Outline the padded ROI actually used for above-mask extraction.
            let roi_pad = ROI {
                l: *l,
                t: *t,
                r: *r,
                b: *b,
            };
            let roi_opt = Some(&roi_pad);

            println!(
                "cut[{i}] at_mask (label pixels):\n{}",
                mask_to_ascii(&at_mask, None)
            );
            println!("cut[{i}] above_mask:\n{}", mask_to_ascii(above_m, roi_opt));
            println!("cut[{i}] dil_abv_mask:\n{}", mask_to_ascii(dil_abv_m, None));
            println!("cut[{i}] cut_mask:\n{}", mask_to_ascii(cut_m, None));
        }

        // Print the paths grouped by their exact Z value.
        let mut paths_by_z: std::collections::BTreeMap<i32, Vec<ToolPath>> =
            std::collections::BTreeMap::new();

        for path in &_paths {
            let z = path.points.first().map(|p| p.z).unwrap_or(0);
            paths_by_z.entry(z).or_default().push(path.clone());
        }

        println!("--- Raster toolpaths by Z ---------------------------------------------------");

        for (&z, z_paths) in paths_by_z.iter().rev() {
            println!(
                "paths (rasterized) z={}:\n{}",
                z,
                toolpaths_to_ascii(z_paths, ply_im.w, ply_im.h)
            );
        }
    }
}
