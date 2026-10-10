//! `--renderer skia-cpu|gpu` for [`fframes::cli`]: the same binary renders with Skia on the CPU
//! or on the GPU (Metal on macOS, Vulkan elsewhere).
use crate::{SkiaCpuCtx, SkiaFFramesRenderer, SkiaPipelineConfig};
use fframes::cli::{Renderer, Renderers, Runner, clap::Args};
use fframes::{FFramesRenderBackend, Video};
use std::process::ExitCode;

/// The Skia backends for `Runner::renderers`. GPU contexts are only created when they render.
///
/// ```ignore
/// cli::new(&video, options)
///     .renderers(SkiaRenderers::gpu(SkiaPipelineConfig::default()))
///     .run()
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct SkiaRenderers {
    /// What renders without `--renderer`, `None` for the runner's backend.
    pub default: Option<Renderer>,
    pub pipeline: SkiaPipelineConfig,
}

impl SkiaRenderers {
    /// Skia on the GPU unless `--renderer` asks for another backend.
    pub fn gpu(pipeline: SkiaPipelineConfig) -> Self {
        Self {
            default: Some(Renderer::Gpu),
            pipeline,
        }
    }
}

impl Renderers for SkiaRenderers {
    fn default_renderer(&self) -> Option<Renderer> {
        self.default
    }

    fn run<'r, 'a, 'media: 'a, V: Video + Send + Sync, B: FFramesRenderBackend, A: Args>(
        self,
        renderer: Renderer,
        runner: Runner<'r, 'a, 'media, V, B, A>,
    ) -> ExitCode {
        let pipeline = self.pipeline;
        match renderer {
            Renderer::Cpu => runner
                .backend(fframes::cpu::CpuRenderingBackend::default())
                .run(),
            Renderer::SkiaCpu => {
                let cpu = SkiaCpuCtx::new(V::WIDTH, V::HEIGHT);
                runner
                    .backend(SkiaFFramesRenderer::new(pipeline, &cpu))
                    .run()
            }
            #[cfg(feature = "metal")]
            Renderer::Gpu => match crate::metal::SkiaMetalCtx::new(V::WIDTH, V::HEIGHT) {
                Ok(gpu) => runner
                    .backend(SkiaFFramesRenderer::new(pipeline, &gpu))
                    .run(),
                Err(err) => runner.fail(err),
            },
            #[cfg(all(feature = "vulkan", not(feature = "metal")))]
            Renderer::Gpu => match crate::vulkan::SkiaVulkanCtx::new(V::WIDTH, V::HEIGHT) {
                Ok(gpu) => runner
                    .backend(SkiaFFramesRenderer::new(pipeline, &gpu))
                    .run(),
                Err(err) => runner.fail(err),
            },
            #[cfg(not(any(feature = "metal", feature = "vulkan")))]
            Renderer::Gpu => {
                runner.fail("built without the metal or vulkan feature of fframes_skia_renderer")
            }
        }
    }
}
