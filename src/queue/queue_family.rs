use ash::vk::{QueueFamilyProperties, QueueFlags};

///Gives info about queue that would be created with such queue family
///
///Readonly struct
#[derive(Debug, Clone, Copy)]
pub struct QueueFamily {
    id: usize,
    properties: QueueFamilyProperties,
    is_present: bool,
}
impl QueueFamily {
    pub(crate) fn new(id: usize, properties: QueueFamilyProperties, is_present: bool) -> Self {
        Self {
            id,
            properties,
            is_present,
        }
    }
    ///Gives queue family id
    pub fn id(&self) -> usize {
        self.id
    }
    ///Transfer flag
    pub fn is_transfer(&self) -> bool {
        self.properties.queue_flags.contains(QueueFlags::TRANSFER)
    }
    ///Compute flag
    pub fn is_compute(&self) -> bool {
        self.properties.queue_flags.contains(QueueFlags::COMPUTE)
    }
    ///Graphics flag
    pub fn is_graphics(&self) -> bool {
        self.properties.queue_flags.contains(QueueFlags::GRAPHICS)
    }
    ///Present flag
    pub fn is_present(&self) -> bool {
        self.is_present
    }
    ///Amount of queues that can be created with this family queue
    pub fn count(&self) -> u32 {
        self.properties.queue_count
    }
    ///Used for rating queue default
    ///
    ///It prefers unique queues, that support as much as possible capabilities, but main preference is present and graphics
    ///
    pub fn rate_unique(&self) -> i32 {
        let mut score = 0;
        //queue that can do both present and graphics is less headache with sync
        score += if self.is_graphics() { 1000 } else { -100 };
        score += if self.is_present() { 1000 } else { -100 };
        //it's cool if queue also can do compute and transfer
        score += if self.is_compute() && self.is_transfer() {
            2
        } else {
            0
        };
        score
    }
}
impl PartialEq for QueueFamily {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
