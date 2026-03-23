use std::error::Error;

use crate::queue::logical_queue::Queue;

///Queue specialized for graphics and present calls
pub struct RenderQueue(Queue, Queue, bool);
impl RenderQueue {
    pub fn new(graphics_queue: Queue, present_queue: Queue) -> Self {
        let shared = if graphics_queue.queue_family() == present_queue.queue_family() {
            true
        } else {
            false
        };
        Self(graphics_queue, present_queue, shared)
    }
    pub fn present_queue(&self) -> &Queue {
        &self.0
    }
    pub fn graphics_queue(&self) -> &Queue {
        &self.1
    }
    pub fn shared(&self) -> bool {
        self.2
    }
}
