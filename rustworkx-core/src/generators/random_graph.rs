// Licensed under the Apache License, Version 2.0 (the "License"); you may
// not use this file except in compliance with the License. You may obtain
// a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.

#![allow(clippy::float_cmp)]

use crate::dictmap::DictMap;
use indexmap::IndexSet;
use std::hash::Hash;

use ndarray::ArrayView2;
use petgraph::data::{Build, Create};
use petgraph::visit::{
    Data, EdgeRef, GraphBase, GraphProp, IntoEdgeReferences, IntoEdgesDirected,
    IntoNodeIdentifiers, NodeCount, NodeIndexable,
};
use petgraph::{Incoming, Outgoing};

use hashbrown::HashSet;
use rand::prelude::*;
use rand_distr::{Distribution, Uniform};
use rand_pcg::Pcg64;

use super::star_graph;
use super::InvalidInputError;

/// Generates a random regular graph
///
/// A regular graph is one where each node has same number of neighbors. This function takes in
/// number of nodes and degrees as two functions which are used in order to generate a random regular graph.
///
/// This is only defined for undirected graphs, which have no self-directed edges or parallel edges.
///
/// Raises an error if
///
/// This algorithm is based on the implementation of networkx function
/// <https://github.com/networkx/networkx/blob/networkx-2.4/networkx/generators/random_graphs.py>
///
/// A. Steger and N. Wormald,
/// Generating random regular graphs quickly,
/// Probability and Computing 8 (1999), 377-396, 1999.
/// <https://doi.org/10.1017/S0963548399003867>
///
/// Jeong Han Kim and Van H. Vu,
/// Generating random regular graphs,
/// Proceedings of the thirty-fifth ACM symposium on Theory of computing,
/// San Diego, CA, USA, pp 213--222, 2003.
/// <http://portal.acm.org/citation.cfm?id=780542.780576>
///
/// Arguments:
///
/// * `num_nodes` - The number of nodes for creating the random graph.
/// * `degree` - The number of edges connected to each node.
/// * `seed` - An optional seed to use for the random number generator.
/// * `default_node_weight` - A callable that will return the weight to use
///   for newly created nodes.
/// * `default_edge_weight` - A callable that will return the weight object
///   to use for newly created edges.
/// # Example
/// ```rust
/// use rustworkx_core::petgraph;
/// use rustworkx_core::generators::random_regular_graph;
///
/// let g: petgraph::graph::UnGraph<(), ()> = random_regular_graph(
///     4,
///     2,
///     Some(2025),
///     || {()},
///     || {()},
/// ).unwrap();
/// assert_eq!(g.node_count(), 4);
/// assert_eq!(g.edge_count(), 4);
/// ```
pub fn random_regular_graph<G, T, F, H, M>(
    num_nodes: usize,
    degree: usize,
    seed: Option<u64>,
    mut default_node_weight: F,
    mut default_edge_weight: H,
) -> Result<G, InvalidInputError>
where
    G: Build + Create + Data<NodeWeight = T, EdgeWeight = M> + NodeIndexable + GraphProp,
    F: FnMut() -> T,
    H: FnMut() -> M,
    G::NodeId: Eq + Hash + Copy,
{
    if num_nodes == 0 {
        return Err(InvalidInputError {});
    }
    let mut rng: Pcg64 = match seed {
        Some(seed) => Pcg64::seed_from_u64(seed),
        None => Pcg64::from_os_rng(),
    };

    if (num_nodes * degree) % 2 != 0 {
        return Err(InvalidInputError {});
    }

    if degree >= num_nodes {
        return Err(InvalidInputError {});
    }

    let mut graph = G::with_capacity(num_nodes, num_nodes);
    for _ in 0..num_nodes {
        graph.add_node(default_node_weight());
    }

    if degree == 0 {
        return Ok(graph);
    }

    let suitable = |edges: &IndexSet<(G::NodeId, G::NodeId)>,
                    potential_edges: &DictMap<G::NodeId, usize>|
     -> bool {
        if potential_edges.is_empty() {
            return true;
        }
        for (s1, _) in potential_edges.iter() {
            for (s2, _) in potential_edges.iter() {
                if s1 == s2 {
                    break;
                }
                let (u, v) = if graph.to_index(*s1) > graph.to_index(*s2) {
                    (*s2, *s1)
                } else {
                    (*s1, *s2)
                };
                if !edges.contains(&(u, v)) {
                    return true;
                }
            }
        }
        false
    };

    let mut try_creation = || -> Option<IndexSet<(G::NodeId, G::NodeId)>> {
        let mut edges: IndexSet<(G::NodeId, G::NodeId)> = IndexSet::with_capacity(num_nodes);
        let mut stubs: Vec<G::NodeId> = (0..num_nodes)
            .flat_map(|x| std::iter::repeat(graph.from_index(x)).take(degree))
            .collect();
        while !stubs.is_empty() {
            let mut potential_edges: DictMap<G::NodeId, usize> = DictMap::default();
            stubs.shuffle(&mut rng);

            let mut i = 0;
            while i + 1 < stubs.len() {
                let s1 = stubs[i];
                let s2 = stubs[i + 1];
                let (u, v) = if graph.to_index(s1) > graph.to_index(s2) {
                    (s2, s1)
                } else {
                    (s1, s2)
                };
                if u != v && !edges.contains(&(u, v)) {
                    edges.insert((u, v));
                } else {
                    *potential_edges.entry(u).or_insert(0) += 1;
                    *potential_edges.entry(v).or_insert(0) += 1;
                }
                i += 2;
            }

            if !suitable(&edges, &potential_edges) {
                return None;
            }
            stubs = Vec::new();
            for (key, value) in potential_edges.iter() {
                for _ in 0..*value {
                    stubs.push(*key);
                }
            }
        }

        Some(edges)
    };

    let edges = loop {
        match try_creation() {
            Some(created_edges) => {
                break created_edges;
            }
            None => continue,
        }
    };
    for (u, v) in edges {
        graph.add_edge(u, v, default_edge_weight());
    }

    Ok(graph)
}

