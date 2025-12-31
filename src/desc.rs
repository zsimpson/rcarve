use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct Guid(pub String);

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
    pub carve_desc: CarveDesc,
    #[serde(default)]
    pub bands: Vec<BandDesc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DimDesc {
    pub bulk_d_inch: f64,
    pub bulk_w_inch: f64,
    pub bulk_h_inch: f64,
    pub padding_inch: f64,
    pub frame_inch: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlyDesc {
    pub owner_layer_guid: Guid,
    pub guid: Guid,
    pub top_thou: i32,
    pub hidden: bool,
    pub is_floor: bool,
    #[serde(default)]
    pub mpoly: Vec<PolyDesc>,
}

pub type FlatVerts = Vec<i32>;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PolyDesc {
    pub exterior: FlatVerts,
    #[serde(default)]
    pub holes: Vec<FlatVerts>,
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
    pub rough_tool_guid: Guid,
    pub refine_tool_guid: Guid,
    pub detail_tool_guid: Option<Guid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BandDesc {
    pub top_thou: i32,
    pub bot_thou: i32,
    pub which: String,
}

pub fn parse_comp_json(json_text: &str) -> Result<CompDesc, serde_json::Error> {
    serde_json::from_str(json_text)
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    "which": "refine"
                },
                {
                    "top_thou": 900,
                    "bot_thou": 800,
                    "which": "refine"
                },
                {
                    "top_thou": 1000,
                    "bot_thou": 200,
                    "which": "rough"
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
                    top_thou: 1000,
                    bot_thou: 900,
                    which: "refine".to_string(),
                },
                BandDesc {
                    top_thou: 900,
                    bot_thou: 800,
                    which: "refine".to_string(),
                },
                BandDesc {
                    top_thou: 1000,
                    bot_thou: 200,
                    which: "rough".to_string(),
                }
            ]
        );

        let ply = comp
            .ply_desc_by_guid
            .get(&Guid("HZWKZRTQJV".to_string()))
            .expect("ply HZWKZRTQJV should exist");
        assert_eq!(ply.top_thou, 100);
        assert!(ply.is_floor);

        assert_eq!(ply.mpoly.len(), 1);
        assert_eq!(ply.mpoly[0].exterior.len(), 8);
        assert_eq!(ply.mpoly[0].holes.len(), 1);
        assert_eq!(ply.mpoly[0].holes[0].len(), 8);

        assert!(comp
            .layer_desc_by_guid
            .contains_key(&Guid("R7Y9XP4VNB".to_string())));
    }
}
