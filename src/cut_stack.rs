use crate::desc::{BandDesc, PlyDesc};
use crate::desc::{Guid, Thou};
use crate::im::Im;
use crate::im::MaskIm;
use crate::im::label::LabelInfo;
use std::cmp::Ordering;

macro_rules! newtype {
    ($name:ident($inner:ty)) => {
        #[derive(Copy, Clone, Debug, Eq, PartialEq)]
        pub struct $name(pub $inner);
    };
}

newtype!(PlyI(u16));
newtype!(RegionI(u16));

pub type PlyIm = Im<u16, 1, PlyI>;
pub type RegionIm = Im<u16, 1, RegionI>;

/// A CutPlane is a ply with additional information:
///   * The ply_i is the value in the ply_im corresponding to this ply.
///   * The pos_work_im is a mask image indicating where this ply exists in the work area.
///   * The cut_im is a mask image indicating where the tool can be centered to cut this ply.
///     it has been dilated by the tool radius and has the dilated areas above the ply removed.
///   * has_overcut indicates whether this ply has "filled any holes" meaning that there
///     are regions below this ply that will be overcut when cutting this ply as an optimization.
///     (see Overcut discussion below)
///  * is_floor indicates whether this ply is a floor ply (the base of a band)
///    which affects the depth-first carving order -- regions below this floor are carved
///    before other siblings of this ply.
///  * region_iz is the list of region indices (labels) corresponding to this ply.
pub struct CutPlane {
    pub ply_guid: Guid, // For debugging and reference
    pub top_thou: Thou, // Cloned from PlyDesc
    pub ply_i: PlyI,
    pub pos_work_im: Option<MaskIm>,
    pub cut_im: Option<MaskIm>,
    pub has_overcut: bool,
    pub is_floor: bool,
    pub region_iz: Vec<RegionI>, // A ply can have multiple labels if it's discontiguous
}

/// A CutBand represents a band of plies to be cut in a single pass.
/// CutPlanes within the band are cuttable in any order unless they contain a has_overcut ply
/// which forces a depth-first carving order.
/// That is in sorting the cut_planes, all plies below a has_overcut ply are ordered first from top-to-bottom.
/// Then all remaining plies can be cut in any any convenient order (greedy, TSP, etc)
pub struct CutBand {
    pub band_desc: BandDesc,
    pub top_thou: Thou, // Exclusive
    pub bot_thou: Thou, // Inclusive
    pub cut_planes: Vec<CutPlane>,
}

/// Consider a height-map like the following.
///   For convenience call "1" the ocean; "3" the island; "4" the volcano; and "2" the lake.
///   Note that there is some sea level area inside the lake. We say that "1" has two disjoint regions.
///
/// 111111111111111111111111111111
/// 111444433333333333333333331111
/// 111444433333333333333333331111
/// 111333333333333333333333331111
/// 111333222222222222222233331111
/// 111333222211111112222233331111
/// 111333222211111112222233331111
/// 111333222222222222222233331111
/// 111333333333333333333333331111
/// 111333333333333333333333331111
///
/// Suppose the sorted_ply_descs are:
///  [0] = Dummy
///  [1] = Thou 100
///  [2] = Thou 400
///  [3] = Thou 700
///  [4] = Thou 900
///
/// That maps to the following height-map (*100):
///
/// 111111111111111111111111111111
/// 111999977777777777777777771111
/// 111999977777777777777777771111
/// 111777777777777777777777771111
/// 111777444444444444444477771111
/// 111777444411111114444477771111
/// 111777444411111114444477771111
/// 111777444444444444444477771111
/// 111777777777777777777777771111
/// 111777777777777777777777771111
///
/// Here is a region map. Note that region labels are arbitrary and not ordered by height.
/// and that a single ply can have multiple regions if it's discontiguous (such as ply [1])
/// 111111111111111111111111111111
/// 111222233333333333333333331111
/// 111222233333333333333333331111
/// 111333333333333333333333331111
/// 111333444444444444444433331111
/// 111333444455555554444433331111
/// 111333444455555554444433331111
/// 111333444444444444444433331111
/// 111333333333333333333333331111
/// 111333333333333333333333331111
///
/// A CutPlane represents one of the height-map levels to be cut.
/// Note that a CutPlane may contains multiple regions (region_iz) because a ply can be discontiguous.
///
/// Out goal is to cut down from a solid block to the desired height-map.
/// Because the cutting tool can only cut so deep per pass, multiple CutPlanes are grouped into CutBands.
/// CutBands point to all the CutPlanes that are accessible in a single cutting pass.
/// Every CutBand contains at exactly 1 "floor" CutPlane which represents areas that contain pixel lower than this band.
/// In other words, the floor of a CutBand are pixels that need to be cut in order to "reveal" regions in lower bands.
///
/// In principle, we can cut all the regions of the CutPlanes within a CutBand in any order because they are all accessible in that pass;
/// in practice we will use a greedy or TSP approach to minimize tool travel over this set of CutPlanes.
/// However, there is are several important optimizations to this rule.
/// (1) If cutting a region associated with a "floor" CutPlane we want to depth-first carve all regions below it
/// before returning to other regions in the same band. This significantly reduces horizontal tool travel because
/// we continue down with geometry that is already exposed rather than returning to the top of the band repeatedly.
/// (2) There are times that it is faster to "overcut" regions below the current CutPlane. See the Overcut discussion below.
/// To support these optimizations, we need to build a sorted tree of CutRegion nodes.

