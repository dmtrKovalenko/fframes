use super::SkiaBackend;
use crate::skia_backend::{SkiaFFramesRenderer, SkiaPipelineConfig};
use ash::vk::{self, Handle};
use ash::{Entry, Instance};
use fframes::{FFramesRendererError, FFramesRendererResult};
use skia_safe::gpu::ganesh::context_options::{Enable, ShaderCacheStrategy};
use skia_safe::gpu::{self, DirectContext};
use skia_safe::{AlphaType, ColorType, ImageInfo};
use skia_safe::{Surface, gpu::SurfaceOrigin};
use std::ffi::{CString, c_void};
use std::os::raw::c_char;

#[allow(dead_code)]
pub struct SkiaVulkanCtx {
    entry: Entry,
    instance: Instance,
    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    queue_family_index: u32,
    /// Every Skia context gets its own queue (round robin when there are more contexts),
    /// because a queue must not be submitted to from several threads at once.
    queues: Vec<vk::Queue>,
    /// One lock per queue: contexts that end up on the same queue submit one at a time.
    queue_locks: Vec<super::QueueLock>,
    next_queue: std::sync::atomic::AtomicUsize,
    width: usize,
    height: usize,
    /// What Skia has to know about a device it did not see being created: the API version
    /// of the instance and the enabled instance and device extensions.
    foreign_device: Option<ForeignDevice>,
    /// The libav device this context renders on when the device is shared with `FFmpeg`
    /// ([`Self::new_shared_with_encoder`]). The device belongs to `FFmpeg` then.
    #[cfg(feature = "vulkan-video")]
    encoder_device: Option<fframes::AvBuffer>,
}

struct ForeignDevice {
    api_version: gpu::vk::Version,
    instance_extensions: Vec<String>,
    device_extensions: Vec<String>,
}

/// A Skia GPU context and the queue it submits to.
pub(crate) struct VulkanContext {
    pub(crate) gpu: DirectContext,
    pub(crate) queue_lock: super::QueueLock,
    pub(crate) queue: vk::Queue,
    #[cfg_attr(not(feature = "vulkan-video"), allow(dead_code))]
    pub(crate) queue_index: u32,
}

unsafe impl Send for SkiaVulkanCtx {}
unsafe impl Sync for SkiaVulkanCtx {}

impl SkiaVulkanCtx {
    pub fn new(width: usize, height: usize) -> FFramesRendererResult<Self> {
        unsafe {
            let entry = Entry::load().map_err(|e| {
                FFramesRendererError::Skia(format!("Failed to load Vulkan entry: {e}"))
            })?;

            let instance: Instance = {
                let api_version = vulkan_version(&entry)?.map_or_else(
                    || vk::make_api_version(0, 1, 1, 0),
                    |(major, minor, patch)| {
                        vk::make_api_version(0, major as u32, minor as u32, patch as u32)
                    },
                );

                let app_name = CString::new("fframes_skia_renderer").map_err(|e| {
                    FFramesRendererError::Skia(format!("Failed to create CString: {e}"))
                })?;

                let extension_names = [
                    vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_NAME,
                    vk::KHR_SURFACE_NAME,
                    vk::KHR_PORTABILITY_ENUMERATION_NAME,
                ];

                let extension_name_ptrs: Vec<*const c_char> =
                    extension_names.iter().map(|name| name.as_ptr()).collect();

                let app_info = vk::ApplicationInfo::default()
                    .application_name(&app_name)
                    .application_version(0)
                    .engine_name(&app_name)
                    .engine_version(0)
                    .api_version(api_version);

                let create_info = vk::InstanceCreateInfo::default()
                    .application_info(&app_info)
                    .enabled_extension_names(&extension_name_ptrs)
                    .flags(vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR);

                entry.create_instance(&create_info, None).map_err(|e| {
                    FFramesRendererError::Skia(format!("Failed to create Vulkan instance: {e}"))
                })?
            };

            let physical_device = {
                let physical_devices = instance.enumerate_physical_devices().map_err(|e| {
                    FFramesRendererError::Skia(format!("Failed to enumerate physical devices: {e}"))
                })?;

                physical_devices.iter().copied().next().ok_or_else(|| {
                    FFramesRendererError::Skia("No Vulkan physical devices found".to_string())
                })?
            };

            Self::new_with_device(entry, instance, physical_device, width, height)
        }
    }

