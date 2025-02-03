mod resource_provider;
mod skia_pipeline;

mod gpu_backends;
#[allow(dead_code)]
mod ordered_sender;
pub use gpu_backends::*;

mod skia_backend;
pub use skia_backend::*;

#[cfg(feature = "debug")]
mod metrics;

// reexports
#[cfg(feature = "metal")]
pub use metal_rs as metal;
pub use skia_safe;