/// Generate a G<sub>np</sub> random graph, also known as an
/// Erdős-Rényi graph or a binomial graph.
///
/// For number of nodes `n` and probability `p`, the G<sub>np</sub>
/// graph algorithm creates `n` nodes, and for all the `n * (n - 1)` possible edges,
/// each edge is created independently with probability `p`.
/// In general, for any probability `p`, the expected number of edges returned
/// is `m = p * n * (n - 1)`. If `p = 0` or `p = 1`, the returned
/// graph is not random and will always be an empty or a complete graph respectively.
/// An empty graph has zero edges and a complete directed graph has `n (n - 1)` edges.
/// The run time is `O(n + m)` where `m` is the expected number of edges mentioned above.
/// When `p = 0`, run time always reduces to `O(n)`, as the lower bound.
/// When `p = 1`, run time always goes to `O(n + n * (n - 1))`, as the upper bound.
///
/// For `0 < p < 1`, the algorithm is based on the implementation of the networkx function
/// ``fast_gnp_random_graph``,
/// <https://github.com/networkx/networkx/blob/networkx-2.4/networkx/generators/random_graphs.py#L49-L120>
///
/// Vladimir Batagelj and Ulrik Brandes,
///    "Efficient generation of large random networks",
///    Phys. Rev. E, 71, 036113, 2005.
///
/// Arguments:
///
/// * `num_nodes` - The number of nodes for creating the random graph.
/// * `probability` - The probability of creating an edge between two nodes as a float.
/// * `seed` - An optional seed to use for the random number generator.
/// * `default_node_weight` - A callable that will return the weight to use
///   for newly created nodes.
/// * `default_edge_weight` - A callable that will return the weight object
///   to use for newly created edges.
///
/// # Example
/// ```rust
/// use rustworkx_core::petgraph;
/// use rustworkx_core::generators::gnp_random_graph;
///
/// let g: petgraph::graph::DiGraph<(), ()> = gnp_random_graph(
///     20,
///     1.0,
///     None,
///     || {()},
///     || {()},
/// ).unwrap();
/// assert_eq!(g.node_count(), 20);
/// assert_eq!(g.edge_count(), 20 * (20 - 1));
/// ```
pub fn gnp_random_graph<G, T, F, H, M>(
    num_nodes: usize,
    probability: f64,
    seed: Option<u64>,
    mut default_node_weight: F,
    mut default_edge_weight: H,
) -> Result<G, InvalidInputError>
where
    G: Build + Create + Data<NodeWeight = T, EdgeWeight = M> + NodeIndexable + GraphProp,
    F: FnMut() -> T,
    H: FnMut() -> M,
    G::NodeId: Eq + Hash,
{
    if num_nodes == 0 {
        return Err(InvalidInputError {});
    }
    let mut rng: Pcg64 = match seed {
        Some(seed) => Pcg64::seed_from_u64(seed),
        None => Pcg64::from_os_rng(),
    };
    let mut graph = G::with_capacity(num_nodes, num_nodes);
    let directed = graph.is_directed();

    for _ in 0..num_nodes {
        graph.add_node(default_node_weight());
    }
    if !(0.0..=1.0).contains(&probability) {
        return Err(InvalidInputError {});
    }

    if probability > 0.0 {
        if (probability - 1.0).abs() < f64::EPSILON {
            for u in 0..num_nodes {
                let start_node = if directed { 0 } else { u + 1 };
                for v in start_node..num_nodes {
                    if !directed || u != v {
                        // exclude self-loops
                        let u_index = graph.from_index(u);
                        let v_index = graph.from_index(v);
                        graph.add_edge(u_index, v_index, default_edge_weight());
                    }
                }
            }
        } else {
            let num_nodes: isize = match num_nodes.try_into() {
                Ok(nodes) => nodes,
                Err(_) => return Err(InvalidInputError {}),
            };
            let lp: f64 = (1.0 - probability).ln();
            let between = Uniform::new(0.0, 1.0).unwrap();

            // For directed, create inward edges to a v
            if directed {
                let mut v: isize = 0;
                let mut w: isize = -1;
                while v < num_nodes {
                    let random: f64 = between.sample(&mut rng);
                    let lr: f64 = (1.0 - random).ln();
                    w = w + 1 + ((lr / lp) as isize);
                    while w >= v && v < num_nodes {
                        w -= v;
                        v += 1;
                    }
                    // Skip self-loops
                    if v == w {
                        w -= v;
                        v += 1;
                    }
                    if v < num_nodes {
                        let v_index = graph.from_index(v as usize);
                        let w_index = graph.from_index(w as usize);
                        graph.add_edge(w_index, v_index, default_edge_weight());
                    }
                }
            }

            // For directed and undirected, create outward edges from a v
            // Nodes in graph are from 0,n-1 (start with v as the second node index).
            let mut v: isize = 1;
            let mut w: isize = -1;
            while v < num_nodes {
                let random: f64 = between.sample(&mut rng);
                let lr: f64 = (1.0 - random).ln();
                w = w + 1 + ((lr / lp) as isize);
                while w >= v && v < num_nodes {
                    w -= v;
                    v += 1;
                }
                if v < num_nodes {
                    let v_index = graph.from_index(v as usize);
                    let w_index = graph.from_index(w as usize);
                    graph.add_edge(v_index, w_index, default_edge_weight());
                }
            }
        }
    }
    Ok(graph)
}

// /// Return a `G_{nm}` directed graph, also known as an
// /// Erdős-Rényi graph.
// ///
// /// Generates a random directed graph out of all the possible graphs with `n` nodes and
// /// `m` edges. The generated graph will not be a multigraph and will not have self loops.
// ///
// /// For `n` nodes, the maximum edges that can be returned is `n (n - 1)`.
// /// Passing `m` higher than that will still return the maximum number of edges.
// /// If `m = 0`, the returned graph will always be empty (no edges).
// /// When a seed is provided, the results are reproducible. Passing a seed when `m = 0`
// /// or `m >= n (n - 1)` has no effect, as the result will always be an empty or a complete graph respectively.
// ///
// /// This algorithm has a time complexity of `O(n + m)`

