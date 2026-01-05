use crate::cut_stack::{PlyIm, RegionIm, RegionNode, RegionRoot};
use crate::desc::Thou;
use crate::dilate_im::im_dilate;
use crate::im::MaskIm;
use crate::im::label::LabelInfo;

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
) -> Vec<ToolPath>
{
    let w = region_im.w;
    let h = region_im.h;
    let mut region_mask_im = MaskIm::new(w, h);
    let mut above_mask_im = MaskIm::new(w, h);
    let mut scratch_mask_im = MaskIm::new(w, h);

    let tool_dia_pix: usize = (tool_radius_pix.saturating_mul(2) + 1) as usize;

    let mut paths: Vec<ToolPath> = Vec::new();

    // Recurse through the region tree
    fn recurse_region_tree(
        node: &RegionNode,
        region_mask_im: &mut MaskIm,
        above_mask_im: &mut MaskIm,
        scratch_mask_im: &mut MaskIm,
        tool_dia_pix: usize,
        ply_im: &PlyIm,
        region_infos: &[LabelInfo],
        _paths: &mut Vec<ToolPath>,
    ) {
        match node {
            RegionNode::Floor { children, .. } => {
                for child in children {
                    recurse_region_tree(
                        child,
                        region_mask_im,
                        above_mask_im,
                        scratch_mask_im,
                        tool_dia_pix,
                        ply_im,
                        region_infos,
                        _paths,
                    );
                }
            }
            RegionNode::Cut { region_i, .. } => {
                // Fill the two working images with 0.
                region_mask_im.arr.fill(0);
                above_mask_im.arr.fill(0);
                scratch_mask_im.arr.fill(0);

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
                    if pix_i < region_mask_im.arr.len() {
                        region_mask_im.arr[pix_i] = 255;
                    }
                }

                // Dilate the current region into tool-centerable space.
                im_dilate(region_mask_im, scratch_mask_im, tool_dia_pix);
                std::mem::swap(region_mask_im, scratch_mask_im);

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
                im_dilate(above_mask_im, scratch_mask_im, tool_dia_pix);
                for i in 0..region_mask_im.arr.len() {
                    if scratch_mask_im.arr[i] > 0 {
                        region_mask_im.arr[i] = 0;
                    }
                }

            }
        }
    }

    for child in &region_root.children {
        recurse_region_tree(
            child,
            &mut region_mask_im,
            &mut above_mask_im,
            &mut scratch_mask_im,
            tool_dia_pix,
            ply_im,
            region_infos,
            &mut paths,
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

    fn collect_cut_masks(
        region_root: &RegionRoot,
        tool_dia_pix: usize,
        ply_im: &PlyIm,
        region_infos: &[LabelInfo],
        w: usize,
        h: usize,
    ) -> Vec<MaskIm> {
        let mut region_mask_im = MaskIm::new(w, h);
        let mut above_mask_im = MaskIm::new(w, h);
        let mut scratch_mask_im = MaskIm::new(w, h);

        let mut out: Vec<MaskIm> = Vec::new();

        fn rec(
            node: &RegionNode,
            region_mask_im: &mut MaskIm,
            above_mask_im: &mut MaskIm,
            scratch_mask_im: &mut MaskIm,
            tool_dia_pix: usize,
            ply_im: &PlyIm,
            region_infos: &[LabelInfo],
            out: &mut Vec<MaskIm>,
        ) {
            match node {
                RegionNode::Floor { children, .. } => {
                    for child in children {
                        rec(
                            child,
                            region_mask_im,
                            above_mask_im,
                            scratch_mask_im,
                            tool_dia_pix,
                            ply_im,
                            region_infos,
                            out,
                        );
                    }
                }
                RegionNode::Cut { region_i, .. } => {
                    region_mask_im.arr.fill(0);
                    above_mask_im.arr.fill(0);
                    scratch_mask_im.arr.fill(0);

                    let label_i = region_i.0 as usize;
                    if label_i == 0 || label_i >= region_infos.len() {
                        return;
                    }
                    let label_info = &region_infos[label_i];

                    let start_i = label_info.start_y * ply_im.s + label_info.start_x;
                    if start_i >= ply_im.arr.len() {
                        return;
                    }
                    let curr_ply_i = ply_im.arr[start_i];

                    for &pix_i in &label_info.pixel_iz {
                        if pix_i < region_mask_im.arr.len() {
                            region_mask_im.arr[pix_i] = 255;
                        }
                    }

                    im_dilate(region_mask_im, scratch_mask_im, tool_dia_pix);
                    std::mem::swap(region_mask_im, scratch_mask_im);

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

                    im_dilate(above_mask_im, scratch_mask_im, tool_dia_pix);
                    for i in 0..region_mask_im.arr.len() {
                        if scratch_mask_im.arr[i] > 0 {
                            region_mask_im.arr[i] = 0;
                        }
                    }

                    let mut snap = MaskIm::new(region_mask_im.w, region_mask_im.h);
                    snap.arr.copy_from_slice(&region_mask_im.arr);
                    out.push(snap);
                }
            }
        }

        for child in &region_root.children {
            rec(
                child,
                &mut region_mask_im,
                &mut above_mask_im,
                &mut scratch_mask_im,
                tool_dia_pix,
                ply_im,
                region_infos,
                &mut out,
            );
        }

        out
    }

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
        );

        // The current implementation is a stub; lock in that it returns an empty set
        // but does not panic when given a realistic region tree.
        assert!(paths.is_empty());
    }

    #[test]
    fn surface_tool_path_generation_dump_better_image() {
        let ply_im = ply_im_from_ascii(
            r#"
                111111111111111111111111111111
                111444433333333333333333331111
                111444433333333333333333331111
                111333333333333333333333331111
                111333222222222222222233331111
                111333222211111112222233331111
                111333222211111112222233331111
                111333222222222222222233331111
                111333333333333333333333331111
                111333333333333333333333331111
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

        let tool_radius_pix = 2_u32;
        let tool_dia_pix: usize = (tool_radius_pix.saturating_mul(2) + 1) as usize;

        // Primary call under test (should not panic).
        let _paths = surface_tool_path_generation(
            &region_root,
            &tool_radius_pix,
            &ply_im,
            &region_im,
            &region_infos,
        );

        // Dump ascii maps for visual inspection.
        println!("ply_im:\n{}", im_u16_to_ascii(&ply_im));
        println!("region_im:\n{}", im_u16_to_ascii(&region_im));

        let cut_masks = collect_cut_masks(
            &region_root,
            tool_dia_pix,
            &ply_im,
            &region_infos,
            ply_im.w,
            ply_im.h,
        );
        for (i, m) in cut_masks.iter().enumerate() {
            println!("cut_mask[{i}]:\n{}", mask_to_ascii(m));
        }
    }
}