/// Overcut optimization
///
/// Suppose that we have an island with many small lakes.
///     111111111111111111111111111111
///     111991119911111199111119911111
///     111991119911111199111119911111
///     111111111111111111111111111111
///     111991119911111199111119911111
///     111991119911111199111119911111
///     111111111111111111111111111111
///
///  If there's perimeters involved then it can be better to cut the whole island
///  and then come back to cut the lakers rather than cutting around the lakes first.
///  
///  Let a be the un-shared perimeter of A and b be the un-shared perimeter of B
///  Let s be the shared perimeter between A and B
///    aaaaaaa     aasbbb
///    asssssa     aAsBBb
///    asBBBsa     aAsBBb
///    asBBBsa     aAsBBb
///    assssaa     aAsBBb
///    aaaaaaa     aasbbb

///  If perimeters are included in the cost model then the two cut options are:
///      Without the overcut: Cost = a + s + A + b + s + B
///      With    the overcut: Cost = a + b + A + B + s + b + B
///  After cancelling: Cost = . + . + . + . + s + .
///                  : Cost = . + . + . + . + . + b + B
///  After canceling common terms:
///    Cost difference = s > b + B. When s is greater than b + B, overcut is preferred.
///  Notes, this assumes that B has been converted into equivilent linear units (the number of length of the tool lines it would take to cut B)
///
/// However, without perimeters in the cost model is harder to estimate as you need to take into account
/// the time to travel from region to region.
///      Without the overcut: Cost = A + B
///      With    the overcut: Cost = A + B + B
/// On the face of it it appears that the overcut is never preferred, but if you consider that
/// without over cut that the tool has to stop at the boundary of A and then move to B and start again
/// that travel time might be worth eliminating.
///
/// For now, we're going to try the strategy eliminating all perimeter cuts from the rough and smooth passes
/// therefore making overcuts never prefered and therefore we choose to eliminate this code entirely for now.

