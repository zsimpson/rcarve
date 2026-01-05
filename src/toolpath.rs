use crate::cut_stack::{PlyIm, RegionIm, RegionNode, RegionRoot};
use crate::desc::Thou;
use crate::dilate_im::im_dilate;
use crate::im::MaskIm;
use crate::im::label::{LabelInfo, ROI};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V3 {
    pub x: i32, // Pixels
    pub y: i32, // Pixels
    pub z: i32, // Thou
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPath {
    pub points: Vec<V3>,
    pub tool_thou: Thou,
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

pub fn surface_tool_path_generation(
    region_root: &RegionRoot,
    tool_radius_pix: &u32,
    ply_im: &PlyIm,
    region_im: &RegionIm,
    region_infos: &[LabelInfo],
    mut on_region_masks: Option<
        &mut dyn FnMut(&RegionNode, (usize, usize, usize, usize), &MaskIm, &MaskIm, &MaskIm),
    >,
) -> Vec<ToolPath>
{
    let w = region_im.w;
    let h = region_im.h;
    let mut cut_mask_im = MaskIm::new(w, h);
    let mut above_mask_im = MaskIm::new(w, h);
    let mut dil_above_mask_im = MaskIm::new(w, h);

    let tool_dia_pix: usize = (tool_radius_pix.saturating_mul(2) + 1) as usize;

    let mut paths: Vec<ToolPath> = Vec::new();

    // Recurse through the region tree
    fn recurse_region_tree(
        node: &RegionNode,
        cut_mask_im: &mut MaskIm,
        above_mask_im: &mut MaskIm,
        dil_abv_mask_im: &mut MaskIm,
        tool_dia_pix: usize,
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
                        cut_mask_im,
                        above_mask_im,
                        dil_abv_mask_im,
                        tool_dia_pix,
                        ply_im,
                        region_infos,
                        _paths,
                        on_region_masks,
                    );
                }
            }
            RegionNode::Cut { region_i, .. } => {
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

                // Optional debug/testing hook: after computing masks for a cut leaf.
                if let Some(cb) = on_region_masks.as_mut() {
                    (**cb)(node, (l, t, r, b), cut_mask_im, above_mask_im, dil_abv_mask_im);
                }

            }
        }
    }

    for child in &region_root.children {
        recurse_region_tree(
            child,
            &mut cut_mask_im,
            &mut above_mask_im,
            &mut dil_above_mask_im,
            tool_dia_pix,
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
    use crate::cut_stack::{create_cut_bands, create_region_tree};
    use crate::im::label::label_im;
    use crate::test_helpers::{im_u16_to_ascii, mask_to_ascii, ply_im_from_ascii, stub_band_desc, stub_ply_desc};

    fn count_cut_leaves(node: &crate::cut_stack::RegionNode) -> usize {
        match node {
            crate::cut_stack::RegionNode::Cut { .. } => 1,
            crate::cut_stack::RegionNode::Floor { children, .. } => {
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
        let region_im: RegionIm = region_im_raw.retag::<crate::cut_stack::RegionI>();

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

        let tool_radius_pix = 1_u32;
        let paths = surface_tool_path_generation(
            &region_root,
            &tool_radius_pix,
            &ply_im,
            &region_im,
            &region_infos,
            None,
        );

        // The current implementation is a stub; lock in that it returns an empty set
        // but does not panic when given a realistic region tree.
        assert!(paths.is_empty());
    }

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
        let band_descs = vec![stub_band_desc(500, 0, "rough")];

        let (region_im_raw, region_infos) = label_im(&ply_im);
        let region_im: RegionIm = region_im_raw.retag::<crate::cut_stack::RegionI>();

        let cut_bands = create_cut_bands(
            "rough",
            &ply_im,
            &band_descs,
            &region_im,
            &region_infos,
            &ply_descs,
        );
        let region_root = create_region_tree(&cut_bands, &region_infos);

        let tool_radius_pix = 1_u32;
        let tool_dia_pix: usize = (tool_radius_pix.saturating_mul(2) + 1) as usize;

        let mut node_results: Vec<(RegionNode, (usize, usize, usize, usize), MaskIm, MaskIm, MaskIm)> =
            Vec::new();
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
        let _paths = surface_tool_path_generation(
            &region_root,
            &tool_radius_pix,
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
            region_root.children.iter().map(count_cut_leaves).sum::<usize>(),
            "expected one callback per cut leaf"
        );

        println!("tool_dia_pix: {tool_dia_pix}");
        for (i, (region_node, (l, t, r, b), cut_m, above_m, dil_abv_m)) in node_results.iter().enumerate() {

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

            println!("--- Cut leaf {i} masks --------------------------------------------------------");
            println!("cut[{i}] region_node: {:?}", region_node);
            println!("cut[{i}] label_i: {label_i}, curr_ply_i: {:?}", curr_ply_i);
            // Outline the padded ROI actually used for above-mask extraction.
            let roi_pad = ROI { l: *l, t: *t, r: *r, b: *b };
            let roi_opt = Some(&roi_pad);

            println!("cut[{i}] at_mask (label pixels):\n{}", mask_to_ascii(&at_mask, None));
            println!("cut[{i}] above_mask:\n{}", mask_to_ascii(above_m, roi_opt));
            println!("cut[{i}] dil_abv_mask:\n{}", mask_to_ascii(dil_abv_m, None));
            println!("cut[{i}] cut_mask:\n{}", mask_to_ascii(cut_m, None));
        }
    }
}

