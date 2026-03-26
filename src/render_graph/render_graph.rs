use std::collections::HashMap;

use ash::vk::{
    BufferMemoryBarrier, CommandBuffer, DependencyFlags, ImageAspectFlags, ImageMemoryBarrier, ImageSubresourceRange,
};

use crate::{
    device::DeviceContext,
    render_graph::{
        dag::DirectedAcyclicGraph,
        operations::gpu_operation::Operation,
        resource_state::StateTransition,
        sync::{SyncOp, SyncPoint},
    },
    rendering::renderer_bundle::RendererBundle,
};

/// This struct purpose is to optimize rendering with batching, optimal synchronization usage, parallelization and culling
pub struct RenderGraph {
    dag: DirectedAcyclicGraph<Operation, Vec<StateTransition>>,
    target_node: usize,
}
impl Default for RenderGraph {
    fn default() -> Self {
        Self::new()
    }
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
                                .find(|y| StateTransition::edge_makes_sense(y, &x))
                                .map(|y| {
                                    
                                    StateTransition::new(x, *y)
                                })
                        })
                        .collect::<Vec<StateTransition>>();
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
        let compiled = self.dag.compile(self.target_node)?;

        let mut last_state = HashMap::new();
        let mut actions = Vec::with_capacity(compiled.len());
        for x in compiled {
            let node = self.dag.get_node(x).cloned().unwrap();
            let pre_sync = node.resource_state(bundle).as_ref().unwrap().iter().fold(
                SyncPoint::default(),
                |mut acc, &x| {
                    if let Some(last) = last_state.get(&x.resource_id()) {
                        let transition = StateTransition::new(*last, x);
                        if transition.sync_makes_sense() {
                            let sync_op = SyncOp::from_transition(&transition);
                            acc.push_sync_op(sync_op);
                        }
                    } else if let Some(resource_state) = bundle.resource_state.get(x.resource_id()) {
                        let transition = StateTransition::new(resource_state, x);
                        if transition.sync_makes_sense() {
                            let sync_op = SyncOp::from_transition(&transition);
                            acc.push_sync_op(sync_op);
                        }
                    } else {
                        acc.push_sync_op(SyncOp::from_unitialized(x));
                    }
                    last_state.insert(x.resource_id(), x);
                    acc
                },
            );
            if !pre_sync.is_empty() {
                actions.push(Action::Sync(pre_sync));
            }
            actions.push(Action::Op(node));
        }
        Some(actions)
    }
    pub fn dag(&self) -> &DirectedAcyclicGraph<Operation, Vec<StateTransition>> {
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
                for (stages, syncs) in sync_point.sync_ops().iter() {
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
                                if let Some(texture) =
                                    bundle.texture_container.get_image(*texture_id)
                                {
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
        }
    }
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
