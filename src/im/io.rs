use super::core::Im;
use image::ImageResult;
use std::path::Path;

// Helpers for i32 PNG packing/unpacking
// -----------------------------------------------------------------------------
fn dim_mismatch_err() -> image::ImageError {
    image::ImageError::Parameter(image::error::ParameterError::from_kind(
        image::error::ParameterErrorKind::DimensionMismatch,
    ))
}

fn pack_i32_as_rgba8(pixels: &[i32]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(pixels.len() * 4);
    for v in pixels {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn unpack_rgba8_as_i32(raw_rgba: &[u8]) -> Result<Vec<i32>, image::ImageError> {
    if raw_rgba.len() % 4 != 0 {
        return Err(dim_mismatch_err());
    }

    let mut out: Vec<i32> = Vec::with_capacity(raw_rgba.len() / 4);
    for px in raw_rgba.chunks_exact(4) {
        out.push(i32::from_le_bytes([px[0], px[1], px[2], px[3]]));
    }
    Ok(out)
}

// PNG I/O
// -----------------------------------------------------------------------------
impl Im<u8, 1> {
    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> ImageResult<()> {
        let img = image::GrayImage::from_raw(self.w as u32, self.h as u32, self.arr.clone())
            .ok_or_else(|| {
                image::ImageError::Parameter(image::error::ParameterError::from_kind(
                    image::error::ParameterErrorKind::DimensionMismatch,
                ))
            })?;

        img.save_with_format(path, image::ImageFormat::Png)
    }
}

impl Im<u8, 4> {
    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> ImageResult<()> {
        let img = image::RgbaImage::from_raw(self.w as u32, self.h as u32, self.arr.clone())
            .ok_or_else(|| {
                image::ImageError::Parameter(image::error::ParameterError::from_kind(
                    image::error::ParameterErrorKind::DimensionMismatch,
                ))
            })?;

        img.save_with_format(path, image::ImageFormat::Png)
    }
}

impl Im<u16, 1> {
    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> ImageResult<()> {
        let img = image::ImageBuffer::<image::Luma<u16>, _>::from_raw(
            self.w as u32,
            self.h as u32,
            self.arr.clone(),
        )
        .ok_or_else(|| {
            image::ImageError::Parameter(image::error::ParameterError::from_kind(
                image::error::ParameterErrorKind::DimensionMismatch,
            ))
        })?;

        img.save_with_format(path, image::ImageFormat::Png)
    }
}

impl Im<u16, 4> {
    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> ImageResult<()> {
        let img = image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(
            self.w as u32,
            self.h as u32,
            self.arr.clone(),
        )
        .ok_or_else(|| {
            image::ImageError::Parameter(image::error::ParameterError::from_kind(
                image::error::ParameterErrorKind::DimensionMismatch,
            ))
        })?;

        img.save_with_format(path, image::ImageFormat::Png)
    }
}

impl Im<i32, 1> {
    // PNG doesn't support 32-bit single-channel integer pixels, so we losslessly
    // round-trip by packing each i32 into RGBA8 (little-endian bytes).
    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> ImageResult<()> {
        let raw = pack_i32_as_rgba8(&self.arr);

        let img = image::RgbaImage::from_raw(self.w as u32, self.h as u32, raw)
            .ok_or_else(dim_mismatch_err)?;

        img.save_with_format(path, image::ImageFormat::Png)
    }

    pub fn load_png<P: AsRef<Path>>(path: P) -> ImageResult<Self> {
        let img = image::open(path)?.into_rgba8();
        let w = img.width() as usize;
        let h = img.height() as usize;
        let raw = img.into_raw();

        if raw.len() != w * h * 4 {
            return Err(dim_mismatch_err());
        }

        let arr = unpack_rgba8_as_i32(&raw)?;
        Ok(Self { w, h, s: w, arr })
    }
}

// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i32_png_pack_is_lossless() {
        let src: [i32; 6] = [0, 1, -1, 123456789, i32::MIN, i32::MAX];
        let packed = pack_i32_as_rgba8(&src);
        let unpacked = unpack_rgba8_as_i32(&packed).unwrap();
        assert_eq!(unpacked, src);
    }
}
