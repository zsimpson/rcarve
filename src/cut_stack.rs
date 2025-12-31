use crate::desc::BandDesc;
use crate::im::Im;
use crate::im::LabelInfo;

use std::collections::HashMap;

type Thou = i32; // in thousandths of an inch

type LabelVal = u16;
type LabelIm = Im<LabelVal, 1>;

type SepVal = u16;
type SepIm = Im<SepVal, 1>;

// pub struct ROI {
//     pub x: usize,
//     pub y: usize,
//     pub w: usize,
//     pub h: usize,
// }

// There's different band struct for different tools / operations!
// I'll continue to assume tht these will be made by the client side
// in case they need to be editable.

// But that means that for a given operaiton (rough, etc) that I need
// to allocate a Band struct for each band and populate it wil the right data
// So that's a function

pub struct Band {
    // pub band_desc: &BandDesc,
    pub top_thou: Thou,
    pub bot_thou: Thou,
    pub thous_top_to_bot: Vec<Thou>,
    pub label_ids: Vec<LabelVal>, // A ply can have multiple labels if it's discontiguous
}

// Note sure yet how I'm making these from Descs so I just want to hard od them for now
// pub fn create_bands_rough(band_descs: &[&BandDesc], ) {
//     for bd in band_descs {
//         if bd.mode == "rough" {
//             let band = Band {
//                 band_desc: &bd,
//                 top_thou: bd.top_thou as Thou,
//                 bot_thou: bd.bot_thou as Thou,
//                 thous_top_to_bot: Vec::new(),
//                 label_ids: Vec::new(),
//             };
//         }
//     }
// }

#[allow(dead_code)]
pub fn create_bands(
    sep_im: &SepIm,
    sep_to_thou: &HashMap<LabelVal, Thou>,
    band_descs: &[BandDesc],
    label_infos: &[LabelInfo],
) -> Vec<Band> {
    // Label connected components in `sep_im` (treating 0 as background).
    // Each label corresponds to one contiguous region with a single `sep_im` value.
    // let (_label_id_im, label_infos): (Im<LabelVal>, Vec<LabelInfo>) = label_im(sep_im);

    let mut bands: Vec<Band> = band_descs
        .iter()
        .map(|bd| Band {
            top_thou: bd.top_thou as Thou,
            bot_thou: bd.bot_thou as Thou,
            thous_top_to_bot: Vec::new(),
            label_ids: Vec::new(),
        })
        .collect();

    // label_infos[0] is reserved.
    for (label_i, info) in label_infos.iter().enumerate().skip(1) {
        let label_id: LabelVal = label_i
            .try_into()
            .unwrap_or_else(|_| panic!("too many labels for LabelVal (u16): {label_i}"));

        // Representative pixel for this connected component.
        let sep_val: LabelVal = sep_im.arr[info.start_y * sep_im.s + info.start_x];
        if sep_val == 0 {
            // Should not happen since label_im skips background, but be tolerant.
            continue;
        }

        let thou: Thou = *sep_to_thou
            .get(&sep_val)
            .unwrap_or_else(|| panic!("missing sep_to_thou entry for sep label {sep_val}"));

        // Assign this labeled component into the first matching band (top inclusive, bot exclusive).
        let mut assigned = false;
        for band in bands.iter_mut() {
            let band_top = band.top_thou.max(band.bot_thou);
            let band_bot = band.top_thou.min(band.bot_thou);
            if band_bot < thou && thou <= band_top {
                band.label_ids.push(label_id);
                band.thous_top_to_bot.push(thou);
                assigned = true;
                break;
            }
        }

        if !assigned {
            panic!("label {label_id} (sep_val={sep_val}, thou={thou}) did not fit any band");
        }
    }

    for band in bands.iter_mut() {
        // Top-to-bottom: descending thou values.
        band.thous_top_to_bot.sort_by(|a, b| b.cmp(a));
        band.thous_top_to_bot.dedup();
    }

    bands
}

