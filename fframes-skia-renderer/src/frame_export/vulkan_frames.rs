use super::planes::Channel;
use crate::HardwareFrameTarget;
use crate::vulkan::SkiaVulkanCtx;
use ash::vk::{self, Handle};
use fframes::ffmpeg_sys_fframes::{
    AVHWDeviceContext, AVHWFramesContext, AVPixelFormat, AVVkFrame, AVVulkanDeviceContext,
    AVVulkanFramesContext,
};
use fframes::{
    AvBuffer, EncoderInput, FFramesRendererError, FFramesRendererResult, VideoEncoderInfo,
    VideoFrame,
};
use skia_safe::gpu::{self, DirectContext};
use skia_safe::runtime_effect::ChildPtr;
use skia_safe::{
    AlphaType, BlendMode, ColorType, FilterMode, ImageInfo, MipmapMode, Paint, RuntimeEffect,
    SamplingOptions, Surface, TileMode,
};

/// Copies that may be in flight on the GPU before the rendering thread waits for the oldest.
const IN_FLIGHT_COPIES: usize = 4;

const PLANE_SHADER: &str = "
uniform shader frame;
// frame pixels per sample of the plane
uniform float2 subsampling;
uniform float4 firstWeights;
uniform float4 secondWeights;
uniform float2 offsets;

half4 main(float2 coord) {
    // The center of a subsampled sample is the corner between the frame pixels it covers,
    // where bilinear filtering returns their average.
    float4 color = float4(frame.eval(coord * subsampling));
    return half4(float4(
        dot(color, firstWeights) + offsets.x,
        dot(color, secondWeights) + offsets.y,
        0.0,
        1.0));
}
";

fn skia_error(message: &str) -> FFramesRendererError {
    FFramesRendererError::Skia(message.to_owned())
}

fn vulkan_error(what: &str, result: vk::Result) -> FFramesRendererError {
    FFramesRendererError::Skia(format!("{what}: {result}"))
}

/// Offers NV12 Vulkan frames to encoders that take them, when the device is shared.
pub(crate) fn negotiate(
    backend: &SkiaVulkanCtx,
    encoder: &VideoEncoderInfo<'_>,
) -> Option<EncoderInput> {
    let device = backend.encoder_device()?;
    if !super::accepts_hardware_frames(encoder, AVPixelFormat::AV_PIX_FMT_VULKAN) {
        return None;
    }

    // libav adds the usage flags the encoder needs to the images of the pool.
    let input = EncoderInput::hardware_frames(
        device,
        AVPixelFormat::AV_PIX_FMT_VULKAN,
        AVPixelFormat::AV_PIX_FMT_NV12,
        (encoder.width, encoder.height),
        |_| {},
    )
    .ok()?;

    // Draw a frame the way the render will, so a device that can not do it is found out
    // before the render depends on it.
    let (gpu, target) =
        VulkanFrameTarget::new(backend, &input, encoder.width, encoder.height).ok()?;
    super::trial_frame(Box::new(target), gpu).ok()?;

    Some(input)
}

/// A plane of the converted frame: the surface the shader draws it into and the image
/// behind that surface.
struct Plane {
    surface: Surface,
    image: vk::Image,
    extent: vk::Extent3D,
    channels: (Channel, Option<Channel>),
    subsampling: f32,
}

impl Plane {
    fn new(
        gpu: &mut DirectContext,
        color_type: ColorType,
        (width, height): (i32, i32),
        channels: (Channel, Option<Channel>),
        subsampling: f32,
    ) -> FFramesRendererResult<Self> {
        let mut surface = gpu::surfaces::render_target(
            gpu,
            gpu::Budgeted::Yes,
            &ImageInfo::new((width, height), color_type, AlphaType::Premul, None),
            None,
            gpu::SurfaceOrigin::TopLeft,
            None,
            false,
            None,
        )
        .ok_or_else(|| skia_error("can not create a GPU surface for a plane of the frame"))?;

        let image = crate::backends::vulkan_readback::backing_image(&mut surface)
            .ok_or_else(|| skia_error("a plane surface has no Vulkan image"))?;

        Ok(Self {
            surface,
            image,
            extent: vk::Extent3D {
                width: width as u32,
                height: height as u32,
                depth: 1,
            },
            channels,
            subsampling,
        })
    }