/// create_cut_bands creates the CutBands for a given cut_pass
/// Create one CutBand instance per BandDesc that matches the cut_pass.
/// Create 1+ CutPlanes for each CutBand; one per labeled region in the ply_im that falls within the band's thou range plus a floor.
pub fn create_cut_bands(
    cut_pass: &str, // A name for the cut pass, e.g. "rough", "refine_smooth", "refine_perimeter", "detail"
    ply_im: &PlyIm, // The ply vals are sorted so that higher thou values have higher ply vals.
    band_descs: &[BandDesc], // All band descriptions (will be filtered by cut_pass), gives thou ranges for each (will be cloned into CutBand)
    region_im: &RegionIm,    // The labeled connected component image from labeling the ply_im
    region_infos: &[LabelInfo], // The connected component infos from labeling the ply_im
    ply_descs: &Vec<PlyDesc>, // All ply descriptions, indexed by ply_i (sorted bottom to top)
) -> Vec<CutBand> {
    let _ = region_im;

    let mut cut_bands: Vec<CutBand> = band_descs
        .iter()
        .filter(|bd| bd.cut_pass == cut_pass)
        .map(|bd| CutBand {
            band_desc: bd.clone(),
            top_thou: bd.top_thou.clone(),
            bot_thou: bd.bot_thou.clone(),
            cut_planes: Vec::new(),
        })
        .collect();

    // Create the CutPlanes by iterating the ply_descs
    for (ply_i_usize, ply_desc) in ply_descs.iter().enumerate() {
        let ply_i = PlyI(ply_i_usize as u16);
        let thou = &ply_desc.top_thou;

        // Find the band that this ply belongs to
        for band in cut_bands.iter_mut() {
            let band_top = &band.top_thou;
            let band_bot = &band.bot_thou;

            if band_bot <= thou && thou < band_top {
                // This ply belongs to this band
                let cut_plane = CutPlane {
                    ply_guid: ply_desc.guid.clone(),
                    top_thou: ply_desc.top_thou.clone(),
                    ply_i,
                    pos_work_im: None,  // To be filled in later
                    cut_im: None,       // To be filled in later
                    has_overcut: false, // Overcut logic removed for now
                    is_floor: false,
                    region_iz: Vec::new(), // To be filled in below
                };

                band.cut_planes.push(cut_plane);
                break;
            }
        }
    }

    // Go back through the bands and add a floor ply.
    // Sort all cut_planes by top_thou from top to bottom.
    for (band_i, band) in cut_bands.iter_mut().enumerate() {
        // Create a dummy floor ply
        let floor_ply = CutPlane {
            ply_guid: Guid(format!("floor_{}", band_i)),
            top_thou: band.bot_thou.clone(),
            ply_i: PlyI(0),    // Dummy ply_i
            pos_work_im: None, // To be filled in later
            cut_im: None,      // To be filled in later
            has_overcut: false,
            is_floor: true,
            region_iz: Vec::new(),
        };
        band.cut_planes.push(floor_ply);

        // Sort cut planes deterministically:
        // - Keep the special dummy plane (ply_i == 0, non-floor) at index 0.
        // - Keep the floor plane last.
        // - Sort the remaining planes from top to bottom (descending top_thou).
        band.cut_planes.sort_by(|a, b| {
            let a_is_dummy = !a.is_floor && a.ply_i.0 == 0;
            let b_is_dummy = !b.is_floor && b.ply_i.0 == 0;

            match (a_is_dummy, b_is_dummy) {
                (true, false) => return Ordering::Less,
                (false, true) => return Ordering::Greater,
                _ => {}
            }

            match (a.is_floor, b.is_floor) {
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => b.top_thou.0.cmp(&a.top_thou.0),
            }
        });
    }

    // Now assign region_iz to each CutPlane by scanning the region_info
    // The region_info contains a point in the set (start_x, start_y)
    // and that can be used to lookup the ply_i in the ply_im.
    // From the ply_i we can find the corresponding CutPlane and add the region_i to its list.
    // label_im reserves region_infos[0] for "background"; skip it.
    for (region_i_usize, region_info) in region_infos.iter().enumerate().skip(1) {
        let region_i = RegionI(region_i_usize as u16);
        let (x, y) = (region_info.start_x, region_info.start_y);
        let x = x as usize;
        let y = y as usize;
        assert!(
            x < ply_im.w && y < ply_im.h,
            "region start point is outside ply_im bounds"
        );
        let ply_i_val = ply_im.arr[y * ply_im.s + x];
        let ply_i = PlyI(ply_i_val);
        // Find the CutPlane corresponding to this ply_i
        for band in cut_bands.iter_mut() {
            for cut_plane in band.cut_planes.iter_mut() {
                if cut_plane.ply_i == ply_i {
                    cut_plane.region_iz.push(region_i);
                    break;
                }
            }
        }
    }

    // De-duplicate region_iz lists
    for band in cut_bands.iter_mut() {
        for cut_plane in band.cut_planes.iter_mut() {
            cut_plane.region_iz.sort_by(|a, b| a.0.cmp(&b.0));
            cut_plane.region_iz.dedup();
        }
    }

    cut_bands
}

