use std::collections::{HashMap, HashSet};

use ash::vk::{
    AccessFlags, BufferMemoryBarrier, CommandBuffer, DependencyFlags, ImageAspectFlags,
    ImageLayout, ImageMemoryBarrier, ImageSubresourceRange, PipelineStageFlags,
};

use crate::{
    device::DeviceContext,
    render_graph::{
        dag::DirectedAcyclicGraph, operations::gpu_operation::Operation, resource::ResourceId,
    },
    rendering::{
        buffer_container::GeneralBufferId, renderer_bundle::RendererBundle,
        texture_container::TextureId,
    },
};

/// This struct purpose is to optimize rendering with batching, optimal synchronization usage, parallelization and culling
pub struct RenderGraph {
    dag: DirectedAcyclicGraph<Operation, Vec<ResourceTransition>>,
    target_node: usize,
}
impl RenderGraph {
    pub fn new() -> Self {
        Self {
            dag: DirectedAcyclicGraph::new(),
            target_node: 0,
        }
    }
    /// Inserts operation into render graph
    ///
    /// If operation is not infuencing any target operations it would be culled at compilation
    pub fn add_operation(&mut self, op: Operation) {
        self.dag.add_node(op);
    }

    /// Inserts operation into render graph marked as targeted, `target` operations are the ones that define output of app
    pub fn add_target_op(&mut self, target: Operation) {
        self.dag.add_node(target);
        self.target_node = self.dag.node_count() - 1;
    }