    fn draw(
        &mut self,
        effect: &RuntimeEffect,
        frame: &skia_safe::Image,
    ) -> FFramesRendererResult<()> {
        let filter = if self.subsampling > 1. {
            FilterMode::Linear
        } else {
            FilterMode::Nearest
        };
        let frame = frame
            .to_shader(
                (TileMode::Clamp, TileMode::Clamp),
                SamplingOptions::new(filter, MipmapMode::None),
                None,
            )
            .ok_or_else(|| skia_error("can not sample the rendered frame"))?;

        let (first_weights, first_offset) = self.channels.0.weights();
        let (second_weights, second_offset) =
            self.channels.1.map_or(([0.; 4], 0.), Channel::weights);
        let uniforms = super::pack_uniforms(
            effect,
            &[
                ("subsampling", &[self.subsampling, self.subsampling]),
                ("firstWeights", &first_weights),
                ("secondWeights", &second_weights),
                ("offsets", &[first_offset, second_offset]),
            ],
        )?;
        let shader = effect
            .make_shader(uniforms, &[ChildPtr::Shader(frame)], None)
            .ok_or_else(|| skia_error("can not instantiate the plane shader"))?;

        let mut paint = Paint::default();
        paint.set_shader(shader);
        paint.set_blend_mode(BlendMode::Src);
        self.surface.canvas().draw_paint(&paint);
        Ok(())
    }
}

/// The lock of the queue a GPU context submits to. `FFmpeg` uses the queues of the shared
/// device too and guards them with these locks, so everything Skia or we submit to the
/// queue has to hold it.
#[derive(Clone)]
struct SharedQueueLock {
    /// The lock other Skia contexts on the same queue take.
    backend_lock: crate::QueueLock,
    encoder_device: AvBuffer,
    queue_family_index: u32,
    queue_index: u32,
}

impl SharedQueueLock {
    /// Runs `submit` while no other thread submits to the queue.
    fn locked<T>(&self, submit: impl FnOnce() -> T) -> T {
        let _backend = crate::lock_queue(Some(&self.backend_lock));
        unsafe {
            let device = self.encoder_device.data::<AVHWDeviceContext>();
            let hwctx = &*(*device).hwctx.cast::<AVVulkanDeviceContext>();

            if let Some(lock) = hwctx.lock_queue {
                lock(device, self.queue_family_index, self.queue_index);
            }
            let result = submit();
            if let Some(unlock) = hwctx.unlock_queue {
                unlock(device, self.queue_family_index, self.queue_index);
            }
            result
        }
    }
}

/// A command buffer for one copy and the fence that tells when the GPU is done with it.
struct CopySlot {
    commands: vk::CommandBuffer,
    fence: vk::Fence,
    in_flight: bool,
}

/// Renders frames into the Vulkan images of the encoder's frame pool, see the module docs.
pub(crate) struct VulkanFrameTarget {
    scene: Surface,
    luma: Plane,
    chroma: Plane,
    effect: RuntimeEffect,
    device: ash::Device,
    queue: vk::Queue,
    queue_lock: SharedQueueLock,
    command_pool: vk::CommandPool,
    slots: Vec<CopySlot>,
    next_slot: usize,
    frames: AvBuffer,
}

impl VulkanFrameTarget {
    pub(crate) fn new(
        backend: &SkiaVulkanCtx,
        input: &EncoderInput,
        width: i32,
        height: i32,
    ) -> FFramesRendererResult<(DirectContext, Self)> {
        let (Some(encoder_device), Some(frames)) = (backend.encoder_device(), &input.hw_frames_ctx)
        else {
            return Err(skia_error(
                "Vulkan frames need a device shared with FFmpeg (SkiaVulkanCtx::new_shared_with_encoder)",
            ));
        };
        if input.software_format() != AVPixelFormat::AV_PIX_FMT_NV12 {
            return Err(skia_error("only NV12 Vulkan frames are supported"));
        }
        if width % 2 != 0 || height % 2 != 0 {
            return Err(skia_error(
                "NV12 Vulkan frames need an even width and height",
            ));
        }

        let context = backend.create_context()?;
        let mut gpu = context.gpu;

        let scene = gpu::surfaces::render_target(
            &mut gpu,
            gpu::Budgeted::Yes,
            &ImageInfo::new(
                (width, height),
                ColorType::RGBA8888,
                AlphaType::Premul,
                None,
            ),
            None,
            gpu::SurfaceOrigin::TopLeft,
            None,
            false,
            None,
        )
        .ok_or_else(|| skia_error("can not create the GPU surface for the frame"))?;
        let luma = Plane::new(
            &mut gpu,
            ColorType::R8UNorm,
            (width, height),
            (Channel::Y, None),
            1.,
        )?;
        let chroma = Plane::new(
            &mut gpu,
            ColorType::R8G8UNorm,
            (width / 2, height / 2),
            (Channel::U, Some(Channel::V)),
            2.,
        )?;
        let effect = RuntimeEffect::make_for_shader(PLANE_SHADER, None)
            .map_err(|err| FFramesRendererError::Skia(format!("plane shader: {err}")))?;

        let device = backend.device().clone();
        let queue_family_index = backend.queue_family_index();
        let (command_pool, slots) = unsafe {
            let command_pool = device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .queue_family_index(queue_family_index)
                        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                    None,
                )
                .map_err(|err| vulkan_error("can not create a command pool", err))?;

            let mut slots = Vec::with_capacity(IN_FLIGHT_COPIES);
            let allocated = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(IN_FLIGHT_COPIES as u32),
                )
                .and_then(|buffers| {
                    for commands in buffers {
                        slots.push(CopySlot {
                            commands,
                            fence: device.create_fence(&vk::FenceCreateInfo::default(), None)?,
                            in_flight: false,
                        });
                    }
                    Ok(())
                });

