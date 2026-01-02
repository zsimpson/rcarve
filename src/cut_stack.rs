use crate::desc::{BandDesc, PolyDesc};
use crate::im::{Lum16Im, MaskIm};
use crate::desc::{Thou, Guid};
use std::convert::TryFrom;

macro_rules! newtype {
    ($name:ident($inner:ty)) => {
        #[derive(Copy, Clone, Debug, Eq, PartialEq)]
        pub struct $name(pub $inner);
    };
}

newtype!(PlyI(u16));
newtype!(PlyIm(Lum16Im));

newtype!(RegionI(u16));
newtype!(RegionIm(Lum16Im));

pub struct CutPlane {
    pub ply_guid: Guid,
    pub top_thou: Thou,
    pub ply_i: PlyI,
    pub pos_work_im: MaskIm,
    pub cut_im: MaskIm,
    pub has_overcut: bool,
    pub is_floor: bool,
    pub region_iz: Vec<RegionI>, // A ply can have multiple labels if it's discontiguous
}

pub struct CutBand {
    pub band_desc: &BandDesc,
    pub top_thou: Thou,
    pub bot_thou: Thou,
    pub cut_planes: Vec<CutPlane>,  // sorted top to bottom
}

pub fn create_cut_bands(
    cut_pass: &str,
    ply_im: &PlyIm,
    // ply_to_thou: &HashMap<PlyVal, Thou>,
    band_descs: &[BandDesc],
    region_infos: &[LabelInfo],
) -> Vec<CutBand> {
    let mut cut_bands: Vec<CutBand> = band_descs
        .iter()
        .filter(|bd| bd.cut_pass == cut_pass)
        .map(|bd| {
            CutBand {
                band_desc: bd,
                top_thou: bd.top_thou as Thou,
                bot_thou: bd.bot_thou as Thou,
                cut_planes: Vec::new(),
            }
        })
        .collect();

    
    for (region_i, info) in region_infos.iter().enumerate().skip(1) {  // skip(1) because region[0] is reserved.
        let region_i = RegionI::try_from(region_i)
            .unwrap_or_else(|_| panic!("too many labels for LabelVal (u16): {region_i}"));

        // Representative pixel for this connected component.
        let sep_val: PlyVal = ply_im.arr[info.start_y * ply_im.s + info.start_x];
        if sep_val == 0 {
            // Should not happen since label_im skips background, but be tolerant.
            continue;
        }

        let thou: Thou = *ply_to_thou
            .get(&sep_val)
            .unwrap_or_else(|| panic!("missing sep_to_thou entry for sep label {sep_val}"));

        // Assign this labeled component into every matching band (top inclusive, bot exclusive).
        // Bands may overlap (e.g. per-tool bands with the same range), so don't stop at the first.
        let mut assigned_any = false;
        for band in cut_bands.iter_mut() {
            let band_top = band.top_thou.max(band.bot_thou);
            let band_bot = band.top_thou.min(band.bot_thou);
            if band_bot < thou && thou <= band_top {
                band.label_ids.push(label_id);
                band.thous_top_to_bot.push(thou);
                assigned_any = true;
            }
        }

        if !assigned_any {
            panic!("label {label_id} (sep_val={sep_val}, thou={thou}) did not fit any band");
        }
    }

    for band in cut_bands.iter_mut() {
        // Top-to-bottom: descending thou values.
        band.thous_top_to_bot.sort_by(|a, b| b.cmp(a));
        band.thous_top_to_bot.dedup();
    }

    cut_bands
}




// TODO: Move the a better doc
// There's this important thing about hole filling that i reall yhave to
// get straight here

// Within a single band there are different top thous
// suppose that there is a small area high (small island) in the band
// and a huge area that it low (sea). We don't want to clear the
// whole band at the height of the island. So in that case it
// makes sense to carve the perimeter of the island down to the sea
// then excvate the top of the island then the hufe expanse of the sea

// But now conside the case where there's huge complex island with some
// small lakes. It makes the most sense in that case to excavate
// to the top of the island including over the lakes and then
// come back and perimter the lakes and go down.

// Let "overcut" be the situation where is is advantageous
// to cut over a lake without outlining it. What I called "hole filling"
// before.

// Imagine that the for loop over the top-to-bottom sorted top_thous
// of the plies within a band. We need to decide, should anything BELOW
// this ply be included in the excavation? That is, are there
// areas proximal to this ply's pixels that should be overcut?

// Try to grow the ply's pixels for over cut
// Assume there's a neighbor table, ie for each region there's a list
// of neighboring regions.

// For each neightbor, consider adding it to the over cut
// In so doing we're adding surfacing removals but we're reducing perimeter cuts
// We need a metric of how much each of those cost

// If we add that neighbor region to the overcut then we've added surfacing removal
// but we've removed the perimeter at the boundary between the two regions because
// when the lower region is cut is boundary will reveal the top boundary

// Think of a square with a diving line somewhere along the x axis

// Suppose A is above B.


//  +--tA----+-----tB-----+
//  |        |            |
// lA  A   shared   B     rB
//  |        |            |
//  +--bA----+-----bB-----+

// As x moves to the right A area gests bigger.

// If we don't overcut B then the operations are:
//     X perim(unshared of A)
//     perim(shared along A side)
//     X perim(shared along B side)
//     X perim(unshared of B)
//     X area(A)
//     X area(B)

// If we do overcut B then the operations are:
//     X perim(unshared of A)
//     X perim(unshared of B)
//     X area(A)
//     X area(B)
//     area(B) -- again!
//     X perim(shared along B side)


// So choosing to overcut removes "perim(shared along A side)" but it adds an "area(B)"

// We should only overcut if the cost of the extra area(B) is less than the cost of the shared perimeter.
// Plus perimeters are a little slower than areas so we can weight them.

// Note that this is the shared boundry, not the total boundary which isn't what we did last time.