    /// Create vulkan context with a pointer to the specific device and queue
    pub fn new_with_device(
        entry: Entry,
        instance: Instance,
        physical_device: vk::PhysicalDevice,
        width: usize,
        height: usize,
    ) -> FFramesRendererResult<Self> {
        unsafe {
            let (queue_family_index, queue_count) = instance
                .get_physical_device_queue_family_properties(physical_device)
                .iter()
                .enumerate()
                .find_map(|(index, info)| {
                    if info.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                        Some((index as u32, info.queue_count.clamp(1, 16)))
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    FFramesRendererError::Skia("Failed to find graphics queue family".to_string())
                })?;

            let device = {
                let priorities = vec![1.0; queue_count as usize];
                let queue_info = [vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(queue_family_index)
                    .queue_priorities(&priorities)];

                // TODO figure out if we need more required extensions
                let device_extension_names = [vk::KHR_SWAPCHAIN_NAME];
                let device_extension_ptrs: Vec<*const c_char> = device_extension_names
                    .iter()
                    .map(|name| name.as_ptr())
                    .collect();

                let features = vk::PhysicalDeviceFeatures::default();
                let device_create_info = vk::DeviceCreateInfo::default()
                    .queue_create_infos(&queue_info)
                    .enabled_extension_names(&device_extension_ptrs)
                    .enabled_features(&features);

                instance
                    .create_device(physical_device, &device_create_info, None)
                    .map_err(|e| {
                        FFramesRendererError::Skia(format!("Failed to create device: {e}"))
                    })?
            };

            let queues: Vec<_> = (0..queue_count)
                .map(|index| device.get_device_queue(queue_family_index, index))
                .collect();

            Ok(SkiaVulkanCtx {
                entry,
                instance,
                device,
                physical_device,
                queue_family_index,
                queue_locks: queue_locks(&queues),
                queues,
                next_queue: std::sync::atomic::AtomicUsize::new(0),
                width,
                height,
                foreign_device: None,
                #[cfg(feature = "vulkan-video")]
                encoder_device: None,
            })
        }
    }

    /// Renders on the Vulkan device of `FFmpeg` (`av_hwdevice_ctx_create`) instead of an own
    /// one. Sharing the device is what lets Vulkan Video encoders (`h264_vulkan`,
    /// `hevc_vulkan`) take the rendered frames as GPU images: with such an encoder the
    /// frames are converted to NV12 on the GPU and never read back. Every other encoder
    /// works as with [`Self::new`].
    ///
    /// Fails when `FFmpeg` can not open a Vulkan device (Vulkan 1.3 is required).
    #[cfg(feature = "vulkan-video")]
    pub fn new_shared_with_encoder(width: usize, height: usize) -> FFramesRendererResult<Self> {
        use fframes::ffmpeg_sys_fframes::{
            AVHWDeviceContext, AVHWDeviceType, AVVulkanDeviceContext,
        };

        let encoder_device = fframes::hardware_device(AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN)
            .map_err(|err| {
                FFramesRendererError::Skia(format!("FFmpeg can not open a Vulkan device: {err}"))
            })?;

        unsafe {
            let hwctx = &*(*encoder_device.data::<AVHWDeviceContext>())
                .hwctx
                .cast::<AVVulkanDeviceContext>();

            let entry = Entry::load().map_err(|e| {
                FFramesRendererError::Skia(format!("Failed to load Vulkan entry: {e}"))
            })?;
            let instance = Instance::load(
                entry.static_fn(),
                vk::Instance::from_raw(hwctx.inst as usize as u64),
            );
            let device = ash::Device::load(
                instance.fp_v1_0(),
                vk::Device::from_raw(hwctx.act_dev as usize as u64),
            );

            let family = hwctx.qf[..hwctx.nb_qf.max(0) as usize]
                .iter()
                .find(|family| {
                    family.num > 0 && family.flags & vk::QueueFlags::GRAPHICS.as_raw() != 0
                })
                .ok_or_else(|| {
                    FFramesRendererError::Skia(
                        "FFmpeg's Vulkan device has no graphics queue".to_string(),
                    )
                })?;
            let queue_family_index = family.idx as u32;
            let queue_flags = vk::DeviceQueueCreateFlags::from_raw(hwctx.queue_flags);
            let queues: Vec<_> = (0..family.num.clamp(1, 16) as u32)
                .map(|queue_index| {
                    if queue_flags.is_empty() {
                        device.get_device_queue(queue_family_index, queue_index)
                    } else {
                        // queues created with flags are only reachable with them
                        device.get_device_queue2(
                            &vk::DeviceQueueInfo2::default()
                                .flags(queue_flags)
                                .queue_family_index(queue_family_index)
                                .queue_index(queue_index),
                        )
                    }
                })
                .collect();

            Ok(SkiaVulkanCtx {
                entry,
                instance,
                device,
                physical_device: vk::PhysicalDevice::from_raw(hwctx.phys_dev as usize as u64),
                queue_family_index,
                queue_locks: queue_locks(&queues),
                queues,
                next_queue: std::sync::atomic::AtomicUsize::new(0),
                width,
                height,
                foreign_device: Some(ForeignDevice {
                    // what libav creates its instance with
                    api_version: gpu::vk::Version::new(1, 3, 0),
                    instance_extensions: extension_names(
                        hwctx.enabled_inst_extensions,
                        hwctx.nb_enabled_inst_extensions,
                    ),
                    device_extensions: extension_names(
                        hwctx.enabled_dev_extensions,
                        hwctx.nb_enabled_dev_extensions,
                    ),
                }),
                encoder_device: Some(encoder_device),
            })
        }
    }

