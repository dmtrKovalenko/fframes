//! Native regression: forward real Vulkan calls while counting live allocations.
use super::*;
use crate::QueueLock;
use fframes::usvgr;
use std::ffi::CStr;
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, AtomicIsize, AtomicU64, AtomicUsize, Ordering},
};

static DEVICE: OnceLock<ash::DeviceFnV1_0> = OnceLock::new();
static GET_PROC: OnceLock<vk::PFN_vkGetDeviceProcAddr> = OnceLock::new();
static IMAGES: AtomicIsize = AtomicIsize::new(0);
static MEMORY: AtomicIsize = AtomicIsize::new(0);
static FAIL_MEMORY: AtomicBool = AtomicBool::new(false);
static FAILED_ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static QUEUES: OnceLock<Vec<vk::Queue>> = OnceLock::new();
static LOCKS: OnceLock<Vec<QueueLock>> = OnceLock::new();
static FAIL_FACTORY: AtomicBool = AtomicBool::new(false);
static FACTORY_UNLOCKED: AtomicBool = AtomicBool::new(false);
static FAIL_WAIT: AtomicBool = AtomicBool::new(false);
static COPY_PENDING: AtomicBool = AtomicBool::new(false);
static COPY_QUEUE: AtomicU64 = AtomicU64::new(0);
static FAILED_WAITS: AtomicUsize = AtomicUsize::new(0);
static EARLY_BUFFER_DESTROY: AtomicBool = AtomicBool::new(false);
static SUBMITS: AtomicUsize = AtomicUsize::new(0);
static WAITS: AtomicUsize = AtomicUsize::new(0);
static UNLOCKED_WAIT: AtomicBool = AtomicBool::new(false);

fn any_lock_held() -> bool {
    LOCKS
        .get()
        .unwrap()
        .iter()
        .any(|lock| lock.try_lock().is_err())
}

unsafe extern "system" fn queue_submit(
    queue: vk::Queue,
    count: u32,
    submits: *const vk::SubmitInfo<'_>,
    fence: vk::Fence,
) -> vk::Result {
    let result = unsafe { (DEVICE.get().unwrap().queue_submit)(queue, count, submits, fence) };
    if result == vk::Result::SUCCESS {
        SUBMITS.fetch_add(1, Ordering::SeqCst);
        if fence != vk::Fence::null() && FAIL_WAIT.load(Ordering::SeqCst) {
            COPY_QUEUE.store(queue.as_raw(), Ordering::SeqCst);
            COPY_PENDING.store(true, Ordering::SeqCst);
        }
    }
    result
}

unsafe extern "system" fn queue_wait_idle(queue: vk::Queue) -> vk::Result {
    let index = QUEUES
        .get()
        .unwrap()
        .iter()
        .position(|value| *value == queue)
        .unwrap();
    if LOCKS.get().unwrap()[index].try_lock().is_ok() {
        UNLOCKED_WAIT.store(true, Ordering::SeqCst);
    }
    let result = unsafe { (DEVICE.get().unwrap().queue_wait_idle)(queue) };
    if result == vk::Result::SUCCESS {
        WAITS.fetch_add(1, Ordering::SeqCst);
        if COPY_QUEUE.load(Ordering::SeqCst) == queue.as_raw() {
            COPY_PENDING.store(false, Ordering::SeqCst);
        }
    }
    result
}

unsafe extern "system" fn wait_for_fences(
    device: vk::Device,
    count: u32,
    fences: *const vk::Fence,
    all: vk::Bool32,
    timeout: u64,
) -> vk::Result {
    if COPY_PENDING.load(Ordering::SeqCst) && FAIL_WAIT.swap(false, Ordering::SeqCst) {
        FAILED_WAITS.fetch_add(1, Ordering::SeqCst);
        return vk::Result::TIMEOUT;
    }
    unsafe { (DEVICE.get().unwrap().wait_for_fences)(device, count, fences, all, timeout) }
}

unsafe extern "system" fn destroy_buffer(
    device: vk::Device,
    buffer: vk::Buffer,
    allocator: *const vk::AllocationCallbacks<'_>,
) {
    if COPY_PENDING.load(Ordering::SeqCst) {
        EARLY_BUFFER_DESTROY.store(true, Ordering::SeqCst);
        // Test-only safety net: record the bad order, then protect the real driver.
        let queue = vk::Queue::from_raw(COPY_QUEUE.load(Ordering::SeqCst));
        let result = unsafe { (DEVICE.get().unwrap().queue_wait_idle)(queue) };
        assert_eq!(result, vk::Result::SUCCESS);
        COPY_PENDING.store(false, Ordering::SeqCst);
    }
    unsafe { (DEVICE.get().unwrap().destroy_buffer)(device, buffer, allocator) };
}

