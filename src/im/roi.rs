#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ROI {
    pub l: usize,
    pub t: usize,
    /// Exclusive right bound.
    pub r: usize,
    /// Exclusive bottom bound.
    pub b: usize,
}

impl ROI {
    /// Width of the ROI.
    pub fn w(&self) -> usize {
        self.r - self.l
    }

    /// Height of the ROI.
    pub fn h(&self) -> usize {
        self.b - self.t
    }

    /// Make a new ROI from this one by `pad` pixels in all directions, clamped to the given max dims.
    pub fn padded(&self, pad: usize, max_w: usize, max_h: usize) -> ROI {
        let l = self.l.saturating_sub(pad);
        let t = self.t.saturating_sub(pad);
        let r = (self.r + pad).min(max_w);
        let b = (self.b + pad).min(max_h);
        ROI { l, t, r, b }
    }

    pub fn union(&mut self, other: ROI) {
        self.l = self.l.min(other.l);
        self.t = self.t.min(other.t);
        self.r = self.r.max(other.r);
        self.b = self.b.max(other.b);
    }
}
