use serde::Deserialize;
use std::fmt;
use std::collections::HashMap;

macro_rules! transparent_newtype {
    ($name:ident($inner:ty)) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub $inner);
    };
}

macro_rules! transparent_newtype_copy {
    ($name:ident($inner:ty)) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub $inner);
    };
}

transparent_newtype_copy!(Thou(i32));

transparent_newtype!(Guid(String));
impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// At some point I might consider a Deref impl here, but for now keep it simple.
type FlatVerts = Vec<i32>;

// Parsed `mpoly` coordinates are expected to be in normalized/real units after applying `ply_mat`.
// Those values can be fractional (e.g. 0.2), so we store them as fixed-point integers until we
// later convert to pixel-space.
const MPOLY_NORM_FIXED_DENOM: f64 = 1_000_000.0;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    Inch,
    Mm,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompDesc {
    pub version: u32,
    pub guid: Guid,
    pub dim_desc: DimDesc,
    pub ply_desc_by_guid: HashMap<Guid, PlyDesc>,
    pub layer_desc_by_guid: HashMap<Guid, LayerDesc>,
    #[serde(default)]
    pub tool_descs: Vec<ToolDesc>,
    pub carve_desc: CarveDesc,
    #[serde(default)]
    pub bands: Vec<BandDesc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolDesc {
    pub guid: Guid,
    pub units: Units,
    pub kind: String,
    pub diameter: f64,
    pub length: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DimDesc {
    pub bulk_d_inch: f64,
    pub bulk_w_inch: f64,
    pub bulk_h_inch: f64,
    pub padding_inch: f64,
    pub frame_inch: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PolyDesc {
    pub exterior: FlatVerts,
    #[serde(default)]
    pub holes: Vec<FlatVerts>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(from = "PlyDescRaw")]
pub struct PlyDesc {
    pub owner_layer_guid: Guid,
    pub guid: Guid,
    pub top_thou: Thou,
    pub hidden: bool,
    pub is_floor: bool,
    #[serde(default = "default_ply_mat")]
    pub ply_mat: Vec<f32>,
    #[serde(default)]
    pub mpoly: Vec<crate::mpoly::MPoly>,
}

#[derive(Debug, Clone, Deserialize)]
struct PlyDescRaw {
    pub owner_layer_guid: Guid,
    pub guid: Guid,
    pub top_thou: Thou,
    pub hidden: bool,
    pub is_floor: bool,
    #[serde(default = "default_ply_mat")]
    pub ply_mat: Vec<f32>,
    #[serde(default)]
    pub mpoly: Vec<PolyDesc>,
}

impl From<PlyDescRaw> for PlyDesc {
    fn from(raw: PlyDescRaw) -> Self {
        let ply_xform = crate::mat3::Mat3::from_ply_mat(&raw.ply_mat).unwrap_or_default();
        let mpoly = raw
            .mpoly
            .iter()
            .map(|pd| polydesc_to_mpoly(pd, &ply_xform))
            .collect();

        Self {
            owner_layer_guid: raw.owner_layer_guid,
            guid: raw.guid,
            top_thou: raw.top_thou,
            hidden: raw.hidden,
            is_floor: raw.is_floor,
            ply_mat: raw.ply_mat,
            mpoly,
        }
    }
}


fn default_ply_mat() -> Vec<f32> {
    vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}

#[derive(Debug, Clone, Deserialize)]
pub struct LayerDesc {
    pub guid: Guid,
    pub hidden: bool,
    pub is_frame: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CarveDesc {
    pub grain_y: bool,
    pub rough_tool_guid: Option<Guid>,
    pub refine_tool_guid: Option<Guid>,
    pub detail_tool_guid: Option<Guid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BandDesc {
    pub top_thou: Thou,
    pub bot_thou: Thou,
    pub cut_pass: String,
}

pub fn parse_comp_json(json_text: &str) -> Result<CompDesc, serde_json::Error> {
    serde_json::from_str(json_text)
}

impl CompDesc {
    /// Applies an axis-aligned affine transform to every `mpoly` in every `PlyDesc`.
    ///
    /// The transform is applied as: `x' = round(x * sx) + dx`, `y' = round(y * sy) + dy`.
    ///
    /// This is useful when JSON parsing produces normalized/real-unit coordinates and you want
    /// to convert into pixel space.
    pub fn with_adjusted_mpolys(mut self, translation: (i64, i64), scale: (f64, f64)) -> Self {
        let (dx, dy) = translation;
        let (sx, sy) = scale;

        for ply_desc in self.ply_desc_by_guid.values_mut() {
            for mpoly in ply_desc.mpoly.iter_mut() {
                *mpoly = mpoly.scaled_translated_div(sx, sy, MPOLY_NORM_FIXED_DENOM, dx, dy);
            }
        }

        self
    }

    /// Convenience helper for converting normalized/real-unit geometry into pixels.
    ///
    /// Equivalent to `with_adjusted_mpolys((0, 0), (pixels_per_unit, pixels_per_unit))`.
    pub fn with_mpolys_scaled_to_pixels(self, pixels_per_unit: f64) -> Self {
        self.with_adjusted_mpolys((0, 0), (pixels_per_unit, pixels_per_unit))
    }
}

use crate::mat3::Mat3;
use crate::mpoly::{IntPath, IntPoint, MPoly};
pub fn polydesc_to_mpoly(polydesc: &PolyDesc, ply_xform: &Mat3) -> MPoly {
    fn flat_verts_to_path(flat: &[i32], ply_xform: &Mat3) -> Option<IntPath> {
        // flat = [x0, y0, x1, y1, ...]
        // Need at least 3 points (6 ints).
        if flat.len() < 6 {
            return None;
        }

        let n = flat.len() - (flat.len() % 2);
        if n < 6 {
            return None;
        }

        let mut pts: Vec<IntPoint> = Vec::with_capacity(n / 2);
        for xy in flat[..n].chunks_exact(2) {
            let (x, y) = ply_xform.transform_point2(xy[0] as f64, xy[1] as f64);
            pts.push(IntPoint::from_scaled(
                (x * MPOLY_NORM_FIXED_DENOM).round() as i64,
                (y * MPOLY_NORM_FIXED_DENOM).round() as i64,
            ));
        }

        Some(IntPath::new(pts))
    }

    let mut paths: Vec<IntPath> = Vec::with_capacity(1 + polydesc.holes.len());
    if let Some(ext) = flat_verts_to_path(&polydesc.exterior, ply_xform) {
        paths.push(ext);
    }
    for hole in &polydesc.holes {
        if let Some(h) = flat_verts_to_path(hole, ply_xform) {
            paths.push(h);
        }
    }

    MPoly::new(paths)
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpolys_scale_from_ply_mat_normalized_units_into_pixels() {
        // JSON vertices are in 0..500 units. `ply_mat` scales by 0.002, so coordinates become 0..1
        // (normalized). Then `with_adjusted_mpolys` should map normalized coords into pixels using:
        //   scale = (bulk_w_inch * ppi, bulk_h_inch * ppi)
        //   translation = (frame_inch * ppi, frame_inch * ppi)
        let sample = r#"
        {
            "version": 3,
            "guid": "G",
            "dim_desc": {
                "bulk_d_inch": 1.0,
                "bulk_w_inch": 4.0,
                "bulk_h_inch": 4.0,
                "padding_inch": 0.0,
                "frame_inch": 0.5
            },
            "ply_desc_by_guid": {
                "HZWKZRTQJV": {
                    "owner_layer_guid": "L1",
                    "guid": "HZWKZRTQJV",
                    "top_thou": 850,
                    "hidden": false,
                    "is_floor": false,
                    "ply_mat": [0.002, 0.0, 0.0, 0.002, 0.0, 0.0],
                    "mpoly": [
                        {
                            "exterior": [100,100, 400,100, 400,400, 100,400],
                            "holes": []
                        }
                    ]
                }
            },
            "layer_desc_by_guid": {
                "L1": { "guid": "L1", "hidden": false, "is_frame": false }
            },
            "carve_desc": {
                "grain_y": true,
                "rough_tool_guid": null,
                "refine_tool_guid": null,
                "detail_tool_guid": null
            }
        }
        "#;

        let comp: CompDesc = parse_comp_json(sample).expect("sample json should deserialize");
        let ppi: usize = 200;

        let scale = (
            comp.dim_desc.bulk_w_inch * ppi as f64,
            comp.dim_desc.bulk_h_inch * ppi as f64,
        );
        let frame_px = (comp.dim_desc.frame_inch * ppi as f64).round() as i64;

        let comp_px = comp.with_adjusted_mpolys((frame_px, frame_px), scale);
        let ply = comp_px
            .ply_desc_by_guid
            .get(&Guid("HZWKZRTQJV".to_string()))
            .expect("ply should exist");
        assert_eq!(ply.mpoly.len(), 1);

        let ext = ply.mpoly[0].iter().next().expect("exterior path should exist");
        let pts: Vec<(i64, i64)> = ext.iter().map(|pt| (pt.x_scaled(), pt.y_scaled())).collect();

        // 100 -> 0.2 -> 0.2*(4*200)=160, + frame(0.5*200)=100 => 260
        // 400 -> 0.8 -> 0.8*(4*200)=640, + 100 => 740
        assert_eq!(pts, vec![(260, 260), (740, 260), (740, 740), (260, 740)]);
    }

    #[test]
    fn comp_desc_deserializes_sample_json() {
        let sample = r#"
        {
            "version": 2,
            "guid": "JGYYJQBHTX",
            "dim_desc": {
                "bulk_d_inch": 0.75,
                "bulk_w_inch": 4,
                "bulk_h_inch": 4,
                "padding_inch": 0,
                "frame_inch": 0.5,
                "tolerance": 1,
                "pixels_per_inch": 200
            },
            "ply_desc_by_guid": {
                "HZWKZRTQJV": {
                    "owner_layer_guid": "R7Y9XP4VNB",
                    "guid": "HZWKZRTQJV",
                    "top_thou": 100,
                    "mpoly": [
                        {
                            "exterior": [0,0, 10,0, 10,10, 0,10],
                            "holes": [
                                [2,2, 8,2, 8,8, 2,8]
                            ]
                        }
                    ],
                    "ply_val": 1,
                    "priority": 1,
                    "hidden": false,
                    "is_hole": false,
                    "is_floor": true,
                    "effect_inst_guid": "none",
                    "tolerance": 1.2
                },
                "ZWKKED69NS": {
                    "owner_layer_guid": "SU6EKCGPM6",
                    "guid": "ZWKKED69NS",
                    "top_thou": 406,
                    "ply_val": 2,
                    "priority": 1,
                    "hidden": false,
                    "is_hole": false,
                    "is_floor": false,
                    "effect_inst_guid": "none",
                    "tolerance": 1.5
                },
                "EUUKYM6QYH": {
                    "owner_layer_guid": "H3VSUR3V61",
                    "guid": "EUUKYM6QYH",
                    "top_thou": 750,
                    "ply_val": 3,
                    "priority": 1,
                    "hidden": false,
                    "is_hole": false,
                    "is_floor": false,
                    "effect_inst_guid": "none",
                    "tolerance": 1.2
                }
            },
            "layer_desc_by_guid": {
                "R7Y9XP4VNB": {
                    "guid": "R7Y9XP4VNB",
                    "hidden": false,
                    "is_frame": false
                },
                "H3VSUR3V61": {
                    "guid": "H3VSUR3V61",
                    "hidden": false,
                    "is_frame": false
                },
                "SU6EKCGPM6": {
                    "guid": "SU6EKCGPM6",
                    "hidden": false,
                    "is_frame": false
                }
            },
            "carve_desc": {
                "grain_y": true,
                "refine_tool_guid": "W5C7NZWAK4",
                "rough_tool_guid": "EBES3PGSC3",
                "detail_margin_thou": 5,
                "detail_tool_guid": null,
                "stipple_tool_guid": null,
                "debug": true,
                "polish_tool_guid": null,
                "units": "inch",
                "hole_fill_threshold_in_tool_areas": 10
            },
            "bands": [
                {
                    "top_thou": 1000,
                    "bot_thou": 900,
                    "cut_pass": "refine"
                },
                {
                    "top_thou": 900,
                    "bot_thou": 800,
                    "cut_pass": "refine"
                },
                {
                    "top_thou": 1000,
                    "bot_thou": 200,
                    "cut_pass": "rough"
                }
            ]
        }
        "#;

        let comp: CompDesc = parse_comp_json(sample).expect("sample json should deserialize");

        assert_eq!(comp.version, 2);
        assert_eq!(comp.guid, Guid("JGYYJQBHTX".to_string()));
        assert!(comp.carve_desc.detail_tool_guid.is_none());
        assert_eq!(comp.ply_desc_by_guid.len(), 3);

        assert_eq!(comp.bands.len(), 3);
        assert_eq!(
            comp.bands,
            vec![
                BandDesc {
                    top_thou: Thou(1000),
                    bot_thou: Thou(900),
                    cut_pass: "refine".to_string(),
                },
                BandDesc {
                    top_thou: Thou(900),
                    bot_thou: Thou(800),
                    cut_pass: "refine".to_string(),
                },
                BandDesc {
                    top_thou: Thou(1000),
                    bot_thou: Thou(200),
                    cut_pass: "rough".to_string(),
                }
            ]
        );

        let ply = comp
            .ply_desc_by_guid
            .get(&Guid("HZWKZRTQJV".to_string()))
            .expect("ply HZWKZRTQJV should exist");
        assert_eq!(ply.top_thou, Thou(100));
        assert!(ply.is_floor);

        assert_eq!(ply.mpoly.len(), 1);
        assert_eq!(ply.mpoly[0].len(), 2, "exterior + 1 hole");
        let path_lens: Vec<usize> = ply.mpoly[0].iter().map(|p| p.len()).collect();
        assert_eq!(path_lens, vec![4, 4], "exterior + hole vertex counts");

        assert!(comp
            .layer_desc_by_guid
            .contains_key(&Guid("R7Y9XP4VNB".to_string())));
    }
}

