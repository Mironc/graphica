use std::thread::ThreadId;

use ash::vk::Queue as Q;
use dashmap::{DashMap, mapref::one::RefMut};

use super::{local_thread_pool::LocalCommandPool, queue_family::QueueFamily};
use crate::{device::DeviceContext, swapchain::FrameData};
/// This struct represents queue for commiting commands
///
/// This structure provides command pools for frame and local thread usage
#[derive(Debug)]
pub struct Queue {
    raw_queue: Q,
    queue_family: QueueFamily,
    command_pools: DashMap<(usize, ThreadId), LocalCommandPool>,
}
impl Queue {
    pub fn new(raw_queue: Q, queue_family: QueueFamily) -> Self {
        Self {
            raw_queue,
            queue_family,
            command_pools: DashMap::new(),
        }
    }
    ///Raw handle -> `VKQueue`
    pub fn handle(&self) -> Q {
        self.raw_queue
    }

    ///Gives you command pool that's made for your fif and thread
    pub fn get_commandpool(
        &self,
        logical_device: &DeviceContext,
        frame_data: &FrameData,
    ) -> RefMut<'_, (usize, ThreadId), LocalCommandPool> {
        let thread_id = std::thread::current().id();
        self.command_pools
            .entry((frame_data.fif_id(), thread_id))
            .or_insert_with(|| LocalCommandPool::new(logical_device, self.queue_family))
    }
    pub fn queue_family(&self) -> &QueueFamily {
        &self.queue_family
    }
    //TODO:pub fn submit(&self, : Synchronization) {}
}
