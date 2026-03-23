use ash::vk::{
    CommandBuffer, CommandBufferAllocateInfo, CommandPool, CommandPoolCreateInfo,
    CommandPoolResetFlags,
};

use super::queue_family::QueueFamily;
use crate::device::DeviceContext;

/// Command pool that is bond to certain thread
///
///
#[derive(Debug, Clone)]
pub struct LocalCommandPool {
    command_pool: CommandPool,
    command_buffers: Vec<CommandBuffer>,
    used: usize,
}
impl LocalCommandPool {
    pub fn new(logical_device: &DeviceContext, queue_family: QueueFamily) -> Self {
        let commandpool_createinfo =
            CommandPoolCreateInfo::default().queue_family_index(queue_family.id() as u32);
        let command_pool =
            unsafe { logical_device.create_command_pool(&commandpool_createinfo, None) }
                .expect("Couldn't create command pool");
        Self {
            command_pool,
            command_buffers: Vec::new(),
            used: 0,
        }
    }
    ///Gives clean command buffer
    pub fn get_buffer(&mut self, logical_device: &DeviceContext) -> CommandBuffer {
        if self.used < self.command_buffers.len() {
            let buffer = self.command_buffers[self.used];
            self.used += 1;
            return buffer;
        }

        // 2. Если все буферы заняты, выделяем новый
        let allocate_info = ash::vk::CommandBufferAllocateInfo::default()
            .command_pool(self.command_pool)
            .level(ash::vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);

        let buffers = unsafe {
            logical_device
                .allocate_command_buffers(&allocate_info)
                .expect("Couldn't allocate command buffer")
        };

        let command_buffer = buffers[0]; // Забираем хэндл
        self.command_buffers.push(command_buffer);
        self.used += 1;

        command_buffer
    }
    ///Returns all buffers that was written
    pub fn get_written_buffers(&self) -> &[CommandBuffer] {
        &self.command_buffers[0..self.used]
    }
    ///Zeroes all buffers
    ///
    ///Intended to use only after all work for current frame is done
    pub fn reset(&mut self, logical_device: &DeviceContext) {
        let pool_reset = CommandPoolResetFlags::empty();
        unsafe {
            logical_device
                .reset_command_pool(self.command_pool, pool_reset)
                .expect("Couldn't free command pool")
        };
        self.used = 0;
    }
}