    /// The libav hardware device of a context created with
    /// [`Self::new_shared_with_encoder`].
    #[cfg(feature = "vulkan-video")]
    pub fn encoder_device(&self) -> Option<&fframes::AvBuffer> {
        self.encoder_device.as_ref()
    }

    #[cfg(feature = "vulkan-video")]
    pub(crate) fn device(&self) -> &ash::Device {
        &self.device
    }

    #[cfg(feature = "vulkan-video")]
    pub(crate) fn queue_family_index(&self) -> u32 {
        self.queue_family_index
    }

    /// A new Skia context on the next queue of the device.
    pub(crate) fn create_context(&self) -> FFramesRendererResult<VulkanContext> {
        unsafe {
            let get_proc = |of| {
                let proc_ptr = match of {
                    gpu::vk::GetProcOf::Instance(instance, name) => {
                        let ash_instance = vk::Instance::from_raw(instance as _);
                        self.entry.get_instance_proc_addr(ash_instance, name)
                    }
                    gpu::vk::GetProcOf::Device(device, name) => {
                        let ash_device = vk::Device::from_raw(device as _);
                        self.instance.get_device_proc_addr(ash_device, name)
                    }
                };
                match proc_ptr {
                    Some(f) => f as *const c_void,
                    None => std::ptr::null(),
                }
            };

            let queue_index = self
                .next_queue
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                % self.queues.len();
            let queue = self.queues[queue_index];
            let queue_lock = self.queue_locks[queue_index].clone();
            // The factory may destroy a partially initialized GPU context on failure.
            let _guard = super::lock_queue(Some(&queue_lock));

            // Initialize Skia Vulkan backend context
            let mut backend_context = gpu::vk::BackendContext::new_builder(
                self.instance.handle().as_raw() as _,
                self.physical_device.as_raw() as _,
                self.device.handle().as_raw() as _,
                (queue.as_raw() as _, self.queue_family_index as usize),
                &get_proc,
                self.foreign_device
                    .as_ref()
                    .map(|device| device.api_version),
            );
            if let Some(device) = &self.foreign_device {
                let instance_extensions: Vec<&str> = device
                    .instance_extensions
                    .iter()
                    .map(String::as_str)
                    .collect();
                let device_extensions: Vec<&str> = device
                    .device_extensions
                    .iter()
                    .map(String::as_str)
                    .collect();
                backend_context =
                    backend_context.with_extensions(&instance_extensions, &device_extensions);
            }
            let backend_context = backend_context.build();

            // Configure context options (same as in Metal implementation)
            let mut gpu_context_opts = gpu::ContextOptions::new();

            // Cache configuration
            gpu_context_opts.glyph_cache_texture_maximum_bytes = 64 * 1024 * 1024; // 64MB
            gpu_context_opts.allow_multiple_glyph_cache_textures = Enable::Yes;
            gpu_context_opts.buffer_map_threshold = 4096;
            gpu_context_opts.minimum_staging_buffer_size = 1_048_576;

            // Path rendering optimizations
            gpu_context_opts.allow_path_mask_caching = true;
            gpu_context_opts.disable_distance_field_paths = false;
            gpu_context_opts.disable_coverage_counting_paths = true;

            // Shader configuration
            gpu_context_opts.runtime_program_cache_size = 256;
            gpu_context_opts.shader_cache_strategy = ShaderCacheStrategy::BackendBinary;
            gpu_context_opts.reduced_shader_variations = false;

            // Batch processing
            gpu_context_opts.reduce_ops_task_splitting = Enable::Yes;

            let gpu = gpu::direct_contexts::make_vulkan(&backend_context, Some(&gpu_context_opts))
                .ok_or_else(|| {
                    FFramesRendererError::Skia("Failed to create GPU context".to_string())
                })?;

            Ok(VulkanContext {
                gpu,
                queue_lock: queue_lock.clone(),
                queue,
                queue_index: queue_index as u32,
            })
        }
    }
}

impl SkiaBackend for SkiaVulkanCtx {
    fn create_skia_surface(&self) -> FFramesRendererResult<(Surface, Option<DirectContext>)> {
        let context = self.create_skia_context()?;
        Ok((context.surface, context.gpu))
    }

