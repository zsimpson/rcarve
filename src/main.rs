use rcarve::desc::{parse_comp_json};

const TEST_JSON: &str = r#"
    {
        "version": 3,
        "guid": "JGYYJQBHTX",
        "dim_desc": {
            "bulk_d_inch": 0.75,
            "bulk_w_inch": 4,
            "bulk_h_inch": 4,
            "padding_inch": 0,
            "frame_inch": 0.5
        },
        "ply_desc_by_guid": {
            "HZWKZRTQJV": {
                "owner_layer_guid": "R7Y9XP4VNB",
                "guid": "HZWKZRTQJV",
                "top_thou": 100,
                "hidden": false,
                "is_floor": false,
                "mpoly": [
                    {
                        "exterior": [0,0, 10,0, 10,10, 0,10],
                        "holes": [
                            [2,2, 8,2, 8,8, 2,8]
                        ]
                    }
                ]
            },
            "ZWKKED69NS": {
                "owner_layer_guid": "R7Y9XP4VNB",
                "guid": "ZWKKED69NS",
                "top_thou": 200,
                "hidden": false,
                "is_floor": false,
                "mpoly": [
                    {
                        "exterior": [4,4, 6,4, 6,6, 4,6],
                        "holes": []
                    }
                ]
            }
        },
        "layer_desc_by_guid": {
            "R7Y9XP4VNB": {
                "guid": "R7Y9XP4VNB",
                "hidden": false,
                "is_frame": false
            }
        },
        "carve_desc": {
            "grain_y": true,
            "rough_tool_guid": "EBES3PGSC3",
            "refine_tool_guid": "W5C7NZWAK4",
            "detail_tool_guid": null
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

fn main() {
    let comp_desc = parse_comp_json(TEST_JSON).expect("Failed to parse comp JSON");
    println!("Parsed CompDesc: {:?}", comp_desc);
}