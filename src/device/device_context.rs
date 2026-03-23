use std::{
    error::Error,
    ops::Deref,
    sync::{Mutex, MutexGuard},
};

use ash::{
    Device,
    ext::debug_utils,
    khr::swapchain,
    vk::{DeviceCreateInfo, DeviceQueueCreateInfo},
};
use gpu_allocator::{
    AllocatorDebugSettings,
    vulkan::{Allocator, AllocatorCreateDesc},
};

use crate::{
    context::GraphicsContext, device::graphics_queue::RenderQueue,
    device::physical_device::PDevice, queue::logical_queue::Queue,
};
pub struct DeviceContext {
    logical_device: Device,
    debug_fns: debug_utils::Device,
    pdevice: PDevice,
    render_queue: RenderQueue,
    alloc: Mutex<Allocator>,
}
impl DeviceContext {
    pub fn new(context: &GraphicsContext, pdevice: &PDevice) -> Result<Self, Box<dyn Error>> {
        let mut create_queues = Vec::new();
        let universal_queue = pdevice
            .universal_queue_family()
            .expect("TODO: Add support for non universal queues platforms");
        create_queues.push(
            DeviceQueueCreateInfo::default()
                .queue_family_index(universal_queue.id() as u32)
                .queue_priorities(&[1.0]),
        );
        let device_extensions_ptrs = [
            swapchain::NAME.as_ptr(),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            ash::khr::portability_subset::NAME.as_ptr(),
        ];
        let device_create_info = DeviceCreateInfo::default()
            .queue_create_infos(&create_queues)
            .enabled_extension_names(&device_extensions_ptrs);
        let device = unsafe {
            context
                .instance()
                .create_device(pdevice.handle(), &device_create_info, None)
        }?;
        let graphics_queue = unsafe { device.get_device_queue(universal_queue.id() as u32, 0) };
        let graphics_queue = Queue::new(graphics_queue, universal_queue);

        let present_queue = unsafe { device.get_device_queue(universal_queue.id() as u32, 0) };
        let present_queue = Queue::new(present_queue, universal_queue);

        let render_queue = RenderQueue::new(graphics_queue, present_queue);
        let alloc_createinfo = AllocatorCreateDesc {
            instance: context.instance().instance().clone(),
            device: device.clone(),
            physical_device: pdevice.handle(),
            debug_settings: AllocatorDebugSettings::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        };
        let alloc = Mutex::new(Allocator::new(&alloc_createinfo)?);
        let debug_fns = debug_utils::Device::new(&context.instance(), &device);
        Ok(Self {
            logical_device: device,
            pdevice: pdevice.clone(),
            debug_fns,
            render_queue,
            alloc,
        })
    }
    pub fn pdevice(&self) -> &PDevice {
        &self.pdevice
    }
    pub fn render_queue(&self) -> &RenderQueue {
        &self.render_queue
    }

    pub fn allocator(&self) -> MutexGuard<'_, Allocator> {
        if self.alloc.is_poisoned() {
            self.alloc.clear_poison();
        }
        self.alloc.lock().unwrap()
    }

    pub fn debug_fns(&self) -> &debug_utils::Device {
        &self.debug_fns
    }
}
impl Deref for DeviceContext {
    type Target = Device;

    fn deref(&self) -> &Self::Target {
        &self.logical_device
    }
}
impl Drop for DeviceContext {
    fn drop(&mut self) {
        unsafe { self.device_wait_idle().unwrap() }
    }
}