    /// This function is the main feature of render graph
    ///
    /// It finishes graph, culls unused branches, orders operations and includes synchronization points
    pub fn compile(&mut self, bundle: &mut RendererBundle) -> Option<Vec<Action>> {
        let nodes = self.dag.nodes().clone();

        let mut stack = vec![self.target_node];
        let mut visited = std::collections::HashSet::new();
        visited.insert(self.target_node);

        //Fills in graph: creates edges in graph based on whetever operation or any of it children makes influence onto the target's operation read_resources()
        while let Some(current_id) = stack.pop() {
            let current_node = nodes.get(current_id).unwrap();
            for (node_id, node) in nodes.iter().enumerate() {
                if node_id == current_id || self.dag.has_edge(node_id, current_id) {
                    continue;
                }
                if node_id > current_id {
                    break;
                }
                let initial_resource_state = node.resource_state(bundle);
                let final_resource_state = current_node.resource_state(bundle);
                if let (Some(initial_resource_state), Some(final_resource_state)) =
                    (initial_resource_state, final_resource_state)
                {
                    let result = initial_resource_state
                        .into_iter()
                        .filter_map(|x| {
                            final_resource_state
                                .iter()
                                .find(|y| {
                                    y.resource_id == x.resource_id
                                        && (x.resource_usage.pipeline_stage()
                                            != y.resource_usage.pipeline_stage()
                                            || (x.resource_usage.resource_access()
                                                != ResourceAccess::Read
                                                || y.resource_usage.resource_access()
                                                    != ResourceAccess::Read))
                                })
                                .map(|y| {
                                    let trans = ResourceTransition::new(x, *y);
                                    trans
                                })
                        })
                        .collect::<Vec<ResourceTransition>>();
                    if !result.is_empty() {
                        if visited.insert(node_id) {
                            stack.push(node_id);
                        }
                        self.dag.add_edge_cyclic(node_id, current_id, result);
                    }
                }
            }
        }
        //Removes nodes that are not influencing on target nodes and makes topological sort (linearizing operation order)
        if let Some(compiled) = self.dag.compile(self.target_node) {
            let mut written = HashSet::new();
            let actions = compiled
                .iter()
                .cloned()
                .flat_map(|x| {
                    let mut actions = Vec::new();
                    let node = self.dag.get_node(x).cloned().unwrap();
                    let pre_sync = node.resource_state(bundle).as_ref().unwrap().iter().fold(
                        SyncPoint::default(),
                        |mut acc, &x| {
                            if written.insert(x.resource_id) {
                                if let Some(resource_state) =
                                    bundle.resource_state.get(x.resource_id)
                                {
                                    let transition = ResourceTransition::new(resource_state, x);
                                    let sync_op = SyncOp::from_transition(&transition);

                                    acc.push_sync_op(sync_op);
                                } else {
                                    acc.push_sync_op(SyncOp::from_unitialized(x));
                                }
                            }
                            acc
                        },
                    );
                    if !pre_sync.is_empty() {
                        actions.push(Action::Sync(pre_sync));
                    }
                    actions.push(Action::Op(node));
                    for transition in self.dag.edges()[x].iter() {
                        actions.push(Action::Sync(SyncPoint::from_transitions(&transition.1)));
                    }
                    actions
                })
                .collect::<Vec<Action>>();
            return Some(actions);
        }
        log::error!("render_graph contains loops");
        None
    }
    pub fn dag(&self) -> &DirectedAcyclicGraph<Operation, Vec<ResourceTransition>> {
        &self.dag
    }
    ///This operation is intented to use after all work is done
    pub fn clear(&mut self) {
        self.dag.clear();
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Op(Operation),
    Sync(SyncPoint),
}

impl Action {
    pub fn execute(
        &self,
        bundle: &mut RendererBundle,
        command_buffer: CommandBuffer,
        device: &DeviceContext,
    ) {
        match self {
            Action::Op(operation) => {
                operation.execute(device, command_buffer, bundle);
            }
            Action::Sync(sync_point) => {
                for (stages, syncs) in sync_point.sync_ops.iter() {
                    let mut image_barriers = Vec::new();
                    let mut buffer_barriers = Vec::new();
                    for sync_op in syncs.iter() {
                        match sync_op {
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
                                if let Some(texture) =
                                    bundle.texture_container.get_image(*texture_id)
                                {
                                    let image_aspect = if texture.texture_format().is_color() {
                                        ImageAspectFlags::COLOR
                                    } else {
                                        if texture.texture_format().is_depth_stencil() {
                                            ImageAspectFlags::DEPTH | ImageAspectFlags::STENCIL
                                        } else {
                                            ImageAspectFlags::DEPTH
                                        }
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
                                pipeline_stage_flags,
                                pipeline_stage_flags1,
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
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SyncPoint {
    sync_ops: Vec<((PipelineStageFlags, PipelineStageFlags), Vec<SyncOp>)>,
}
impl SyncPoint {
    pub fn from_transitions(transitions: &Vec<ResourceTransition>) -> Self {
        let mut sync_ops = Vec::new();
        let mut grouped = HashMap::new();

        for transition in transitions.into_iter() {
            let entry: &mut Vec<&ResourceTransition> = grouped
                .entry((
                    transition
                        .resource_state_from
                        .resource_usage
                        .pipeline_stage(),
                    transition.resource_state_to.resource_usage.pipeline_stage(),
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
        ) = (new_state.resource_id, new_state.resource_usage)
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
        ) = (new_state.resource_id, new_state.resource_usage)
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
        ) = (new_state.resource_id, new_state.resource_usage)
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
    pub fn from_transition(transition: &ResourceTransition) -> Self {
        let from = transition.resource_state_from.resource_usage;
        let to = transition.resource_state_to.resource_usage;
        match transition.resource_state_from.resource_id {
            ResourceId::Texture(texture_id) => match (from, to) {
                (
                    ResourceUsage::Texture(image_layout, pipeline_stage_flags, access_flags, _),
                    ResourceUsage::Texture(
                        image_layout_1,
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
                (
                    ResourceUsage::Texture(image_layout, pipeline_stage_flags, access_flags, _),
                    ResourceUsage::TextureTranstional(
                        image_layout_10,
                        _,
                        pipeline_stage_flags_1,
                        access_flags_1,
                        access,
                    ),
                ) => SyncOp::Texture(
                    texture_id,
                    image_layout,
                    image_layout_10,
                    pipeline_stage_flags,
                    pipeline_stage_flags_1,
                    access_flags,
                    access_flags_1,
                    access,
                ),
                (
                    ResourceUsage::TextureTranstional(
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
                (
                    ResourceUsage::TextureTranstional(
                        _,
                        image_layout_1,
                        pipeline_stage_flags,
                        access_flags,
                        _,
                    ),
                    ResourceUsage::TextureTranstional(
                        image_layout_10,
                        _,
                        pipeline_stage_flags_1,
                        access_flags_1,
                        access,
                    ),
                ) => SyncOp::Texture(
                    texture_id,
                    image_layout_1,
                    image_layout_10,
                    pipeline_stage_flags,
                    pipeline_stage_flags_1,
                    access_flags,
                    access_flags_1,
                    access,
                ),
                _ => panic!(
                    "from and to is not about textures or unimplemented \nfrom:{:?} \nto:{:?}",
                    from, to
                ),
            },
            ResourceId::Buffer(general_buffer_id) => {
                if let (
                    ResourceUsage::Buffer(ps1, offset, size, ac1, _),
                    ResourceUsage::Buffer(ps2, offset1, size1, ac2, access),
                ) = (from, to)
                {
                    SyncOp::Buffer(general_buffer_id, ps1, ps2, ac1, ac2, offset, size, access)
                } else {
                    panic!("The hell you doing why they are not buffers?")
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
        ResourceState {
            resource_id: self.resource_id(),
            resource_usage: match self {
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
        }
    }
}
#[derive(Debug, Clone)]
pub struct ResourceTransition {
    resource_state_from: ResourceState,
    resource_state_to: ResourceState,
}
impl ResourceTransition {
    pub fn new(resource_state_from: ResourceState, resource_state_to: ResourceState) -> Self {
        Self {
            resource_state_from,
            resource_state_to,
        }
    }
    pub fn makes_sense(&self) -> bool {
        if let (
            ResourceUsage::Texture(_, _, _, _),
            ResourceUsage::TextureTranstional(layout, _, _, _, _),
        ) = (
            self.resource_state_from.resource_usage,
            self.resource_state_to.resource_usage,
        ) {
            if layout == ImageLayout::UNDEFINED {
                return false;
            }
        }
        self.resource_state_to.resource_id == self.resource_state_from.resource_id
            && (self.resource_state_from.resource_usage.pipeline_stage()
                != self.resource_state_to.resource_usage.pipeline_stage()
                || (self.resource_state_from.resource_usage.resource_access()
                    != ResourceAccess::Read
                    || self.resource_state_to.resource_usage.resource_access()
                        != ResourceAccess::Read))
    }
    pub fn resource_state_from(&self) -> ResourceState {
        self.resource_state_from
    }

    pub fn resource_state_to(&self) -> ResourceState {
        self.resource_state_to
    }
}
///TODO:
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
            ResourceUsage::Texture(_, _, _, resource_access) => resource_access,
            ResourceUsage::TextureTranstional(_, _, _, _, resource_access) => resource_access,
            ResourceUsage::Buffer(_, _, _, _, resource_access) => resource_access,
        }
    }
    pub fn pipeline_stage(&self) -> PipelineStageFlags {
        *match self {
            ResourceUsage::Texture(_, ps, _, _) => ps,
            ResourceUsage::TextureTranstional(_, _, ps, _, _) => ps,
            ResourceUsage::Buffer(ps, _, _, _, _) => ps,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceAccess {
    Read,
    Write,
    ReadWrite,
}
// #[cfg(test)]
// pub mod test {
//     use crate::{
//         render_graph::{
//             self, operations::gpu_operation::Operation, render_graph::RenderGraph,
//             resource::ResourceId,
//         },
//         rendering::texture_container::{self, CreateTexture, CreateTextureView},
//     };

//     #[test]
//     pub fn test_basic() {
//         let mut rendergraph = RenderGraph::new();
//         let mut texture_container = texture_container::TextureContainer::new();
//         let present = texture_container.create_texture_view_null();
//         let source = texture_container.create_texture_view_null();
//         let intermid = texture_container.create_texture_view_null();

//         let present_op = Operation::Present(ResourceId::Texture(present));
//         let draw_op =
//             Operation::DrawCall(ResourceId::Texture(intermid), ResourceId::Texture(present));
//         let draw_op1 =
//             Operation::DrawCall(ResourceId::Texture(source), ResourceId::Texture(intermid));
//         rendergraph.add_target_op(present_op);
//         rendergraph.add_operation(draw_op);
//         rendergraph.add_operation(draw_op1);
//         assert_eq!(
//             Some(vec![draw_op1, draw_op, present_op]),
//             rendergraph.compile()
//         );
//         println!("{:?}", rendergraph.compile());
//     }
//     #[test]
//     pub fn test_parallel() {
//         let mut rendergraph = RenderGraph::new();
//         let mut texture_container = texture_container::TextureContainer::new();
//         let present = texture_container.create_texture_view_null();
//         let source = texture_container.create_texture_view_null();
//         let source_2_inter = texture_container.create_texture_view_null();
//         let source_2 = texture_container.create_texture_view_null();
//         println!(
//             "present:{:?} source:{:?} source_2_inter:{:?} source_2:{:?}",
//             present, source, source_2_inter, source_2
//         );
//         let present_op = Operation::Present(ResourceId::Texture(present));
//         let draw_op =
//             Operation::DrawCall(ResourceId::Texture(source), ResourceId::Texture(present));
//         let draw_op1 = Operation::DrawCall(
//             ResourceId::Texture(source_2_inter),
//             ResourceId::Texture(present),
//         );
//         let dep_op1 = Operation::DrawCall(
//             ResourceId::Texture(source_2),
//             ResourceId::Texture(source_2_inter),
//         );
//         rendergraph.add_target_op(present_op);
//         rendergraph.add_operation(draw_op);
//         rendergraph.add_operation(draw_op1);
//         rendergraph.add_operation(dep_op1);
//         assert_eq!(
//             Some(vec![draw_op, dep_op1, draw_op1, present_op]),
//             rendergraph.compile()
//         );
//     }
// }
