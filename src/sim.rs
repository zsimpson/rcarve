use crate::im::Lum16Im;
use crate::region_tree::CutBand;
use crate::toolpath::ToolPath;

/// Simulate toolpaths and into a Lum16Im representing the result.
pub fn sim_toolpaths(
    im: &mut Lum16Im,
    toolpaths: &Vec<ToolPath>,
    cut_bands: &Vec<CutBand>,
    w: usize,
    h: usize,
) {
}
