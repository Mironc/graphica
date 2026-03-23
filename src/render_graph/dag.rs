use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct DirectedAcyclicGraph<N, E> {
    nodes: Vec<N>,
    edges: Vec<Vec<(usize, E)>>,
    rev_edges: Vec<Vec<usize>>,
}
impl<N, E> DirectedAcyclicGraph<N, E> {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            rev_edges: Vec::new(),
        }
    }
    /// Returns node data for given `id`,
    /// if `id` is not present in graph returns `None`
    pub fn get_node(&self, id: usize) -> Option<&N> {
        self.nodes.get(id)
    }
    /// Returns number of nodes in graph
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    /// Counts number of descending nodes, including itself
    pub fn descendants_count(&self, node: usize) -> usize {
        if let Some(neighbours) = self.edges.get(node) {
            return neighbours
                .iter()
                .map(|x| self.descendants_count(x.0))
                .sum::<usize>()
                + 1;
        }
        1
    }
    ///Adds node into the graph
    pub fn add_node(&mut self, node: N) {
        self.nodes.push(node);
        self.edges.push(Vec::new());
        self.rev_edges.push(Vec::new());
    }

    pub fn has_cycle(&self) -> bool {
        for node in self.nodes.iter().enumerate() {
            let mut stack = vec![node.0];
            let mut visited = HashSet::new();

            while let Some(current_node) = stack.pop() {
                if current_node == node.0 {
                    return true;
                }
                if visited.insert(current_node) {
                    if let Some(children) = self.edges.get(current_node) {
                        for child in children.iter() {
                            stack.push(child.0);
                        }
                    }
                }
            }
        }
        false
    }
    /// Checks if adding edge between nodes create cycle
    pub fn would_cycle(&self, node_from: usize, node_to: usize) -> bool {
        if node_from == node_to {
            return true;
        }

        let mut stack = vec![node_to];
        let mut visited = HashSet::new();

        while let Some(current) = stack.pop() {
            if current == node_from {
                return true;
            }

            if visited.insert(current) {
                if let Some(children) = self.edges.get(current) {
                    for (child_idx, _) in children {
                        stack.push(*child_idx);
                    }
                }
            }
        }
        false
    }

    /// Returns None if node creates cycle or nodes are not present in graph
    pub fn add_edge(&mut self, node_from: usize, node_to: usize, edge_data: E) -> Option<()> {
        if self.would_cycle(node_from, node_to) {
            return None;
        }
        self.add_edge_cyclic(node_from, node_to, edge_data)
    }

    /// Returns None if node_from or node_to is not present in graph or node_from == node_to
    ///
    /// May introduce cycles into graph
    pub fn add_edge_cyclic(
        &mut self,
        node_from: usize,
        node_to: usize,
        edge_data: E,
    ) -> Option<()> {
        if node_from == node_to || self.nodes.len() < node_from || self.nodes.len() < node_to {
            return None;
        }
        self.edges[node_from].push((node_to, edge_data));
        self.rev_edges[node_to].push(node_from);
        Some(())
    }
    /// Checks if there's edge between two nodes.
    ///
    /// Beware that it's works only in one way
    ///
    /// If node is not present in graph it will return false
    pub fn has_edge(&self, node_from: usize, node_to: usize) -> bool {
        if let Some(edges) = self.edges.get(node_from) {
            return edges.iter().any(|(x, _)| node_to == *x);
        }
        false
    }

    /// Returns vector where index gives info about children that node has
    pub fn edges(&self) -> &Vec<Vec<(usize, E)>> {
        &self.edges
    }

    ///Return vector where index gives info about parents that node has
    pub fn rev_edges(&self) -> &Vec<Vec<usize>> {
        &self.rev_edges
    }

    ///Return node data as vector
    pub fn nodes(&self) -> &Vec<N> {
        &self.nodes
    }

    ///Firstly removes nodes that are not connected to target node
    ///
    ///Secondly makes topological sort
    pub fn compile(&self, target_node: usize) -> Option<Vec<usize>> {
        let n = self.nodes.len();

        let mut alive = vec![false; n];
        let mut stack = vec![target_node];

        while let Some(idx) = stack.pop() {
            if !alive[idx] {
                alive[idx] = true;
                for &parent_idx in &self.rev_edges[idx] {
                    stack.push(parent_idx);
                }
            }
        }

        let mut in_degree = vec![0; n];
        for (from, neighbors) in self.edges.iter().enumerate() {
            if !alive[from] {
                continue;
            }
            for (to, _) in neighbors {
                if alive[*to] {
                    in_degree[*to] += 1;
                }
            }
        }

        let mut queue = VecDeque::new();
        for i in 0..n {
            if alive[i] && in_degree[i] == 0 {
                queue.push_back(i);
            }
        }

        let mut schedule = Vec::with_capacity(n);
        while let Some(u) = queue.pop_front() {
            schedule.push(u);

            for (v, _) in &self.edges[u] {
                if alive[*v] {
                    in_degree[*v] -= 1;
                    if in_degree[*v] == 0 {
                        queue.push_back(*v);
                    }
                }
            }
        }

        let alive_count = alive.iter().filter(|&&a| a).count();
        if schedule.len() == alive_count {
            Some(schedule)
        } else {
            None
        }
    }
    ///Clears graph, removes all nodes and edges
    pub fn clear(&mut self) {
        self.edges.clear();
        self.nodes.clear();
        self.rev_edges.clear();
    }
}
#[cfg(test)]
mod tests {
    use crate::render_graph::dag::DirectedAcyclicGraph;