            if let Err(err) = allocated {
                for slot in &slots {
                    device.destroy_fence(slot.fence, None);
                }
                device.destroy_command_pool(command_pool, None);
                return Err(vulkan_error("can not allocate command buffers", err));
            }
            (command_pool, slots)
        };

        Ok((
            gpu,
            Self {
                scene,
                luma,
                chroma,
                effect,
                device,
                queue: context.queue,
                queue_lock: SharedQueueLock {
                    backend_lock: context.queue_lock,
                    encoder_device: encoder_device.clone(),
                    queue_family_index,
                    queue_index: context.queue_index,
                },
                command_pool,
                slots,
                next_slot: 0,
                frames: frames.clone(),
            },
        ))
    }

    /// Waits until the copies that are still running on the GPU are done.
    fn wait_for_copies(&mut self) {
        for slot in &mut self.slots {
            if std::mem::take(&mut slot.in_flight) {
                unsafe {
                    let _ = self.device.wait_for_fences(&[slot.fence], true, u64::MAX);
                }
            }
        }
    }

    /// Copies the converted planes into the images of `frame` and hands the images over to
    /// the next user of the frame (the encoder), the way libav's own Vulkan code does.
    // `data[0]` of a Vulkan frame is the `AVVkFrame` libav allocated.
    #[allow(clippy::cast_ptr_alignment)]
    unsafe fn copy_planes(&mut self, frame: &VideoFrame) -> FFramesRendererResult<()> {
        unsafe {
            let frames_context = self.frames.data::<AVHWFramesContext>();
            let frames_hwctx = &*(*frames_context).hwctx.cast::<AVVulkanFramesContext>();
            let vk_frame = (*frame.as_ptr()).data[0].cast::<AVVkFrame>();
            if vk_frame.is_null() {
                return Err(skia_error("the hardware frame has no Vulkan image"));
            }

            let slot_index = self.next_slot;
            self.next_slot = (self.next_slot + 1) % self.slots.len();
            let slot = &mut self.slots[slot_index];
            if std::mem::take(&mut slot.in_flight) {
                self.device
                    .wait_for_fences(&[slot.fence], true, u64::MAX)
                    .map_err(|err| vulkan_error("waiting for a frame copy failed", err))?;
            }
            self.device
                .reset_fences(&[slot.fence])
                .and_then(|()| {
                    self.device
                        .reset_command_buffer(slot.commands, vk::CommandBufferResetFlags::empty())
                })
                .map_err(|err| vulkan_error("can not reuse a command buffer", err))?;
            let (commands, fence) = (slot.commands, slot.fence);

            if let Some(lock) = frames_hwctx.lock_frame {
                lock(frames_context, vk_frame);
            }
            let result = self.submit_copy(vk_frame, commands, fence);
            if let Some(unlock) = frames_hwctx.unlock_frame {
                unlock(frames_context, vk_frame);
            }
            result?;

            self.slots[slot_index].in_flight = true;
            Ok(())
        }
    }

    /// Records and submits the copy. The frame has to be locked.
    unsafe fn submit_copy(
        &self,
        vk_frame: *mut AVVkFrame,
        commands: vk::CommandBuffer,
        fence: vk::Fence,
    ) -> FFramesRendererResult<()> {
        unsafe {
            let frame = &mut *vk_frame;
            // NV12 is one image with two planes, or an image per plane on devices
            // without multiplanar formats.
            let images = frame.img.iter().take_while(|image| **image != 0).count();
            if images == 0 || images > 2 {
                return Err(skia_error("unexpected image layout of the hardware frame"));
            }
            let image = |index: usize| vk::Image::from_raw(frame.img[index]);
            let (luma_target, chroma_target) = if images == 1 {
                (
                    (image(0), vk::ImageAspectFlags::PLANE_0),
                    (image(0), vk::ImageAspectFlags::PLANE_1),
                )
            } else {
                (
                    (image(0), vk::ImageAspectFlags::COLOR),
                    (image(1), vk::ImageAspectFlags::COLOR),
                )
            };

            let device = &self.device;
            let record = || -> Result<(), vk::Result> {
                device.begin_command_buffer(
                    commands,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )?;

                // Every byte of the images is overwritten, so their old content (and
                // layout) does not matter.
                let barriers: Vec<_> = (0..images)
                    .map(|index| {
                        vk::ImageMemoryBarrier2::default()
                            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            .src_access_mask(vk::AccessFlags2::NONE)
                            .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
                            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                            .old_layout(vk::ImageLayout::UNDEFINED)
                            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .image(image(index))
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: 0,
                                layer_count: 1,
                            })
                    })
                    .collect();
                device.cmd_pipeline_barrier2(
                    commands,
                    &vk::DependencyInfo::default().image_memory_barriers(&barriers),
                );

                for (plane, (target, aspect)) in
                    [(&self.luma, luma_target), (&self.chroma, chroma_target)]
                {
                    let layers = |aspect_mask| vk::ImageSubresourceLayers {
                        aspect_mask,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    };
                    device.cmd_copy_image(
                        commands,
                        plane.image,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        target,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[vk::ImageCopy {
                            src_subresource: layers(vk::ImageAspectFlags::COLOR),
                            src_offset: vk::Offset3D::default(),
                            dst_subresource: layers(aspect),
                            dst_offset: vk::Offset3D::default(),
                            extent: plane.extent,
                        }],
                    );
                }

                device.end_command_buffer(commands)
            };
            record().map_err(|err| vulkan_error("can not record the frame copy", err))?;

            // The timeline semaphore of an image orders its users: wait for the previous
            // one, signal the next value for the one after us.
            let semaphore = |index: usize, value: u64| {
                vk::SemaphoreSubmitInfo::default()
                    .semaphore(vk::Semaphore::from_raw(frame.sem[index]))
                    .value(value)
                    .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            };
            let waits: Vec<_> = (0..images)
                .map(|index| semaphore(index, frame.sem_value[index]))
                .collect();
            let signals: Vec<_> = (0..images)
                .map(|index| semaphore(index, frame.sem_value[index] + 1))
                .collect();
            let command_infos = [vk::CommandBufferSubmitInfo::default().command_buffer(commands)];
            let submit = vk::SubmitInfo2::default()
                .wait_semaphore_infos(&waits)
                .command_buffer_infos(&command_infos)
                .signal_semaphore_infos(&signals);

            self.queue_lock
                .locked(|| device.queue_submit2(self.queue, &[submit], fence))
                .map_err(|err| vulkan_error("can not submit the frame copy", err))?;

            for index in 0..images {
                frame.sem_value[index] += 1;
                frame.layout[index] = vk::ImageLayout::TRANSFER_DST_OPTIMAL.as_raw();
                frame.access[index] = vk::AccessFlags2::TRANSFER_WRITE.as_raw();
            }

            Ok(())
        }
    }
}