pub fn debug_print_cut_bands(cut_bands: &Vec<CutBand>) {
    for (band_i, band) in cut_bands.iter().enumerate() {
        println!(
            "CutBand[{}]: top_thou={:?}, bot_thou={:?}, num_cut_planes={}",
            band_i,
            band.top_thou,
            band.bot_thou,
            band.cut_planes.len()
        );
        for (cut_plane_i, cut_plane) in band.cut_planes.iter().enumerate() {
            println!(
                "  CutPlane[{}]: ply_guid={}, top_thou={:?}, ply_i={}, is_floor={}, num_regions={}",
                cut_plane_i,
                cut_plane.ply_guid.0,
                cut_plane.top_thou,
                cut_plane.ply_i.0,
                cut_plane.is_floor,
                cut_plane.region_iz.len()
            );
        }
    }
}


/// The goal of this module is to build the sorted tree of region nodes.
/// This requires a call to create_cut_bands() which creates the CutBands and CutPlanes
/// and then a call to build_cut_region_tree() which renders the product (dilation, masking, etc)
/// and then builds the region tree for depth-first carving.

// Siblings are represented by ordering within a Vec in the parent.

pub struct RegionTree {
    pub cut_bands: Vec<CutBand>,
    pub roots: Vec<RegionNode>,
}

#[derive(Clone, Debug)]
pub enum RegionNode {
    /// A floor node gates access to deeper bands.
    /// Cutting the floor reveals its children, which are 1+ regions in lower bands.
    Floor {
        band_i: usize,
        cut_plane_i: usize,
        children: Vec<RegionNode>,
    },
    /// A leaf region to cut (a single connected component at a specific ply in a band).
    Cut {
        band_i: usize,
        cut_plane_i: usize,
        region_i: RegionI,
    },
}

/// Create a region tree for depth-first traversal.
///
/// This is intentionally minimal for now:
/// - Each band contributes a list of nodes (siblings): one Floor node plus zero or more Cut leaves.
/// - A band's Floor node has children equal to the entire next band's node list.
///
/// This matches the semantics: the union of all pixels below a band must be cut (the floor)
/// before *any* region in lower bands can be cut.
pub fn create_region_tree(cut_bands: Vec<CutBand>) -> RegionTree {
    if cut_bands.is_empty() {
        return RegionTree {
            cut_bands,
            roots: Vec::new(),
        };
    }

    let mut band_nodes: Vec<Vec<RegionNode>> = Vec::with_capacity(cut_bands.len());

    // TODO: Just enforce that floor are the last cut_plane in each band?

    for (band_i, band) in cut_bands.iter().enumerate() {
        let floor_plane_i = band
            .cut_planes
            .iter()
            .position(|cp| cp.is_floor)
            .expect("each CutBand must contain exactly one floor CutPlane");

        let mut nodes: Vec<RegionNode> = Vec::new();
        nodes.push(RegionNode::Floor {
            band_i,
            cut_plane_i: floor_plane_i,
            children: Vec::new(),
        });

        for (cut_plane_i, cut_plane) in band.cut_planes.iter().enumerate() {
            if cut_plane.is_floor {
                continue;
            }
            for &region_i in &cut_plane.region_iz {
                nodes.push(RegionNode::Cut {
                    band_i,
                    cut_plane_i,
                    region_i,
                });
            }
        }

        band_nodes.push(nodes);
    }

    // Nest bands by wiring each band's floor children to the entire next band's nodes.
    for band_i in (0..band_nodes.len().saturating_sub(1)).rev() {
        let children = std::mem::take(&mut band_nodes[band_i + 1]);  // TODO explain
        let floor_node = band_nodes[band_i]
            .get_mut(0)
            .expect("each band node list must contain a floor node");
        match floor_node {
            RegionNode::Floor { children: c, .. } => {
                *c = children;
            }
            RegionNode::Cut { .. } => panic!("band node list must start with a floor node"),
        }
    }

    let roots = std::mem::take(&mut band_nodes[0]);
    RegionTree { cut_bands, roots }
}