/// Generate a G<sub>nm</sub> random graph, also known as an
/// Erdős-Rényi graph.
///
/// Generates a random directed graph out of all the possible graphs with `n` nodes and
/// `m` edges. The generated graph will not be a multigraph and will not have self loops.
///
/// For `n` nodes, the maximum edges that can be returned is `n * (n - 1)`.
/// Passing `m` higher than that will still return the maximum number of edges.
/// If `m = 0`, the returned graph will always be empty (no edges).
/// When a seed is provided, the results are reproducible. Passing a seed when `m = 0`
/// or `m >= n * (n - 1)` has no effect, as the result will always be an empty or a
/// complete graph respectively.
///
/// This algorithm has a time complexity of `O(n + m)`
///
/// Arguments:
///
/// * `num_nodes` - The number of nodes to create in the graph.
/// * `num_edges` - The number of edges to create in the graph.
/// * `seed` - An optional seed to use for the random number generator.
/// * `default_node_weight` - A callable that will return the weight to use
///   for newly created nodes.
/// * `default_edge_weight` - A callable that will return the weight object
///   to use for newly created edges.
///
/// # Example
/// ```rust
/// use rustworkx_core::petgraph;
/// use rustworkx_core::generators::gnm_random_graph;
///
/// let g: petgraph::graph::DiGraph<(), ()> = gnm_random_graph(
///     20,
///     12,
///     None,
///     || {()},
///     || {()},
/// ).unwrap();
/// assert_eq!(g.node_count(), 20);
/// assert_eq!(g.edge_count(), 12);
/// ```
pub fn gnm_random_graph<G, T, F, H, M>(
    num_nodes: usize,
    num_edges: usize,
    seed: Option<u64>,
    mut default_node_weight: F,
    mut default_edge_weight: H,
) -> Result<G, InvalidInputError>
where
    G: GraphProp + Build + Create + Data<NodeWeight = T, EdgeWeight = M> + NodeIndexable,
    F: FnMut() -> T,
    H: FnMut() -> M,
    for<'b> &'b G: GraphBase<NodeId = G::NodeId> + IntoEdgeReferences,
    G::NodeId: Eq + Hash,
{
    if num_nodes == 0 {
        return Err(InvalidInputError {});
    }

    fn find_edge<G>(graph: &G, source: usize, target: usize) -> bool
    where
        G: GraphBase + NodeIndexable,
        for<'b> &'b G: GraphBase<NodeId = G::NodeId> + IntoEdgeReferences,
    {
        let mut found = false;
        for edge in graph.edge_references() {
            if graph.to_index(edge.source()) == source && graph.to_index(edge.target()) == target {
                found = true;
                break;
            }
        }
        found
    }

    let mut rng: Pcg64 = match seed {
        Some(seed) => Pcg64::seed_from_u64(seed),
        None => Pcg64::from_os_rng(),
    };
    let mut graph = G::with_capacity(num_nodes, num_edges);
    let directed = graph.is_directed();

    for _ in 0..num_nodes {
        graph.add_node(default_node_weight());
    }
    // if number of edges to be created is >= max,
    // avoid randomly missed trials and directly add edges between every node
    let div_by = if directed { 1 } else { 2 };
    if num_edges >= num_nodes * (num_nodes - 1) / div_by {
        for u in 0..num_nodes {
            let start_node = if directed { 0 } else { u + 1 };
            for v in start_node..num_nodes {
                // avoid self-loops
                if !directed || u != v {
                    let u_index = graph.from_index(u);
                    let v_index = graph.from_index(v);
                    graph.add_edge(u_index, v_index, default_edge_weight());
                }
            }
        }
    } else {
        let mut created_edges: usize = 0;
        let between = Uniform::new(0, num_nodes).unwrap();
        while created_edges < num_edges {
            let u = between.sample(&mut rng);
            let v = between.sample(&mut rng);
            let u_index = graph.from_index(u);
            let v_index = graph.from_index(v);
            // avoid self-loops and multi-graphs
            if u != v && !find_edge(&graph, u, v) {
                graph.add_edge(u_index, v_index, default_edge_weight());
                created_edges += 1;
            }
        }
    }
    Ok(graph)
}

/// Generate a graph from the stochastic block model.
///
/// The stochastic block model is a generalization of the G<sub>np</sub> random graph
/// (see [gnp_random_graph] ). The connection probability of
/// nodes `u` and `v` depends on their block and is given by
/// `probabilities[blocks[u]][blocks[v]]`, where `blocks[u]` is the block membership
/// of vertex `u`. The number of nodes and the number of blocks are inferred from
/// `sizes`.
///
/// Arguments:
///
/// * `sizes` - Number of nodes in each block.
/// * `probabilities` - B x B array that contains the connection probability between
///   nodes of different blocks. Must be symmetric for undirected graphs.
/// * `loops` - Determines whether the graph can have loops or not.
/// * `seed` - An optional seed to use for the random number generator.
/// * `default_node_weight` - A callable that will return the weight to use
///   for newly created nodes.
/// * `default_edge_weight` - A callable that will return the weight object
///   to use for newly created edges.
///
/// # Example
/// ```rust
/// use ndarray::arr2;
/// use rustworkx_core::petgraph;
/// use rustworkx_core::generators::sbm_random_graph;
///
/// let g = sbm_random_graph::<petgraph::graph::DiGraph<(), ()>, (), _, _, ()>(
///     &[1, 2],
///     &ndarray::arr2(&[[0., 1.], [0., 1.]]).view(),
///     true,
///     Some(10),
///     || (),
///     || (),
/// )
/// .unwrap();
/// assert_eq!(g.node_count(), 3);
/// assert_eq!(g.edge_count(), 6);
/// ```
pub fn sbm_random_graph<G, T, F, H, M>(
    sizes: &[usize],
    probabilities: &ndarray::ArrayView2<f64>,
    loops: bool,
    seed: Option<u64>,
    mut default_node_weight: F,
    mut default_edge_weight: H,
) -> Result<G, InvalidInputError>
where
    G: Build + Create + Data<NodeWeight = T, EdgeWeight = M> + NodeIndexable + GraphProp,
    F: FnMut() -> T,
    H: FnMut() -> M,
    G::NodeId: Eq + Hash,
{
    let num_nodes: usize = sizes.iter().sum();
    if num_nodes == 0 {
        return Err(InvalidInputError {});
    }
    let num_communities = sizes.len();
    if probabilities.nrows() != num_communities
        || probabilities.ncols() != num_communities
        || probabilities.iter().any(|&x| !(0. ..=1.).contains(&x))
    {
        return Err(InvalidInputError {});
    }

    let mut graph = G::with_capacity(num_nodes, num_nodes);
    let directed = graph.is_directed();
    if !directed && !symmetric_array(probabilities) {
        return Err(InvalidInputError {});
    }

    for _ in 0..num_nodes {
        graph.add_node(default_node_weight());
    }
    let mut rng: Pcg64 = match seed {
        Some(seed) => Pcg64::seed_from_u64(seed),
        None => Pcg64::from_os_rng(),
    };
    let mut blocks = Vec::new();
    {
        let mut block = 0;
        let mut vertices_left = sizes[0];
        for _ in 0..num_nodes {
            while vertices_left == 0 {
                block += 1;
                vertices_left = sizes[block];
            }
            blocks.push(block);
            vertices_left -= 1;
        }
    }

    let between = Uniform::new(0.0, 1.0).unwrap();
    for v in 0..(if directed || loops {
        num_nodes
    } else {
        num_nodes - 1
    }) {
        for w in ((if directed { 0 } else { v })..num_nodes).filter(|&w| w != v || loops) {
            if &between.sample(&mut rng)
                < probabilities.get((blocks[v], blocks[w])).unwrap_or(&0_f64)
            {
                graph.add_edge(
                    graph.from_index(v),
                    graph.from_index(w),
                    default_edge_weight(),
                );
            }
        }
    }
    Ok(graph)
}

