use std::collections::HashMap;

use ash::vk::{AccessFlags, ImageLayout, PipelineStageFlags};

use crate::{
    render_graph::{
        resource::ResourceId,
        resource_state::{ResourceAccess, ResourceState, ResourceUsage, StateTransition},
    },
    rendering::{buffer_container::GeneralBufferId, texture_container::TextureId},
};

#[derive(Debug, Clone, Default)]
pub struct SyncPoint {
    sync_ops: Vec<((PipelineStageFlags, PipelineStageFlags), Vec<SyncOp>)>,
}
impl SyncPoint {
    pub fn from_transitions(transitions: &[StateTransition]) -> Self {
        let mut sync_ops = Vec::new();
        let mut grouped = HashMap::new();

        for transition in transitions.iter() {
            let entry: &mut Vec<&StateTransition> = grouped
                .entry((
                    transition
                        .resource_state_from()
                        .resource_usage()
                        .pipeline_stage(),
                    transition
                        .resource_state_to()
                        .resource_usage()
                        .pipeline_stage(),
                ))
                .or_default();
            entry.push(transition);
        }
        for (stage, group) in grouped.into_iter() {
            let mut sync_data = Vec::new();
            for transition in group {
                sync_data.push(SyncOp::from_transition(transition));
            }
            sync_ops.push((stage, sync_data));
        }
        Self { sync_ops }
    }
    pub fn push_sync_op(&mut self, sync_op: SyncOp) {
        if let Some(bucket) = self
            .sync_ops
            .iter_mut()
            .find(|(ps, _)| *ps == sync_op.pipeline_stage_from_to())
        {
            bucket.1.push(sync_op);
        } else {
            self.sync_ops
                .push(((sync_op.pipeline_stage_from_to()), vec![sync_op]));
        }
    }
    pub fn is_empty(&self) -> bool {
        self.sync_ops.is_empty()
    }

