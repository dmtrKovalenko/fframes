//! Opens the Vulkan device of a [`super::vulkan::SkiaVulkanCtx`] as a wgpu device too
//! (feature `wgpu`). Skia then samples textures rendered with wgpu in place.
//!
//! wgpu needs the device created with the extensions and the features it relies on. The
//! [`Interop`] finds them before the device exists and adds them to the device creation.

use std::ffi::CStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ash::vk;
use wgpu::hal;
use wgpu::hal::api::Vulkan;

/// What wgpu needs from a device, found before the device exists.
pub(super) struct Interop {
    instance: hal::vulkan::Instance,
    adapter: hal::ExposedAdapter<Vulkan>,
    extensions: Vec<&'static CStr>,
    features: hal::vulkan::PhysicalDeviceFeatures,
}

impl Interop {
    /// # Safety
    ///
    /// `instance` must come from `entry` with `api_version` and `instance_extensions`. It
    /// must outlive everything made from the result.
    pub(super) unsafe fn new(
        entry: &ash::Entry,
        instance: &ash::Instance,
        api_version: u32,
        instance_extensions: &[&'static CStr],
        physical_device: vk::PhysicalDevice,
    ) -> Result<Self, String> {
        let instance = unsafe {
            hal::vulkan::Instance::from_raw(
                entry.clone(),
                instance.clone(),
                api_version,
                0,
                None,
                instance_extensions.to_vec(),
                wgpu::InstanceFlags::empty(),
                wgpu::MemoryBudgetThresholds::default(),
                false,
                // the instance belongs to the Skia context
                Some(Box::new(|| ())),
            )
        }
        .map_err(|e| e.to_string())?;
        let adapter = instance
            .expose_adapter(physical_device)
            .ok_or_else(|| "wgpu does not support this GPU".to_owned())?;
        let extensions = adapter
            .adapter
            .required_device_extensions(wgpu::Features::empty());
        let features = adapter
            .adapter
            .physical_device_features(&extensions, wgpu::Features::empty());
        Ok(Self {
            instance,
            adapter,
            extensions,
            features,
        })
    }

    /// The device extensions wgpu relies on.
    pub(super) fn extensions(&self) -> &[&'static CStr] {
        &self.extensions
    }

    /// Adds the features wgpu relies on to the device creation.
    pub(super) fn add_to_device_create<'a>(
        &'a mut self,
        info: vk::DeviceCreateInfo<'a>,
    ) -> vk::DeviceCreateInfo<'a> {
        self.features.add_to_device_create(info)
    }

    /// The wgpu device on queue `queue_index` of `family_index` of `device`.
    ///
    /// On success the wgpu device owns `device`: the last wgpu object that refers to it
    /// destroys it, so no wgpu object outlives it. On failure the caller keeps it.
    ///
    /// # Safety
    ///
    /// `device` must come from the physical device of [`Self::new`], created with
    /// [`Self::add_to_device_create`] and `extensions` (a superset of [`Self::extensions`]).
    /// The caller must not destroy it after a success.
    pub(super) unsafe fn open(
        self,
        device: &ash::Device,
        extensions: &[&'static CStr],
        family_index: u32,
        queue_index: u32,
    ) -> Result<(wgpu::Device, wgpu::Queue), String> {
        // the device becomes wgpu's only when the wgpu device exists, see below
        let owned = Arc::new(AtomicBool::new(false));
        let destroy = {
            let (owned, device) = (Arc::clone(&owned), device.clone());
            move || {
                if owned.load(Ordering::Acquire) {
                    unsafe { device.destroy_device(None) };
                }
            }
        };
        let open = unsafe {
            self.adapter.adapter.device_from_raw(
                device.clone(),
                Some(Box::new(destroy)),
                extensions,
                wgpu::Features::empty(),
                &wgpu::Limits::default(),
                &wgpu::MemoryHints::Performance,
                family_index,
                queue_index,
            )
        }
        .map_err(|e| e.to_string())?;
        let instance = unsafe { wgpu::Instance::from_hal::<Vulkan>(self.instance) };
        let adapter = unsafe { instance.create_adapter_from_hal(self.adapter) };
        let shared = unsafe {
            adapter.create_device_from_hal(
                open,
                &wgpu::DeviceDescriptor {
                    label: Some("fframes shared"),
                    ..Default::default()
                },
            )
        }
        .map_err(|e| e.to_string())?;
        owned.store(true, Ordering::Release);
        Ok(shared)
    }
}