fn symmetric_array<T: std::cmp::PartialEq>(mat: &ArrayView2<T>) -> bool {
    let n = mat.nrows();
    for (i, row) in mat.rows().into_iter().enumerate().take(n - 1) {
        for (j, m_ij) in row.iter().enumerate().skip(i + 1) {
            if m_ij != mat.get((j, i)).unwrap() {
                return false;
            }
        }
    }
    true
}

#[inline]
fn pnorm(x: f64, p: f64) -> f64 {
    if p == 1.0 || p == f64::INFINITY {
        x.abs()
    } else if p == 2.0 {
        x * x
    } else {
        x.abs().powf(p)
    }
}

fn distance(x: &[f64], y: &[f64], p: f64) -> f64 {
    let it = x.iter().zip(y.iter()).map(|(xi, yi)| pnorm(xi - yi, p));

    if p == f64::INFINITY {
        it.fold(-1.0, |max, x| if x > max { x } else { max })
    } else {
        it.sum()
    }
}

/// Generate a random geometric graph in the unit cube of dimensions `dim`.
///
/// The random geometric graph model places `num_nodes` nodes uniformly at
/// random in the unit cube. Two nodes are joined by an edge if the
/// distance between the nodes is at most `radius`.
///
/// Each node has a node attribute ``'pos'`` that stores the
/// position of that node in Euclidean space as provided by the
/// ``pos`` keyword argument or, if ``pos`` was not provided, as
/// generated by this function.
///
/// Arguments
///
/// * `num_nodes` - The number of nodes to create in the graph.
/// * `radius` - Distance threshold value.
/// * `dim` - Dimension of node positions. Default: 2
/// * `pos` - Optional list with node positions as values.
/// * `p` - Which Minkowski distance metric to use.  `p` has to meet the condition
///   ``1 <= p <= infinity``.
///   If this argument is not specified, the L<sup>2</sup> metric
///   (the Euclidean distance metric), `p = 2` is used.
/// * `seed` - An optional seed to use for the random number generator.
/// * `default_edge_weight` - A callable that will return the weight object
///   to use for newly created edges.
///
/// # Example
/// ```rust
/// use rustworkx_core::petgraph;
/// use rustworkx_core::generators::random_geometric_graph;
///
/// let g: petgraph::graph::UnGraph<Vec<f64>, ()> = random_geometric_graph(
///     10,
///     1.42,
///     2,
///     None,
///     2.0,
///     None,
///     || {()},
/// ).unwrap();
/// assert_eq!(g.node_count(), 10);
/// assert_eq!(g.edge_count(), 45);
/// ```
pub fn random_geometric_graph<G, H, M>(
    num_nodes: usize,
    radius: f64,
    dim: usize,
    pos: Option<Vec<Vec<f64>>>,
    p: f64,
    seed: Option<u64>,
    mut default_edge_weight: H,
) -> Result<G, InvalidInputError>
where
    G: GraphBase + Build + Create + Data<NodeWeight = Vec<f64>, EdgeWeight = M> + NodeIndexable,
    H: FnMut() -> M,
    for<'b> &'b G: GraphBase<NodeId = G::NodeId> + IntoEdgeReferences,
    G::NodeId: Eq + Hash,
{
    if num_nodes == 0 {
        return Err(InvalidInputError {});
    }
    let mut rng: Pcg64 = match seed {
        Some(seed) => Pcg64::seed_from_u64(seed),
        None => Pcg64::from_os_rng(),
    };
    let mut graph = G::with_capacity(num_nodes, num_nodes);

    let radius_p = pnorm(radius, p);
    let dist = Uniform::new(0.0, 1.0).unwrap();
    let pos = pos.unwrap_or_else(|| {
        (0..num_nodes)
            .map(|_| (0..dim).map(|_| dist.sample(&mut rng)).collect())
            .collect()
    });
    if num_nodes != pos.len() {
        return Err(InvalidInputError {});
    }
    for pval in pos.iter() {
        graph.add_node(pval.clone());
    }
    for u in 0..(num_nodes - 1) {
        for v in (u + 1)..num_nodes {
            if distance(&pos[u], &pos[v], p) < radius_p {
                graph.add_edge(
                    graph.from_index(u),
                    graph.from_index(v),
                    default_edge_weight(),
                );
            }
        }
    }
    Ok(graph)
}

/// Generate a random Barabási–Albert preferential attachment algorithm
///
/// A graph of `n` nodes is grown by attaching new nodes each with `m`
/// edges that are preferentially attached to existing nodes with high degree.
/// If the graph is directed for the purposes of the extension algorithm all
/// edges are treated as weak (meaning both incoming and outgoing).
///
/// The algorithm implemented in this function is described in:
///
/// A. L. Barabási and R. Albert "Emergence of scaling in random networks",
/// Science 286, pp 509-512, 1999.
///
/// Arguments:
///
/// * `n` - The number of nodes to extend the graph to.
/// * `m` - The number of edges to attach from a new node to existing nodes.
/// * `seed` - An optional seed to use for the random number generator.
/// * `initial_graph` - An optional starting graph to expand, if not specified
///   a star graph of `m` nodes is generated and used. If specified the input
///   graph is mutated by this function and is expected to be moved into this
///   function.
/// * `default_node_weight` - A callable that will return the weight to use
///   for newly created nodes.
/// * `default_edge_weight` - A callable that will return the weight object
///   to use for newly created edges.
///
/// An `InvalidInput` error is returned under the following conditions. If `m < 1`
/// or `m >= n` and if an `initial_graph` is specified and the number of nodes in
/// `initial_graph` is `< m` or `> n`.
///
/// # Example
/// ```rust
/// use rustworkx_core::petgraph;
/// use rustworkx_core::generators::barabasi_albert_graph;
/// use rustworkx_core::generators::star_graph;
///
/// let graph: petgraph::graph::UnGraph<(), ()> = barabasi_albert_graph(
///     20,
///     12,
///     Some(42),
///     None,
///     || {()},
///     || {()},
/// ).unwrap();
/// assert_eq!(graph.node_count(), 20);
/// assert_eq!(graph.edge_count(), 107);
/// ```
pub fn barabasi_albert_graph<G, T, F, H, M>(
    n: usize,
    m: usize,
    seed: Option<u64>,
    initial_graph: Option<G>,
    mut default_node_weight: F,
    mut default_edge_weight: H,
) -> Result<G, InvalidInputError>
where
    G: Data<NodeWeight = T, EdgeWeight = M>
        + NodeIndexable
        + GraphProp
        + NodeCount
        + Build
        + Create,
    for<'b> &'b G: GraphBase<NodeId = G::NodeId> + IntoEdgesDirected + IntoNodeIdentifiers,
    F: FnMut() -> T,
    H: FnMut() -> M,
    G::NodeId: Eq + Hash,
{
    if m < 1 || m >= n {
        return Err(InvalidInputError {});
    }
    let mut rng: Pcg64 = match seed {
        Some(seed) => Pcg64::seed_from_u64(seed),
        None => Pcg64::from_os_rng(),
    };
    let mut graph = match initial_graph {
        Some(initial_graph) => initial_graph,
        None => star_graph(
            Some(m),
            None,
            &mut default_node_weight,
            &mut default_edge_weight,
            false,
            false,
        )?,
    };
    if graph.node_count() < m || graph.node_count() > n {
        return Err(InvalidInputError {});
    }

    let mut repeated_nodes: Vec<G::NodeId> = graph
        .node_identifiers()
        .flat_map(|x| {
            let degree = graph
                .edges_directed(x, Outgoing)
                .chain(graph.edges_directed(x, Incoming))
                .count();
            std::iter::repeat(x).take(degree)
        })
        .collect();
    let mut source = graph.node_count();
    while source < n {
        let source_index = graph.add_node(default_node_weight());
        let mut targets: HashSet<G::NodeId> = HashSet::with_capacity(m);
        while targets.len() < m {
            targets.insert(*repeated_nodes.choose(&mut rng).unwrap());
        }
        for target in &targets {
            graph.add_edge(source_index, *target, default_edge_weight());
        }
        repeated_nodes.extend(targets);
        repeated_nodes.extend(vec![source_index; m]);
        source += 1
    }
    Ok(graph)
}

