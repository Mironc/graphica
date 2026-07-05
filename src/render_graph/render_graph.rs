use std::collections::HashMap;

use ash::vk::CommandBuffer;

use crate::{
    device::DeviceContext,
    render_graph::{
        dag::DirectedAcyclicGraph,
        operations::gpu_operation::Operation,
        resource::ResourceId,
        resource_state::{ResourceState, StateTransition},
        sync::{SyncOp, SyncPoint},
    },
    rendering::renderer_bundle::RendererBundle,
};

/// This struct purpose is to optimize rendering with batching, optimal synchronization usage, parallelization and culling
pub struct RenderGraph {
    dag: DirectedAcyclicGraph<Operation, Vec<StateTransition>>,
    labeled_nodes: HashMap<usize, String>,
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
            labeled_nodes: HashMap::default(),
        }
    }
    /// Inserts operation into render graph
    ///
    /// If operation is not infuencing any target operations it would be culled at compilation
    pub fn add_operation(&mut self, op: Operation) {
        self.dag.add_node(op);
    }
    /// Inserts operation into render graph with a label
    ///
    /// If operation is not infuencing any target operations it would be culled at compilation
    pub fn add_operation_labeled(&mut self, op: Operation, label: String) {
        self.dag.add_node(op);
        self.labeled_nodes
            .insert(self.dag().node_count() - 1, label);
    }
    /// Inserts operation into render graph marked as targeted, `target` operations are the ones that define output of app
    pub fn add_target_op(&mut self, target: Operation) {
        self.dag.add_node(target);
        self.target_node = self.dag.node_count() - 1;
    }
    /// Inserts operation into render graph marked as targeted with label
    ///
    pub fn add_target_op_labeled(&mut self, target: Operation, label: String) {
        self.dag.add_node(target);
        self.target_node = self.dag.node_count() - 1;
        self.labeled_nodes
            .insert(self.dag().node_count() - 1, label);
    }
    /// Creates edges in graph based on whetever operation writes into resources that are used by other operation
    fn fill_in(&mut self, bundle: &RendererBundle) {
        // Contains all nodes resource states
        let all_node_states: Vec<Vec<ResourceState>> = self
            .dag
            .nodes()
            .iter()
            .map(|n| n.resource_state(bundle).unwrap_or_default())
            .collect();

        // Contains all states of resource in nodes
        let mut resource_map: HashMap<ResourceId, Vec<(usize, ResourceState)>> = HashMap::new();
        for (node_id, states) in all_node_states.iter().enumerate() {
            for state in states {
                resource_map
                    .entry(state.resource_id())
                    .or_default()
                    .push((node_id, *state));
            }
        }

        // TODO: Refactor this and create a better algo if there's any
        // Start from the op node, and goes down
        let mut stack = vec![self.target_node];
        let mut visited = std::collections::HashSet::new();
        visited.insert(self.target_node);

        while let Some(current_id) = stack.pop() {
            let current_states = &all_node_states[current_id];

            for state_to in current_states.iter() {
                let res_id = state_to.resource_id();
                if let Some(potential_producers) = resource_map.get(&res_id) {
                    for (prev_id, state_from) in potential_producers.iter() {
                        if *prev_id >= current_id {
                            break;
                        }
                        if StateTransition::edge_makes_sense(state_from, state_to) {
                            let transition = StateTransition::new(*state_from, *state_to);
                            self.dag
                                .add_edge_cyclic(*prev_id, current_id, vec![transition]);

                            if visited.insert(*prev_id) {
                                stack.push(*prev_id);
                            }
                        }
                    }
                }
            }
        }
    }

    /// This function is the main feature of render graph
    ///
    /// It finishes graph, culls unused branches, orders operations and includes synchronization points
    pub fn compile(&mut self, bundle: &RendererBundle) -> Option<Executable> {
        self.fill_in(bundle);

        //Removes nodes that are not influencing on target nodes and makes topological sort (linearizing operation order)
        let compiled = self.dag.compile(self.target_node)?;

        let mut ops = Vec::with_capacity(compiled.len());
        for &i in compiled.iter() {
            let node = self.dag.get_node(i).cloned()?;
            ops.push(node);
        }
        let mut last_states = HashMap::new();
        // Finds all draw passes and looks for
        // Might be useful to use hashmap<resource_id,vec<(node_idx,state)>>, so it can be used to lookup next and previous state faster with bin search
        let mut sync_passes = HashMap::new();
        for (idx, node) in ops.iter().enumerate() {
            let states = node.resource_state(bundle)?;
            if let Operation::DrawCall(_) = &node {
                let mut transitions = Vec::new();
                'outer: for state in states.iter() {
                    // check cached state of resources
                    let prev_state = if let Some(s) = last_states.get(&state.resource_id()) {
                        *s
                    } else {
                        bundle.resource_state.get_or_default(state.resource_id())
                    };
                    // Go to next nodes and sort out next state
                    for i in idx + 1..ops.len() {
                        let next_states = ops[i].resource_state(bundle)?;
                        if let Some(next_state) = next_states
                            .iter()
                            .find(|x| x.resource_id() == state.resource_id())
                        {
                            transitions.push((prev_state, Some(*next_state)));
                            continue 'outer;
                        }
                        transitions.push((prev_state, None));
                    }
                }
                sync_passes.insert(idx, transitions);
            }
            states.iter().for_each(|x| {
                last_states.insert(x.resource_id(), *x);
            });
        }

        for (idx, transitions) in sync_passes.into_iter() {
            if let Operation::DrawCall(draw_data) = &mut ops[idx] {
                draw_data.insert_sync(transitions);
            }
        }

        let mut last_state = HashMap::new();
        let mut actions = Vec::with_capacity(compiled.len());
        for (idx, node) in ops.into_iter().enumerate() {
            let mut pre_sync = SyncPoint::default();
            for &x in node.resource_state(bundle).as_ref()?.iter() {
                if let Some(last) = last_state.get(&x.resource_id()) {
                    let transition = StateTransition::new(*last, x);
                    if transition.sync_makes_sense() {
                        let sync_op = SyncOp::from_transition(&transition);
                        pre_sync.push_sync_op(sync_op);
                    }
                } else {
                    let resource_state = bundle.resource_state.get_or_default(x.resource_id());
                    let transition = StateTransition::new(resource_state, x);
                    if transition.sync_makes_sense() {
                        let sync_op = SyncOp::from_transition(&transition);
                        pre_sync.push_sync_op(sync_op);
                    }
                }
                last_state.insert(x.resource_id(), x);
            }
            if !pre_sync.is_empty() {
                actions.push(Action::Sync(pre_sync));
            }
            actions.push(Action::Op((node, idx)));
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
#[cfg(feature = "graph-visualize")]
impl RenderGraph {
    /// Turns render graph into graphviz .dot format
    pub fn into_dot(&mut self, bundle: &RendererBundle) -> String {
        use petgraph::{dot::Dot, graph::DiGraph, prelude::NodeIndex};

        self.fill_in(bundle);
        let render_graph = self.dag();
        let mut graph = DiGraph::default();
        for node in render_graph.nodes() {
            graph.add_node(node);
        }
        for (i, node) in render_graph.edges().iter().enumerate() {
            for (to, _) in node.iter() {
                let paths = render_graph.count_paths(i, *to);
                if paths == 1 {
                    graph.add_edge(i.into(), (*to).into(), ());
                }
            }
        }
        let binding = |_, node_ref: (NodeIndex<usize>, &&Operation)| {
            let label = self.labeled_nodes.get(&node_ref.0.index());
            format!("{}", node_ref.1.fmt_dot(bundle, label.map(|x| x.as_str())))
        };
        let as_dot = Dot::with_attr_getters(
            &graph,
            &[
                petgraph::dot::Config::EdgeNoLabel,
                petgraph::dot::Config::NodeNoLabel,
            ],
            &|_, _| format!(""),
            &binding,
        );
        format!("{:?}", as_dot)
    }
    /// Compiles render graph and then turns into graphviz .dot format
    pub fn compile_into_dot(&mut self, bundle: &RendererBundle) -> Option<String> {
        use petgraph::{dot::Dot, graph::DiGraph, prelude::NodeIndex};
        let compiled = self.compile(&bundle)?;
        let mut digraph = DiGraph::default();
        for (i, act) in compiled.iter().enumerate() {
            digraph.add_node(act);
            if i > 0 {
                digraph.add_edge((i - 1).into(), i.into(), ());
            }
        }
        let binding = |_, node_ref: (NodeIndex<usize>, &&Action)| {
            node_ref.1.fmt_dot(bundle, &self.labeled_nodes)
        };
        let as_dot = Dot::with_attr_getters(
            &digraph,
            &[
                petgraph::dot::Config::EdgeNoLabel,
                petgraph::dot::Config::NodeNoLabel,
            ],
            &|_, _| format!(""),
            &binding,
        );
        Some(format!("{:?}", as_dot))
    }
}
pub type Executable = Vec<Action>;
#[derive(Debug, Clone)]
pub enum Action {
    Op((Operation, usize)),
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
                operation.0.execute(device, command_buffer, bundle);
            }
            Action::Sync(sync_point) => {
                sync_point.execute(device, command_buffer, bundle);
            }
        }
    }
    #[cfg(feature = "graph-visualize")]
    pub fn fmt_dot(&self, bundle: &RendererBundle, labels: &HashMap<usize, String>) -> String {
        match self {
            Action::Op(op) => op.0.fmt_dot(bundle, labels.get(&op.1).map(|x| x.as_str())),
            Action::Sync(sync_point) => sync_point.fmt_dot(bundle),
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
