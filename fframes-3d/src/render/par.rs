//! Data parallel loops. They use rayon natively and run sequentially in the browser.

#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

/// Calls `f` for every index in `0..count`.
pub(crate) fn for_each(count: usize, f: impl Fn(usize) + Sync + Send) {
    #[cfg(not(target_arch = "wasm32"))]
    (0..count).into_par_iter().for_each(f);
    #[cfg(target_arch = "wasm32")]
    (0..count).for_each(f);
}

/// Calls `f` with the index and the items of every `chunk` long part of `data`.
pub(crate) fn chunks_mut<T: Send>(
    data: &mut [T],
    chunk: usize,
    f: impl Fn(usize, &mut [T]) + Sync + Send,
) {
    #[cfg(not(target_arch = "wasm32"))]
    data.par_chunks_mut(chunk)
        .enumerate()
        .for_each(|(i, c)| f(i, c));
    #[cfg(target_arch = "wasm32")]
    data.chunks_mut(chunk)
        .enumerate()
        .for_each(|(i, c)| f(i, c));
}

/// A pointer into a buffer that several threads write at the same time. Each thread must
/// write a different set of indices.
#[derive(Clone, Copy)]
pub(crate) struct SharedMut<T>(*mut T);

unsafe impl<T: Send> Send for SharedMut<T> {}
unsafe impl<T: Send> Sync for SharedMut<T> {}

impl<T> SharedMut<T> {
    pub(crate) fn new(data: &mut [T]) -> Self {
        Self(data.as_mut_ptr())
    }

    /// # Safety
    /// `index` must be in bounds. No other thread may access `index` at the same time.
    #[inline]
    pub(crate) unsafe fn write(self, index: usize, value: T) {
        unsafe { self.0.add(index).write(value) }
    }
}