/// Generate a random bipartite graph.
///
/// A bipartite graph is a graph whose nodes can be divided into two disjoint sets,
/// informally called "left nodes" and "right nodes", so that every edge connects
/// some left node and some right node.
///
/// Given a number `n` of left nodes, a number `m` of right nodes, and a probability `p`,
/// the algorithm creates a graph with `n + m` nodes. For all the `n * m` possible edges,
/// each edge is created independently with probability `p`.
///
/// Arguments:
///
/// * `num_l_nodes` - The number of "left" nodes in the random bipartite graph.
/// * `num_r_nodes` - The number of "right" nodes in the random bipartite graph.
/// * `probability` - The probability of creating an edge between two nodes as a float.
/// * `seed` - An optional seed to use for the random number generator.
/// * `default_node_weight` - A callable that will return the weight to use
///   for newly created nodes.
/// * `default_edge_weight` - A callable that will return the weight object
///   to use for newly created edges.
///
/// # Example
/// ```rust
/// use rustworkx_core::petgraph;
/// use rustworkx_core::generators::random_bipartite_graph;
///
/// let g: petgraph::graph::DiGraph<(), ()> = random_bipartite_graph(
///     20,
///     20,
///     1.0,
///     None,
///     || {()},
///     || {()},
/// ).unwrap();
/// assert_eq!(g.node_count(), 20 + 20);
/// assert_eq!(g.edge_count(), 20 * 20);
/// ```
pub fn random_bipartite_graph<G, T, F, H, M>(
    num_l_nodes: usize,
    num_r_nodes: usize,
    probability: f64,
    seed: Option<u64>,
    mut default_node_weight: F,
    mut default_edge_weight: H,
) -> Result<G, InvalidInputError>
where
    G: Build + Create + Data<NodeWeight = T, EdgeWeight = M> + NodeIndexable + GraphProp,
    F: FnMut() -> T,
    H: FnMut() -> M,
    G::NodeId: Eq + Hash,
{
    if num_l_nodes == 0 && num_r_nodes == 0 {
        return Err(InvalidInputError {});
    }
    if !(0.0..=1.0).contains(&probability) {
        return Err(InvalidInputError {});
    }

    let mut rng: Pcg64 = match seed {
        Some(seed) => Pcg64::seed_from_u64(seed),
        None => Pcg64::from_os_rng(),
    };
    let mut graph = G::with_capacity(num_l_nodes + num_r_nodes, num_l_nodes + num_r_nodes);

    for _ in 0..num_l_nodes + num_r_nodes {
        graph.add_node(default_node_weight());
    }

    let between = Uniform::new(0.0, 1.0).unwrap();
    for v in 0..num_l_nodes {
        for w in 0..num_r_nodes {
            let random: f64 = between.sample(&mut rng);
            if random < probability {
                graph.add_edge(
                    graph.from_index(v),
                    graph.from_index(num_l_nodes + w),
                    default_edge_weight(),
                );
            }
        }
    }
    Ok(graph)
}

