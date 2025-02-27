use crate::skia_backend::{SkiaFFramesRenderer, SkiaPipelineConfig};
use ash::vk::{self, Handle};
use ash::{Entry, Instance};
use fframes_renderer::{FFramesRendererError, FFramesRendererResult};
use skia_safe::ColorType;
use skia_safe::gpu;
use skia_safe::gpu::ganesh::context_options::*;
use skia_safe::gpu::ganesh::vk::backend_render_targets;
use std::ffi::{CStr, CString, c_void};
use std::os::raw::c_char;

pub fn vulkan_version() -> FFramesRendererResult<Option<(usize, usize, usize)>> {
    let entry = unsafe { Entry::load() }
        .map_err(|e| FFramesRendererError::Skia(format!("Failed to load Vulkan entry: {}", e)))?;

    let detected_version = unsafe {
        entry.try_enumerate_instance_version().map_err(|e| {
            FFramesRendererError::Skia(format!("Failed to enumerate instance version: {}", e))
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

unsafe fn find_memory_type_index(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    requirements: vk::MemoryRequirements,
    properties: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let mem_properties = unsafe { instance.get_physical_device_memory_properties(physical_device) };

    (0..mem_properties.memory_type_count).find(|i| {
        let suitable = (requirements.memory_type_bits & (1 << i)) != 0;
        let memory_type = mem_properties.memory_types[*i as usize];
        suitable && memory_type.property_flags.contains(properties)
    })
}

pub struct SkiaVulkanCtx {
    entry: ash::Entry,
    instance: ash::Instance,
    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    queue_family_index: usize,
    queue: vk::Queue,
    allocator: gpu::vk::Alloc,
    image: vk::Image,
    width: usize,
    height: usize,
}

impl SkiaVulkanCtx {
    pub fn new(width: usize, height: usize) -> FFramesRendererResult<Self> {
        unsafe {
            let entry = Entry::load().map_err(|e| {
                FFramesRendererError::Skia(format!("Failed to load Vulkan entry: {}", e))
            })?;

            let instance: Instance = {
                let api_version = vulkan_version()?
                    .map(|(major, minor, patch)| {
                        vk::make_api_version(0, major as u32, minor as u32, patch as u32)
                    })
                    .unwrap_or_else(|| vk::make_api_version(0, 1, 1, 0));

                let app_name = CString::new("fframes_skia_renderer").map_err(|e| {
                    FFramesRendererError::Skia(format!("Failed to create CString: {}", e))
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
                    FFramesRendererError::Skia(format!("Failed to create Vulkan instance: {}", e))
                })?
            };

            let (physical_device, queue_family_index) = {
                let physical_devices = instance.enumerate_physical_devices().map_err(|e| {
                    FFramesRendererError::Skia(format!(
                        "Failed to enumerate physical devices: {}",
                        e
                    ))
                })?;

                physical_devices
                    .iter()
                    .map(|physical_device| {
                        instance
                            .get_physical_device_queue_family_properties(*physical_device)
                            .iter()
                            .enumerate()
                            .find_map(|(index, info)| {
                                let supports_graphic =
                                    info.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                                supports_graphic.then_some((*physical_device, index))
                            })
                    })
                    .find_map(|v| v)
                    .ok_or_else(|| {
                        FFramesRendererError::Skia(
                            "Failed to find suitable Vulkan device".to_string(),
                        )
                    })?
            };

            let device: ash::Device = {
                let features = vk::PhysicalDeviceFeatures::default();
                let priorities = [1.0];

                let queue_info = [vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(queue_family_index as _)
                    .queue_priorities(&priorities)];

                let available_extensions = instance
                    .enumerate_device_extension_properties(physical_device)
                    .map_err(|e| {
                        FFramesRendererError::Skia(format!(
                            "Failed to enumerate device extensions: {}",
                            e
                        ))
                    })?;

                let mut device_extension_names = vec![vk::KHR_SWAPCHAIN_NAME];
                if available_extensions.iter().any(|ext| {
                    CStr::from_ptr(ext.extension_name.as_ptr()) == vk::KHR_PORTABILITY_SUBSET_NAME
                }) {
                    device_extension_names.push(vk::KHR_PORTABILITY_SUBSET_NAME);
                }

                let device_extension_ptrs: Vec<*const c_char> = device_extension_names
                    .iter()
                    .map(|name| name.as_ptr())
                    .collect();

                let device_create_info = vk::DeviceCreateInfo::default()
                    .queue_create_infos(&queue_info)
                    .enabled_extension_names(&device_extension_ptrs)
                    .enabled_features(&features);

                instance
                    .create_device(physical_device, &device_create_info, None)
                    .map_err(|e| {
                        FFramesRendererError::Skia(format!("Failed to create device: {}", e))
                    })?
            };

            let queue_index: usize = 0;
            let queue = device.get_device_queue(queue_family_index as _, queue_index as _);

            let image_create_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .extent(vk::Extent3D {
                    width: width as u32,
                    height: height as u32,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::LINEAR)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let image = device.create_image(&image_create_info, None).map_err(|e| {
                FFramesRendererError::Skia(format!("Failed to create image: {}", e))
            })?;

            let mem_requirements = device.get_image_memory_requirements(image);
            let memory_type_index = find_memory_type_index(
                &instance,
                physical_device,
                mem_requirements,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
            .ok_or_else(|| {
                FFramesRendererError::Skia("Failed to find suitable memory type".to_string())
            })?;

            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(mem_requirements.size)
                .memory_type_index(memory_type_index);

            let memory = device.allocate_memory(&alloc_info, None).map_err(|e| {
                FFramesRendererError::Skia(format!("Failed to allocate memory: {}", e))
            })?;

            device.bind_image_memory(image, memory, 0).map_err(|e| {
                FFramesRendererError::Skia(format!("Failed to bind image memory: {}", e))
            })?;

            let allocator = gpu::vk::Alloc::from_device_memory(
                memory.as_raw() as _,
                0,
                mem_requirements.size,
                gpu::vk::AllocFlag::empty(),
            );

            Ok(SkiaVulkanCtx {
                entry,
                instance,
                device,
                physical_device,
                queue_family_index,
                queue,
                allocator,
                image,
                width,
                height,
            })
        }
    }
}

impl SkiaFFramesRenderer {
    pub fn new_vulkan(
        ctx: &SkiaVulkanCtx,
        pipeline_config: SkiaPipelineConfig,
    ) -> FFramesRendererResult<Self> {
        let get_proc = |of| unsafe {
            let proc_ptr = match of {
                gpu::vk::GetProcOf::Instance(instance, name) => {
                    let ash_instance = vk::Instance::from_raw(instance as _);
                    ctx.entry.get_instance_proc_addr(ash_instance, name)
                }
                gpu::vk::GetProcOf::Device(device, name) => {
                    let ash_device = vk::Device::from_raw(device as _);
                    ctx.instance.get_device_proc_addr(ash_device, name)
                }
            };
            match proc_ptr {
                Some(f) => f as *const c_void,
                None => std::ptr::null(),
            }
        };

        let backend_context = unsafe {
            gpu::vk::BackendContext::new(
                ctx.instance.handle().as_raw() as _,
                ctx.physical_device.as_raw() as _,
                ctx.device.handle().as_raw() as _,
                (ctx.queue.as_raw() as _, ctx.queue_family_index),
                &get_proc as _,
            )
        };

        let mut gpu_context_opts = gpu::ContextOptions::new();

        gpu_context_opts.glyph_cache_texture_maximum_bytes = 64 * 1024 * 1024;
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

        let mut gpu_context =
            gpu::direct_contexts::make_vulkan(&backend_context, Some(&gpu_context_opts))
                .ok_or_else(|| {
                    FFramesRendererError::Skia("Failed to create GPU context".to_string())
                })?;

        let surface = unsafe {
            let image_info = gpu::vk::ImageInfo::new(
                ctx.image.as_raw() as _,
                ctx.allocator,
                gpu::vk::ImageTiling::OPTIMAL,
                gpu::vk::ImageLayout::UNDEFINED,
                gpu::vk::Format::R8G8B8A8_UNORM,
                1,
                ctx.queue_family_index as u32,
                None,
                gpu::Protected::No,
                None,
            );

            let backend_render_target =
                backend_render_targets::make_vk((ctx.width as i32, ctx.height as i32), &image_info);

            gpu::surfaces::wrap_backend_render_target(
                &mut gpu_context,
                &backend_render_target,
                gpu::SurfaceOrigin::TopLeft,
                ColorType::RGBA8888,
                None,
                None,
            )
            .ok_or_else(|| {
                FFramesRendererError::Skia("Failed to wrap backend render target".to_string())
            })?
        };

        Ok(SkiaFFramesRenderer::new(
            pipeline_config,
            surface,
            Some(gpu_context),
        ))
    }
}
