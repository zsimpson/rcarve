#[allow(unused_imports)]
use crate::debug_ui;

use crate::desc::Thou;
use crate::dilate_im::im_dilate;
use crate::im::label::{LabelInfo, ROI};
use crate::im::{Im, MaskIm};
use crate::region_tree::{CutBand, PlyIm, RegionI, RegionIm, RegionNode, RegionRoot};
use crate::trace::{Contour, contours_by_suzuki_abe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IV3 {
    pub x: i32, // Pixels
    pub y: i32, // Pixels
    pub z: i32, // Thou
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CutPixels {
    pub pixels_changed: u64,
    pub depth_sum_thou: u64,
}

impl CutPixels {
    #[inline]
    pub fn add_pixel_change(&mut self, old_z: u16, new_z: u16) {
        debug_assert!(new_z <= old_z);
        if new_z < old_z {
            self.pixels_changed += 1;
            self.depth_sum_thou += (old_z - new_z) as u64;
        }
    }

    #[inline]
    pub fn merge(&mut self, other: CutPixels) {
        self.pixels_changed += other.pixels_changed;
        self.depth_sum_thou += other.depth_sum_thou;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPath {
    pub points: Vec<IV3>,
    pub closed: bool,
    pub tool_dia_pix: usize,
    pub tool_i: usize,
    pub tree_node_id: usize,
    pub cuts: CutPixels,
}

fn create_perimeter_tool_paths(
    contour: &Contour,
    target_z_thou: Thou,
    tool_i: usize,
    tool_dia_pix: usize,
    tree_node_id: usize,
) -> Vec<ToolPath> {
    let z = target_z_thou.0;
    let mut points: Vec<IV3> = Vec::with_capacity(contour.points.len());
    for &pt in &contour.points {
        points.push(IV3 {
            x: pt.x as i32,
            y: pt.y as i32,
            z,
        });
    }

    vec![ToolPath {
        points,
        closed: true,
        tool_dia_pix,
        tool_i,
        tree_node_id,
        cuts: CutPixels::default(),
    }]
}
// fn repeat_toolpaths(
//     base_toolpaths: Vec<ToolPath>,
//     target_z_thou: Thou,
//     parent_z_thou: Thou,
//     z_step_thou: Thou,
// ) -> Vec<ToolPath> {
//     if base_toolpaths.is_empty() {
//         return base_toolpaths;
//     }

//     let target_z = target_z_thou.0;
//     let step = z_step_thou.0.max(1);
//     let mut z = parent_z_thou.0.max(target_z);
//     let next_z = z.saturating_sub(step);
//     z = if next_z < target_z { target_z } else { next_z };

//     // If parent is already at/below the target, nothing to expand.
//     if z <= target_z {
//         return base_toolpaths;
//     }

//     // Create copies at intermediate Zs, then append the originals (which are at target Z).
//     let mut out: Vec<ToolPath> = Vec::new();
//     while z > target_z {
//         for tp in &base_toolpaths {
//             let mut tp2 = tp.clone();
//             for pt in &mut tp2.points {
//                 pt.z = z;
//             }
//             out.push(tp2);
//         }

//         let next_z = z.saturating_sub(step);
//         z = if next_z < target_z { target_z } else { next_z };
//     }

//     out.extend(base_toolpaths);
//     out
// }

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
    tool_i: usize,
    tool_dia_pix: usize,
    tool_step_pix: usize,
    z_thou: Thou,
    tree_node_id: usize,
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
                    closed: false,
                    tool_dia_pix,
                    tool_i,
                    tree_node_id,
                    cuts: CutPixels::default(),
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
                closed: false,
                tool_dia_pix,
                tool_i,
                tree_node_id,
                cuts: CutPixels::default(),
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

pub fn create_toolpaths_from_region_tree(
    name: &str,
    region_root: &RegionRoot,
    cut_bands: &[CutBand],
    tool_i: usize,
    tool_dia_pix: usize,
    step_size_pix: usize,
    margin_pix: usize,
    pride_thou: Thou,
    ply_im: &PlyIm,
    region_im: &RegionIm,
    region_infos: &[LabelInfo],
    n_perimeters: usize,
    perimeter_step_size_pix: usize,
    gen_surfaces: bool,
    mut on_region_masks: Option<&mut dyn FnMut(&RegionNode, &ROI, &MaskIm, &MaskIm, &MaskIm)>,
    // bulk_z_thou: Thou,
) -> Vec<ToolPath> {
    let w = region_im.w;
    let h = region_im.h;
    let mut cut_mask_im = MaskIm::new(w, h);
    let mut above_mask_im = MaskIm::new(w, h);
    let mut dil_above_mask_im = MaskIm::new(w, h);
    let mut dil_cut_mask_im = MaskIm::new(w, h);

    let mut paths: Vec<ToolPath> = Vec::new();

    fn splat_region_i_into_mask_im(
        region_i: RegionI,
        region_infos: &[LabelInfo],
        mask_im: &mut MaskIm,
    ) {
        let label_i = region_i.0 as usize;
        if label_i == 0 || label_i >= region_infos.len() {
            return;
        }
        let label_info = &region_infos[label_i];
        for &pix_i in &label_info.pixel_iz {
            if pix_i < mask_im.arr.len() {
                mask_im.arr[pix_i] = 255;
            }
        }
    }

    // Recurse through the region tree
    fn recurse_region_tree(
        name: &str,
        node: &RegionNode,
        cut_bands: &[CutBand],
        cut_mask_im: &mut MaskIm,
        above_mask_im: &mut MaskIm,
        dil_abv_mask_im: &mut MaskIm,
        dil_cut_mask_im: &mut MaskIm,
        tool_i: usize,
        tool_dia_pix: usize,
        step_size_pix: usize,
        margin_pix: usize,
        pride_thou: Thou,
        ply_im: &PlyIm,
        region_infos: &[LabelInfo],
        paths: &mut Vec<ToolPath>,
        n_perimeters: usize,
        perimeter_step_size_pix: usize,
        gen_surfaces: bool,
        on_region_masks: &mut Option<&mut dyn FnMut(&RegionNode, &ROI, &MaskIm, &MaskIm, &MaskIm)>,
        // parent_z_thou: Thou,
    ) {
        // TODO: Optimze by clearing on the ROI after the fact
        cut_mask_im.arr.fill(0);
        above_mask_im.arr.fill(0);
        dil_abv_mask_im.arr.fill(0);
        dil_cut_mask_im.arr.fill(0);

        let mut roi: ROI = ROI {
            l: 0_usize,
            t: 0_usize,
            r: 0_usize,
            b: 0_usize,
        };
        let curr_ply_i_u16: u16;
        let z_thou: Thou;

        let _is_node_floor = matches!(node, RegionNode::Floor { .. });

        // NOTE: `im_dilate` takes a *diameter* in pixels, but for toolpath planning we usually
        // think in terms of an expansion *radius*.
        let tool_rad_pix = tool_dia_pix / 2;
        let base_rad_pix = tool_rad_pix + margin_pix;

        // Splat in the current node's regions. For floors there is 1+, for cuts there is 1.
        // and find the ROI
        match node {
            RegionNode::Floor {
                region_iz,
                loweset_ply_i_in_band,
                bottom_thou,
                ..
            } => {
                for region_i in region_iz {
                    splat_region_i_into_mask_im(*region_i, region_infos, cut_mask_im);

                    let label_i = region_i.0 as usize;
                    assert!(label_i < region_infos.len());
                    let label_info = &region_infos[label_i];
                    roi.union(label_info.roi);
                }
                curr_ply_i_u16 = (loweset_ply_i_in_band.0 as u16).saturating_sub(1); // Floor uses ply below the lowest in band
                z_thou = *bottom_thou;
            }
            RegionNode::Cut {
                band_i: _,
                cut_plane_i: _,
                region_i,
                z_thou: node_z_thou,
                ..
            } => {
                z_thou = node_z_thou.clone();

                splat_region_i_into_mask_im(*region_i, region_infos, cut_mask_im);

                let label_i = region_i.0 as usize;
                assert!(label_i < region_infos.len());
                let label_info = &region_infos[label_i];
                roi.union(label_info.roi);

                curr_ply_i_u16 =
                    ply_im.get_or_default(label_info.start_x, label_info.start_y, 0, 0);
            }
        }

        // Build the above_mask_im by expanding the ROI and copying any ply pixels that
        // are above the current region's ply threshold.
        // Recall that ply_im is sorted form the bottom; higher ply indices have higher values.
        //
        // Expand by the maximum radius we will use across perimeter passes so the subtraction is
        // correct for all offsets.
        let n_dilation_passes = n_perimeters.max(1);
        let max_rad_pix = base_rad_pix
            .saturating_add(perimeter_step_size_pix.saturating_mul(n_dilation_passes.saturating_sub(1)));
        let padded_roi = roi.padded(max_rad_pix, ply_im.w, ply_im.h);
        for y in padded_roi.t..padded_roi.b {
            let row = y * ply_im.s;
            for x in padded_roi.l..padded_roi.r {
                let i = row + x;
                if ply_im.arr[i] > curr_ply_i_u16 {
                    above_mask_im.arr[i] = 255;
                }
            }
        }

        // Add a one-pixel border around the above mask to ensure the edges are excluded from the cut.
        let s = above_mask_im.s;
        let w_minus_1 = ply_im.w.saturating_sub(1);
        let h_minus_1_mul_s = ply_im.h.saturating_sub(1) * s;
        for y in padded_roi.t..padded_roi.b {
            above_mask_im.arr[y * s + 0] = 255;
            above_mask_im.arr[y * s + w_minus_1] = 255;
        }
        for x in padded_roi.l..padded_roi.r {
            above_mask_im.arr[x] = 255;
            above_mask_im.arr[h_minus_1_mul_s + x] = 255;
        }

        // debug_ui::add_mask_im(
        //     &format!("region_above_mask={} is_floor={}", z_thou.0, is_node_floor),
        //     above_mask_im,
        // );

        // Each perimeter pass uses a larger dilation radius.
        for dilation_i in 0..n_dilation_passes {
            let rad_pix = base_rad_pix.saturating_add(perimeter_step_size_pix.saturating_mul(dilation_i));

            // Convert radius -> diameter for `im_dilate` (which uses `radius = dia/2`).
            // `2*rad+1` ensures each +1 in radius always changes the dilation.
            let max_dia_pix = ply_im.w.min(ply_im.h).max(1);
            let dia_pix = rad_pix
                .saturating_mul(2)
                .saturating_add(1)
                .min(max_dia_pix);

            // Dilate the above mask to the same radius as the current cut mask.
            im_dilate(above_mask_im, dil_abv_mask_im, dia_pix);

            // Apply the pride offset at cut time (not the region-plane time).
            let cut_z_thou = Thou(z_thou.0.saturating_add(pride_thou.0));

            // debug_ui::add_mask_im(
            //     &format!("{} cut_mask_im before={}", name, cut_z_thou.0),
            //     cut_mask_im,
            // );

            // Dilate the current region into tool-centerable space.
            im_dilate(cut_mask_im, dil_cut_mask_im, dia_pix);

            // Swap because I was just using dil_cut_mask_im as a placeholder.
            // and I really want that into the cut_mask_im.
            // std::mem::swap(cut_mask_im, dil_cut_mask_im);

            // debug_ui::add_mask_im(
            //     &format!("{} dil_cut_mask_im before={} dilation_i={}", name, cut_z_thou.0, dilation_i),
            //     dil_cut_mask_im,
            // );
            
            // Subtract dilation above from cut_mask.
            // TODO: Optimize by limiting the dilation to the padded ROI.
            for y in padded_roi.t..padded_roi.b {
                let row = y * ply_im.s;
                for x in padded_roi.l..padded_roi.r {
                    let i = row + x;
                    if dil_abv_mask_im.arr[i] > 0 {
                        dil_cut_mask_im.arr[i] = 0;
                    }
                }
            }

            // debug_ui::add_mask_im(
            //     &format!("{} dil_cut_mask_im after={} dilation_i={}", name, cut_z_thou.0, dilation_i),
            //     dil_cut_mask_im,
            // );

            let mut node_toolpaths: Vec<ToolPath> = Vec::new();

            if gen_surfaces && dilation_i == 0 {
                let toolpaths = create_raster_surface_tool_paths_from_cut_mask(
                    dil_cut_mask_im,
                    &padded_roi,
                    tool_i,
                    tool_dia_pix,
                    step_size_pix,
                    cut_z_thou,
                    node.get_id(),
                );
                node_toolpaths.extend(toolpaths);
            }

            if n_perimeters > 0 {
                // Suzuki–Abe operates on a 1-channel i32 image and mutates it in-place.
                // TODO: Consider a refactor to generate the masks as i32 directly.
                // TODO: Move this allocation out of the inner loop.
                let mut cut_mask_im_i32 = Im::<i32, 1>::new(cut_mask_im.w, cut_mask_im.h);
                for (dst, &src) in cut_mask_im_i32.arr.iter_mut().zip(dil_cut_mask_im.arr.iter()) {
                    *dst = if src != 0 { 1 } else { 0 };
                }

                let tolerance = 1.0; // TODO
                let contours = contours_by_suzuki_abe(&mut cut_mask_im_i32);
                for contour in contours {
                    let simp_contour = contour.simplify_by_rdp(tolerance);
                    let toolpaths = create_perimeter_tool_paths(
                        &simp_contour,
                        cut_z_thou,
                        tool_i,
                        tool_dia_pix,
                        node.get_id(),
                    );
                    node_toolpaths.extend(toolpaths);
                }

            }

            // After generating surface + perimeter toolpaths at the target Z, add repeated passes
            // at intermediate Z steps down from the parent plane.
            // let z_step_thou = Thou(50);
            // let node_toolpaths = repeat_toolpaths(node_toolpaths, z_thou, parent_z_thou, z_step_thou);
            paths.extend(node_toolpaths);
        }

        // Optional debug/testing hook: after computing masks for a cut leaf.
        if let Some(cb) = on_region_masks.as_mut() {
            (**cb)(
                node,
                &padded_roi,
                cut_mask_im,
                above_mask_im,
                dil_abv_mask_im,
            );
        }

        match node {
            RegionNode::Floor { children, .. } => {
                for child in children {
                    recurse_region_tree(
                        name,
                        child,
                        cut_bands,
                        cut_mask_im,
                        above_mask_im,
                        dil_abv_mask_im,
                        dil_cut_mask_im,
                        tool_i,
                        tool_dia_pix,
                        step_size_pix,
                        margin_pix,
                        pride_thou,
                        ply_im,
                        region_infos,
                        paths,
                        n_perimeters,
                        perimeter_step_size_pix,
                        gen_surfaces,
                        on_region_masks,
                    );
                }
            }
            RegionNode::Cut { .. } => {}
        }
    }

    for child in region_root.children() {
        recurse_region_tree(
            name,
            child,
            cut_bands,
            &mut cut_mask_im,
            &mut above_mask_im,
            &mut dil_above_mask_im,
            &mut dil_cut_mask_im,
            tool_i,
            tool_dia_pix,
            step_size_pix,
            margin_pix,
            pride_thou,
            ply_im,
            region_infos,
            &mut paths,
            n_perimeters,
            perimeter_step_size_pix,
            gen_surfaces,
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
        let total_cut_leaves: usize = region_root.children().iter().map(count_cut_leaves).sum();
        assert!(total_cut_leaves > 0, "test setup must produce cut leaves");

        let tool_dia_pix = 2_usize;
        let tool_step_pix = 1_usize;
        let paths = create_toolpaths_from_region_tree(
            "test",
            &region_root,
            &cut_bands,
            0,
            tool_dia_pix,
            tool_step_pix,
            0,
            Thou(0),
            &ply_im,
            &region_im,
            &region_infos,
            0,
            1,
            true,
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
        let paths = create_raster_surface_tool_paths_from_cut_mask(&m, &roi, 0, 1, 1, Thou(123), 0);

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

        let mut node_results: Vec<(RegionNode, ROI, MaskIm, MaskIm, MaskIm)> = Vec::new();
        let snapshot = |src: &MaskIm| {
            let mut dst = MaskIm::new(src.w, src.h);
            dst.arr.copy_from_slice(&src.arr);
            dst
        };
        let mut on_region_masks = |node: &RegionNode,
                                   roi_pad: &ROI,
                                   cut_mask_im: &MaskIm,
                                   above_mask_im: &MaskIm,
                                   dil_abv_mask_im: &MaskIm| {
            if matches!(node, RegionNode::Cut { .. }) {
                node_results.push((
                    node.clone(),
                    *roi_pad,
                    snapshot(cut_mask_im),
                    snapshot(above_mask_im),
                    snapshot(dil_abv_mask_im),
                ));
            }
        };

        // Primary call under test (should not panic).
        let _paths = create_toolpaths_from_region_tree(
            "test",
            &region_root,
            &cut_bands,
            0,
            tool_dia_pix,
            tool_step_pix,
            0,
            Thou(0),
            &ply_im,
            &region_im,
            &region_infos,
            0,
            1,
            true,
            Some(&mut on_region_masks),
        );

        // Dump ascii maps for visual inspection.
        println!("ply_im:\n{}", im_u16_to_ascii(&ply_im));
        println!("region_im:\n{}", im_u16_to_ascii(&region_im));

        assert_eq!(
            node_results.len(),
            region_root
                .children()
                .iter()
                .map(count_cut_leaves)
                .sum::<usize>(),
            "expected one callback per cut leaf"
        );

        for (i, (region_node, roi_pad, cut_m, above_m, dil_abv_m)) in
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
            let roi_opt = Some(roi_pad);

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

    #[test]
    fn break_long_toolpaths_does_not_drop_paths() {
        let mut toolpaths = vec![
            ToolPath {
                points: vec![IV3 { x: 0, y: 0, z: 0 }, IV3 { x: 10, y: 0, z: 0 }],
                closed: false,
                tool_dia_pix: 1,
                tool_i: 0,
                tree_node_id: 0,
                cuts: CutPixels::default(),
            },
            ToolPath {
                points: vec![IV3 { x: 5, y: 5, z: 0 }, IV3 { x: 6, y: 6, z: 0 }],
                closed: false,
                tool_dia_pix: 1,
                tool_i: 0,
                tree_node_id: 0,
                cuts: CutPixels::default(),
            },
        ];

        break_long_toolpaths(&mut toolpaths, 1000);
        assert_eq!(toolpaths.len(), 2);
        assert!(toolpaths.iter().all(|tp| tp.points.len() >= 2));
    }

    #[test]
    fn break_long_toolpaths_ignores_z_only_jumps() {
        let mut toolpaths = vec![ToolPath {
            points: vec![
                IV3 { x: 0, y: 0, z: 0 },
                IV3 {
                    x: 0,
                    y: 0,
                    z: 10_000,
                },
            ],
            closed: false,
            tool_dia_pix: 1,
            tool_i: 0,
            tree_node_id: 0,
            cuts: CutPixels::default(),
        }];

        // Even though z jumps, XY distance is 0 so it should not be broken.
        break_long_toolpaths(&mut toolpaths, 1);
        assert_eq!(toolpaths.len(), 1);
        assert_eq!(toolpaths[0].points.len(), 2);
    }

    #[test]
    fn break_long_toolpaths_splits_on_long_mid_segment() {
        let mut toolpaths = vec![ToolPath {
            points: vec![
                IV3 { x: 0, y: 0, z: 0 },
                IV3 { x: 1, y: 0, z: 0 },
                // Big jump in XY from previous point => should trigger a split.
                IV3 { x: 100, y: 0, z: 0 },
            ],
            closed: false,
            tool_dia_pix: 1,
            tool_i: 0,
            tree_node_id: 0,
            cuts: CutPixels::default(),
        }];

        break_long_toolpaths(&mut toolpaths, 10);

        // We should get more segments back (the 1->100 segment subdivides).
        assert!(toolpaths.len() > 2);
        assert!(toolpaths.iter().all(|tp| tp.points.len() == 2));

        // Every segment should now be <= max length in XY.
        for tp in &toolpaths {
            let a = tp.points[0];
            let b = tp.points[1];
            let dx = (a.x - b.x) as i64;
            let dy = (a.y - b.y) as i64;
            let d2 = dx * dx + dy * dy;
            assert!(d2 <= 100, "segment too long: d2={d2}");
        }
    }
}

pub fn break_long_toolpaths(toolpaths: &mut Vec<ToolPath>, max_segment_len_pix: usize) {
    if toolpaths.is_empty() {
        return;
    }

    let max_segment_len_pix = max_segment_len_pix.max(1);
    let max_len2: i64 = (max_segment_len_pix as i64) * (max_segment_len_pix as i64);

    fn dist2_xy(a: &IV3, b: &IV3) -> i64 {
        let dx = (a.x as i64) - (b.x as i64);
        let dy = (a.y as i64) - (b.y as i64);
        dx * dx + dy * dy
    }

    let mut new_toolpaths: Vec<ToolPath> = Vec::new();

    for tp in toolpaths.drain(..) {
        if tp.points.len() < 2 {
            new_toolpaths.push(tp);
            continue;
        }

        let want_closed = tp.closed;

        // Normalize closed loops to a ring without a duplicated closing vertex;
        // we will explicitly handle the closing edge.
        let mut pts: Vec<IV3> = tp.points;
        if want_closed {
            if pts.len() >= 2 && pts.first() == pts.last() {
                pts.pop();
            }
        }

        // If no segment exceeds the max length, keep the original path (preserving `closed`).
        // This avoids converting every toolpath into 2-point segments.
        let mut needs_split = false;
        if pts.len() >= 2 {
            for seg in pts.windows(2) {
                if dist2_xy(&seg[0], &seg[1]) > max_len2 {
                    needs_split = true;
                    break;
                }
            }
            if !needs_split && want_closed {
                let a = *pts.last().unwrap();
                let b = pts[0];
                if dist2_xy(&a, &b) > max_len2 {
                    needs_split = true;
                }
            }
        }

        if !needs_split {
            if pts.len() >= 2 {
                if want_closed {
                    // Re-close explicitly for consumers that expect it.
                    if pts.first() != pts.last() {
                        let first = pts[0];
                        pts.push(first);
                    }
                    new_toolpaths.push(ToolPath {
                        points: pts,
                        closed: true,
                        tool_dia_pix: tp.tool_dia_pix,
                        tool_i: tp.tool_i,
                        tree_node_id: tp.tree_node_id,
                        cuts: CutPixels::default(),
                    });
                } else {
                    new_toolpaths.push(ToolPath {
                        points: pts,
                        closed: false,
                        tool_dia_pix: tp.tool_dia_pix,
                        tool_i: tp.tool_i,
                        tree_node_id: tp.tree_node_id,
                        cuts: CutPixels::default(),
                    });
                }
            }
            continue;
        }

        // Helper to emit one or more <=max segments between a and b.
        let mut emit_subdivided = |a: IV3, b: IV3| {
            let d2 = dist2_xy(&a, &b);
            if d2 <= max_len2 {
                new_toolpaths.push(ToolPath {
                    points: vec![a, b],
                    closed: false,
                    tool_dia_pix: tp.tool_dia_pix,
                    tool_i: tp.tool_i,
                    tree_node_id: tp.tree_node_id,
                    cuts: CutPixels::default(),
                });
                return;
            }

            // Subdivide into N segments so each is <= max_segment_len_pix in XY.
            let dx = (b.x - a.x) as f64;
            let dy = (b.y - a.y) as f64;
            let dist = (dx * dx + dy * dy).sqrt();
            let steps = ((dist / (max_segment_len_pix as f64)).ceil() as usize).max(1);

            let mut prev = a;
            for i in 1..=steps {
                let t = (i as f64) / (steps as f64);
                let x = (a.x as f64 + (b.x - a.x) as f64 * t).round() as i32;
                let y = (a.y as f64 + (b.y - a.y) as f64 * t).round() as i32;
                let z = (a.z as f64 + (b.z - a.z) as f64 * t).round() as i32;
                let next = IV3 { x, y, z };
                if next != prev {
                    new_toolpaths.push(ToolPath {
                        points: vec![prev, next],
                        closed: false,
                        tool_dia_pix: tp.tool_dia_pix,
                        tool_i: tp.tool_i,
                        tree_node_id: tp.tree_node_id,
                        cuts: CutPixels::default(),
                    });
                    prev = next;
                }
            }
        };

        if pts.len() >= 2 {
            for seg in pts.windows(2) {
                emit_subdivided(seg[0], seg[1]);
            }

            // Closing edge for closed paths.
            if want_closed {
                let a = *pts.last().unwrap();
                let b = pts[0];
                emit_subdivided(a, b);
            }
        }
    }

    *toolpaths = new_toolpaths;
}

pub fn sort_tool_paths(toolpaths: &mut Vec<ToolPath>, region_root: &RegionRoot) {
    fn band_i(node: &RegionNode) -> usize {
        match node {
            RegionNode::Floor { band_i, .. } => *band_i,
            RegionNode::Cut { band_i, .. } => *band_i,
        }
    }

    // Tree traversal for cutting order:
    // - Keep sibling ordering as-built (caller said siblings can be any order).
    // - A floor node reveals its children: we visit its subtree immediately after the floor.
    fn build_node_visit_order(region_root: &RegionRoot) -> Vec<usize> {
        fn recurse(nodes: &[RegionNode], out: &mut Vec<usize>) {
            if nodes.is_empty() {
                return;
            }

            // Sibling nodes must all be in the same band.
            let b0 = band_i(&nodes[0]);
            debug_assert!(nodes.iter().all(|n| band_i(n) == b0));
            assert!(nodes.iter().all(|n| band_i(n) == b0));

            for n in nodes {
                out.push(n.get_id());
                if let RegionNode::Floor { children, .. } = n {
                    recurse(children, out);
                }
            }
        }

        let mut order: Vec<usize> = Vec::new();
        recurse(region_root.children(), &mut order);
        order
    }

    fn dist2_xy(a: &IV3, b: &IV3) -> i64 {
        let dx = (a.x as i64) - (b.x as i64);
        let dy = (a.y as i64) - (b.y as i64);
        dx * dx + dy * dy
    }

    fn choose_open_orientation(tp: &mut ToolPath, curr: &IV3) {
        if tp.points.len() <= 1 {
            return;
        }
        let first = tp.points.first().unwrap();
        let last = tp.points.last().unwrap();
        let d_first = dist2_xy(curr, first);
        let d_last = dist2_xy(curr, last);
        if d_last < d_first {
            tp.points.reverse();
        }
    }

    fn roll_closed_to_nearest(tp: &mut ToolPath, curr: &IV3) {
        if tp.points.len() <= 1 {
            return;
        }

        // Many downstream consumers treat a path as "closed" by expecting the last
        // vertex to equal the first (so the final segment is explicit). Rotating a
        // closed polyline that already has a duplicated closing vertex would break
        // that invariant (and appear to "lose" the closing segment).
        //
        // Normalize to a ring without the duplicated last vertex, then re-close at end.
        let had_closing_dup = tp
            .points
            .first()
            .zip(tp.points.last())
            .map(|(a, b)| a == b)
            .unwrap_or(false);
        if had_closing_dup {
            tp.points.pop();
            if tp.points.len() <= 1 {
                // Restore the closure representation.
                let first = tp
                    .points
                    .first()
                    .copied()
                    .unwrap_or(IV3 { x: 0, y: 0, z: 0 });
                tp.points.push(first);
                return;
            }
        }

        let mut best_i = 0usize;
        let mut best_d = dist2_xy(curr, &tp.points[0]);
        for (i, p) in tp.points.iter().enumerate().skip(1) {
            let d = dist2_xy(curr, p);
            if d < best_d {
                best_d = d;
                best_i = i;
            }
        }
        if best_i != 0 {
            tp.points.rotate_left(best_i);
        }

        // Deterministic direction choice at the chosen start: take the direction
        // with the smaller next-step distance (ties deterministic by (y,x,z)).
        if tp.points.len() >= 3 {
            let next = &tp.points[1];
            let prev = &tp.points[tp.points.len() - 1];
            let dn = dist2_xy(&tp.points[0], next);
            let dp = dist2_xy(&tp.points[0], prev);
            let prev_key = (prev.y, prev.x, prev.z);
            let next_key = (next.y, next.x, next.z);
            if dp < dn || (dp == dn && prev_key < next_key) {
                tp.points.reverse();
                // After reversing, the chosen start (previously index 0) is now at the end.
                // Rotate it back to index 0.
                let n = tp.points.len();
                if n > 1 {
                    tp.points.rotate_left(n - 1);
                }
            }
        }

        // Re-close the loop explicitly.
        let first = tp.points[0];
        if tp.points.last().copied() != Some(first) {
            tp.points.push(first);
        }
    }

    fn order_toolpaths_for_node(mut tps: Vec<ToolPath>, curr: &mut IV3) -> Vec<ToolPath> {
        // Top-down within the node.
        tps.sort_by_key(|tp| std::cmp::Reverse(tp.points.first().map(|p| p.z).unwrap_or(0)));

        let mut out: Vec<ToolPath> = Vec::with_capacity(tps.len());
        while !tps.is_empty() {
            let mut best_i = 0usize;
            let mut best_cost: (i64, i32, u8, i32, i32, usize) = (i64::MAX, 0, 0, 0, 0, 0);

            for (i, tp) in tps.iter().enumerate() {
                let start = tp.points.first().unwrap_or(&IV3 { x: 0, y: 0, z: 0 });
                let end = tp.points.last().unwrap_or(start);
                let mut d = dist2_xy(curr, start);
                if !tp.closed {
                    d = d.min(dist2_xy(curr, end));
                }
                let z = start.z;
                let closed_key = if tp.closed { 0u8 } else { 1u8 };
                let key = (d, -z, closed_key, start.y, start.x, tp.points.len());
                if key < best_cost {
                    best_cost = key;
                    best_i = i;
                }
            }

            let mut tp = tps.swap_remove(best_i);
            if tp.closed {
                roll_closed_to_nearest(&mut tp, curr);
            } else {
                choose_open_orientation(&mut tp, curr);
            }

            if let Some(last) = tp.points.last().copied() {
                *curr = last;
            }
            out.push(tp);
        }
        out
    }

    // Bucket toolpaths by node id.
    let mut per_node: Vec<Vec<ToolPath>> = vec![Vec::new(); region_root.get_n_nodes().max(1)];
    for tp in toolpaths.drain(..) {
        if tp.tree_node_id < per_node.len() {
            per_node[tp.tree_node_id].push(tp);
        } else {
            // Unknown node id, keep it in a trailing bucket.
            per_node[0].push(tp);
        }
    }

    let node_order = build_node_visit_order(region_root);
    let mut curr = IV3 { x: 0, y: 0, z: 0 };
    for node_id in node_order {
        if node_id >= per_node.len() {
            continue;
        }
        let bucket = std::mem::take(&mut per_node[node_id]);
        let ordered = order_toolpaths_for_node(bucket, &mut curr);
        toolpaths.extend(ordered);
    }

    // Append anything not reached by the traversal (should be rare).
    for bucket in per_node.into_iter() {
        toolpaths.extend(bucket);
    }
}

#[cfg(test)]
mod sort_tests {
    use super::*;
    use crate::im::label::label_im;
    use crate::region_tree::{create_cut_bands, create_region_tree};
    use crate::test_helpers::{ply_im_from_ascii, stub_band_desc, stub_ply_desc};

    fn build_node_visit_order_for_test(region_root: &RegionRoot) -> Vec<usize> {
        // Keep in sync with the implementation in sort_tool_paths.
        fn band_i(node: &RegionNode) -> usize {
            match node {
                RegionNode::Floor { band_i, .. } => *band_i,
                RegionNode::Cut { band_i, .. } => *band_i,
            }
        }
        fn recurse(nodes: &[RegionNode], out: &mut Vec<usize>) {
            if nodes.is_empty() {
                return;
            }
            let b0 = band_i(&nodes[0]);
            assert!(nodes.iter().all(|n| band_i(n) == b0));

            for n in nodes {
                out.push(n.get_id());
                if let RegionNode::Floor { children, .. } = n {
                    recurse(children, out);
                }
            }
        }

        let mut out = Vec::new();
        recurse(region_root.children(), &mut out);
        out
    }

    #[test]
    fn sort_toolpaths_respects_region_tree_order() {
        let ply_im = ply_im_from_ascii(
            r#"
                11111
                12221
                12321
                12221
                11111
            "#,
        );

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

        let mut toolpaths = create_toolpaths_from_region_tree(
            "test",
            &region_root,
            &cut_bands,
            0,
            2,
            1,
            0,
            Thou(0),
            &ply_im,
            &region_im,
            &region_infos,
            0,
            1,
            true,
            None,
        );

        // Deliberately scramble the toolpaths a bit.
        toolpaths.reverse();
        if toolpaths.len() >= 3 {
            toolpaths.swap(0, 2);
        }

        sort_tool_paths(&mut toolpaths, &region_root);

        let node_order = build_node_visit_order_for_test(&region_root);
        let mut id_to_rank: Vec<usize> = vec![usize::MAX; region_root.get_n_nodes()];
        for (rank, &id) in node_order.iter().enumerate() {
            id_to_rank[id] = rank;
        }

        let mut last_rank = 0usize;
        for (i, tp) in toolpaths.iter().enumerate() {
            let r = id_to_rank
                .get(tp.tree_node_id)
                .copied()
                .unwrap_or(usize::MAX);
            if i == 0 {
                last_rank = r;
            } else {
                assert!(r >= last_rank, "node rank should be nondecreasing");
                last_rank = r;
            }
        }
    }

    #[test]
    fn sort_toolpaths_normalizes_open_and_closed_starts() {
        let ply_im = ply_im_from_ascii(
            r#"
                11
                11
            "#,
        );

        let ply_descs = vec![
            stub_ply_desc("dummy", 0, true),
            stub_ply_desc("ply100", 100, false),
        ];
        let band_descs = vec![stub_band_desc(200, 0, "rough")];

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
        let some_node_id = region_root
            .children()
            .first()
            .map(|n| n.get_id())
            .unwrap_or(0);

        let mut toolpaths = vec![
            // Open path intentionally reversed (start should become the smaller end).
            ToolPath {
                points: vec![IV3 { x: 5, y: 0, z: 100 }, IV3 { x: 1, y: 0, z: 100 }],
                closed: false,
                tool_dia_pix: 1,
                tool_i: 0,
                tree_node_id: some_node_id,
                cuts: CutPixels::default(),
            },
            // Closed path intentionally not rotated.
            ToolPath {
                points: vec![
                    IV3 { x: 2, y: 0, z: 100 },
                    IV3 { x: 3, y: 0, z: 100 },
                    IV3 { x: 1, y: 0, z: 100 },
                    IV3 { x: 4, y: 0, z: 100 },
                ],
                closed: true,
                tool_dia_pix: 1,
                tool_i: 0,
                tree_node_id: some_node_id,
                cuts: CutPixels::default(),
            },
        ];

        sort_tool_paths(&mut toolpaths, &region_root);

        // Find our two toolpaths again by their closed flag.
        let open = toolpaths.iter().find(|tp| !tp.closed).unwrap();
        assert_eq!(open.points[0].x, 1);
        assert_eq!(open.points[1].x, 5);

        let closed = toolpaths.iter().find(|tp| tp.closed).unwrap();
        // After running the open path, current position is at x=5, so the closed loop
        // is rolled to start at the nearest vertex (x=4).
        assert_eq!(closed.points[0].x, 4);
    }
}

/*

Now i want to implement sort_tool_paths().  Here's an example of a _region_root tree:

Root: num_children=3
  0: Cut: path='0', parent_id=, band_i=0, cut_plane_i=0, ply_guid=HZWKZRTQJV, top_thou=850, region_i=3, region_size=80000, z_thou=850
  1: Flr: path='1', parent_id=, band_i=0, cut_plane_i=1, ply_guid=floor_0, top_thou=800, floor_regions=[1, 2]
    2: Cut: path='1.0', parent_id=1, band_i=1, cut_plane_i=0, ply_guid=ZWKKED69NS, top_thou=500, region_i=2, region_size=11900, z_thou=500
    3: Cut: path='1.1', parent_id=1, band_i=1, cut_plane_i=1, ply_guid=FLOOR_PLY_DESC, top_thou=100, region_i=1, region_size=148100, z_thou=100
  4: Flr: path='2', parent_id=, band_i=0, cut_plane_i=1, ply_guid=floor_0, top_thou=800, floor_regions=[4]
    5: Cut: path='2.0', parent_id=4, band_i=1, cut_plane_i=1, ply_guid=FLOOR_PLY_DESC, top_thou=100, region_i=4, region_size=10000, z_thou=100


The toolpaths are list of ToolPath and toolPath includes tree_node_id which can be access via the LUT in RegionRoot.get_node_by_id

So the rules of sorting are that
    Toolpaths come in two varieties: closed and open.
        Closed toolpaths are perimeters which are loops and can be started anywhere.
        Open toolpaths are should always start and one end or the other.
        When a tool path is sorted it may need to be "rolled" so that its first verts start at [0]

    Sibling nodes must all be in the same band_i (assert that)

    Sibling nodes can be sorted in any order amoung themselves.

    A floor node "reveals" chilren below it and those children nodes are added before sibling nodes.

    All of a node and its children must be cut together before moving to the next sibling node.

*/
