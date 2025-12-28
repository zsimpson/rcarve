use serde::Deserialize;
use std::collections::HashMap;

mod im;

#[cfg(test)]
mod poly_test;





#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct CarveJson {
    version: u32,
    guid: String,
    dim_desc: DimDesc,
    ply_desc_by_guid: HashMap<String, PlyDesc>,
    layer_desc_by_guid: HashMap<String, LayerDesc>,
    carve_desc: CarveDesc,

    #[serde(default)]
    effect_inst_by_guid: HashMap<String, serde_json::Value>,
    is_staging: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct DimDesc {
    bulk_d_inch: f64,
    bulk_w_inch: f64,
    bulk_h_inch: f64,
    padding_inch: f64,
    frame_inch: f64,
    tolerance: f64,
    pixels_per_inch: u32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct PlyDesc {
    owner_layer_guid: String,
    guid: String,
    top_thou: i64,
    ply_val: i64,
    // priority: i64,
    hidden: bool,
    // is_hole: bool,
    is_floor: bool,
    effect_inst_guid: String,
    tolerance: f64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct LayerDesc {
    guid: String,
    hidden: bool,
    is_frame: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct CarveDesc {
    grain_y: bool,
    refine_tool_guid: String,
    rough_tool_guid: String,
    detail_margin_thou: i64,
    detail_tool_guid: Option<String>,
    stipple_tool_guid: Option<String>,
    debug: bool,
    polish_tool_guid: Option<String>,
    units: String,
    hole_fill_threshold_in_tool_areas: i64,
}

fn test_json() {
    // Open ./test_data/carve.json. Here's an example
    // I might contain other keys that we want to ignore for now

    let json_text = std::fs::read_to_string("./test_data/carve.json")
        .unwrap_or_else(|e| panic!("failed to read carve.json: {e}"));

    let carve: CarveJson =
        serde_json::from_str(&json_text).unwrap_or_else(|e| panic!("invalid carve.json: {e}"));

    assert_eq!(carve.version, 2);
    assert_eq!(carve.guid, "JGYYJQBHTX");

    assert_eq!(carve.dim_desc.pixels_per_inch, 200);
    assert!((carve.dim_desc.bulk_d_inch - 0.75).abs() < 1e-9);
    assert!((carve.dim_desc.frame_inch - 0.5).abs() < 1e-9);

    assert_eq!(carve.ply_desc_by_guid.len(), 3);
    let ply1 = carve
        .ply_desc_by_guid
        .get("HZWKZRTQJV")
        .expect("missing ply HZWKZRTQJV");
    assert_eq!(ply1.ply_val, 1);
    assert_eq!(ply1.top_thou, 100);
    assert!(ply1.is_floor);
    assert!(!ply1.hidden);
    assert_eq!(ply1.effect_inst_guid, "none");

    assert_eq!(carve.layer_desc_by_guid.len(), 3);
    let layer = carve
        .layer_desc_by_guid
        .get("R7Y9XP4VNB")
        .expect("missing layer R7Y9XP4VNB");
    assert_eq!(layer.guid, "R7Y9XP4VNB");
    assert!(!layer.hidden);
    assert!(!layer.is_frame);

    assert_eq!(carve.carve_desc.units, "inch");
    assert!(carve.carve_desc.debug);
    assert_eq!(carve.carve_desc.detail_margin_thou, 5);
    assert!(carve.carve_desc.detail_tool_guid.is_none());
    assert!(carve.carve_desc.stipple_tool_guid.is_none());
    assert!(carve.carve_desc.polish_tool_guid.is_none());

    assert!(carve.effect_inst_by_guid.is_empty());
    assert!(!carve.is_staging);

    println!(
        "Parsed carve.json ok: version={}, guid={}, plies={}, layers={}",
        carve.version,
        carve.guid,
        carve.ply_desc_by_guid.len(),
        carve.layer_desc_by_guid.len()
    );

}

fn test_png() {
    // Open ./coral1.png and then fill a black square 20x20 in the top left corner
    // And then save it back to coral2.png

    let input_path = "./test_data/coral1.png";
    let output_path = "./test_data/_coral2.png";

    let mut img = image::open(input_path)
        .unwrap_or_else(|e| panic!("failed to open {input_path}: {e}"))
        .to_rgba8();

    let width = img.width();
    let height = img.height();

    let square_w = 20u32.min(width);
    let square_h = 20u32.min(height);

    // Still a simple double loop, but avoids `put_pixel` overhead by writing bytes directly.
    if square_w > 0 && square_h > 0 {
        let bytes_per_pixel = 4usize;

        let width_u = width as usize;
        let square_w_u = square_w as usize;
        let square_h_u = square_h as usize;

        let row_bytes = width_u * bytes_per_pixel;
        let buf = img.as_flat_samples_mut().samples;

        for y in 0..square_h_u {
            let row_start = y * row_bytes;
            for x in 0..square_w_u {
                let i = row_start + x * bytes_per_pixel;
                buf[i] = 0;
                buf[i + 1] = 0;
                buf[i + 2] = 0;
                buf[i + 3] = 255;
            }
        }
    }

    img.save(output_path)
        .unwrap_or_else(|e| panic!("failed to save {output_path}: {e}"));

    println!("Saved modified image to {output_path}");
}



fn main() {
    test_json();
    test_png();
}