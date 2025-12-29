
mod im;
mod desc;
mod cut_stack;

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
    test_png();
}