/// Generate a hyperbolic random undirected graph (also called hyperbolic geometric graph).
///
/// The hyperbolic random graph model connects pairs of nodes with a probability
/// that decreases as their hyperbolic distance increases.
///
/// The number of nodes and the dimension are inferred from the coordinates `pos` of the
/// hyperboloid model. The "time" coordinates are inferred from the others, meaning that
/// at least 2 coordinates must be provided per node. If `beta` is `None`, all pairs of
/// nodes with a distance smaller than ``r`` are connected.
///
/// Arguments:
///
/// * `pos` - Hyperboloid model coordinates of the nodes `[p_1, p_2, ...]` where `p_i` is the
///   position of node i. The "time" coordinates are inferred.
/// * `beta` - Sigmoid sharpness (nonnegative) of the connection probability.
/// * `r` - Distance at which the connection probability is 0.5 for the probabilistic model.
///   Threshold when `beta` is `None`.
/// * `seed` - An optional seed to use for the random number generator.
/// * `default_node_weight` - A callable that will return the weight to use
///   for newly created nodes.
/// * `default_edge_weight` - A callable that will return the weight object
///   to use for newly created edges.
///
/// # Example
/// ```rust
/// use rustworkx_core::petgraph;
/// use rustworkx_core::generators::hyperbolic_random_graph;
///
/// let g: petgraph::graph::UnGraph<(), ()> = hyperbolic_random_graph(
///     &[vec![3_f64.sinh(), 0.],
///       vec![-0.5_f64.sinh(), 0.],
///       vec![-1_f64.sinh(), 0.]],
///     2.,
///     None,
///     None,
///     || {()},
///     || {()},
/// ).unwrap();
/// assert_eq!(g.node_count(), 3);
/// assert_eq!(g.edge_count(), 1);
/// ```
pub fn hyperbolic_random_graph<G, T, F, H, M>(
    pos: &[Vec<f64>],
    r: f64,
    beta: Option<f64>,
    seed: Option<u64>,
    mut default_node_weight: F,
    mut default_edge_weight: H,
) -> Result<G, InvalidInputError>
where
    G: Build + Create + Data<NodeWeight = T, EdgeWeight = M> + NodeIndexable + GraphProp,
    F: FnMut() -> T,
    H: FnMut() -> M,
    G::NodeId: Eq + Hash,
{
    let num_nodes = pos.len();
    if num_nodes == 0 {
        return Err(InvalidInputError {});
    }
    if pos
        .iter()
        .any(|xs| xs.iter().any(|x| x.is_nan() || x.is_infinite()))
    {
        return Err(InvalidInputError {});
    }
    let dim = pos[0].len();
    if dim < 2 || pos.iter().any(|x| x.len() != dim) {
        return Err(InvalidInputError {});
    }
    if beta.is_some_and(|b| b < 0. || b.is_nan()) {
        return Err(InvalidInputError {});
    }
    if r < 0. || r.is_nan() {
        return Err(InvalidInputError {});
    }

    let mut rng: Pcg64 = match seed {
        Some(seed) => Pcg64::seed_from_u64(seed),
        None => Pcg64::from_os_rng(),
    };
    let mut graph = G::with_capacity(num_nodes, num_nodes);
    if graph.is_directed() {
        return Err(InvalidInputError {});
    }

    for _ in 0..num_nodes {
        graph.add_node(default_node_weight());
    }

    let between = Uniform::new(0.0, 1.0).unwrap();
    for (v, p1) in pos.iter().enumerate().take(num_nodes - 1) {
        for (w, p2) in pos.iter().enumerate().skip(v + 1) {
            let dist = hyperbolic_distance(p1, p2);
            let is_edge = match beta {
                Some(b) => {
                    let prob_inverse = (b / 2. * (dist - r)).exp() + 1.;
                    let u: f64 = between.sample(&mut rng);
                    prob_inverse * u < 1.
                }
                None => dist < r,
            };
            if is_edge {
                graph.add_edge(
                    graph.from_index(v),
                    graph.from_index(w),
                    default_edge_weight(),
                );
            }
        }
    }
    Ok(graph)
}

#[inline]
fn hyperbolic_distance(x: &[f64], y: &[f64]) -> f64 {
    let mut sum_squared_x = 0.;
    let mut sum_squared_y = 0.;
    let mut inner_product = 0.;
    for (x_i, y_i) in x.iter().zip(y.iter()) {
        if x_i.is_infinite() || y_i.is_infinite() || x_i.is_nan() || y_i.is_nan() {
            return f64::NAN;
        }
        sum_squared_x = x_i.mul_add(*x_i, sum_squared_x);
        sum_squared_y = y_i.mul_add(*y_i, sum_squared_y);
        inner_product = x_i.mul_add(*y_i, inner_product);
    }
    let arg = (1. + sum_squared_x).sqrt() * (1. + sum_squared_y).sqrt() - inner_product;
    if arg < 1. {
        0.
    } else {
        arg.acosh()
    }
}

#[cfg(test)]
mod tests {
    use crate::generators::InvalidInputError;
    use crate::generators::{
        barabasi_albert_graph, gnm_random_graph, gnp_random_graph, hyperbolic_random_graph,
        path_graph, random_bipartite_graph, random_geometric_graph, random_regular_graph,
        sbm_random_graph,
    };
    use crate::petgraph;

    use super::hyperbolic_distance;

    // Test gnp_random_graph

    #[test]
    fn test_gnp_random_graph_directed() {
        let g: petgraph::graph::DiGraph<(), ()> =
            gnp_random_graph(20, 0.5, Some(10), || (), || ()).unwrap();
        assert_eq!(g.node_count(), 20);
        assert_eq!(g.edge_count(), 189);
    }

    #[test]
    fn test_random_regular_graph() {
        let g: petgraph::graph::UnGraph<(), ()> =
            random_regular_graph(4, 2, Some(10), || (), || ()).unwrap();
        assert_eq!(g.node_count(), 4);
        assert_eq!(g.edge_count(), 4);
        for node in g.node_indices() {
            assert_eq!(g.edges(node).count(), 2)
        }
    }

