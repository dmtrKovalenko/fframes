use std::ffi::CStr;
use std::mem::ManuallyDrop;
use std::rc::Rc;

use fframes_skia_renderer::skia_safe::gpu::ganesh::vk::backend_render_targets;
use fframes_skia_renderer::skia_safe::gpu::{
    self, DirectContext, FlushInfo, SurfaceOrigin, SyncCpu,
};
use fframes_skia_renderer::skia_safe::{Canvas, ColorType, Surface};
use fframes_skia_renderer::vulkan::ash::{
    self,
    vk::{self, Handle},
};
use fframes_skia_renderer::vulkan::{SkiaVulkanCtx, VulkanContext};
use fframes_skia_renderer::{QueueLock, lock_queue};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle};
use winit::window::Window;

use super::WindowTarget;
use crate::error::{PlayerError, PlayerResult};

/// What has to outlive the Vulkan surface besides the window: on macOS the `CAMetalLayer`
/// that `MoltenVK` presents to.
#[cfg(target_os = "macos")]
type SurfaceLayer = raw_window_metal::Layer;
#[cfg(not(target_os = "macos"))]
type SurfaceLayer = ();

/// Skia draws into the images of a swapchain of the window.
pub(super) struct VulkanTarget {
    swapchain: Option<Swapchain>,
    // Dropped in `drop`, after the swapchain images it draws into.
    gpu: ManuallyDrop<DirectContext>,
    queue: vk::Queue,
    queue_lock: QueueLock,
    surface: vk::SurfaceKHR,
    surface_fn: ash::khr::surface::Instance,
    swapchain_fn: ash::khr::swapchain::Device,
    /// Signaled once an acquired image can be drawn into.
    acquired: vk::Fence,
    present_mode: vk::PresentModeKHR,
    /// The window size in physical pixels.
    size: (u32, u32),
    _layer: SurfaceLayer,
    backend: SkiaVulkanCtx,
    _window: Rc<Window>,
}

struct Swapchain {
    handle: vk::SwapchainKHR,
    /// A Skia surface on every image of the swapchain.
    images: Vec<Surface>,
    /// The swapchain no longer matches the window and is recreated before the next frame.
    outdated: bool,
}

impl Swapchain {
    fn destroy(self, swapchain_fn: &ash::khr::swapchain::Device) {
        // Skia's views of the images go first.
        drop(self.images);
        unsafe { swapchain_fn.destroy_swapchain(self.handle, None) };
    }
}

/// Formats Skia draws into as they are shown: 8 bit, sRGB encoded.
const FORMATS: [(vk::Format, gpu::vk::Format, ColorType); 2] = [
    (
        vk::Format::B8G8R8A8_UNORM,
        gpu::vk::Format::B8G8R8A8_UNORM,
        ColorType::BGRA8888,
    ),
    (
        vk::Format::R8G8B8A8_UNORM,
        gpu::vk::Format::R8G8B8A8_UNORM,
        ColorType::RGBA8888,
    ),
];

impl VulkanTarget {
    pub(super) fn new(window: Rc<Window>, vsync: bool) -> PlayerResult<Self> {
        let extension = surface_extension(window.display_handle()?.as_raw())?;
        // The size is for the backend's own surfaces, which the player does not use.
        let backend = SkiaVulkanCtx::new_with_instance_extensions(16, 16, &[extension])?;
        let VulkanContext {
            gpu,
            queue_lock,
            queue,
            ..
        } = backend.create_context()?;
        let surface_fn = ash::khr::surface::Instance::new(backend.entry(), backend.instance());
        let swapchain_fn = ash::khr::swapchain::Device::new(backend.instance(), backend.device());
        let (surface, layer) = create_surface(&backend, &window)?;
        let size = window.inner_size();

        // From here on `drop` releases what was created.
        let mut target = Self {
            swapchain: None,
            gpu: ManuallyDrop::new(gpu),
            queue,
            queue_lock,
            surface,
            surface_fn,
            swapchain_fn,
            acquired: vk::Fence::null(),
            // vsync, the only mode every driver has
            present_mode: vk::PresentModeKHR::FIFO,
            size: (size.width, size.height),
            _layer: layer,
            backend,
            _window: window,
        };

        let can_present = unsafe {
            target.surface_fn.get_physical_device_surface_support(
                target.backend.physical_device(),
                target.backend.queue_family_index(),
                target.surface,
            )
        }
        .map_err(vulkan_error("can not query the window surface"))?;
        if !can_present {
            return Err(PlayerError::Surface(
                "the graphics queue of the GPU can not present to the window".to_owned(),
            ));
        }

        if !vsync {
            let modes = unsafe {
                target.surface_fn.get_physical_device_surface_present_modes(
                    target.backend.physical_device(),
                    target.surface,
                )
            }
            .map_err(vulkan_error("can not query the window surface"))?;
            // `MAILBOX` replaces the queued image instead of showing it right away.
            if let Some(mode) = [vk::PresentModeKHR::IMMEDIATE, vk::PresentModeKHR::MAILBOX]
                .into_iter()
                .find(|mode| modes.contains(mode))
            {
                target.present_mode = mode;
            }
        }

        target.acquired = unsafe {
            target
                .backend
                .device()
                .create_fence(&vk::FenceCreateInfo::default(), None)
        }
        .map_err(vulkan_error("can not create a fence"))?;

        // Fails here rather than on the first frame, so the CPU backend can take over.
        target.recreate_swapchain()?;
        Ok(target)
    }

