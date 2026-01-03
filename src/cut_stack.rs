use crate::desc::{BandDesc, PlyDesc};
use crate::im::{MaskIm};
use crate::desc::{Thou, Guid};
use crate::im::label::LabelInfo;
use crate::im::Im;

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
    pub ply_guid: Guid,  // For debugging and reference
    pub top_thou: Thou,  // Cloned from PlyDesc
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
    pub top_thou: Thou,  // Exclusive
    pub bot_thou: Thou,  // Inclusive
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
/// Every Cutband contains at exactly 1 "floor" CutPlane which represents areas that contain pixel lower than this band.
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

/// The goal of this module is to build the sorted tree of CutRegion nodes.
/// This requires a call to create_cut_bands() which creates the CutBands and CutPlanes
/// and then a call to build_cut_region_tree() which renders the product (dilation, masking, etc)
/// and then builds the CutRegion tree for depth-first carving.

pub struct CutRegion {
    pub cut_plane: CutPlane,
    pub region_i: RegionI,
    pub child_regions: Vec<CutRegion>,
}

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
    cut_pass: &str,  // A name for the cut pass, e.g. "rough", "refine_smooth", "refine_perimeter", "detail"
    ply_im: &PlyIm,  // The ply vals are sorted so that higher thou values have higher ply vals.
    band_descs: &[BandDesc],  // All band descriptions (will be filtered by cut_pass), gives thou ranges for each (will be cloned into CutBand)
    region_im: &RegionIm,  // The labeled connected component image from labeling the ply_im
    region_infos: &[LabelInfo],  // The connected component infos from labeling the ply_im
    ply_descs: &Vec<PlyDesc>,  // All ply descriptions, indexed by ply_i (sorted bottom to top)
) -> Vec<CutBand> {
    let _ = region_im;

    let mut cut_bands: Vec<CutBand> = band_descs
        .iter()
        .filter(|bd| bd.cut_pass == cut_pass)
        .map(|bd| {
            CutBand {
                band_desc: bd.clone(),
                top_thou: bd.top_thou.clone(),
                bot_thou: bd.bot_thou.clone(),
                cut_planes: Vec::new(),
            }
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
                    pos_work_im: None, // To be filled in later
                    cut_im: None, // To be filled in later
                    has_overcut: false,  // Overcut logic removed for now
                    is_floor: false,
                    region_iz: Vec::new(),  // To be filled in below
                };

                band.cut_planes.push(cut_plane);
                break;
            }
        }
    }

    // Go back through the bands. If a band has a ply that happens to be equal to its floor
    // then mark that ply as the floor ply. If it doesn't then create a dummy floor ply.
    // Sort all cut_planes by top_thou from top to bottom.
    for (band_i, band) in cut_bands.iter_mut().enumerate() {
        let mut has_floor = false;
        for cut_plane in band.cut_planes.iter_mut() {
            if cut_plane.top_thou == band.bot_thou {
                cut_plane.is_floor = true;
                has_floor = true;
                break;
            }
        }
        if !has_floor {
            // Create a dummy floor ply
            let dummy_floor_ply = CutPlane {
                ply_guid: Guid(format!("floor_{}", band_i)),
                top_thou: band.bot_thou.clone(),
                ply_i: PlyI(0),  // Dummy ply_i
                pos_work_im: None, // To be filled in later
                cut_im: None, // To be filled in later
                has_overcut: false,
                is_floor: true,
                region_iz: Vec::new(),
            };
            band.cut_planes.push(dummy_floor_ply);
        }
    }

    // Now assign region_iz to each CutPlane by scanning the region_info
    // The region_info contains a point in the set (start_x, start_y)
    // and that can be used to lookup the ply_i in the ply_im.
    // From the ply_i we can find the corresponding CutPlane and add the region_i to its list.
    for (region_i_usize, region_info) in region_infos.iter().enumerate() {
        let region_i = RegionI(region_i_usize as u16);
        let (x, y) = (region_info.start_x, region_info.start_y);
        let x = x as usize;
        let y = y as usize;
        assert!(x < ply_im.w && y < ply_im.h, "region start point is outside ply_im bounds");
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

    // Deduplicate region_iz lists
    for band in cut_bands.iter_mut() {
        for cut_plane in band.cut_planes.iter_mut() {
            cut_plane.region_iz.sort_by(|a, b| a.0.cmp(&b.0));
            cut_plane.region_iz.dedup();
        }
    }

    cut_bands
}


#[cfg(test)]
mod tests {
    use super::*;

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
            BandDesc {
                top_thou: Thou(1000),
                bot_thou: Thou(650),
                cut_pass: "rough".to_string(),
            },
            BandDesc {
                top_thou: Thou(650),
                bot_thou: Thou(0),
                cut_pass: "rough".to_string(),
            },
            BandDesc {
                top_thou: Thou(1000),
                bot_thou: Thou(0),
                cut_pass: "refine".to_string(),  // Should be ignored
            },
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
        let band0_thous: Vec<i32> = cut_bands[0].cut_planes.iter().map(|cp| cp.top_thou.0).collect();
        assert!(band0_thous.contains(&900));
        assert!(band0_thous.contains(&700));
        assert!(cut_bands[0]
            .cut_planes
            .iter()
            .any(|cp| cp.is_floor && cp.top_thou == Thou(650)));

        // Band 1 should include plies at 400 and 100, and use the dummy ply at 0 as the floor.
        let band1_thous: Vec<i32> = cut_bands[1].cut_planes.iter().map(|cp| cp.top_thou.0).collect();
        assert!(band1_thous.contains(&400));
        assert!(band1_thous.contains(&100));
        assert!(cut_bands[1]
            .cut_planes
            .iter()
            .any(|cp| cp.is_floor && cp.top_thou == Thou(0)));
    }
}