pub fn debug_print_region_tree(node: &RegionNode, ply_descs: &Vec<PlyDesc>, region_infos: &Vec<LabelInfo>, indent: usize) {
    let indent_str = " ".repeat(indent);
    match node {
        RegionNode::Floor { band_i, cut_plane_i, children } => {
            println!("{}Floor: band_i={}, cut_plane_i={}", indent_str, band_i, cut_plane_i);
            for child in children {
                debug_print_region_tree(child, ply_descs, region_infos, indent + 2);
            }
        }
        RegionNode::Cut { band_i, cut_plane_i, region_i } => {
            let ply_desc = &ply_descs[*band_i];
            let region_info = &region_infos[region_i.0 as usize];
            println!("{}Cut: band_i={}, cut_plane_i={}, ply_guid={}, top_thou={:?}, region_i={}, region_size={}", 
                indent_str, band_i, cut_plane_i, ply_desc.guid.0, ply_desc.top_thou, region_i.0, region_info.size);
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::im::label::label_im;

    #[allow(dead_code)]
    fn im_u16_to_ascii<S>(im: &Im<u16, 1, S>) -> String {
        let max_v = im.arr.iter().copied().max().unwrap_or(0);
        let cell_w = std::cmp::max(1, max_v.to_string().len());
        let mut out = String::new();
        for y in 0..im.h {
            for x in 0..im.w {
                let v = im.arr[y * im.s + x];
                // if x > 0 {
                //     out.push(' ');
                // }
                out.push_str(&format!("{:>width$}", v, width = cell_w));
            }
            out.push('\n');
        }
        out
    }

    fn stub_ply_desc(guid: &str, top_thou: i32, hidden: bool) -> PlyDesc {
        PlyDesc {
            owner_layer_guid: Guid("layer0".to_string()),
            guid: Guid(guid.to_string()),
            top_thou: Thou(top_thou),
            hidden,
            is_floor: false,
            ply_mat: vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            mpoly: Vec::new(),
        }
    }

    fn stub_band_desc(top_thou: i32, bot_thou: i32, cut_pass: &str) -> BandDesc {
        BandDesc {
            top_thou: Thou(top_thou),
            bot_thou: Thou(bot_thou),
            cut_pass: cut_pass.to_string(),
        }
    }

    fn ply_im_from_ascii(grid: &str) -> PlyIm {
        let rows: Vec<&str> = grid
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        let h = rows.len();
        assert!(h > 0, "grid must have at least one non-empty row");
        let w = rows[0].len();
        assert!(w > 0, "grid rows must be non-empty");
        for r in &rows {
            assert_eq!(r.len(), w, "all rows must have equal length");
        }

        let mut ply_im = PlyIm::new(w, h);
        for (y, row) in rows.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                let v = ch
                    .to_digit(10)
                    .unwrap_or_else(|| panic!("invalid label char '{ch}', expected digit"))
                    as u16;
                ply_im.arr[y * ply_im.s + x] = v;
            }
        }
        ply_im
    }

    #[test]
    fn it_creates_bands() {
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

        // Suppose the sorted_ply_descs are:
        //  [0] = Dummy
        //  [1] = Thou 100
        //  [2] = Thou 400
        //  [3] = Thou 700
        //  [4] = Thou 900
        let ply_descs = vec![
            stub_ply_desc("dummy", 0, true),
            stub_ply_desc("ply100", 100, false),
            stub_ply_desc("ply400", 400, false),
            stub_ply_desc("ply700", 700, false),
            stub_ply_desc("ply900", 900, false),
        ];

        let band_descs = vec![
            stub_band_desc(1000, 650, "rough"),
            stub_band_desc(650, 0, "rough"),
            stub_band_desc(1000, 0, "refine"), // Should be ignored
        ];

        // Region labeling isn't relevant to the band-splitting behavior being tested here,
        // so pass an empty label set.
        let region_im = RegionIm::new(ply_im.w, ply_im.h);
        let region_infos: Vec<LabelInfo> = Vec::new();

        let cut_bands = create_cut_bands(
            "rough",
            &ply_im,
            &band_descs,
            &region_im,
            &region_infos,
            &ply_descs,
        );

        // Only the two "rough" bands should be created.
        assert_eq!(cut_bands.len(), 2);
        assert_eq!(cut_bands[0].top_thou, Thou(1000));
        assert_eq!(cut_bands[0].bot_thou, Thou(650));
        assert_eq!(cut_bands[1].top_thou, Thou(650));
        assert_eq!(cut_bands[1].bot_thou, Thou(0));

        // Each band must have exactly one floor CutPlane.
        for band in &cut_bands {
            let floor_count = band.cut_planes.iter().filter(|cp| cp.is_floor).count();
            assert_eq!(floor_count, 1);
        }

        // Band 0 should include plies at 900 and 700, plus a dummy floor at 650.
        let band0_thous: Vec<i32> = cut_bands[0]
            .cut_planes
            .iter()
            .map(|cp| cp.top_thou.0)
            .collect();
        assert!(band0_thous.contains(&900));
        assert!(band0_thous.contains(&700));
        assert!(
            cut_bands[0]
                .cut_planes
                .iter()
                .any(|cp| cp.is_floor && cp.top_thou == Thou(650))
        );

        // Band 1 should include plies at 400 and 100, and use the dummy ply at 0 as the floor.
        let band1_thous: Vec<i32> = cut_bands[1]
            .cut_planes
            .iter()
            .map(|cp| cp.top_thou.0)
            .collect();
        assert!(band1_thous.contains(&400));
        assert!(band1_thous.contains(&100));
        assert!(
            cut_bands[1]
                .cut_planes
                .iter()
                .any(|cp| cp.is_floor && cp.top_thou == Thou(0))
        );
    }

    #[test]
    fn it_nests_bands_via_floor_nodes() {
        // Minimal setup: 2 rough bands, no labeled regions.
        let ply_im = ply_im_from_ascii(
            r#"
                111
                111
                111
            "#,
        );

        let ply_descs = vec![stub_ply_desc("dummy", 0, true), stub_ply_desc("ply100", 100, false)];

        let band_descs = vec![stub_band_desc(200, 100, "rough"), stub_band_desc(100, 0, "rough")];

        let region_im = RegionIm::new(ply_im.w, ply_im.h);
        let region_infos: Vec<LabelInfo> = Vec::new();

        let cut_bands = create_cut_bands(
            "rough",
            &ply_im,
            &band_descs,
            &region_im,
            &region_infos,
            &ply_descs,
        );

        let tree = create_region_tree(cut_bands);
        assert_eq!(tree.roots.len(), 1, "top band should produce a single floor node");

        match &tree.roots[0] {
            RegionNode::Floor { children, .. } => {
                assert_eq!(children.len(), 1, "top floor should reveal the next band's nodes");
                assert!(
                    matches!(&children[0], RegionNode::Floor { .. }),
                    "child should be the next band's floor node"
                );
            }
            RegionNode::Cut { .. } => panic!("root must be a floor node"),
        }
    }

    #[test]
    fn it_builds_complex_tree() {
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

        let ply_descs = vec![
            stub_ply_desc("dummy", 0, true),
            stub_ply_desc("ply100", 100, false),
            stub_ply_desc("ply400", 400, false),
            stub_ply_desc("ply700", 700, false),
            stub_ply_desc("ply900", 900, false),
        ];

        let band_descs = vec![
            stub_band_desc(1000, 650, "rough"),
            stub_band_desc(650, 0, "rough"),
        ];

        let (region_im, region_infos): (RegionIm, Vec<LabelInfo>) = {
            let (im, infos): (Im<u16, 1>, Vec<LabelInfo>) = label_im(&ply_im);
            (im.retag::<RegionI>(), infos)
        };

        // Dump the region_im as ascii for visual verification
        // println!("region_im (labels):\n{}", im_u16_to_ascii(&region_im));

        let expected_region_im: RegionIm = ply_im_from_ascii(
            r#"
                111111111111111111111111111111
                111222233333333333333333331111
                111222233333333333333333331111
                111333333333333333333333331111
                111333444444444444444433331111
                111333444455555554444433331111
                111333444455555554444433331111
                111333444444444444444433331111
                111333333333333333333333331111
                111333333333333333333333331111
            "#,
        )
        .retag::<RegionI>();

        assert_eq!(region_im, expected_region_im);

        // The example image has 5 connected components:
        // - value 1 appears in 2 disjoint regions (ocean + inner sea)
        // - values 2/3/4 each appear as a single region
        // label_im reserves index 0.
        assert_eq!(region_infos.len(), 6);

        let mut component_counts_by_ply_i: std::collections::HashMap<u16, usize> =
            std::collections::HashMap::new();
        for info in region_infos.iter().skip(1) {
            let ply_i_val = ply_im.arr[info.start_y * ply_im.s + info.start_x];
            *component_counts_by_ply_i.entry(ply_i_val).or_insert(0) += 1;
        }
        assert_eq!(component_counts_by_ply_i.get(&1).copied(), Some(2));
        assert_eq!(component_counts_by_ply_i.get(&2).copied(), Some(1));
        assert_eq!(component_counts_by_ply_i.get(&3).copied(), Some(1));
        assert_eq!(component_counts_by_ply_i.get(&4).copied(), Some(1));

        let cut_bands = create_cut_bands(
            "rough",
            &ply_im,
            &band_descs,
            &region_im,
            &region_infos,
            &ply_descs,
        );

        // Ensure create_cut_bands attached the right number of regions to each ply.
        let mut region_counts_by_ply_i: std::collections::HashMap<u16, usize> =
            std::collections::HashMap::new();
        for band in &cut_bands {
            for cp in &band.cut_planes {
                if cp.is_floor {
                    continue;
                }
                *region_counts_by_ply_i.entry(cp.ply_i.0).or_insert(0) += cp.region_iz.len();
            }
        }
        assert_eq!(region_counts_by_ply_i.get(&1).copied(), Some(2));
        assert_eq!(region_counts_by_ply_i.get(&2).copied(), Some(1));
        assert_eq!(region_counts_by_ply_i.get(&3).copied(), Some(1));
        assert_eq!(region_counts_by_ply_i.get(&4).copied(), Some(1));

        let region_tree = create_region_tree(cut_bands);

        // Structure expectations:
        // - Roots are the top band's siblings: [floor0, cut(ply900), cut(ply700)]
        // - floor0 reveals the next band's siblings: [floor1, cut(ply400), cut(ply100 outer), cut(ply100 inner)]
        assert_eq!(region_tree.roots.len(), 3);
        match &region_tree.roots[0] {
            RegionNode::Floor {
                band_i,
                children,
                ..
            } => {
                assert_eq!(*band_i, 0);
                assert_eq!(children.len(), 4);
                match &children[0] {
                    RegionNode::Floor {
                        band_i: child_band_i,
                        children: grand_children,
                        ..
                    } => {
                        assert_eq!(*child_band_i, 1);
                        assert!(grand_children.is_empty());
                    }
                    RegionNode::Cut { .. } => panic!("expected child[0] to be the next band's floor"),
                }
            }
            RegionNode::Cut { .. } => panic!("expected roots[0] to be a floor"),
        }

        // Count Cut nodes per band index.
        fn accumulate_cut_counts(nodes: &[RegionNode], counts: &mut Vec<usize>) {
            for n in nodes {
                match n {
                    RegionNode::Floor { band_i, children, .. } => {
                        if *band_i >= counts.len() {
                            counts.resize(*band_i + 1, 0);
                        }
                        accumulate_cut_counts(children, counts);
                    }
                    RegionNode::Cut { band_i, .. } => {
                        if *band_i >= counts.len() {
                            counts.resize(*band_i + 1, 0);
                        }
                        counts[*band_i] += 1;
                    }
                }
            }
        }

        let mut cut_counts_by_band: Vec<usize> = Vec::new();
        accumulate_cut_counts(&region_tree.roots, &mut cut_counts_by_band);

        // Band 0 has ply 700 and 900 => 2 regions.
        // Band 1 has ply 100 (2 regions) and 400 (1 region) => 3 regions.
        assert_eq!(cut_counts_by_band.get(0).copied(), Some(2));
        assert_eq!(cut_counts_by_band.get(1).copied(), Some(3));

        debug_print_cut_bands(&region_tree.cut_bands);
        debug_print_region_tree(&region_tree.roots[0], &ply_descs, &region_infos, 0);
    }
}
