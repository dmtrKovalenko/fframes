pub mod neon_triangle;
pub use neon_triangle::*;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
