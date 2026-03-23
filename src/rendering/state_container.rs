use std::collections::HashMap;

use crate::render_graph::{render_graph::ResourceState, resource::ResourceId};

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
    pub fn get(&self, resource_id: ResourceId) -> Option<ResourceState> {
        self.states.get(&resource_id).and_then(|x| Some(*x))
    }
    pub fn remove(&mut self, resource_id: ResourceId) {
        self.states.remove(&resource_id);
    }
}
