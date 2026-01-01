#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat3 {
    // Row-major 3x3 matrix.
    m: [[f64; 3]; 3],
}

impl Mat3 {
    pub const fn identity() -> Self {
        Self {
            m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    /// Constructs a homogeneous 3x3 matrix from a 2D affine transform.
    ///
    /// The expected 6-element layout is `[a, b, c, d, e, f]` such that:
    ///
    /// - `x' = a*x + c*y + e`
    /// - `y' = b*x + d*y + f`
    pub fn from_affine2(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self {
            m: [[a, c, e], [b, d, f], [0.0, 0.0, 1.0]],
        }
    }

    /// Parses a `ply_mat` (typically 6 floats) into a `Mat3`.
    ///
    /// Returns `None` if there are fewer than 6 elements.
    pub fn from_ply_mat(ply_mat: &[f32]) -> Option<Self> {
        if ply_mat.len() < 6 {
            return None;
        }
        Some(Self::from_affine2(
            ply_mat[0] as f64,
            ply_mat[1] as f64,
            ply_mat[2] as f64,
            ply_mat[3] as f64,
            ply_mat[4] as f64,
            ply_mat[5] as f64,
        ))
    }

    /// Applies this transform to a 2D point (implicitly using homogeneous `w=1`).
    #[inline]
    pub fn transform_point2(&self, x: f64, y: f64) -> (f64, f64) {
        let x2 = self.m[0][0] * x + self.m[0][1] * y + self.m[0][2];
        let y2 = self.m[1][0] * x + self.m[1][1] * y + self.m[1][2];
        (x2, y2)
    }

    /// Returns a transform that applies this matrix, then translates by `(tx, ty)`.
    ///
    /// This is equivalent to left-multiplying by a translation matrix `T(tx, ty)`.
    #[inline]
    pub fn then_translate(self, tx: f64, ty: f64) -> Self {
        let mut out = self;
        out.m[0][2] += tx;
        out.m[1][2] += ty;
        out
    }
}

impl Default for Mat3 {
    fn default() -> Self {
        Self::identity()
    }
}