impl HardwareFrameTarget for VulkanFrameTarget {
    fn begin_frame(&mut self, _gpu: &mut DirectContext) -> FFramesRendererResult<&mut Surface> {
        Ok(&mut self.scene)
    }

    fn retire(self: Box<Self>, gpu: DirectContext) {
        self.retire_with(gpu);
    }

    fn finish_frame(&mut self, gpu: &mut DirectContext) -> FFramesRendererResult<VideoFrame> {
        let frame = self.scene.image_snapshot();
        self.luma.draw(&self.effect, &frame)?;
        self.chroma.draw(&self.effect, &frame)?;
        drop(frame);

        // Skia leaves the planes ready to be copied from. Flushing them flushes the frame
        // they sample too.
        let copy_source = gpu::vk::mutable_texture_states::new_vulkan(
            gpu::vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::QUEUE_FAMILY_IGNORED,
        );
        for plane in [&mut self.luma, &mut self.chroma] {
            gpu.flush_surface_with_texture_state(
                &mut plane.surface,
                &gpu::FlushInfo::default(),
                Some(&copy_source),
            );
        }
        if !self.queue_lock.locked(|| gpu.submit(gpu::SyncCpu::No)) {
            return Err(skia_error("can not submit the frame to the GPU"));
        }

        let frame = VideoFrame::from_hw_frames(&self.frames)
            .map_err(|err| FFramesRendererError::from_chunk(0, err))?;
        // Goes to the queue Skia submitted the draws to, so the copy runs after them.
        unsafe { self.copy_planes(&frame)? };

        Ok(frame)
    }
}

impl VulkanFrameTarget {
    /// See [`HardwareFrameTarget::retire`].
    fn retire_with(mut self: Box<Self>, gpu: DirectContext) {
        self.wait_for_copies();
        let queue_lock = self.queue_lock.clone();
        drop(self);
        // destroying the context waits for its queue
        queue_lock.locked(|| drop(gpu));
    }
}

impl Drop for VulkanFrameTarget {
    fn drop(&mut self) {
        self.wait_for_copies();
        unsafe {
            for slot in &self.slots {
                self.device.destroy_fence(slot.fence, None);
            }
            self.device.destroy_command_pool(self.command_pool, None);
        }
    }
}
