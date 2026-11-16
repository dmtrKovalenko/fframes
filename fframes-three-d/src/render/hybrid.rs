//! [`Hybrid`]: frames on the CPU rasterizer while the GPU is busy.

/// How [`Renderer::hybrid`](crate::Renderer::hybrid) renders frames on the CPU while the GPU
/// is busy.
///
/// A video renders its frames on many threads, see [`fframes::get_thread_count`]. When
/// other threads have two frames on the GPU and fewer than `frames` threads render on the
/// CPU, the thread renders its next `run` frames on the CPU rasterizer, in a pool of
/// `threads` threads. A single frame, for example a preview, renders on the GPU.
///
/// Which frames render on the CPU depends on the timing, so keep it off for pixel
/// snapshots. The CPU and the GPU draw the same picture, but not to the last bit: on
/// average they differ by less than one 8-bit step, a little more in textures, depth of
/// field and the bloom of very bright points. Frames of both kinds that alternate would
/// flicker, so a thread stays on the CPU for a run of frames. The fframes renderers give each
/// thread consecutive frames, so the picture changes only where a run starts and ends.
///
/// It helps only when the CPU is not much slower than the GPU. A frame on all cores of the
/// CPU takes 3 to 6 times as long as on an Intel UHD 770, but those cores take the power of
/// the GPU: renders with it were not faster. An Apple M4 Max renders the same frame 30 to 50
/// times faster on its GPU, and renders with it were 1.1 to 7 times slower. Measure the
/// render time with and without it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hybrid {
    /// The threads of the CPU rasterizer pool. The first hybrid frame of the process makes
    /// the pool, later frames use it, also with another value.
    pub threads: usize,
    /// The threads that render on the CPU at the same time.
    pub frames: usize,
    /// The frames that a thread renders on the CPU in a row. Each render of a hybrid renderer
    /// counts, also of another renderer on the same thread.
    pub run: usize,
}

impl Default for Hybrid {
    /// Half of the CPU threads, two frames at a time, runs of 30 frames. The other half
    /// keeps building the frames, encoding them and feeding the GPU.
    fn default() -> Self {
        let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        Self {
            threads: (threads / 2).max(1),
            frames: 2,
            run: 30,
        }
    }
}

#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
pub(super) use scheduler::{CpuTurn, cpu_turn};

#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
mod scheduler {
    use std::cell::RefCell;
    use std::marker::PhantomData;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::Hybrid;
    use crate::gpu::Gpu;

    /// The frames that other threads have on the GPU before a thread starts a run on the
    /// CPU. With 2, the GPU has the next frame ready when it finishes one.
    pub(crate) const GPU_QUEUE: usize = 2;

    /// The runs that go on now, see [`Hybrid::frames`].
    pub(crate) static CPU_RUNS: AtomicUsize = AtomicUsize::new(0);

    thread_local! {
        /// The run of this thread.
        static RUN: RefCell<Option<CpuRun>> = const { RefCell::new(None) };
    }

    /// Frames that a thread renders on the CPU in a row. It frees its place in
    /// [`CPU_RUNS`] when it is dropped: after its last frame, or when the thread ends.
    struct CpuRun {
        /// The frames of the run that did not start yet.
        left: usize,
    }

