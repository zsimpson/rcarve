use crate::cut_stack::{RegionNode, RegionRoot};
use crate::desc::Thou;

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
    _tool_radius_pix: u32,
) -> Vec<ToolPath>
{
    let paths = Vec::new();

    // Recurse through the region tree
    fn recurse_region_tree(node: &RegionNode) {
        match node {
            RegionNode::Floor { children, .. } => {
                for child in children {
                    recurse_region_tree(child);
                }
            }
            RegionNode::Cut { .. } => {}
        }
    }

    for child in &region_root.children {
        recurse_region_tree(child);
    }

    paths
}