    pub fn sync_ops(&self) -> &[((PipelineStageFlags, PipelineStageFlags), Vec<SyncOp>)] {
        &self.sync_ops
    }
}
#[derive(Debug, Clone, Copy)]
pub enum SyncOp {
    Texture(
        TextureId,
        ImageLayout,
        ImageLayout,
        PipelineStageFlags,
        PipelineStageFlags,
        AccessFlags,
        AccessFlags,
        ResourceAccess,
    ),
    Buffer(
        GeneralBufferId,
        PipelineStageFlags,
        PipelineStageFlags,
        AccessFlags,
        AccessFlags,
        u64,
        u64,
        ResourceAccess,
    ),
}
impl SyncOp {
    pub fn pipeline_stage_from_to(&self) -> (PipelineStageFlags, PipelineStageFlags) {
        match self {
            SyncOp::Texture(_, _, _, pipeline_stage_flags, pipeline_stage_flags1, _, _, _) => {
                (*pipeline_stage_flags, *pipeline_stage_flags1)
            }
            SyncOp::Buffer(_, pipeline_stage_flags, pipeline_stage_flags1, _, _, _, _, _) => {
                (*pipeline_stage_flags, *pipeline_stage_flags1)
            }
        }
    }
    pub fn from_unitialized(new_state: ResourceState) -> Self {
        if let (
            ResourceId::Texture(texture_id),
            ResourceUsage::Texture(image_layout, pipeline_stage_flags, access_flags, access),
        ) = (new_state.resource_id(), new_state.resource_usage())
        {
            return SyncOp::Texture(
                texture_id,
                ImageLayout::UNDEFINED,
                image_layout,
                PipelineStageFlags::TOP_OF_PIPE,
                pipeline_stage_flags,
                AccessFlags::empty(),
                access_flags,
                access,
            );
        }
        if let (
            ResourceId::Texture(texture_id),
            ResourceUsage::TextureTranstional(
                image_layout,
                _,
                pipeline_stage_flags,
                access_flags,
                access,
            ),
        ) = (new_state.resource_id(), new_state.resource_usage())
        {
            return SyncOp::Texture(
                texture_id,
                ImageLayout::UNDEFINED,
                image_layout,
                PipelineStageFlags::TOP_OF_PIPE,
                pipeline_stage_flags,
                AccessFlags::empty(),
                access_flags,
                access,
            );
        }
        if let (
            ResourceId::Buffer(buffer_id),
            ResourceUsage::Buffer(pipeline_stage_flags, offset, size, access_flags, access),
        ) = (new_state.resource_id(), new_state.resource_usage())
        {
            return SyncOp::Buffer(
                buffer_id,
                PipelineStageFlags::TOP_OF_PIPE,
                pipeline_stage_flags,
                AccessFlags::empty(),
                access_flags,
                offset,
                size,
                access,
            );
        }
        panic!("resource usage and resource_id are not both the same thing?")
    }
    pub fn from_transition(transition: &StateTransition) -> Self {
        let from = transition.resource_state_from().resource_usage();
        let to = transition.resource_state_to().resource_usage();
        match transition.resource_state_from().resource_id() {
            ResourceId::Texture(texture_id) => match (from, to) {
                (
                    ResourceUsage::Texture(image_layout, pipeline_stage_flags, access_flags, _)
                    | ResourceUsage::TextureTranstional(
                        _,
                        image_layout,
                        pipeline_stage_flags,
                        access_flags,
                        _,
                    ),
                    ResourceUsage::Texture(
                        image_layout_1,
                        pipeline_stage_flags_1,
                        access_flags_1,
                        access,
                    )
                    | ResourceUsage::TextureTranstional(
                        image_layout_1,
                        _,
                        pipeline_stage_flags_1,
                        access_flags_1,
                        access,
                    ),
                ) => SyncOp::Texture(
                    texture_id,
                    image_layout,
                    image_layout_1,
                    pipeline_stage_flags,
                    pipeline_stage_flags_1,
                    access_flags,
                    access_flags_1,
                    access,
                ),
                _ => panic!(
                    "from and to is not for textures or unimplemented \nfrom:{:?} \nto:{:?}",
                    from, to
                ),
            },
            ResourceId::Buffer(general_buffer_id) => {
                if let (
                    ResourceUsage::Buffer(ps1, offset, size, ac1, _),
                    ResourceUsage::Buffer(ps2, _offset1, _size1, ac2, access),
                ) = (from, to)
                {
                    SyncOp::Buffer(general_buffer_id, ps1, ps2, ac1, ac2, offset, size, access)
                } else {
                    panic!(
                        "from and to is not for buffers or unimplemented \nfrom:{:?} \nto:{:?}",
                        from, to
                    )
                }
            }
        }
    }
    pub fn resource_id(&self) -> ResourceId {
        match self {
            SyncOp::Texture(texture_id, _, _, _, _, _, _, _) => ResourceId::Texture(*texture_id),
            SyncOp::Buffer(general_buffer_id, _, _, _, _, _, _, _) => {
                ResourceId::Buffer(*general_buffer_id)
            }
        }
    }
    pub fn resource_access(&self) -> ResourceAccess {
        *match self {
            SyncOp::Texture(_, _, _, _, _, _, _, access) => access,
            SyncOp::Buffer(_, _, _, _, _, _, _, access) => access,
        }
    }
    pub fn resource_state_after(&self) -> ResourceState {
        ResourceState::new(
            self.resource_id(),
            match self {
                SyncOp::Texture(
                    _,
                    _,
                    image_layout1,
                    _,
                    pipeline_stage_flags1,
                    _,
                    access_flags1,
                    resource_access,
                ) => ResourceUsage::Texture(
                    *image_layout1,
                    *pipeline_stage_flags1,
                    *access_flags1,
                    *resource_access,
                ),
                SyncOp::Buffer(
                    _,
                    _,
                    pipeline_stage_flags1,
                    _,
                    access_flags1,
                    offset,
                    size,
                    resource_access,
                ) => ResourceUsage::Buffer(
                    *pipeline_stage_flags1,
                    *offset,
                    *size,
                    *access_flags1,
                    *resource_access,
                ),
            },
        )
    }
}