    impl CpuRun {
        /// A run of `hybrid.run` frames, `None` when `hybrid.frames` runs go on.
        fn start(hybrid: &Hybrid) -> Option<Self> {
            CPU_RUNS
                .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                    (n < hybrid.frames).then_some(n + 1)
                })
                .ok()
                .map(|_| Self {
                    left: hybrid.run.max(1),
                })
        }
    }

    impl Drop for CpuRun {
        fn drop(&mut self) {
            CPU_RUNS.fetch_sub(1, Ordering::AcqRel);
        }
    }

    /// A frame of a run. When it is dropped after the last frame, the run ends, so an idle
    /// thread does not keep a place in [`CPU_RUNS`].
    pub(crate) struct CpuTurn {
        pool: &'static rayon::ThreadPool,
        /// The run belongs to the thread that renders the frame.
        _thread: PhantomData<*const ()>,
    }

    impl CpuTurn {
        /// Runs `f` in the pool of the CPU rasterizer.
        pub(crate) fn install<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
            self.pool.install(f)
        }
    }

    impl Drop for CpuTurn {
        fn drop(&mut self) {
            RUN.with_borrow_mut(|run| {
                if run.as_ref().is_some_and(|r| r.left == 0) {
                    *run = None;
                }
            });
        }
    }

    /// The next frame of this thread on the CPU, `None` for the GPU. A run goes on until its
    /// last frame. A new run starts when other threads have [`GPU_QUEUE`] frames on `gpu`.
    pub(crate) fn cpu_turn(hybrid: &Hybrid, gpu: &Gpu) -> Option<CpuTurn> {
        RUN.with_borrow_mut(|run| {
            if run.is_none() {
                if gpu.frames_in_flight() < GPU_QUEUE {
                    return None;
                }
                *run = CpuRun::start(hybrid);
            }
            let run = run.as_mut()?;
            run.left -= 1;
            Some(CpuTurn {
                pool: cpu_pool(hybrid.threads),
                _thread: PhantomData,
            })
        })
    }

    /// The pool of the CPU rasterizer for hybrid frames. The threads of the frames wait in
    /// it, and the work of other pools does not wait for it.
    fn cpu_pool(threads: usize) -> &'static rayon::ThreadPool {
        static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
        POOL.get_or_init(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads.max(1))
                .thread_name(|i| format!("fframes_three_d-cpu-{i}"))
                .build()
                .expect("the CPU rasterizer thread pool")
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::*;

        fn scene() -> (Scene, Camera) {
            let mut scene = Scene::new().environment(Environment::studio());
            scene.add(Node::mesh(
                Geometry::rounded_box(1.0, 1.0, 1.0, 0.1, 4),
                Material::metal(0xd4d8e0, 0.3).clearcoat(1.0),
            ));
            scene.add(Node::mesh(
                Geometry::sphere(0.4, 32, 16),
                Material::standard(0xff5500),
            ));
            scene.add(Node::light(Light::sun(
                Rgb::WHITE,
                3.0,
                vec3(3.0, 6.0, 4.0),
            )));
            let camera = Camera::perspective(35.0).orbit(Vec3::ZERO, 4.0, 30.0, 20.0);
            (scene, camera)
        }

        /// One test, because the runs and the frames on the GPU are global.
        #[test]
        fn runs_render_on_the_cpu_and_end() {
            let Some(gpu) = crate::gpu::gpu() else {
                eprintln!("no GPU, skipped");
                return;
            };
            let (scene, camera) = scene();
            let base = Renderer::new(160, 90).bloom(Bloom::default());
            let on_gpu = base.clone().gpu().render_pixels(&scene, &camera).rgba;
            let on_cpu = base.clone().cpu().render_pixels(&scene, &camera).rgba;
            assert_ne!(on_gpu, on_cpu, "the test needs pictures that differ");
            let hybrid = base.clone().gpu().hybrid(Hybrid {
                threads: 2,
                frames: 1,
                run: 3,
            });

            // An idle GPU renders a single frame.
            assert_eq!(hybrid.render_pixels(&scene, &camera).rgba, on_gpu);
            assert_eq!(CPU_RUNS.load(Ordering::Acquire), 0);

            // Other threads keep the GPU busy: a run of three frames starts.
            let busy: Vec<_> = (0..GPU_QUEUE).map(|_| gpu.count_in_flight()).collect();
            assert_eq!(hybrid.render_pixels(&scene, &camera).rgba, on_cpu);
            assert_eq!(CPU_RUNS.load(Ordering::Acquire), 1);
            drop(busy);
            // The run goes on with an idle GPU. A renderer without hybrid does not end it.
            assert_eq!(
                base.clone().gpu().render_pixels(&scene, &camera).rgba,
                on_gpu
            );
            assert_eq!(hybrid.render_pixels(&scene, &camera).rgba, on_cpu);
            assert_eq!(hybrid.render_pixels(&scene, &camera).rgba, on_cpu);
            // The last frame ended the run.
            assert_eq!(CPU_RUNS.load(Ordering::Acquire), 0);
            assert_eq!(hybrid.render_pixels(&scene, &camera).rgba, on_gpu);

            // Another thread does not start a run while `frames` runs go on.
            let busy: Vec<_> = (0..GPU_QUEUE).map(|_| gpu.count_in_flight()).collect();
            assert_eq!(hybrid.render_pixels(&scene, &camera).rgba, on_cpu);
            std::thread::scope(|s| {
                s.spawn(|| assert_eq!(hybrid.render_pixels(&scene, &camera).rgba, on_gpu));
            });
            drop(busy);
            assert_eq!(hybrid.render_pixels(&scene, &camera).rgba, on_cpu);
            assert_eq!(hybrid.render_pixels(&scene, &camera).rgba, on_cpu);
            assert_eq!(CPU_RUNS.load(Ordering::Acquire), 0);
            assert_eq!(gpu.frames_in_flight(), 0);
        }
    }
}