    /// Replaces the swapchain with one for the current window size. Leaves none while the
    /// window has no area.
    fn recreate_swapchain(&mut self) -> PlayerResult<()> {
        let device = self.backend.device();
        let physical_device = self.backend.physical_device();
        let queue_family_index = self.backend.queue_family_index();

        let old = self.swapchain.take();
        if old.is_some() {
            // The images of the old swapchain may still be presented. Every context that
            // submits to the device shares the queue and the lock of the player's context.
            let _queue = lock_queue(Some(&self.queue_lock));
            unsafe { device.device_wait_idle() }
                .map_err(vulkan_error("can not wait for the GPU"))?;
        }

        let (capabilities, formats) = unsafe {
            (
                self.surface_fn
                    .get_physical_device_surface_capabilities(physical_device, self.surface),
                self.surface_fn
                    .get_physical_device_surface_formats(physical_device, self.surface),
            )
        };
        let capabilities =
            capabilities.map_err(vulkan_error("can not query the window surface"))?;
        let formats = formats.map_err(vulkan_error("can not query the window surface"))?;

        // `u32::MAX`: the surface takes the size of the swapchain (Wayland).
        let extent = if capabilities.current_extent.width == u32::MAX {
            vk::Extent2D {
                width: self.size.0.clamp(
                    capabilities.min_image_extent.width,
                    capabilities.max_image_extent.width,
                ),
                height: self.size.1.clamp(
                    capabilities.min_image_extent.height,
                    capabilities.max_image_extent.height,
                ),
            }
        } else {
            capabilities.current_extent
        };
        if extent.width == 0 || extent.height == 0 {
            if let Some(old) = old {
                old.destroy(&self.swapchain_fn);
            }
            return Ok(());
        }

        let Some((format, skia_format, color_type)) = FORMATS.into_iter().find(|(format, ..)| {
            formats.iter().any(|supported| {
                supported.format == *format
                    && supported.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
        }) else {
            return Err(PlayerError::Surface(format!(
                "the window supports none of the formats Skia draws into: {formats:?}"
            )));
        };

        // Skia copies to and from render targets it wraps.
        let required_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        if !capabilities.supported_usage_flags.contains(required_usage) {
            return Err(PlayerError::Surface(format!(
                "the window images can not be used as {required_usage:?}"
            )));
        }
        // Lets Skia read the destination for blend modes without copying it.
        let usage = required_usage
            | (capabilities.supported_usage_flags & vk::ImageUsageFlags::INPUT_ATTACHMENT);

        let mut image_count = capabilities.min_image_count + 1;
        if capabilities.max_image_count > 0 {
            image_count = image_count.min(capabilities.max_image_count);
        }
        let composite_alpha = [
            vk::CompositeAlphaFlagsKHR::OPAQUE,
            vk::CompositeAlphaFlagsKHR::INHERIT,
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
        ]
        .into_iter()
        .find(|mode| capabilities.supported_composite_alpha.contains(*mode))
        .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE);

        let create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(image_count)
            .image_format(format)
            .image_color_space(vk::ColorSpaceKHR::SRGB_NONLINEAR)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(usage)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(composite_alpha)
            .present_mode(self.present_mode)
            .clipped(true)
            .old_swapchain(
                old.as_ref()
                    .map_or(vk::SwapchainKHR::null(), |old| old.handle),
            );
        let handle = unsafe { self.swapchain_fn.create_swapchain(&create_info, None) };

        // Retired by the new swapchain, or of no use if it could not be created.
        if let Some(old) = old {
            old.destroy(&self.swapchain_fn);
        }
        let handle = handle.map_err(vulkan_error("can not create the swapchain"))?;

        let images = unsafe { self.swapchain_fn.get_swapchain_images(handle) }
            .map_err(vulkan_error("can not get the swapchain images"));
        let images = images.and_then(|images| {
            images
                .into_iter()
                .map(|image| {
                    // SAFETY: the image lives as long as the swapchain, which outlives the
                    // surface (`Swapchain::destroy`).
                    let mut image_info = unsafe {
                        gpu::vk::ImageInfo::new(
                            image.as_raw() as _,
                            gpu::vk::Alloc::default(),
                            gpu::vk::ImageTiling::OPTIMAL,
                            gpu::vk::ImageLayout::UNDEFINED,
                            skia_format,
                            1,
                            queue_family_index,
                            None,
                            None,
                            None,
                        )
                    };
                    image_info.image_usage_flags = usage.as_raw();

                    let render_target = backend_render_targets::make_vk(
                        (extent.width as i32, extent.height as i32),
                        &image_info,
                    );
                    gpu::surfaces::wrap_backend_render_target(
                        &mut self.gpu,
                        &render_target,
                        SurfaceOrigin::TopLeft,
                        color_type,
                        None,
                        None,
                    )
                    .ok_or_else(|| {
                        PlayerError::Skia("can not draw into the swapchain images".to_owned())
                    })
                })
                .collect::<PlayerResult<Vec<_>>>()
        });

        match images {
            Ok(images) => {
                self.swapchain = Some(Swapchain {
                    handle,
                    images,
                    outdated: false,
                });
                Ok(())
            }
            Err(err) => {
                unsafe { self.swapchain_fn.destroy_swapchain(handle, None) };
                Err(err)
            }
        }
    }

    /// The index of the next swapchain image, once it can be drawn into. `None` while the
    /// window has no area.
    fn acquire(&mut self) -> PlayerResult<Option<u32>> {
        // A swapchain that turns out to be out of date is recreated once.
        for _ in 0..2 {
            let Some(swapchain) = self.swapchain.as_mut() else {
                return Ok(None);
            };

            let acquired = unsafe {
                self.swapchain_fn.acquire_next_image(
                    swapchain.handle,
                    u64::MAX,
                    vk::Semaphore::null(),
                    self.acquired,
                )
            };

            match acquired {
                Ok((index, suboptimal)) => {
                    swapchain.outdated |= suboptimal;

                    let device = self.backend.device();
                    unsafe {
                        device
                            .wait_for_fences(&[self.acquired], true, u64::MAX)
                            .and_then(|()| device.reset_fences(&[self.acquired]))
                    }
                    .map_err(vulkan_error("can not wait for the swapchain image"))?;

                    return Ok(Some(index));
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.recreate_swapchain()?,
                Err(err) => return Err(vulkan_error("can not acquire a swapchain image")(err)),
            }
        }

        Ok(None)
    }
}

impl WindowTarget for VulkanTarget {
    fn is_gpu(&self) -> bool {
        true
    }

    fn resize(&mut self, width: u32, height: u32) -> PlayerResult<()> {
        self.size = (width, height);
        if let Some(swapchain) = self.swapchain.as_mut() {
            swapchain.outdated = true;
        }
        Ok(())
    }

    fn present(&mut self, draw: &mut dyn FnMut(&Canvas)) -> PlayerResult<()> {
        if self
            .swapchain
            .as_ref()
            .is_none_or(|swapchain| swapchain.outdated)
        {
            self.recreate_swapchain()?;
        }

        let Some(index) = self.acquire()? else {
            return Ok(());
        };
        let Some(swapchain) = self.swapchain.as_mut() else {
            return Ok(());
        };
        let surface = &mut swapchain.images[index as usize];

        draw(surface.canvas());

        let _queue = lock_queue(Some(&self.queue_lock));
        // Ask Skia for the layout the presentation engine reads. `BackendSurfaceAccess::Present`
        // changes the layout only when Skia's context lists `VK_KHR_swapchain`, and the
        // contexts of `SkiaVulkanCtx` do not.
        let present_layout = gpu::vk::mutable_texture_states::new_vulkan(
            gpu::vk::ImageLayout::PRESENT_SRC_KHR,
            self.backend.queue_family_index(),
        );
        self.gpu.flush_surface_with_texture_state(
            surface,
            &FlushInfo::default(),
            Some(&present_layout),
        );
        self.gpu.submit(SyncCpu::Yes);

        let presented = unsafe {
            self.swapchain_fn.queue_present(
                self.queue,
                &vk::PresentInfoKHR::default()
                    .swapchains(&[swapchain.handle])
                    .image_indices(&[index]),
            )
        };

        match presented {
            Ok(suboptimal) => swapchain.outdated |= suboptimal,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => swapchain.outdated = true,
            Err(err) => return Err(vulkan_error("can not present")(err)),
        }

        Ok(())
    }
}

impl Drop for VulkanTarget {
    fn drop(&mut self) {
        let device = self.backend.device();
        // Waiting for the GPU and dropping the context reach the queue.
        let _queue = lock_queue(Some(&self.queue_lock));
        unsafe {
            let _ = device.device_wait_idle();
        }

        if let Some(swapchain) = self.swapchain.take() {
            swapchain.destroy(&self.swapchain_fn);
        }

        unsafe {
            // SAFETY: not used after this
            ManuallyDrop::drop(&mut self.gpu);
            device.destroy_fence(self.acquired, None);
            self.surface_fn.destroy_surface(self.surface, None);
        }
    }
}

/// The instance extension that creates surfaces for windows of `display`.
fn surface_extension(display: RawDisplayHandle) -> PlayerResult<&'static CStr> {
    match display {
        RawDisplayHandle::AppKit(_) => Ok(ash::ext::metal_surface::NAME),
        RawDisplayHandle::Xlib(_) => Ok(ash::khr::xlib_surface::NAME),
        RawDisplayHandle::Xcb(_) => Ok(ash::khr::xcb_surface::NAME),
        RawDisplayHandle::Wayland(_) => Ok(ash::khr::wayland_surface::NAME),
        RawDisplayHandle::Windows(_) => Ok(ash::khr::win32_surface::NAME),
        other => Err(PlayerError::Window(format!(
            "vulkan can not present to {other:?}"
        ))),
    }
}

#[cfg(target_os = "macos")]
fn create_surface(
    backend: &SkiaVulkanCtx,
    window: &Window,
) -> PlayerResult<(vk::SurfaceKHR, SurfaceLayer)> {
    use winit::raw_window_handle::RawWindowHandle;

    let RawWindowHandle::AppKit(handle) = window.window_handle()?.as_raw() else {
        return Err(PlayerError::Window(
            "vulkan can only present to an AppKit window on macOS".to_owned(),
        ));
    };

    // SAFETY: the view of a live window, `Presenter::new` runs on the main thread.
    let layer = unsafe { raw_window_metal::Layer::from_ns_view(handle.ns_view) };
    let create_info = vk::MetalSurfaceCreateInfoEXT::default().layer(layer.as_ptr().as_ptr());
    let surface = unsafe {
        ash::ext::metal_surface::Instance::new(backend.entry(), backend.instance())
            .create_metal_surface(&create_info, None)
    }
    .map_err(vulkan_error("can not create the window surface"))?;

    Ok((surface, layer))
}

#[cfg(not(target_os = "macos"))]
fn create_surface(
    backend: &SkiaVulkanCtx,
    window: &Window,
) -> PlayerResult<(vk::SurfaceKHR, SurfaceLayer)> {
    use winit::raw_window_handle::RawWindowHandle;

    let (entry, instance) = (backend.entry(), backend.instance());
    let display = window.display_handle()?.as_raw();
    let handle = window.window_handle()?.as_raw();
    let unsupported = || PlayerError::Window(format!("vulkan can not present to {handle:?}"));

    // SAFETY: the handles are of a live window, which outlives the surface.
    let surface = unsafe {
        match (display, handle) {
            (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(window)) => {
                let create_info = vk::XlibSurfaceCreateInfoKHR::default()
                    .dpy(display.display.ok_or_else(unsupported)?.as_ptr())
                    .window(window.window);
                ash::khr::xlib_surface::Instance::new(entry, instance)
                    .create_xlib_surface(&create_info, None)
            }
            (RawDisplayHandle::Xcb(display), RawWindowHandle::Xcb(window)) => {
                let create_info = vk::XcbSurfaceCreateInfoKHR::default()
                    .connection(display.connection.ok_or_else(unsupported)?.as_ptr())
                    .window(window.window.get());
                ash::khr::xcb_surface::Instance::new(entry, instance)
                    .create_xcb_surface(&create_info, None)
            }
            (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(window)) => {
                let create_info = vk::WaylandSurfaceCreateInfoKHR::default()
                    .display(display.display.as_ptr())
                    .surface(window.surface.as_ptr());
                ash::khr::wayland_surface::Instance::new(entry, instance)
                    .create_wayland_surface(&create_info, None)
            }
            (RawDisplayHandle::Windows(_), RawWindowHandle::Win32(window)) => {
                let create_info = vk::Win32SurfaceCreateInfoKHR::default()
                    .hinstance(window.hinstance.map_or(0, std::num::NonZeroIsize::get))
                    .hwnd(window.hwnd.get());
                ash::khr::win32_surface::Instance::new(entry, instance)
                    .create_win32_surface(&create_info, None)
            }
            _ => return Err(unsupported()),
        }
    }
    .map_err(vulkan_error("can not create the window surface"))?;

    Ok((surface, ()))
}

fn vulkan_error(what: &str) -> impl FnOnce(vk::Result) -> PlayerError + '_ {
    move |err| PlayerError::Surface(format!("{what}: {err}"))
}
