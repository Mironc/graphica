use std::collections::HashMap;

use ash::vk::{
    AccessFlags, BufferMemoryBarrier, CommandBuffer, DependencyFlags, ImageAspectFlags,
    ImageLayout, ImageMemoryBarrier, ImageSubresourceRange, PipelineStageFlags,
};

use crate::{
    device::DeviceContext,
    render_graph::{
        resource::ResourceId,
        resource_state::{ResourceAccess, ResourceState, ResourceUsage, StateTransition},
    },
    rendering::{
        buffer_container::GeneralBufferId, renderer_bundle::RendererBundle,
        texture_container::TextureId,
    },
};

#[derive(Debug, Clone, Default)]
pub struct SyncPoint {
    sync_ops: HashMap<(PipelineStageFlags, PipelineStageFlags), Vec<SyncOp>>,
}
impl SyncPoint {
    pub fn execute(
        &self,
        device: &DeviceContext,
        command_buffer: CommandBuffer,
        bundle: &RendererBundle,
    ) {
        for (stages, syncs) in self.sync_ops().iter() {
            let mut image_barriers = Vec::new();
            let mut buffer_barriers = Vec::new();
            for sync_op in syncs.iter() {
                match sync_op {
                    SyncOp::Texture(
                        texture_id,
                        image_layout,
                        image_layout1,
                        _pipeline_stage_flags,
                        _pipeline_stage_flags1,
                        access_flags,
                        access_flags1,
                        _,
                    ) => {
                        if let Some(texture) = bundle.texture_container.get_image(*texture_id) {
                            let image_aspect = if texture.texture_format().is_color() {
                                ImageAspectFlags::COLOR
                            } else if texture.texture_format().is_depth_stencil() {
                                ImageAspectFlags::DEPTH | ImageAspectFlags::STENCIL
                            } else {
                                ImageAspectFlags::DEPTH
                            };
                            let subresource_range = ImageSubresourceRange::default()
                                .level_count(1)
                                .layer_count(1)
                                .aspect_mask(image_aspect);
                            image_barriers.push(
                                ImageMemoryBarrier::default()
                                    .image(texture.handle())
                                    .old_layout(*image_layout)
                                    .new_layout(*image_layout1)
                                    .src_access_mask(*access_flags)
                                    .dst_access_mask(*access_flags1)
                                    .subresource_range(subresource_range),
                            );
                        }
                    }
                    SyncOp::Buffer(
                        general_buffer_id,
                        _pipeline_stage_flags,
                        _pipeline_stage_flags1,
                        access_flags,
                        access_flags1,
                        offset,
                        size,
                        _,
                    ) => {
                        if let Some(buffer) = bundle
                            .buffer_container
                            .get_general_buffer(*general_buffer_id)
                        {
                            buffer_barriers.push(
                                BufferMemoryBarrier::default()
                                    .buffer(buffer.handle())
                                    .offset(*offset)
                                    .size(*size)
                                    .src_access_mask(*access_flags)
                                    .dst_access_mask(*access_flags1),
                            );
                        }
                    }
                }
            }
            unsafe {
                device.cmd_pipeline_barrier(
                    command_buffer,
                    stages.0,
                    stages.1,
                    DependencyFlags::empty(),
                    &[],
                    &buffer_barriers,
                    &image_barriers,
                )
            };
        }
    }
    pub fn from_transitions(transitions: &[StateTransition]) -> Self {
        let mut sync_ops = HashMap::new();
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
            sync_ops.insert(stage, sync_data);
        }
        Self { sync_ops }
    }
    pub fn push_sync_op(&mut self, sync_op: SyncOp) {
        let entry = self
            .sync_ops
            .entry(sync_op.pipeline_stage_from_to())
            .or_default();
        entry.push(sync_op);
    }
    pub fn is_empty(&self) -> bool {
        self.sync_ops.is_empty()
    }

    pub fn sync_ops(&self) -> &HashMap<(PipelineStageFlags, PipelineStageFlags), Vec<SyncOp>> {
        &self.sync_ops
    }
    #[cfg(feature = "graph-visualize")]
    pub fn fmt_dot(&self, bundle: &RendererBundle) -> String {
        use std::fmt::Write;
        let mut sync_content = String::new();
        for sync_op in self.sync_ops.values() {
            for sync_op in sync_op.iter() {
                _ = write!(&mut sync_content, "| {{ {} }}", sync_op.fmt_dot(bundle));
            }
        }

        format!(
            "shape=record,label=\"{{ Sync point | {{ {} }} }}\"",
            sync_content.trim_start_matches("|")
        )
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
    #[cfg(feature = "graph-visualize")]
    pub fn fmt_dot(&self, bundle: &RendererBundle) -> String {
        use crate::rendering::label_container::LabelId;
        match self {
            SyncOp::Texture(
                texture_id,
                image_layout,
                image_layout1,
                pipeline_stage_flags,
                pipeline_stage_flags1,
                access_flags,
                access_flags1,
                _,
            ) => {
                let label = if let Some(label) = bundle
                    .label_container
                    .get_label(&LabelId::Texture(*texture_id))
                {
                    label
                } else {
                    &format!("{:?}", texture_id)
                };
                let image_layout = if image_layout != image_layout1 {
                    format!(
                        "{} =\\> {}",
                        image_layout.fmt_dot(),
                        image_layout1.fmt_dot()
                    )
                } else {
                    format!("")
                };
                let pipe_stage = if pipeline_stage_flags != pipeline_stage_flags1 {
                    format!(
                        "| {} =\\> {}",
                        pipeline_stage_flags.fmt_dot(),
                        pipeline_stage_flags1.fmt_dot()
                    )
                } else {
                    format!("")
                };
                let access = if access_flags != access_flags1 {
                    format!(
                        "| {} =\\> {}",
                        access_flags.fmt_dot(),
                        access_flags1.fmt_dot()
                    )
                } else {
                    format!("")
                };
                let content = format!("{} {} {}", image_layout, pipe_stage, access);
                let trimmed = content.trim_start_matches("|").trim_end();
                if !trimmed.is_empty() {
                    format!("{} | {}", label, trimmed)
                } else {
                    format!("{}", label)
                }
            }
            SyncOp::Buffer(
                general_buffer_id,
                pipeline_stage_flags,
                pipeline_stage_flags1,
                access_flags,
                access_flags1,
                _,
                _,
                _,
            ) => {
                let label = if let Some(label) = bundle
                    .label_container
                    .get_label(&LabelId::Buffer(*general_buffer_id))
                {
                    label
                } else {
                    &format!("{:?}", general_buffer_id.key_data())
                };
                let pipe_stage = if pipeline_stage_flags != pipeline_stage_flags1 {
                    format!(
                        "{} =\\> {}",
                        pipeline_stage_flags.fmt_dot(),
                        pipeline_stage_flags1.fmt_dot()
                    )
                } else {
                    format!("")
                };
                let access = if access_flags != access_flags1 {
                    format!(
                        "| {} =\\> {}",
                        access_flags.fmt_dot(),
                        access_flags1.fmt_dot()
                    )
                } else {
                    format!("")
                };
                let content = format!("{} {}", pipe_stage, access);
                let trimmed = content.trim_start_matches("|").trim_end();
                if !trimmed.is_empty() {
                    format!("{} | {}", label, trimmed)
                } else {
                    format!("{}", label)
                }
            }
        }
    }
}

#[cfg(feature = "graph-visualize")]
trait FmtDot {
    fn fmt_dot(&self) -> String;
}
#[cfg(feature = "graph-visualize")]
impl FmtDot for AccessFlags {
    fn fmt_dot(&self) -> String {
        if self == &AccessFlags::empty() {
            format!("ACCESS_NONE")
        } else {
            //                           So .dot wont put a new tab
            format!("ACCESS_{:?}", self).replace("|", "\\|")
        }
    }
}
#[cfg(feature = "graph-visualize")]
impl FmtDot for ImageLayout {
    fn fmt_dot(&self) -> String {
        format!("LAYOUT_{:?}", self)
    }
}
#[cfg(feature = "graph-visualize")]
impl FmtDot for PipelineStageFlags {
    fn fmt_dot(&self) -> String {
        //                          So .dot wont put a new tab
        format!("STAGE_{:?}", self).replace("|", "\\|")
    }
}
