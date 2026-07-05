use ash::vk::{AccessFlags, ImageLayout, PipelineStageFlags};

use crate::{render_graph::resource::ResourceId, rendering::texture_container::TextureId};

/// Represents resource state in the node and how it is used
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceState {
    resource_id: ResourceId,
    resource_usage: ResourceUsage,
}

impl ResourceState {
    pub fn new(resource_id: ResourceId, resource_usage: ResourceUsage) -> Self {
        Self {
            resource_id,
            resource_usage,
        }
    }
    pub fn image_layout_differs(from: &Self, to: &Self) -> bool {
        if let (ResourceUsage::Texture(i, _, _, _), ResourceUsage::Texture(i1, _, _, _)) =
            (from.resource_usage, to.resource_usage)
        {
            return i != i1;
        }
        match (from.resource_usage, to.resource_usage) {
            (
                ResourceUsage::Texture(i1, _, _, _)
                | ResourceUsage::TextureTranstional(i1, _, _, _, _, _, _),
                ResourceUsage::Texture(i2, _, _, _)
                | ResourceUsage::TextureTranstional(i2, _, _, _, _, _, _),
            ) => i1 != i2,
            _ => false,
        }
    }
    pub fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    pub fn resource_usage(&self) -> ResourceUsage {
        self.resource_usage
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceUsage {
    Texture(ImageLayout, PipelineStageFlags, AccessFlags, ResourceAccess),
    /// If it accepts texture and then changes its layout eg VKRenderPass
    TextureTranstional(
        ImageLayout,
        ImageLayout,
        PipelineStageFlags,
        PipelineStageFlags,
        AccessFlags,
        AccessFlags,
        ResourceAccess,
    ),
    // TODO: Do a bit more sophisticated solution for offset and size dilemma or just make it sync whole buffer
    // but for now just playing along
    Buffer(PipelineStageFlags, u64, u64, AccessFlags, ResourceAccess),
}
impl ResourceUsage {
    pub fn resource_access(&self) -> ResourceAccess {
        *match self {
            ResourceUsage::Texture(_, _, _, resource_access)
            | ResourceUsage::TextureTranstional(_, _, _, _, _, _, resource_access)
            | ResourceUsage::Buffer(_, _, _, _, resource_access) => resource_access,
        }
    }
    pub fn pipeline_stage_from(&self) -> PipelineStageFlags {
        *match self {
            ResourceUsage::Texture(_, ps, _, _)
            | ResourceUsage::TextureTranstional(_, _, ps, _, _, _, _)
            | ResourceUsage::Buffer(ps, _, _, _, _) => ps,
        }
    }
    pub fn pipeline_stage_to(&self) -> PipelineStageFlags {
        *match self {
            ResourceUsage::Texture(_, ps, _, _)
            | ResourceUsage::TextureTranstional(_, _, _, ps, _, _, _)
            | ResourceUsage::Buffer(ps, _, _, _, _) => ps,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceAccess {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTransition {
    resource_state_from: ResourceState,
    resource_state_to: ResourceState,
}
impl StateTransition {
    pub fn new(resource_state_from: ResourceState, resource_state_to: ResourceState) -> Self {
        Self {
            resource_state_from,
            resource_state_to,
        }
    }
    pub fn sync_makes_sense(&self) -> bool {
        if let (
            ResourceUsage::Texture(_, _, _, _)
            | ResourceUsage::TextureTranstional(_, _, _, _, _, _, _),
            ResourceUsage::TextureTranstional(_, _, _, _, _, _, _),
        ) = (
            self.resource_state_from.resource_usage(),
            self.resource_state_to.resource_usage(),
        ) {
            return false;
        }
        self.resource_state_to.resource_id() == self.resource_state_from.resource_id()
            && (self
                .resource_state_from
                .resource_usage()
                .pipeline_stage_from()
                != self.resource_state_to.resource_usage().pipeline_stage_to()
                || (self.resource_state_from.resource_usage().resource_access()
                    != ResourceAccess::Read
                    || self.resource_state_to.resource_usage().resource_access()
                        != ResourceAccess::Read)
                || self.image_layout_changes())
    }
    pub fn image_layout_changes(&self) -> bool {
        ResourceState::image_layout_differs(&self.resource_state_from, &self.resource_state_to)
    }
    pub fn edge_makes_sense(from: &ResourceState, to: &ResourceState) -> bool {
        to.resource_id() == from.resource_id()
            && (from.resource_usage().pipeline_stage_from()
                != to.resource_usage().pipeline_stage_from()
                || (from.resource_usage().resource_access() != ResourceAccess::Read
                    || to.resource_usage().resource_access() != ResourceAccess::Read)
                || ResourceState::image_layout_differs(from, to))
    }
    pub fn resource_state_from(&self) -> ResourceState {
        self.resource_state_from
    }

    pub fn resource_state_to(&self) -> ResourceState {
        self.resource_state_to
    }
}
