use super::SkiaBackend;
use crate::skia_backend::{SkiaFFramesRenderer, SkiaPipelineConfig};
use ash::vk::{self, Handle};
use ash::{Entry, Instance};
use fframes::{FFramesRendererError, FFramesRendererResult};
use skia_safe::ColorType;
use skia_safe::gpu::ganesh::context_options::{Enable, ShaderCacheStrategy};
use skia_safe::gpu::ganesh::vk::backend_render_targets;
use skia_safe::gpu::{self, DirectContext};
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
    next_queue: std::sync::atomic::AtomicUsize,
    width: usize,
    height: usize,
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
                let api_version = vulkan_version(&entry)?
                    .map(|(major, minor, patch)| {
                        vk::make_api_version(0, major as u32, minor as u32, patch as u32)
                    })
                    .unwrap_or_else(|| vk::make_api_version(0, 1, 1, 0));

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

            let queues = (0..queue_count)
                .map(|index| device.get_device_queue(queue_family_index, index))
                .collect();

            Ok(SkiaVulkanCtx {
                entry,
                instance,
                device,
                physical_device,
                queue_family_index,
                queues,
                next_queue: std::sync::atomic::AtomicUsize::new(0),
                width,
                height,
            })
        }
    }

    fn find_memory_type_index(
        &self,
        type_filter: u32,
        properties: vk::MemoryPropertyFlags,
    ) -> Option<u32> {
        let mem_properties = unsafe {
            self.instance
                .get_physical_device_memory_properties(self.physical_device)
        };

        (0..mem_properties.memory_type_count).find(|&i| {
            (type_filter & (1 << i)) != 0
                && mem_properties.memory_types[i as usize]
                    .property_flags
                    .contains(properties)
        })
    }
}

impl SkiaBackend for SkiaVulkanCtx {
    fn create_skia_surface(&self) -> FFramesRendererResult<(Surface, Option<DirectContext>)> {
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

            let queue = self.queues[self
                .next_queue
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                % self.queues.len()];

            // Initialize Skia Vulkan backend context
            let backend_context = gpu::vk::BackendContext::new(
                self.instance.handle().as_raw() as _,
                self.physical_device.as_raw() as _,
                self.device.handle().as_raw() as _,
                (queue.as_raw() as _, self.queue_family_index as usize),
                &get_proc as _,
            );

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

            // Create GPU context
            let mut gpu_context =
                gpu::direct_contexts::make_vulkan(&backend_context, Some(&gpu_context_opts))
                    .ok_or_else(|| {
                        FFramesRendererError::Skia("Failed to create GPU context".to_string())
                    })?;

            let image_create_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .extent(vk::Extent3D {
                    width: self.width as u32,
                    height: self.height as u32,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | vk::ImageUsageFlags::TRANSFER_DST,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);

            let image = self
                .device
                .create_image(&image_create_info, None)
                .map_err(|e| FFramesRendererError::Skia(format!("Failed to create image: {e}")))?;

            // Allocate memory for the image
            let mem_requirements = self.device.get_image_memory_requirements(image);
            let memory_type_index = self
                .find_memory_type_index(
                    mem_requirements.memory_type_bits,
                    vk::MemoryPropertyFlags::DEVICE_LOCAL,
                )
                .ok_or_else(|| {
                    FFramesRendererError::Skia("Failed to find suitable memory type".to_string())
                })?;

            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(mem_requirements.size)
                .memory_type_index(memory_type_index);

            let memory = self
                .device
                .allocate_memory(&alloc_info, None)
                .map_err(|e| {
                    FFramesRendererError::Skia(format!("Failed to allocate memory: {e}"))
                })?;

            self.device
                .bind_image_memory(image, memory, 0)
                .map_err(|e| {
                    FFramesRendererError::Skia(format!("Failed to bind image memory: {e}"))
                })?;

            // this is correct to have at the texture level, this is just a way
            // to get the space already allocated by device and entity
            let allocator = gpu::vk::Alloc::from_device_memory(
                memory.as_raw() as _,
                0,
                mem_requirements.size,
                gpu::vk::AllocFlag::empty(),
            );

            let image_info = gpu::vk::ImageInfo::new(
                image.as_raw() as _,
                allocator,
                gpu::vk::ImageTiling::OPTIMAL,
                gpu::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                gpu::vk::Format::R8G8B8A8_UNORM,
                1,
                Some(self.queue_family_index),
                None,
                gpu::Protected::No,
                None,
            );

            let backend_render_target = backend_render_targets::make_vk(
                (self.width as i32, self.height as i32),
                &image_info,
            );

            let surface = gpu::surfaces::wrap_backend_render_target(
                &mut gpu_context,
                &backend_render_target,
                SurfaceOrigin::TopLeft,
                ColorType::RGBA8888,
                None,
                None,
            )
            .ok_or_else(|| {
                FFramesRendererError::Skia("Failed to wrap backend render target".to_string())
            })?;

            Ok((surface, Some(gpu_context)))
        }
    }
}

impl Drop for SkiaVulkanCtx {
    fn drop(&mut self) {
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