unsafe extern "system" fn create_image(
    device: vk::Device,
    info: *const vk::ImageCreateInfo<'_>,
    allocator: *const vk::AllocationCallbacks<'_>,
    image: *mut vk::Image,
) -> vk::Result {
    let result = unsafe { (DEVICE.get().unwrap().create_image)(device, info, allocator, image) };
    if result == vk::Result::SUCCESS {
        IMAGES.fetch_add(1, Ordering::SeqCst);
    }
    result
}
unsafe extern "system" fn destroy_image(
    device: vk::Device,
    image: vk::Image,
    allocator: *const vk::AllocationCallbacks<'_>,
) {
    unsafe { (DEVICE.get().unwrap().destroy_image)(device, image, allocator) };
    if image != vk::Image::null() {
        IMAGES.fetch_sub(1, Ordering::SeqCst);
    }
}
unsafe extern "system" fn allocate_memory(
    device: vk::Device,
    info: *const vk::MemoryAllocateInfo<'_>,
    allocator: *const vk::AllocationCallbacks<'_>,
    memory: *mut vk::DeviceMemory,
) -> vk::Result {
    if FAIL_MEMORY.load(Ordering::SeqCst) {
        FAILED_ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        return vk::Result::ERROR_OUT_OF_DEVICE_MEMORY;
    }
    let result =
        unsafe { (DEVICE.get().unwrap().allocate_memory)(device, info, allocator, memory) };
    if result == vk::Result::SUCCESS {
        MEMORY.fetch_add(1, Ordering::SeqCst);
    }
    result
}
unsafe extern "system" fn free_memory(
    device: vk::Device,
    memory: vk::DeviceMemory,
    allocator: *const vk::AllocationCallbacks<'_>,
) {
    unsafe { (DEVICE.get().unwrap().free_memory)(device, memory, allocator) };
    if memory != vk::DeviceMemory::null() {
        MEMORY.fetch_sub(1, Ordering::SeqCst);
    }
}
unsafe extern "system" fn get_proc(
    device: vk::Device,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    // SAFETY: Vulkan passes valid null-terminated names; replacement signatures match exactly.
    unsafe {
        let replacement = match CStr::from_ptr(name).to_bytes() {
            b"vkQueueWaitIdle" if FAIL_FACTORY.load(Ordering::SeqCst) => {
                if !any_lock_held() {
                    FACTORY_UNLOCKED.store(true, Ordering::SeqCst);
                }
                return None;
            }
            b"vkQueueSubmit" => Some(queue_submit as *const ()),
            b"vkQueueWaitIdle" => Some(queue_wait_idle as *const ()),
            b"vkWaitForFences" => Some(wait_for_fences as *const ()),
            b"vkDestroyBuffer" => Some(destroy_buffer as *const ()),
            b"vkCreateImage" => Some(create_image as *const ()),
            b"vkDestroyImage" => Some(destroy_image as *const ()),
            b"vkAllocateMemory" => Some(allocate_memory as *const ()),
            b"vkFreeMemory" => Some(free_memory as *const ()),
            _ => None,
        };
        replacement.map_or_else(
            || (GET_PROC.get().unwrap())(device, name),
            |pointer| Some(std::mem::transmute::<*const (), unsafe extern "system" fn()>(pointer)),
        )
    }
}

struct InstanceOwner {
    instance: Instance,
    _entry: Entry,
}
impl Drop for InstanceOwner {
    fn drop(&mut self) {
        unsafe { self.instance.destroy_instance(None) };
    }
}

