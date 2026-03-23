use std::sync::Arc;

use ash::{
    Device,
    vk::{Fence, FenceCreateFlags, FenceCreateInfo, Semaphore, SemaphoreCreateInfo},
};

use crate::device::DeviceContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSync {
    frame_is_done: Fence,
    image_available: Semaphore,
}
impl FrameSync {
    pub fn new(device: &DeviceContext) -> Self {
        let create_semaphore = SemaphoreCreateInfo::default();
        let create_fence = FenceCreateInfo::default().flags(FenceCreateFlags::SIGNALED);
        unsafe {
            let image_available = device
                .create_semaphore(&create_semaphore, None)
                .expect("Couldn't create semaphore");
            let frame_is_done = device
                .create_fence(&create_fence, None)
                .expect("Couldn't create fence");
            Self {
                frame_is_done,
                image_available,
            }
        }
    }
    /// Blocking fn until frame is frame is done
    pub fn wait(&self, device: &DeviceContext) {
        unsafe {
            device
                .wait_for_fences(&[self.frame_is_done], true, u64::MAX)
                .expect("Error occured while waiting for next frame")
        };
    }
    ///Returns semaphore that is responsible for signaling when image is ready for present call
    pub fn image_available(&self) -> Semaphore {
        self.image_available
    }
    /// Clears fence in order to make it usable again
    pub fn clear(&self, device: &DeviceContext) {
        unsafe {
            device
                .reset_fences(&[self.frame_is_done])
                .expect("Couldn't reset frame done fence")
        }
    }
    pub fn destroy(&self, device: &DeviceContext) {
        unsafe {
            device
                .device_wait_idle()
                .expect("Something went wrong while waiting for gpu idle")
        };
        unsafe {
            device.destroy_fence(self.frame_is_done, None);
            device.destroy_semaphore(self.image_available, None);
        }
    }

    /// Handle to fence that tells cpu when gpu is done with frame
    pub fn frame_done(&self) -> Fence {
        self.frame_is_done
    }
}
/// This struct is responsible for signaling when image is ready to be draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageSync {
    render_finished: Semaphore,
}
impl ImageSync {
    pub fn new(device: &DeviceContext) -> Self {
        let create_semaphore = SemaphoreCreateInfo::default();
        unsafe {
            let render_finished = device
                .create_semaphore(&create_semaphore, None)
                .expect("Couldn't create semaphore");
            Self { render_finished }
        }
    }
    ///Returns semaphore that is responsible for signaling when image was presented
    pub fn render_finished(&self) -> Semaphore {
        self.render_finished
    }

    pub fn destroy(&self, device: &Arc<DeviceContext>) {
        unsafe {
            device
                .device_wait_idle()
                .expect("Something went wrong while waiting for gpu idle")
        };
        unsafe {
            device.destroy_semaphore(self.render_finished, None);
        }
    }
}
