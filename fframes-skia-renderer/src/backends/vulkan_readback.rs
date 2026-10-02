use super::{QueueLock, SurfaceReader, lock_queue};
use ash::vk::{self, Handle};
use fframes::{FFramesRendererError, FFramesRendererResult};
use skia_safe::gpu::{self, DirectContext};
use skia_safe::surface::BackendHandleAccess;
use skia_safe::{ColorType, Surface};

fn vulkan_error(what: &str, result: vk::Result) -> FFramesRendererError {
    FFramesRendererError::Skia(format!("{what}: {result}"))
}

/// The Vulkan image behind a GPU surface.
pub(crate) fn backing_image(surface: &mut Surface) -> Option<vk::Image> {
    let info = gpu::surfaces::get_backend_texture(surface, BackendHandleAccess::FlushRead)
        .and_then(|texture| texture.vulkan_image_info())
        .or_else(|| {
            gpu::surfaces::get_backend_render_target(surface, BackendHandleAccess::FlushRead)
                .and_then(|target| target.vulkan_image_info())
        })?;

    Some(vk::Image::from_raw(*info.image() as usize as u64))
}

/// Host memory the GPU copies a surface into.
struct Staging {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: *const u8,
    size: usize,
}

pub(crate) struct VulkanSurfaceReader {
    device: ash::Device,
    queue: vk::Queue,
    queue_lock: QueueLock,
    memory_properties: vk::PhysicalDeviceMemoryProperties,
    command_pool: vk::CommandPool,
    commands: vk::CommandBuffer,
    fence: vk::Fence,
    staging: Option<Staging>,
}

impl VulkanSurfaceReader {
    pub(crate) fn new(
        device: &ash::Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        queue_family_index: u32,
        queue: vk::Queue,
        queue_lock: QueueLock,
    ) -> FFramesRendererResult<Self> {
        unsafe {
            let command_pool = device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .queue_family_index(queue_family_index)
                        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                    None,
                )
                .map_err(|err| vulkan_error("can not create a command pool", err))?;

            let created = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .and_then(|buffers| {
                    let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
                    Ok((buffers[0], fence))
                });

            match created {
                Ok((commands, fence)) => Ok(Self {
                    device: device.clone(),
                    queue,
                    queue_lock,
                    memory_properties: *memory_properties,
                    command_pool,
                    commands,
                    fence,
                    staging: None,
                }),
                Err(err) => {
                    device.destroy_command_pool(command_pool, None);
                    Err(vulkan_error("can not allocate a command buffer", err))
                }
            }
        }
    }

    fn free_staging(&mut self) {
        if let Some(staging) = self.staging.take() {
            unsafe {
                self.device.unmap_memory(staging.memory);
                self.device.destroy_buffer(staging.buffer, None);
                self.device.free_memory(staging.memory, None);
            }
        }
    }

    /// Mapped memory of at least `size` bytes the GPU can copy into. Kept between frames.
    fn staging(&mut self, size: usize) -> FFramesRendererResult<&Staging> {
        if self
            .staging
            .as_ref()
            .is_none_or(|staging| staging.size < size)
        {
            self.free_staging();

            unsafe {
                let buffer = self
                    .device
                    .create_buffer(
                        &vk::BufferCreateInfo::default()
                            .size(size as u64)
                            .usage(vk::BufferUsageFlags::TRANSFER_DST)
                            .sharing_mode(vk::SharingMode::EXCLUSIVE),
                        None,
                    )
                    .map_err(|err| vulkan_error("can not create a readback buffer", err))?;
                let requirements = self.device.get_buffer_memory_requirements(buffer);

                // Cached memory is read at the speed of ordinary memory; fall back to what
                // every device has.
                let visible =
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
                let memory_type = [visible | vk::MemoryPropertyFlags::HOST_CACHED, visible]
                    .into_iter()
                    .find_map(|wanted| {
                        (0..self.memory_properties.memory_type_count).find(|&index| {
                            requirements.memory_type_bits & (1 << index) != 0
                                && self.memory_properties.memory_types[index as usize]
                                    .property_flags
                                    .contains(wanted)
                        })
                    });

                let mapped = memory_type
                    .ok_or(vk::Result::ERROR_FEATURE_NOT_PRESENT)
                    .and_then(|memory_type| {
                        self.device.allocate_memory(
                            &vk::MemoryAllocateInfo::default()
                                .allocation_size(requirements.size)
                                .memory_type_index(memory_type),
                            None,
                        )
                    })
                    .and_then(|memory| {
                        let mapped =
                            self.device
                                .bind_buffer_memory(buffer, memory, 0)
                                .and_then(|()| {
                                    self.device.map_memory(
                                        memory,
                                        0,
                                        vk::WHOLE_SIZE,
                                        vk::MemoryMapFlags::empty(),
                                    )
                                });
                        if mapped.is_err() {
                            self.device.free_memory(memory, None);
                        }
                        mapped.map(|mapped| (memory, mapped))
                    });

                match mapped {
                    Ok((memory, mapped)) => {
                        self.staging = Some(Staging {
                            buffer,
                            memory,
                            mapped: mapped.cast::<u8>().cast_const(),
                            size,
                        });
                    }
                    Err(err) => {
                        self.device.destroy_buffer(buffer, None);
                        return Err(vulkan_error("can not allocate readback memory", err));
                    }
                }
            }
        }

        Ok(self
            .staging
            .as_ref()
            .expect("the staging buffer was just created"))
    }
}

