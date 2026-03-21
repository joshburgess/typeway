//! [`RepeatedField<T>`] — pooled repeated field that reuses allocations.
//!
//! Prost's `Vec<T>` drops all elements on `clear()`, then reallocates on the
//! next decode. GreptimeDB measured a **63% deserialization speedup** by
//! switching to a logical-length approach that reuses the backing buffer.
//!
//! `RepeatedField<T>` tracks a logical length separately from the buffer
//! capacity. `clear()` resets the logical length without dropping elements.
//! On the next decode, existing slots are overwritten instead of reallocated.

use std::ops::{Deref, DerefMut};

/// A repeated protobuf field that pools allocations across decodes.
///
/// Behaves like `Vec<T>` for reading (implements `Deref<Target = [T]>`),
/// but `clear()` only resets the logical length — elements are retained
/// in the backing buffer and overwritten on the next `push()`.
///
/// # Example
///
/// ```
/// use typeway_protobuf::RepeatedField;
///
/// let mut field = RepeatedField::new();
/// field.push(1);
/// field.push(2);
/// field.push(3);
/// assert_eq!(field.len(), 3);
///
/// // Clear resets logical length, but doesn't drop/dealloc.
/// field.clear();
/// assert_eq!(field.len(), 0);
///
/// // Next push reuses existing allocation.
/// field.push(10);
/// assert_eq!(field.len(), 1);
/// assert_eq!(field[0], 10);
/// ```
pub struct RepeatedField<T> {
    buf: Vec<T>,
    len: usize,
}

impl<T> RepeatedField<T> {
    /// Create an empty `RepeatedField`.
    pub fn new() -> Self {
        RepeatedField {
            buf: Vec::new(),
            len: 0,
        }
    }

    /// Create with pre-allocated capacity.
    pub fn with_capacity(cap: usize) -> Self {
        RepeatedField {
            buf: Vec::with_capacity(cap),
            len: 0,
        }
    }

    /// Push an element. Reuses existing buffer slots when possible.
    pub fn push(&mut self, val: T) {
        if self.len < self.buf.len() {
            self.buf[self.len] = val;
        } else {
            self.buf.push(val);
        }
        self.len += 1;
    }

    /// Reset logical length without dropping elements.
    ///
    /// The backing buffer retains its allocation and elements.
    /// Next `push()` overwrites from the beginning.
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// The number of logically present elements.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the field has zero elements.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The capacity of the backing buffer.
    pub fn capacity(&self) -> usize {
        self.buf.capacity()
    }

    /// Convert to a `Vec<T>`, truncating to the logical length.
    pub fn into_vec(mut self) -> Vec<T> {
        self.buf.truncate(self.len);
        self.buf
    }

    /// Iterate over the logically present elements.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.buf[..self.len].iter()
    }

    /// Mutable iteration.
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.buf[..self.len].iter_mut()
    }
}

impl<T> Default for RepeatedField<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Deref for RepeatedField<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.buf[..self.len]
    }
}

impl<T> DerefMut for RepeatedField<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.buf[..self.len]
    }
}

impl<T: Clone> Clone for RepeatedField<T> {
    fn clone(&self) -> Self {
        RepeatedField {
            buf: self.buf[..self.len].to_vec(),
            len: self.len,
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for RepeatedField<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T: PartialEq> PartialEq for RepeatedField<T> {
    fn eq(&self, other: &Self) -> bool {
        self[..] == other[..]
    }
}

impl<T: Eq> Eq for RepeatedField<T> {}

impl<T> From<Vec<T>> for RepeatedField<T> {
    fn from(vec: Vec<T>) -> Self {
        let len = vec.len();
        RepeatedField { buf: vec, len }
    }
}

impl<T: Clone> From<&[T]> for RepeatedField<T> {
    fn from(slice: &[T]) -> Self {
        RepeatedField {
            buf: slice.to_vec(),
            len: slice.len(),
        }
    }
}

impl<'a, T> IntoIterator for &'a RepeatedField<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: serde::Serialize> serde::Serialize for RepeatedField<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.len))?;
        for item in self.iter() {
            seq.serialize_element(item)?;
        }
        seq.end()
    }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for RepeatedField<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let vec = Vec::<T>::deserialize(deserializer)?;
        Ok(RepeatedField::from(vec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_read() {
        let mut f = RepeatedField::new();
        f.push(1);
        f.push(2);
        f.push(3);
        assert_eq!(f.len(), 3);
        assert_eq!(&f[..], &[1, 2, 3]);
    }

    #[test]
    fn clear_reuses_allocation() {
        let mut f = RepeatedField::new();
        f.push(1);
        f.push(2);
        f.push(3);

        let cap_before = f.capacity();
        f.clear();
        assert_eq!(f.len(), 0);
        assert!(f.is_empty());

        // Capacity preserved.
        assert_eq!(f.capacity(), cap_before);

        // Push reuses existing slots.
        f.push(10);
        f.push(20);
        assert_eq!(f.len(), 2);
        assert_eq!(&f[..], &[10, 20]);
    }

    #[test]
    fn grow_beyond_initial() {
        let mut f = RepeatedField::new();
        for i in 0..100 {
            f.push(i);
        }
        assert_eq!(f.len(), 100);
        f.clear();
        for i in 0..200 {
            f.push(i);
        }
        assert_eq!(f.len(), 200);
    }

    #[test]
    fn clone_only_copies_logical() {
        let mut f = RepeatedField::new();
        f.push(1);
        f.push(2);
        f.push(3);
        f.clear();
        f.push(10);

        let cloned = f.clone();
        assert_eq!(cloned.len(), 1);
        assert_eq!(&cloned[..], &[10]);
    }

    #[test]
    fn into_vec() {
        let mut f = RepeatedField::new();
        f.push(1);
        f.push(2);
        let v = f.into_vec();
        assert_eq!(v, vec![1, 2]);
    }

    #[test]
    fn from_vec() {
        let f = RepeatedField::from(vec![1, 2, 3]);
        assert_eq!(f.len(), 3);
        assert_eq!(&f[..], &[1, 2, 3]);
    }

    #[test]
    fn serde_roundtrip() {
        let f = RepeatedField::from(vec![1, 2, 3]);
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, "[1,2,3]");
        let deserialized: RepeatedField<i32> = serde_json::from_str(&json).unwrap();
        assert_eq!(f, deserialized);
    }

    #[test]
    fn debug_format() {
        let f = RepeatedField::from(vec![1, 2, 3]);
        assert_eq!(format!("{f:?}"), "[1, 2, 3]");
    }

    #[test]
    fn iterate() {
        let f = RepeatedField::from(vec![10, 20, 30]);
        let sum: i32 = f.iter().sum();
        assert_eq!(sum, 60);
    }
}
