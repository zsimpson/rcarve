use crate::desc::{BandDesc, PlyDesc};
use crate::desc::{Guid, Thou};
use crate::im::Im;
use crate::im::MaskIm;
use crate::im::label::LabelInfo;
use std::cmp::Ordering;
use std::collections::HashMap;

macro_rules! newtype {
    ($name:ident($inner:ty)) => {
        #[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
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
    ply_descs: &Vec<PlyDesc>, // All ply descriptions, indexed by ply_i (sorted bottom to top). Skip the [0] dummy
) -> Vec<CutBand> {
    let _ = region_im;

    // Assert that the ply_desc[0] is a dummy
    assert!(
        ply_descs
            .get(0)
            .map_or(false, |pd| pd.top_thou.0 == 0 && pd.hidden),
        "ply_descs[0] must be a dummy ply with top_thou 0 and hidden=true"
    );

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
        // Skip dummy
        if ply_i_usize == 0 {
            continue;
        }

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

        // Enforce invariants used by downstream consumers (e.g. create_region_tree).
        assert!(
            band.cut_planes.last().is_some_and(|cp| cp.is_floor),
            "invariant: floor CutPlane must be the last cut_plane in each CutBand"
        );
        debug_assert_eq!(
            band.cut_planes.iter().filter(|cp| cp.is_floor).count(),
            1,
            "invariant: each CutBand must contain exactly one floor CutPlane"
        );
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
                "  CutPlane[{}]: ply_guid={}, top_thou={:?}, ply_i={}, is_floor={}, num_regions={} ",
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

#[derive(Clone, Debug, Default)]
pub struct RegionRoot {
    pub children: Vec<RegionNode>,
}

#[derive(Clone, Debug)]
pub enum RegionNode {
    /// A floor node gates access to deeper bands.
    /// Cutting the floor reveals its children, which are 1+ regions in lower bands.
    Floor {
        band_i: usize,
        cut_plane_i: usize,
        /// The connected set of region labels that make up this floor component.
        ///
        /// This is computed from the region neighbor graph restricted to the regions
        /// that are strictly below this band's floor (i.e. in lower bands).
        region_iz: Vec<RegionI>,
        loweset_ply_i_in_band: PlyI,
        bottom_thou: Thou,
        children: Vec<RegionNode>,
    },
    /// A leaf region to cut (a single connected component at a specific ply in a band).
    Cut {
        band_i: usize,
        cut_plane_i: usize,
        region_i: RegionI,
    },
}

/// A RegionNode represents a single region. But the Floor nodes are special, they represent the union multiple regions below them. Many of those child regions will be contiguous but sometimes there mught be discontiguous parts. In the discontiguous case there'd be more than one floor RegionNode in a given band. So I need to rethink how to model this,. I'm thinknig that in create_region_tree in each band I make a new maskIm for each floor by using the ply_im to extrqact all pixels where the ply_i < the smallest ply_i of the current band. Then we call label on

/// Create a region tree for depth-first traversal.
///
/// This is intentionally minimal for now:
/// - Each band contributes a list of nodes (siblings): zero or more Cut leaves plus 1+ Floor nodes.
/// - Floor nodes represent connected components of the "below this band" region set.
///
/// This matches the semantics: the union of all pixels below a band must be cut (the floor)
/// before *any* region in lower bands can be cut.
/// Create a region tree root for depth-first traversal.
///
/// The returned root is a synthetic entry point that owns only the node forest;
/// `cut_bands` remain owned by the caller.
pub fn create_region_tree(cut_bands: &[CutBand], region_infos: &[LabelInfo]) -> RegionRoot {
    if cut_bands.is_empty() {
        return RegionRoot {
            children: Vec::new(),
        };
    }

    assert!(
        !region_infos.is_empty(),
        "region_infos must include index 0 (reserved/background)"
    );

    let mut nodes_per_band: Vec<Vec<RegionNode>> = Vec::with_capacity(cut_bands.len());

    // Assert that the bands are in top to bottom order (descending top_thou)
    for band_i in 0..cut_bands.len().saturating_sub(1) {
        let band_top = &cut_bands[band_i].top_thou;
        let next_band_top = &cut_bands[band_i + 1].top_thou;
        assert!(
            band_top > next_band_top,
            "invariant: cut_bands must be sorted from top to bottom by top_thou"
        );
    }

    // region_top_thou[r] is the top_thou for the CutPlane that owns region r.
    // Index 0 is reserved/background.
    let mut region_top_thou: Vec<Option<Thou>> = vec![None; region_infos.len()];
    for band in cut_bands {
        for cut_plane in &band.cut_planes {
            if cut_plane.is_floor {
                continue;
            }
            for &region_i in &cut_plane.region_iz {
                let ri = region_i.0 as usize;
                if ri == 0 || ri >= region_top_thou.len() {
                    continue;
                }
                region_top_thou[ri] = Some(cut_plane.top_thou.clone());
            }
        }
    }

    for (band_i, band) in cut_bands.iter().enumerate() {
        let floor_plane_i = band
            .cut_planes
            .len()
            .checked_sub(1)
            .expect("each CutBand must contain at least one CutPlane (the floor)");
        assert!(
            band.cut_planes[floor_plane_i].is_floor,
            "invariant: the floor CutPlane must be the last cut_plane in each CutBand"
        );

        let mut nodes_within_band: Vec<RegionNode> = Vec::new();
        for (cut_plane_i, cut_plane) in band.cut_planes.iter().enumerate() {
            if cut_plane.is_floor {
                continue;
            }
            for &region_i in &cut_plane.region_iz {
                nodes_within_band.push(RegionNode::Cut {
                    band_i,
                    cut_plane_i,
                    region_i,
                });
            }
        }

        // Build 1+ floor nodes for this band by finding the connected components of the
        // region-adjacency graph restricted to regions strictly below this band's floor.
        let mut is_below: Vec<bool> = vec![false; region_infos.len()];
        for region_id in 1..region_top_thou.len() {
            if let Some(thou) = &region_top_thou[region_id] {
                if thou.0 < band.bot_thou.0 {
                    is_below[region_id] = true;
                }
            }
        }

        let mut visited_region_iz: Vec<bool> = vec![false; region_infos.len()];
        let mut floor_region_iz: Vec<Vec<RegionI>> = Vec::new();

        for start in 1..region_infos.len() {
            if !is_below[start] || visited_region_iz[start] {
                continue;
            }

            let mut stack: Vec<usize> = vec![start];
            visited_region_iz[start] = true;
            let mut flooded_region_iz: Vec<RegionI> = Vec::new();
            while let Some(cur) = stack.pop() {
                flooded_region_iz.push(RegionI(cur as u16));
                for (&n, _shared_border) in region_infos[cur].neighbors.iter() {
                    if n == 0 || n >= region_infos.len() {
                        continue;
                    }
                    if !is_below[n] || visited_region_iz[n] {
                        continue;
                    }
                    visited_region_iz[n] = true;
                    stack.push(n);
                }
            }

            flooded_region_iz.sort_by(|a, b| a.0.cmp(&b.0));
            floor_region_iz.push(flooded_region_iz);
        }

        // Degenerate case: if there are no regions below this band, still create a single
        // floor node so traversal remains structurally uniform.
        if floor_region_iz.is_empty() {
            floor_region_iz.push(Vec::new());
        }

        for region_iz in floor_region_iz {
            nodes_within_band.push(RegionNode::Floor {
                band_i,
                cut_plane_i: floor_plane_i,
                region_iz,
                children: Vec::new(),
                loweset_ply_i_in_band: band
                    .cut_planes
                    .iter()
                    .filter(|cp| !cp.is_floor)
                    .map(|cp| cp.ply_i)
                    .min()
                    .unwrap_or(PlyI(0)),
                bottom_thou: band.bot_thou.clone(),
            });
        }

        nodes_per_band.push(nodes_within_band);
    }

    // Nest bands by wiring each band's floor children to the entire next band's nodes.
    for band_i in (0..nodes_per_band.len().saturating_sub(1)).rev() {
        // Move (not clone) the entire next band's nodes into this band's floor's children.
        // `std::mem::take` replaces the Vec with an empty one, letting us transfer ownership
        // while preserving the overall `nodes_per_band` structure during the reverse pass.
        let next_band_nodes = std::mem::take(&mut nodes_per_band[band_i + 1]);

        // Find the contiguous "floors" suffix in the parent band node list.
        let parent_nodes = &mut nodes_per_band[band_i];
        let first_floor_i = parent_nodes
            .iter()
            .position(|n| matches!(n, RegionNode::Floor { .. }))
            .expect("each band node list must contain at least one floor node");
        assert!(
            parent_nodes[first_floor_i..]
                .iter()
                .all(|n| matches!(n, RegionNode::Floor { .. })),
            "invariant: floor nodes must be contiguous at the end of each band"
        );

        let parent_floors_len = parent_nodes.len() - first_floor_i;
        assert!(parent_floors_len > 0);

        // Map region label -> which parent floor component it belongs to.
        // In the degenerate case (no below regions), the single floor has an empty `region_iz`,
        // and we route everything to that sole floor.
        let mut region_to_floor: HashMap<usize, usize> = HashMap::new();
        for (floor_off, node) in parent_nodes[first_floor_i..].iter().enumerate() {
            let RegionNode::Floor { region_iz, .. } = node else {
                unreachable!("checked floors suffix");
            };
            for r in region_iz {
                region_to_floor.insert(r.0 as usize, floor_off);
            }
        }

        let mut buckets: Vec<Vec<RegionNode>> = vec![Vec::new(); parent_floors_len];
        for child in next_band_nodes {
            let rep_region: Option<usize> = match &child {
                RegionNode::Cut { region_i, .. } => Some(region_i.0 as usize),
                RegionNode::Floor { region_iz, .. } => region_iz.first().map(|r| r.0 as usize),
            };

            let floor_off = rep_region
                .and_then(|rid| region_to_floor.get(&rid).copied())
                .unwrap_or(0);
            buckets[floor_off].push(child);
        }

        for floor_off in 0..parent_floors_len {
            let node = &mut parent_nodes[first_floor_i + floor_off];
            match node {
                RegionNode::Floor { children: c, .. } => {
                    *c = std::mem::take(&mut buckets[floor_off]);
                }
                RegionNode::Cut { .. } => unreachable!("floors suffix must contain only floors"),
            }
        }
    }

    fn prune_empty_floors(nodes: &mut Vec<RegionNode>) {
        for n in nodes.iter_mut() {
            match n {
                RegionNode::Floor { children, .. } => prune_empty_floors(children),
                RegionNode::Cut { .. } => {}
            }
        }

        // Remove Floor nodes that don't gate anything.
        nodes.retain(|n| match n {
            RegionNode::Floor { children, .. } => !children.is_empty(),
            _ => true,
        });
    }

    let mut roots = std::mem::take(&mut nodes_per_band[0]);
    prune_empty_floors(&mut roots);

    RegionRoot { children: roots }
}

pub fn debug_print_region_tree(
    root: &RegionRoot,
    cut_bands: &[CutBand],
    region_infos: &[LabelInfo],
    indent: usize,
) {
    let indent_str = " ".repeat(indent);
    println!("{}Root: num_children={}", indent_str, root.children.len());

    fn debug_print_region_tree_node(
        node: &RegionNode,
        cut_bands: &[CutBand],
        region_infos: &[LabelInfo],
        indent: usize,
    ) {
        let indent_str = " ".repeat(indent);
        match node {
            RegionNode::Floor {
                band_i,
                cut_plane_i,
                region_iz,
                children,
                ..
            } => {
                let cp = &cut_bands[*band_i].cut_planes[*cut_plane_i];
                debug_assert!(cp.is_floor);
                println!(
                    "{}Floor: band_i={}, cut_plane_i={}, ply_guid={}, top_thou={:?}, num_floor_regions={}, num_children={}",
                    indent_str,
                    band_i,
                    cut_plane_i,
                    cp.ply_guid.0,
                    cp.top_thou,
                    region_iz.len(),
                    children.len()
                );
                for child in children {
                    debug_print_region_tree_node(child, cut_bands, region_infos, indent + 2);
                }
            }
            RegionNode::Cut {
                band_i,
                cut_plane_i,
                region_i,
            } => {
                let cp = &cut_bands[*band_i].cut_planes[*cut_plane_i];
                debug_assert!(!cp.is_floor);
                let region_info = &region_infos[region_i.0 as usize];
                println!(
                    "{}Cut: band_i={}, cut_plane_i={}, ply_guid={}, top_thou={:?}, region_i={}, region_size={}",
                    indent_str,
                    band_i,
                    cut_plane_i,
                    cp.ply_guid.0,
                    cp.top_thou,
                    region_i.0,
                    region_info.size
                );
            }
        }
    }

    for child in &root.children {
        debug_print_region_tree_node(child, cut_bands, region_infos, indent + 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::im::label::label_im;
    use crate::test_helpers::{ply_im_from_ascii, stub_band_desc, stub_ply_desc};

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
        let region_infos: Vec<LabelInfo> = vec![LabelInfo::default()];

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

        let ply_descs = vec![
            stub_ply_desc("dummy", 0, true),
            stub_ply_desc("ply100", 100, false),
        ];

        let band_descs = vec![
            stub_band_desc(200, 100, "rough"),
            stub_band_desc(100, 0, "rough"),
        ];

        let region_im = RegionIm::new(ply_im.w, ply_im.h);
        let region_infos: Vec<LabelInfo> = vec![LabelInfo::default()];

        let cut_bands = create_cut_bands(
            "rough",
            &ply_im,
            &band_descs,
            &region_im,
            &region_infos,
            &ply_descs,
        );

        let root = create_region_tree(&cut_bands, &region_infos);

        // With no labeled regions, there are no Cut nodes, and (after pruning)
        // there is no need to keep Floor nodes that don't gate anything.
        assert_eq!(root.children.len(), 0);
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
            // bottom band
            stub_ply_desc("ply100", 100, false), // [1]
            stub_ply_desc("ply400", 400, false), // [2]
            // top band
            stub_ply_desc("ply700", 700, false), // [3]
            stub_ply_desc("ply900", 900, false), // [4]
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

        let region_root = create_region_tree(&cut_bands, &region_infos);
        let root_children = &region_root.children;

        let root_floors: Vec<&RegionNode> = root_children
            .iter()
            .filter(|n| matches!(n, RegionNode::Floor { .. }))
            .collect();
        assert!(!root_floors.is_empty(), "top band must have 1+ floor nodes");

        let mut total_children = 0usize;
        for f in &root_floors {
            let RegionNode::Floor {
                band_i, children, ..
            } = *f
            else {
                unreachable!();
            };
            assert_eq!(*band_i, 0);
            total_children += children.len();
        }
        // Band 1's floor is pruned because it has no children, so only the 3 cut nodes remain.
        assert_eq!(total_children, 3, "next band should contribute 3 cut nodes");

        // Count Cut nodes per band index.
        fn accumulate_cut_counts(nodes: &[RegionNode], counts: &mut Vec<usize>) {
            for n in nodes {
                match n {
                    RegionNode::Floor {
                        band_i, children, ..
                    } => {
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
        accumulate_cut_counts(&region_root.children, &mut cut_counts_by_band);

        // Band 0 has ply 700 and 900 => 2 regions.
        // Band 1 has ply 100 (2 regions) and 400 (1 region) => 3 regions.
        assert_eq!(cut_counts_by_band.get(0).copied(), Some(2));
        assert_eq!(cut_counts_by_band.get(1).copied(), Some(3));

        debug_print_cut_bands(&cut_bands);
        println!();

        debug_print_region_tree(&region_root, &cut_bands, &region_infos, 0);
    }
}
