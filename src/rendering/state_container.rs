use std::collections::HashMap;

use ash::vk::{AccessFlags, ImageLayout, PipelineStageFlags};

use crate::render_graph::{
    resource::ResourceId,
    resource_state::{ResourceAccess, ResourceState, ResourceUsage},
};

#[derive(Debug, Clone, Default)]
pub struct StateContainer {
    states: HashMap<ResourceId, ResourceState>,
}
impl StateContainer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert_or_set(&mut self, resource_id: ResourceId, state: ResourceState) {
        self.states.entry(resource_id).or_insert(state);
    }
    pub fn get_or_default(&self, resource_id: ResourceId) -> ResourceState {
        if let Some(state) = self.states.get(&resource_id).copied() {
            return state;
        }
        match &resource_id {
            ResourceId::Texture(_) => {
                let resource_usage = ResourceUsage::Texture(
                    ImageLayout::UNDEFINED,
                    PipelineStageFlags::TOP_OF_PIPE,
                    AccessFlags::empty(),
                    ResourceAccess::Read,
                );
                ResourceState::new(resource_id, resource_usage)
            }
            ResourceId::Buffer(buffer_id) => {
                let resource_usage = ResourceUsage::Buffer(
                    PipelineStageFlags::TOP_OF_PIPE,
                    0,
                    buffer_id.len() * buffer_id.item_size(),
                    AccessFlags::empty(),
                    ResourceAccess::Read,
                );
                ResourceState::new(resource_id, resource_usage)
            }
        }
    }
    pub fn get(&self, resource_id: ResourceId) -> Option<ResourceState> {
        self.states.get(&resource_id).copied()
    }
    pub fn remove(&mut self, resource_id: ResourceId) {
        self.states.remove(&resource_id);
    }
}