#[test]
#[ignore = "requires a Vulkan GPU; run in isolation"]
fn surfaces_release_images_and_memory_on_success_and_failure() {
    let backend = SkiaVulkanCtx::new(320, 180).unwrap();
    // Backend is moved below into an inner scope, so it drops before this guard.
    let owner = InstanceOwner {
        instance: backend.instance.clone(),
        _entry: backend.entry.clone(),
    };
    {
        let mut backend = backend;
        assert!(QUEUES.set(backend.queues.clone()).is_ok());
        assert!(LOCKS.set(backend.queue_locks.clone()).is_ok());
        assert!(DEVICE.set(backend.device.fp_v1_0().clone()).is_ok());
        assert!(
            GET_PROC
                .set(backend.instance.fp_v1_0().get_device_proc_addr)
                .is_ok()
        );
        // Trace both the backend's ash calls and Skia's proc-address based dispatch.
        unsafe {
            backend.instance = Instance::load_with(
                |name| {
                    if name == c"vkGetDeviceProcAddr" {
                        (get_proc as *const ()).cast::<c_void>()
                    } else {
                        backend
                            .entry
                            .get_instance_proc_addr(owner.instance.handle(), name.as_ptr())
                            .map_or(std::ptr::null(), |f| (f as *const ()).cast::<c_void>())
                    }
                },
                owner.instance.handle(),
            );
            backend.device = ash::Device::load_with(
                |name| {
                    get_proc(backend.device.handle(), name.as_ptr())
                        .map_or(std::ptr::null(), |f| (f as *const ()).cast::<c_void>())
                },
                backend.device.handle(),
            );
        }
        FAIL_FACTORY.store(true, Ordering::SeqCst);
        let failed_factory = backend.create_context();
        FAIL_FACTORY.store(false, Ordering::SeqCst);
        assert!(failed_factory.is_err(), "Skia factory must actually fail");
        let factory_unlocked = FACTORY_UNLOCKED.load(Ordering::SeqCst);

        let mut context = backend.create_skia_context().unwrap();
        context.surface.canvas().clear(skia_safe::Color::RED);
        {
            let _lock = super::super::lock_queue(context.queue_lock.as_ref());
            context.gpu.as_mut().unwrap().flush_and_submit();
        }
        let submissions = SUBMITS.load(Ordering::SeqCst);
        assert!(submissions > 0);
        let waits = WAITS.load(Ordering::SeqCst);
        let queue = context.queue_lock.clone();
        {
            let _lock = super::super::lock_queue(queue.as_ref());
            drop(context);
        }
        assert!(WAITS.load(Ordering::SeqCst) > waits);
        assert_eq!(
            (IMAGES.load(Ordering::SeqCst), MEMORY.load(Ordering::SeqCst)),
            (0, 0)
        );

        let input = fframes::EncoderInput::software(
            fframes::ffmpeg_sys_fframes::AVPixelFormat::AV_PIX_FMT_YUV420P,
        );
        let mut renderer = crate::SkiaEncoderFrameRenderer::new(
            &backend,
            crate::SkiaFrameExport::CpuConversion,
            &input,
            320,
            180,
        )
        .unwrap();
        let tree = usvgr::Tree::from_str(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180"/>"#,
            &usvgr::Options::default(),
            &usvgr::fontdb::Database::new(),
        )
        .unwrap();
        FAIL_WAIT.store(true, Ordering::SeqCst);
        let error = renderer
            .render(&tree, skia_safe::Color::RED, |size| vec![0; size])
            .err()
            .unwrap();
        assert!(
            error
                .to_string()
                .contains("waiting for the readback failed")
        );
        assert_eq!(FAILED_WAITS.load(Ordering::SeqCst), 1);
        assert!(COPY_PENDING.load(Ordering::SeqCst));
        drop(renderer);
        assert!(
            !EARLY_BUFFER_DESTROY.load(Ordering::SeqCst),
            "reader destroyed before GPU completion"
        );
        assert!(!factory_unlocked, "Skia factory failure outside queue lock");
        assert!(
            !UNLOCKED_WAIT.load(Ordering::SeqCst),
            "GPU completion outside queue lock"
        );
        assert_eq!(
            (IMAGES.load(Ordering::SeqCst), MEMORY.load(Ordering::SeqCst)),
            (0, 0),
            "failed readback retains allocations after renderer teardown"
        );
        for _ in 0..3 {
            let mut context = backend.create_skia_context().unwrap();
            context.surface.canvas().clear(skia_safe::Color::BLUE);
            let mut paint = skia_safe::Paint::default();
            paint.set_color(skia_safe::Color::from_argb(128, 0, 255, 0));
            context.surface.canvas().save();
            context.surface.canvas().clip_rect(
                skia_safe::Rect::new(0., 90., 320., 180.),
                None,
                false,
            );
            context
                .surface
                .canvas()
                .clear(skia_safe::Color::TRANSPARENT);
            context.surface.canvas().draw_paint(&paint);
            context.surface.canvas().restore();
            let mut pixels = vec![0; 320 * 180 * 4];
            context
                .reader
                .as_mut()
                .unwrap()
                .read(
                    context.gpu.as_mut().unwrap(),
                    &mut context.surface,
                    &mut pixels,
                )
                .unwrap();
            for (row, pixels) in pixels.chunks_exact(320 * 4).enumerate() {
                let expected = if row < 90 {
                    [0, 0, 255, 255]
                } else {
                    [0, 128, 0, 128]
                };
                assert!(pixels.chunks_exact(4).all(|p| p == expected), "row {row}");
            }
            assert!(IMAGES.load(Ordering::SeqCst) > 0);
            assert!(MEMORY.load(Ordering::SeqCst) > 0);
            let queue = context.queue_lock.clone();
            let _lock = super::super::lock_queue(queue.as_ref());
            drop(context);
            assert_eq!(
                (IMAGES.load(Ordering::SeqCst), MEMORY.load(Ordering::SeqCst)),
                (0, 0),
                "image/memory allocations outlive their Skia context"
            );
        }
        FAIL_MEMORY.store(true, Ordering::SeqCst);
        let failed = backend.create_skia_context();
        FAIL_MEMORY.store(false, Ordering::SeqCst);
        assert!(FAILED_ALLOCATIONS.load(Ordering::SeqCst) > 0);
        assert!(
            failed
                .err()
                .unwrap()
                .to_string()
                .contains("Failed to allocate Vulkan render target")
        );
        assert_eq!(
            IMAGES.load(Ordering::SeqCst),
            0,
            "failed construction leaks an image"
        );
        assert_eq!(
            MEMORY.load(Ordering::SeqCst),
            0,
            "failed construction leaks memory"
        );
    }
    drop(owner);
}