    fn create_skia_context(&self) -> FFramesRendererResult<super::SkiaContext> {
        let context = self.create_context()?;
        let lock = context.queue_lock.clone();
        // Declare the guard before the GPU/surface locals so error cleanup stays locked.
        let _guard = super::lock_queue(Some(&lock));
        let VulkanContext {
            gpu: mut gpu_context,
            queue_lock,
            queue,
            ..
        } = context;
        let reader = super::vulkan_readback::VulkanSurfaceReader::new(
            &self.device,
            &unsafe {
                self.instance
                    .get_physical_device_memory_properties(self.physical_device)
            },
            self.queue_family_index,
            queue,
            queue_lock.clone(),
        )?;
        // Skia owns the image and its allocation, including failed construction.
        // A wrapped backend render target only borrows caller-owned Vulkan resources.
        let mut surface = gpu::surfaces::render_target(
            &mut gpu_context,
            gpu::Budgeted::Yes,
            &ImageInfo::new(
                (self.width as i32, self.height as i32),
                ColorType::RGBA8888,
                AlphaType::Premul,
                None,
            ),
            None,
            SurfaceOrigin::TopLeft,
            None,
            false,
            None,
        )
        .ok_or_else(|| {
            FFramesRendererError::Skia("Failed to create Vulkan render target".into())
        })?;

        // Owned surfaces may allocate lazily. Detect allocation failure now, rather
        // than after callers have started writing output or rendering a frame.
        gpu::surfaces::get_backend_render_target(
            &mut surface,
            skia_safe::surface::BackendHandleAccess::FlushRead,
        )
        .ok_or_else(|| {
            FFramesRendererError::Skia("Failed to allocate Vulkan render target".into())
        })?;
        Ok(super::SkiaContext {
            surface,
            gpu: Some(gpu_context),
            queue_lock: Some(queue_lock),
            reader: Some(Box::new(reader)),
        })
    }
    #[cfg(feature = "vulkan-video")]
    fn negotiate_hardware_frames(
        &self,
        encoder: &fframes::VideoEncoderInfo<'_>,
    ) -> Option<fframes::EncoderInput> {
        crate::frame_export::vulkan_frames::negotiate(self, encoder)
    }

    #[cfg(feature = "vulkan-video")]
    fn hardware_frame_target(
        &self,
        input: &fframes::EncoderInput,
        width: i32,
        height: i32,
    ) -> FFramesRendererResult<(DirectContext, Box<dyn crate::HardwareFrameTarget>)> {
        let (gpu, target) =
            crate::frame_export::vulkan_frames::VulkanFrameTarget::new(self, input, width, height)?;
        Ok((gpu, Box::new(target)))
    }
}

impl Drop for SkiaVulkanCtx {
    fn drop(&mut self) {
        // a device shared with FFmpeg is destroyed with its last reference
        #[cfg(feature = "vulkan-video")]
        if self.encoder_device.is_some() {
            return;
        }

        unsafe {
            self.device.destroy_device(None);
            // Instance and entry are managed by Arc, so they'll be cleaned up
            // when the last reference is dropped
        }
    }
}

impl<'a> SkiaFFramesRenderer<'a, SkiaVulkanCtx> {
    #[cfg(feature = "vulkan")]
    /// Provides default implementation of Vulkan based backend, which is possible to replicate
    /// manually with `new` method.
    pub fn new_vulkan(
        ctx: &'a SkiaVulkanCtx,
        pipeline_config: SkiaPipelineConfig,
    ) -> FFramesRendererResult<Self> {
        Ok(Self::new(pipeline_config, ctx))
    }
}

/// Copies a C array of extension names.
#[cfg(feature = "vulkan-video")]
unsafe fn extension_names(names: *const *const c_char, count: i32) -> Vec<String> {
    if names.is_null() {
        return Vec::new();
    }

    unsafe { std::slice::from_raw_parts(names, count.max(0) as usize) }
        .iter()
        .filter(|name| !name.is_null())
        .map(|name| {
            unsafe { std::ffi::CStr::from_ptr(*name) }
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

fn queue_locks(queues: &[vk::Queue]) -> Vec<super::QueueLock> {
    queues.iter().map(|_| super::QueueLock::default()).collect()
}

fn vulkan_version(entry: &Entry) -> FFramesRendererResult<Option<(usize, usize, usize)>> {
    let detected_version = unsafe {
        entry.try_enumerate_instance_version().map_err(|e| {
            FFramesRendererError::Skia(format!("Failed to enumerate instance version: {e}"))
        })?
    };

    Ok(detected_version.map(|ver| {
        (
            vk::api_version_major(ver) as usize,
            vk::api_version_minor(ver) as usize,
            vk::api_version_patch(ver) as usize,
        )
    }))
}

#[cfg(test)]
#[path = "vulkan_lifetime_tests.rs"]
mod lifetime_tests;