    #[test]
    fn test_gnp_random_graph_directed_empty() {
        let g: petgraph::graph::DiGraph<(), ()> =
            gnp_random_graph(20, 0.0, None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), 20);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_gnp_random_graph_directed_complete() {
        let g: petgraph::graph::DiGraph<(), ()> =
            gnp_random_graph(20, 1.0, None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), 20);
        assert_eq!(g.edge_count(), 20 * (20 - 1));
    }

    #[test]
    fn test_gnp_random_graph_undirected() {
        let g: petgraph::graph::UnGraph<(), ()> =
            gnp_random_graph(20, 0.5, Some(10), || (), || ()).unwrap();
        assert_eq!(g.node_count(), 20);
        assert_eq!(g.edge_count(), 105);
    }

    #[test]
    fn test_gnp_random_graph_undirected_empty() {
        let g: petgraph::graph::UnGraph<(), ()> =
            gnp_random_graph(20, 0.0, None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), 20);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_gnp_random_graph_undirected_complete() {
        let g: petgraph::graph::UnGraph<(), ()> =
            gnp_random_graph(20, 1.0, None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), 20);
        assert_eq!(g.edge_count(), 20 * (20 - 1) / 2);
    }

    #[test]
    fn test_gnp_random_graph_error() {
        match gnp_random_graph::<petgraph::graph::DiGraph<(), ()>, (), _, _, ()>(
            0,
            3.0,
            None,
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        };
    }

    // Test gnm_random_graph

    #[test]
    fn test_gnm_random_graph_directed() {
        let g: petgraph::graph::DiGraph<(), ()> =
            gnm_random_graph(20, 100, None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), 20);
        assert_eq!(g.edge_count(), 100);
    }

    #[test]
    fn test_gnm_random_graph_directed_empty() {
        let g: petgraph::graph::DiGraph<(), ()> =
            gnm_random_graph(20, 0, None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), 20);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_gnm_random_graph_directed_complete() {
        let g: petgraph::graph::DiGraph<(), ()> =
            gnm_random_graph(20, 20 * (20 - 1), None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), 20);
        assert_eq!(g.edge_count(), 20 * (20 - 1));
    }

    #[test]
    fn test_gnm_random_graph_directed_max_edges() {
        let n = 20;
        let max_m = n * (n - 1);
        let g: petgraph::graph::DiGraph<(), ()> =
            gnm_random_graph(n, max_m, None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), n);
        assert_eq!(g.edge_count(), max_m);
        // passing the max edges for the passed number of nodes
        let g: petgraph::graph::DiGraph<(), ()> =
            gnm_random_graph(n, max_m + 1, None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), n);
        assert_eq!(g.edge_count(), max_m);
        // passing a seed when passing max edges has no effect
        let g: petgraph::graph::DiGraph<(), ()> =
            gnm_random_graph(n, max_m, Some(55), || (), || ()).unwrap();
        assert_eq!(g.node_count(), n);
        assert_eq!(g.edge_count(), max_m);
    }

    #[test]
    fn test_gnm_random_graph_error() {
        match gnm_random_graph::<petgraph::graph::DiGraph<(), ()>, (), _, _, ()>(
            0,
            0,
            None,
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        };
    }

    // Test sbm_random_graph
    #[test]
    fn test_sbm_directed_complete_blocks_loops() {
        let g = sbm_random_graph::<petgraph::graph::DiGraph<(), ()>, (), _, _, ()>(
            &[1, 2],
            &ndarray::arr2(&[[0., 1.], [0., 1.]]).view(),
            true,
            Some(10),
            || (),
            || (),
        )
        .unwrap();
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 6);
        for (u, v) in [(1, 1), (1, 2), (2, 1), (2, 2), (0, 1), (0, 2)] {
            assert!(g.contains_edge(u.into(), v.into()));
        }
        assert!(!g.contains_edge(1.into(), 0.into()));
        assert!(!g.contains_edge(2.into(), 0.into()));
    }

    #[test]
    fn test_sbm_undirected_complete_blocks_loops() {
        let g = sbm_random_graph::<petgraph::graph::UnGraph<(), ()>, (), _, _, ()>(
            &[1, 2],
            &ndarray::arr2(&[[0., 1.], [1., 1.]]).view(),
            true,
            Some(10),
            || (),
            || (),
        )
        .unwrap();
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 5);
        for (u, v) in [(1, 1), (1, 2), (2, 2), (0, 1), (0, 2)] {
            assert!(g.contains_edge(u.into(), v.into()));
        }
        assert!(!g.contains_edge(0.into(), 0.into()));
    }

    #[test]
    fn test_sbm_directed_complete_blocks_noloops() {
        let g = sbm_random_graph::<petgraph::graph::DiGraph<(), ()>, (), _, _, ()>(
            &[1, 2],
            &ndarray::arr2(&[[0., 1.], [0., 1.]]).view(),
            false,
            Some(10),
            || (),
            || (),
        )
        .unwrap();
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 4);
        for (u, v) in [(1, 2), (2, 1), (0, 1), (0, 2)] {
            assert!(g.contains_edge(u.into(), v.into()));
        }
        assert!(!g.contains_edge(1.into(), 0.into()));
        assert!(!g.contains_edge(2.into(), 0.into()));
        for u in 0..2 {
            assert!(!g.contains_edge(u.into(), u.into()));
        }
    }

    #[test]
    fn test_sbm_undirected_complete_blocks_noloops() {
        let g = sbm_random_graph::<petgraph::graph::UnGraph<(), ()>, (), _, _, ()>(
            &[1, 2],
            &ndarray::arr2(&[[0., 1.], [1., 1.]]).view(),
            false,
            Some(10),
            || (),
            || (),
        )
        .unwrap();
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 3);
        for (u, v) in [(1, 2), (0, 1), (0, 2)] {
            assert!(g.contains_edge(u.into(), v.into()));
        }
        for u in 0..2 {
            assert!(!g.contains_edge(u.into(), u.into()));
        }
    }

    #[test]
    fn test_sbm_bad_array_rows_error() {
        match sbm_random_graph::<petgraph::graph::DiGraph<(), ()>, (), _, _, ()>(
            &[1, 2],
            &ndarray::arr2(&[[0., 1.], [1., 1.], [1., 1.]]).view(),
            true,
            Some(10),
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        };
    }
    #[test]

    fn test_sbm_bad_array_cols_error() {
        match sbm_random_graph::<petgraph::graph::DiGraph<(), ()>, (), _, _, ()>(
            &[1, 2],
            &ndarray::arr2(&[[0., 1., 1.], [1., 1., 1.]]).view(),
            true,
            Some(10),
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        };
    }

    #[test]
    fn test_sbm_asymmetric_array_error() {
        match sbm_random_graph::<petgraph::graph::UnGraph<(), ()>, (), _, _, ()>(
            &[1, 2],
            &ndarray::arr2(&[[0., 1.], [0., 1.]]).view(),
            true,
            Some(10),
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        };
    }

    #[test]
    fn test_sbm_invalid_probability_error() {
        match sbm_random_graph::<petgraph::graph::UnGraph<(), ()>, (), _, _, ()>(
            &[1, 2],
            &ndarray::arr2(&[[0., 1.], [0., -1.]]).view(),
            true,
            Some(10),
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        };
    }

    #[test]
    fn test_sbm_empty_error() {
        match sbm_random_graph::<petgraph::graph::DiGraph<(), ()>, (), _, _, ()>(
            &[],
            &ndarray::arr2(&[[]]).view(),
            true,
            Some(10),
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        };
    }

    // Test random_geometric_graph

    #[test]
    fn test_random_geometric_empty() {
        let g: petgraph::graph::UnGraph<Vec<f64>, ()> =
            random_geometric_graph(20, 0.0, 2, None, 2.0, None, || ()).unwrap();
        assert_eq!(g.node_count(), 20);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_random_geometric_complete() {
        let g: petgraph::graph::UnGraph<Vec<f64>, ()> =
            random_geometric_graph(10, 1.42, 2, None, 2.0, None, || ()).unwrap();
        assert_eq!(g.node_count(), 10);
        assert_eq!(g.edge_count(), 45);
    }

    #[test]
    fn test_random_geometric_bad_num_nodes() {
        match random_geometric_graph::<petgraph::graph::UnGraph<Vec<f64>, ()>, _, ()>(
            0,
            1.0,
            2,
            None,
            2.0,
            None,
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        };
    }

    #[test]
    fn test_random_geometric_bad_pos() {
        match random_geometric_graph::<petgraph::graph::UnGraph<Vec<f64>, ()>, _, ()>(
            3,
            0.15,
            3,
            Some(vec![vec![0.5, 0.5]]),
            2.0,
            None,
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        };
    }

    #[test]
    fn test_barabasi_albert_graph_starting_graph() {
        let starting_graph: petgraph::graph::UnGraph<(), ()> =
            path_graph(Some(40), None, || (), || (), false).unwrap();
        let graph =
            barabasi_albert_graph(500, 40, None, Some(starting_graph), || (), || ()).unwrap();
        assert_eq!(graph.node_count(), 500);
        assert_eq!(graph.edge_count(), 18439);
    }

    #[test]
    fn test_barabasi_albert_graph_invalid_starting_size() {
        match barabasi_albert_graph(
            5,
            40,
            None,
            None::<petgraph::graph::UnGraph<(), ()>>,
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        }
    }

    #[test]
    fn test_barabasi_albert_graph_invalid_equal_starting_size() {
        match barabasi_albert_graph(
            5,
            5,
            None,
            None::<petgraph::graph::UnGraph<(), ()>>,
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        }
    }

    #[test]
    fn test_barabasi_albert_graph_invalid_starting_graph() {
        let starting_graph: petgraph::graph::UnGraph<(), ()> =
            path_graph(Some(4), None, || (), || (), false).unwrap();
        match barabasi_albert_graph(500, 40, None, Some(starting_graph), || (), || ()) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        }
    }

    // Test random_bipartite_graph

    #[test]
    fn test_random_bipartite_graph_directed() {
        let g: petgraph::graph::DiGraph<(), ()> =
            random_bipartite_graph(10, 10, 0.5, Some(10), || (), || ()).unwrap();
        assert_eq!(g.node_count(), 20);
        assert_eq!(g.edge_count(), 57);
    }

    #[test]
    fn test_random_bipartite_graph_directed_empty() {
        let g: petgraph::graph::DiGraph<(), ()> =
            random_bipartite_graph(5, 10, 0.0, None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), 15);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_random_bipartite_graph_directed_complete() {
        let g: petgraph::graph::DiGraph<(), ()> =
            random_bipartite_graph(10, 5, 1.0, None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), 15);
        assert_eq!(g.edge_count(), 10 * 5);
    }

    #[test]
    fn test_random_bipartite_graph_undirected() {
        let g: petgraph::graph::UnGraph<(), ()> =
            random_bipartite_graph(10, 10, 0.5, Some(10), || (), || ()).unwrap();
        assert_eq!(g.node_count(), 20);
        assert_eq!(g.edge_count(), 57);
    }

    #[test]
    fn test_random_bipartite_graph_undirected_empty() {
        let g: petgraph::graph::UnGraph<(), ()> =
            random_bipartite_graph(5, 10, 0.0, None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), 15);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_random_bipartite_graph_undirected_complete() {
        let g: petgraph::graph::UnGraph<(), ()> =
            random_bipartite_graph(10, 5, 1.0, None, || (), || ()).unwrap();
        assert_eq!(g.node_count(), 15);
        assert_eq!(g.edge_count(), 10 * 5);
    }

    #[test]
    fn test_random_bipartite_graph_error() {
        match random_bipartite_graph::<petgraph::graph::DiGraph<(), ()>, (), _, _, ()>(
            0,
            0,
            3.0,
            None,
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        };
    }

    // Test hyperbolic_random_graph
    //
    // Hyperboloid (H^2) "polar" coordinates (r, theta) are transformed to "cartesian"
    // coordinates using
    // z = cosh(r)
    // x = sinh(r)cos(theta)
    // y = sinh(r)sin(theta)

    #[test]
    fn test_hyperbolic_dist() {
        assert_eq!(
            hyperbolic_distance(&[3_f64.sinh(), 0.], &[-0.5_f64.sinh(), 0.]),
            3.5
        );
    }
    #[test]
    fn test_hyperbolic_dist_inf() {
        assert!(hyperbolic_distance(&[f64::INFINITY, 0.], &[0., 0.]).is_nan());
    }

    #[test]
    fn test_hyperbolic_random_graph_seeded() {
        let g = hyperbolic_random_graph::<petgraph::graph::UnGraph<(), ()>, _, _, _, _>(
            &[
                vec![3_f64.sinh(), 0.],
                vec![-0.5_f64.sinh(), 0.],
                vec![0.5_f64.sinh(), 0.],
                vec![0., 0.],
            ],
            0.75,
            Some(10000.),
            Some(10),
            || (),
            || (),
        )
        .unwrap();
        assert_eq!(g.node_count(), 4);
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn test_hyperbolic_random_graph_threshold() {
        let g = hyperbolic_random_graph::<petgraph::graph::UnGraph<(), ()>, _, _, _, _>(
            &[
                vec![3_f64.sinh(), 0.],
                vec![-0.5_f64.sinh(), 0.],
                vec![-1_f64.sinh(), 0.],
            ],
            1.,
            None,
            None,
            || (),
            || (),
        )
        .unwrap();
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn test_hyperbolic_random_graph_invalid_dim_error() {
        match hyperbolic_random_graph::<petgraph::graph::UnGraph<(), ()>, _, _, _, _>(
            &[vec![0.]],
            1.,
            None,
            None,
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        }
    }

    #[test]
    fn test_hyperbolic_random_graph_neg_r_error() {
        match hyperbolic_random_graph::<petgraph::graph::UnGraph<(), ()>, _, _, _, _>(
            &[vec![0., 0.], vec![0., 0.]],
            -1.,
            None,
            None,
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        }
    }

    #[test]
    fn test_hyperbolic_random_graph_neg_beta_error() {
        match hyperbolic_random_graph::<petgraph::graph::UnGraph<(), ()>, _, _, _, _>(
            &[vec![0., 0.], vec![0., 0.]],
            1.,
            Some(-1.),
            None,
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        }
    }

    #[test]
    fn test_hyperbolic_random_graph_diff_dims_error() {
        match hyperbolic_random_graph::<petgraph::graph::UnGraph<(), ()>, _, _, _, _>(
            &[vec![0., 0.], vec![0., 0., 0.]],
            1.,
            None,
            None,
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        }
    }

    #[test]
    fn test_hyperbolic_random_graph_empty_error() {
        match hyperbolic_random_graph::<petgraph::graph::UnGraph<(), ()>, _, _, _, _>(
            &[],
            1.,
            None,
            None,
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        }
    }

    #[test]
    fn test_hyperbolic_random_graph_directed_error() {
        match hyperbolic_random_graph::<petgraph::graph::DiGraph<(), ()>, _, _, _, _>(
            &[vec![0., 0.], vec![0., 0.]],
            1.,
            None,
            None,
            || (),
            || (),
        ) {
            Ok(_) => panic!("Returned a non-error"),
            Err(e) => assert_eq!(e, InvalidInputError),
        }
    }
}