    #[test]
    pub fn test_adding_edges() {
        let mut dag = DirectedAcyclicGraph::new();
        dag.add_node(1);
        dag.add_node(2);
        dag.add_node(3);
        dag.add_node(4);
        assert_eq!(dag.add_edge(0, 1, ()), Some(()));
        assert_eq!(dag.add_edge(1, 2, ()), Some(()));
        assert_eq!(dag.add_edge(0, 3, ()), Some(()));
        assert_eq!(dag.add_edge(2, 0, ()), None); //creates cycle
        assert_eq!(dag.add_edge(5, 0, ()), None); //from is not present in the graph
        assert_eq!(dag.add_edge(0, 7, ()), None); //to is not present in the graph
        assert_eq!(dag.add_edge(0, 0, ()), None); //from == to

        let mut dag = DirectedAcyclicGraph::new();
        dag.add_node(1);
        dag.add_node(2);
        dag.add_node(3);
        dag.add_node(4);
        dag.add_edge_cyclic(0, 1, ());
        dag.add_edge_cyclic(1, 2, ());
        dag.add_edge_cyclic(2, 3, ());
        dag.add_edge_cyclic(3, 0, ()); //creates cycle 0 -> 3 3 -> 0
        assert!(dag.has_cycle());
    }

    #[test]
    pub fn test_descendants_count() {
        let mut dag = DirectedAcyclicGraph::new();
        dag.add_node(1);
        dag.add_node(2);
        dag.add_node(3);
        dag.add_node(4);
        dag.add_edge(0, 1, ());
        dag.add_edge(1, 2, ());
        dag.add_edge(2, 3, ());
        assert_eq!(dag.descendants_count(0), 4); //0 -> 1 -> 2 -> 3 = 4
        assert_eq!(dag.descendants_count(1), 3); //1 -> 2 -> 3 = 3
        assert_eq!(dag.descendants_count(2), 2); //2 -> 3 = 2
    }

    #[test]
    pub fn test_compile() {
        let mut dag = DirectedAcyclicGraph::new();
        dag.add_node(1);
        dag.add_node(2);
        dag.add_node(3);
        dag.add_node(4);
        dag.add_edge(0, 1, ());
        dag.add_edge(1, 2, ());
        dag.add_edge(2, 3, ());
        assert_eq!(dag.compile(3), Some(vec![0, 1, 2, 3]));

        dag.add_node(5);
        dag.add_edge(4, 3, ());
        assert_eq!(dag.compile(3), Some(vec![0, 4, 1, 2, 3]));

        let mut dag = DirectedAcyclicGraph::new();
        dag.add_node(1);
        dag.add_node(2);
        dag.add_node(3);
        dag.add_node(4);
        dag.add_edge(0, 3, ());
        dag.add_edge(2, 3, ());
        dag.add_edge(0, 1, ());
        assert_eq!(dag.compile(3), Some(vec![0, 2, 3]));

        let mut dag = DirectedAcyclicGraph::new();
        dag.add_node(1);
        dag.add_node(2);
        dag.add_node(3);
        dag.add_node(4);
        dag.add_edge_cyclic(0, 1, ());
        dag.add_edge_cyclic(1, 2, ());
        dag.add_edge_cyclic(2, 0, ());
        dag.add_edge_cyclic(2, 3, ());
        assert_eq!(dag.compile(3), None);
    }
}