fn create_cut_stack(sep_im: &SepIm, label_id_im: LabelIm, label_infos:Vec<LabelInfo>, bands: &Vec<Band>) {
    // For each band 
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::im::label_im;

    fn rough_band_desc(top_thou: i32, bot_thou: i32) -> BandDesc {
        BandDesc {
            top_thou,
            bot_thou,
            mode: "rough".to_string(),
        }
    }

    fn rough_two_bands() -> Vec<BandDesc> {
        vec![rough_band_desc(1000, 750), rough_band_desc(750, 500)]
    }

    fn make_sep_im_equal_divisions(dim: usize, divisions: u16) -> SepIm {
        assert!(divisions > 0, "divisions must be > 0");
        let divisions_usize = divisions as usize;

        let mut sep_im: SepIm = SepIm::new(dim, dim);
        for y in 0..dim {
            for x in 0..dim {
                // Map x in [0, dim) into labels 1..=divisions as evenly as possible.
                // This works even when `dim` is not divisible by `divisions`.
                let label = (x * divisions_usize) / dim + 1;
                sep_im.arr[y * sep_im.s + x] = label as u16;
            }
        }
        sep_im
    }

    // fn make_squares_within_squares_sep_im(
    //     dim: usize,
    //     square_size: usize,
    //     gap_size: usize,
    // ) -> SepIm {
    //     let mut sep_im: SepIm = SepIm::new(dim, dim);
    //     let mut label: u16 = 1;

    //     let step = square_size + gap_size;
    //     for y_start in (0..dim).step_by(step) {
    //         for x_start in (0..dim).step_by(step) {
    //             // Fill square
    //             for y in y_start..(y_start + square_size).min(dim) {
    //                 for x in x_start..(x_start + square_size).min(dim) {
    //                     sep_im.arr[y * sep_im.s + x] = label;
    //                 }
    //             }
    //             label += 1;
    //         }
    //     }

    //     sep_im
    // }

    #[test]
    fn it_creates_assigns_to_bands() {
        const DIM: usize = 100;

        let sep_im = make_sep_im_equal_divisions(DIM, 2);

        let band_descs = rough_two_bands();

        let sep_to_thou: HashMap<LabelVal, Thou> = HashMap::from([(1, 900), (2, 800)]);

        let (_label_id_im, label_infos): (Im<LabelVal, 1>, Vec<LabelInfo>) = label_im(&sep_im);

        let bands = create_bands(&sep_im, &sep_to_thou, &band_descs, &label_infos);

        assert_eq!(bands.len(), 2);

        // Both regions map into the first band (750 < thou <= 1000).
        assert_eq!(bands[0].top_thou, 1000);
        assert_eq!(bands[0].bot_thou, 750);
        assert_eq!(bands[0].label_ids, vec![1, 2]);
        assert_eq!(bands[0].thous_top_to_bot, vec![900, 800]);

        // Second band ends up empty for this mapping.
        assert_eq!(bands[1].top_thou, 750);
        assert_eq!(bands[1].bot_thou, 500);
        assert!(bands[1].label_ids.is_empty());
        assert!(bands[1].thous_top_to_bot.is_empty());
    }

    #[test]
    fn it_splits_bands_with_boundary() {
        const DIM: usize = 100;

        let sep_im = make_sep_im_equal_divisions(DIM, 3);

        let band_descs = rough_two_bands();

        let sep_to_thou: HashMap<LabelVal, Thou> = HashMap::from([(1, 900), (2, 800), (3, 700)]);

        let (_label_id_im, label_infos): (SepIm, Vec<LabelInfo>) = label_im(&sep_im);

        let bands = create_bands(&sep_im, &sep_to_thou, &band_descs, &label_infos);
        assert_eq!(bands.len(), 2);

        assert_eq!(bands[0].top_thou, 1000);
        assert_eq!(bands[0].bot_thou, 750);
        assert_eq!(bands[0].label_ids, vec![1, 2]);
        assert_eq!(bands[0].thous_top_to_bot, vec![900, 800]);

        assert_eq!(bands[1].top_thou, 750);
        assert_eq!(bands[1].bot_thou, 500);
        assert_eq!(bands[1].label_ids, vec![3]);
        assert_eq!(bands[1].thous_top_to_bot, vec![700]);
    }
}

//     #[test]
//     fn it_creates_a_cut_stack() {
//         let sep_im = make_squares_within_squares_sep_im(250, 25, 10);
//     }
// }

/*
The input is plies
Each ply has a top_thou:Thou and an mpoly
Once for the whole stack we need to create label_im
    This will be used for recursion during tool path generation
For each band
    generate the offseted perimeters


There's this important thing about hole filling that i reall yhave to
get straight here

Within a single band there are different top thous
suppose that there is a small area high (small island) in the band
and a huge area that it low (sea). We don't want to clear the
whole band at the height of the island. So in that case it
makes sense to carve the perimeter of the island down to the sea
then excvate the top of the island then the hufe expanse of the sea

But now conside the case where there's huge complex island with some
small lakes. It makes the most sense in that case to excavate
to the top of the island including over the lakes and then
come back and perimter the lakes and go down.

Let "overcut" be the situation where is is advantageous
to cut over a lake without outlining it. What I called "hole filling"
before.

Imagine that the for loop over the top-to-bottom sorted top_thous
of the plies within a band. We need to decide, should anything BELOW
this ply be included in the excavation? That is, are there
areas proximal to this ply's pixels that should be overcut?

Try to grow the ply's pixels for over cut
Assume there's a neighbor table, ie for each region there's a list
of neighboring regions.

For each neightbor, consider adding it to the over cut
In so doing we're adding surfacing removals but we're reducing perimeter cuts
We need a metric of how much each of those cost

If we add that neighbor region to the overcut then we've added surfacing removal
but we've removed the perimeter at the boundary between the two regions because
when the lower region is cut is boundary will reveal the top boundary

Think of a square with a diving line somewhere along the x axis

Suppose A is above B.


 +--tA----+-----tB-----+
 |        |            |
lA  A   shared   B     rB
 |        |            |
 +--bA----+-----bB-----+

As x moves to the right A area gests bigger.

If we don't overcut B then the operations are:
    X perim(unshared of A)
    perim(shared along A side)
    X perim(shared along B side)
    X perim(unshared of B)
    X area(A)
    X area(B)

If we do overcut B then the operations are:
    X perim(unshared of A)
    X perim(unshared of B)
    X area(A)
    X area(B)
    area(B) -- again!
    X perim(shared along B side)


So choosing to overcut removes "perim(shared along A side)" but it adds an "area(B)"

We should only overcut if the cost of the extra area(B) is less than the cost of the shared perimeter.
Plus perimeters are a little slower than areas so we can weight them.

Note that this is the shared boundry, not the total boundary which isn't what we did last time.


For each band we need to generate

the vectors for each
*/

// Each band is going to end up with a set of
// perimeters

// fn cut_stack(sep_im:SepIm, roi:ROI, bands:Vec<Thou>) {
// }

// pub fn rough_cut_stack(sep_im:SepIm, roi:ROI, ) {
//     // For each
// }