impl SurfaceReader for VulkanSurfaceReader {
    fn read(
        &mut self,
        gpu: &mut DirectContext,
        surface: &mut Surface,
        pixels: &mut [u8],
    ) -> FFramesRendererResult<()> {
        let (width, height) = (surface.width() as u32, surface.height() as u32);
        let size = width as usize * height as usize * 4;
        if !matches!(
            surface.image_info().color_type(),
            ColorType::RGBA8888 | ColorType::BGRA8888
        ) || pixels.len() < size
        {
            return Err(FFramesRendererError::Skia(
                "the surface reader copies four byte pixels into a buffer of their size".to_owned(),
            ));
        }

        let image = backing_image(surface).ok_or_else(|| {
            FFramesRendererError::Skia("the surface has no Vulkan image".to_owned())
        })?;
        let (buffer, mapped) = {
            let staging = self.staging(size)?;
            (staging.buffer, staging.mapped)
        };

        // Skia records what was drawn (this is the expensive part and needs no queue) and
        // leaves the image in the layout it is copied from.
        gpu.flush_surface_with_texture_state(
            surface,
            &gpu::FlushInfo::default(),
            Some(&gpu::vk::mutable_texture_states::new_vulkan(
                gpu::vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::QUEUE_FAMILY_IGNORED,
            )),
        );

        unsafe {
            let device = &self.device;
            let record = || -> Result<(), vk::Result> {
                device.reset_fences(&[self.fence])?;
                device.reset_command_buffer(self.commands, vk::CommandBufferResetFlags::empty())?;
                device.begin_command_buffer(
                    self.commands,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )?;
                device.cmd_copy_image_to_buffer(
                    self.commands,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    buffer,
                    &[vk::BufferImageCopy {
                        buffer_offset: 0,
                        // rows without padding
                        buffer_row_length: 0,
                        buffer_image_height: 0,
                        image_subresource: vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            mip_level: 0,
                            base_array_layer: 0,
                            layer_count: 1,
                        },
                        image_offset: vk::Offset3D::default(),
                        image_extent: vk::Extent3D {
                            width,
                            height,
                            depth: 1,
                        },
                    }],
                );
                device.end_command_buffer(self.commands)
            };
            record().map_err(|err| vulkan_error("can not record the readback", err))?;

            // The queue runs what it is given in order: the draws, then the copy.
            let submitted = {
                let _queue = lock_queue(Some(&self.queue_lock));
                if gpu.submit(gpu::SyncCpu::No) {
                    let commands = [self.commands];
                    device.queue_submit(
                        self.queue,
                        &[vk::SubmitInfo::default().command_buffers(&commands)],
                        self.fence,
                    )
                } else {
                    Err(vk::Result::ERROR_UNKNOWN)
                }
            };
            submitted.map_err(|err| vulkan_error("can not submit the frame to the GPU", err))?;

            device
                .wait_for_fences(&[self.fence], true, u64::MAX)
                .map_err(|err| vulkan_error("waiting for the readback failed", err))?;
            std::ptr::copy_nonoverlapping(mapped, pixels.as_mut_ptr(), size);
        }

        Ok(())
    }
}

impl Drop for VulkanSurfaceReader {
    fn drop(&mut self) {
        // `read` waits for its copy, so nothing of ours is in flight here
        self.free_staging();
        unsafe {
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.command_pool, None);
        }
    }
}
