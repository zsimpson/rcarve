use std::slice;

/// A simple bucketed grow-only container.
/// Optimized for many appends followed by iteration.
pub struct BucketVec<T, const BUCKET_SIZE: usize = 256> {
    buckets: Vec<Vec<T>>,
    len: usize,
}

impl<T, const BUCKET_SIZE: usize> BucketVec<T, BUCKET_SIZE> {
    pub fn new() -> Self {
        Self {
            buckets: Vec::new(),
            len: 0,
        }
    }

    pub fn with_bucket_capacity(num_buckets: usize) -> Self {
        Self {
            buckets: Vec::with_capacity(num_buckets),
            len: 0,
        }
    }

    #[inline]
    pub fn push(&mut self, value: T) {
        if self
            .buckets
            .last()
            .map_or(true, |b| b.len() == BUCKET_SIZE)
        {
            self.buckets.push(Vec::with_capacity(BUCKET_SIZE));
        }

        self.buckets.last_mut().unwrap().push(value);
        self.len += 1;
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            bucket_iter: self.buckets.iter(),
            elem_iter: None,
        }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            bucket_iter: self.buckets.iter_mut(),
            elem_iter: None,
        }
    }
}

pub struct Iter<'a, T> {
    bucket_iter: slice::Iter<'a, Vec<T>>,
    elem_iter: Option<slice::Iter<'a, T>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ref mut elems) = self.elem_iter {
                if let Some(item) = elems.next() {
                    return Some(item);
                }
            }

            let bucket = self.bucket_iter.next()?;
            self.elem_iter = Some(bucket.iter());
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }
}

pub struct IterMut<'a, T> {
    bucket_iter: slice::IterMut<'a, Vec<T>>,
    elem_iter: Option<slice::IterMut<'a, T>>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ref mut elems) = self.elem_iter {
                if let Some(item) = elems.next() {
                    return Some(item);
                }
            }

            let bucket = self.bucket_iter.next()?;
            self.elem_iter = Some(bucket.iter_mut());
        }
    }
}

