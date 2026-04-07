use anyhow::Result;
use petgraph::algo::dijkstra;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

use crate::models::MemoryId;

/// Edge type for graph relationships
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeType {
    /// Explicit [[...]] wiki link reference
    WikiLink,
    /// High vector similarity (> 0.85)
    Semantic,
    /// Created within time window
    Temporal,
    /// Cites same raw source
    SourceReference,
    /// Hierarchical relationship
    ParentChild,
    /// User-created manual link
    Manual,
}

impl EdgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeType::WikiLink => "wikilink",
            EdgeType::Semantic => "semantic",
            EdgeType::Temporal => "temporal",
            EdgeType::SourceReference => "source",
            EdgeType::ParentChild => "parent_child",
            EdgeType::Manual => "manual",
        }
    }
}

/// Graph edge representing a relationship between memories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source_id: MemoryId,
    pub target_id: MemoryId,
    pub edge_type: EdgeType,
    /// Relationship strength (0.0 - 1.0)
    pub weight: f32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Context (e.g., link text, similarity score explanation)
    pub context: String,
}

/// Graph storage for memory relationships
pub struct GraphStorage {
    /// In-memory graph representation
    graph: Arc<RwLock<DiGraph<String, Edge>>>,
    /// Map from memory_id to node index
    node_map: Arc<RwLock<HashMap<String, NodeIndex>>>,
}

impl GraphStorage {
    pub fn new() -> Self {
        Self {
            graph: Arc::new(RwLock::new(DiGraph::new())),
            node_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add or get a node for a memory
    fn get_or_create_node(&self, memory_id: &str) -> NodeIndex {
        let mut node_map = self.node_map.write().unwrap();
        let mut graph = self.graph.write().unwrap();

        if let Some(&idx) = node_map.get(memory_id) {
            idx
        } else {
            let idx = graph.add_node(memory_id.to_string());
            node_map.insert(memory_id.to_string(), idx);
            idx
        }
    }

    /// Add an edge between two memories
    pub fn add_edge(&self, edge: Edge) -> Result<()> {
        let source_idx = self.get_or_create_node(&edge.source_id);
        let target_idx = self.get_or_create_node(&edge.target_id);

        let mut graph = self.graph.write().unwrap();

        // Check if edge already exists
        let exists = graph
            .edges_connecting(source_idx, target_idx)
            .any(|e| e.weight().edge_type == edge.edge_type);

        if !exists {
            graph.add_edge(source_idx, target_idx, edge.clone());
            debug!(
                "Added edge {} -> {} ({:?})",
                edge.source_id, edge.target_id, edge.edge_type
            );
        }

        Ok(())
    }

    /// Remove an edge
    pub fn remove_edge(
        &self,
        source_id: &str,
        target_id: &str,
        edge_type: EdgeType,
    ) -> Result<bool> {
        let node_map = self.node_map.read().unwrap();
        let mut graph = self.graph.write().unwrap();

        if let (Some(&source_idx), Some(&target_idx)) =
            (node_map.get(source_id), node_map.get(target_id))
        {
            // Find the specific edge to remove
            let mut edge_to_remove = None;
            for edge_idx in graph.edges_connecting(source_idx, target_idx) {
                if edge_idx.weight().edge_type == edge_type {
                    edge_to_remove = Some(edge_idx);
                    break;
                }
            }

            if let Some(edge_ref) = edge_to_remove {
                // Get the edge index from the reference
                let idx = graph.edge_indices().find(|&e| {
                    let endpoints = graph.edge_endpoints(e);
                    if let Some((s, t)) = endpoints {
                        s == source_idx
                            && t == target_idx
                            && graph
                                .edge_weight(e)
                                .map(|w| w.edge_type == edge_type)
                                .unwrap_or(false)
                    } else {
                        false
                    }
                });

                if let Some(edge_idx) = idx {
                    graph.remove_edge(edge_idx);
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Get related memories (direct neighbors)
    pub fn get_related(
        &self,
        memory_id: &str,
        edge_type_filter: Option<EdgeType>,
    ) -> Result<Vec<RelatedMemory>> {
        let node_map = self.node_map.read().unwrap();
        let graph = self.graph.read().unwrap();

        let source_idx = match node_map.get(memory_id) {
            Some(&idx) => idx,
            None => return Ok(vec![]),
        };

        let mut related = Vec::new();

        // Outgoing edges - iterate using neighbor edges
        for (target_idx, edge_weight) in graph.neighbors(source_idx).filter_map(|n| {
            graph
                .edges_connecting(source_idx, n)
                .next()
                .map(|e| (n, e.weight()))
        }) {
            if let Some(filter) = edge_type_filter {
                if edge_weight.edge_type != filter {
                    continue;
                }
            }

            if let Some(target_id) = graph.node_weight(target_idx) {
                related.push(RelatedMemory {
                    id: target_id.clone(),
                    edge_type: edge_weight.edge_type,
                    weight: edge_weight.weight,
                    direction: Direction::Outbound,
                    context: edge_weight.context.clone(),
                });
            }
        }

        // Incoming edges
        for node_idx in graph.node_indices() {
            if let Some(edge_ref) = graph.edges_connecting(node_idx, source_idx).next() {
                let edge = edge_ref.weight();

                if let Some(filter) = edge_type_filter {
                    if edge.edge_type != filter {
                        continue;
                    }
                }

                if let Some(source_id) = graph.node_weight(node_idx) {
                    related.push(RelatedMemory {
                        id: source_id.clone(),
                        edge_type: edge.edge_type,
                        weight: edge.weight,
                        direction: Direction::Inbound,
                        context: edge.context.clone(),
                    });
                }
            }
        }

        Ok(related)
    }

    /// Find shortest path between two memories
    pub fn find_path(&self, source_id: &str, target_id: &str) -> Result<Option<PathResult>> {
        let node_map = self.node_map.read().unwrap();
        let graph = self.graph.read().unwrap();

        let source_idx = match node_map.get(source_id) {
            Some(&idx) => idx,
            None => return Ok(None),
        };

        let target_idx = match node_map.get(target_id) {
            Some(&idx) => idx,
            None => return Ok(None),
        };

        // Use Dijkstra's algorithm
        let shortest_paths = dijkstra(&*graph, source_idx, Some(target_idx), |e| {
            // Cost is inverse of weight (lower weight = higher cost)
            (1.0 - e.weight().weight + 0.01) as f64
        });

        if let Some(&cost) = shortest_paths.get(&target_idx) {
            // Reconstruct path (simplified - actual path reconstruction would need parent tracking)
            let hops = cost as usize + 1; // Approximation

            return Ok(Some(PathResult {
                nodes: vec![source_id.to_string(), target_id.to_string()],
                total_weight: 1.0 / (cost + 1.0) as f32,
                hops,
            }));
        }

        Ok(None)
    }

    /// Get graph statistics
    pub fn get_stats(&self) -> GraphStats {
        let graph = self.graph.read().unwrap();
        let node_map = self.node_map.read().unwrap();

        let node_count = graph.node_count();
        let edge_count = graph.edge_count();

        // Calculate average degree
        let avg_degree = if node_count > 0 {
            (edge_count as f32 * 2.0) / node_count as f32
        } else {
            0.0
        };

        // Count orphan nodes (no edges)
        let mut orphan_count = 0;
        for node_idx in graph.node_indices() {
            let has_outbound = graph.edges(node_idx).next().is_some();
            let has_inbound = graph
                .edges_directed(node_idx, petgraph::Direction::Incoming)
                .next()
                .is_some();

            if !has_outbound && !has_inbound {
                orphan_count += 1;
            }
        }

        GraphStats {
            node_count,
            edge_count,
            orphan_count,
            avg_degree,
        }
    }

    /// Detect orphan nodes (no inbound or outbound edges)
    pub fn find_orphans(&self) -> Vec<String> {
        let graph = self.graph.read().unwrap();
        let mut orphans = Vec::new();

        for node_idx in graph.node_indices() {
            let has_outbound = graph.edges(node_idx).next().is_some();
            let has_inbound = graph
                .edges_directed(node_idx, petgraph::Direction::Incoming)
                .next()
                .is_some();

            if !has_outbound && !has_inbound {
                if let Some(id) = graph.node_weight(node_idx) {
                    orphans.push(id.clone());
                }
            }
        }

        orphans
    }

    /// Suggest missing links based on semantic similarity (placeholder)
    pub fn suggest_links(&self, _memory_id: &str) -> Result<Vec<SuggestedLink>> {
        // This would integrate with vector search to find similar memories
        // and suggest connections that don't exist yet
        Ok(vec![])
    }

    /// Clear all edges
    pub fn clear(&self) {
        let mut graph = self.graph.write().unwrap();
        let mut node_map = self.node_map.write().unwrap();
        graph.clear();
        node_map.clear();
    }

    /// Load graph from stored edges
    pub fn load_from_edges(&self, edges: Vec<Edge>) -> Result<()> {
        self.clear();

        for edge in edges {
            self.add_edge(edge)?;
        }

        info!(
            "Loaded graph with {} edges",
            self.graph.read().unwrap().edge_count()
        );
        Ok(())
    }

    /// Export all edges
    pub fn export_edges(&self) -> Vec<Edge> {
        let graph = self.graph.read().unwrap();
        graph.edge_weights().cloned().collect()
    }
}

impl Default for GraphStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// Direction of relationship
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Direction {
    Outbound,
    Inbound,
    Bidirectional,
}

/// Related memory info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedMemory {
    pub id: MemoryId,
    pub edge_type: EdgeType,
    pub weight: f32,
    pub direction: Direction,
    pub context: String,
}

/// Path result between two memories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathResult {
    pub nodes: Vec<String>,
    pub total_weight: f32,
    pub hops: usize,
}

/// Suggested link
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedLink {
    pub target_id: MemoryId,
    pub reason: String,
    pub confidence: f32,
    pub suggested_edge_type: EdgeType,
}

/// Graph statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub orphan_count: usize,
    pub avg_degree: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_edge(source: &str, target: &str, edge_type: EdgeType) -> Edge {
        Edge {
            id: format!("edge_{}_{}", source, target),
            source_id: source.to_string(),
            target_id: target.to_string(),
            edge_type,
            weight: 1.0,
            created_at: Utc::now(),
            context: "test".to_string(),
        }
    }

    #[test]
    fn test_add_and_get_related() {
        let storage = GraphStorage::new();

        let edge = create_test_edge("mem1", "mem2", EdgeType::WikiLink);
        storage.add_edge(edge).unwrap();

        let related = storage.get_related("mem1", None).unwrap();
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].id, "mem2");
    }

    #[test]
    fn test_find_orphans() {
        let storage = GraphStorage::new();

        // Add an edge between mem1 and mem2
        let edge = create_test_edge("mem1", "mem2", EdgeType::WikiLink);
        storage.add_edge(edge).unwrap();

        // Add isolated node mem3
        storage.get_or_create_node("mem3");

        let orphans = storage.find_orphans();
        assert!(orphans.contains(&"mem3".to_string()));
        assert!(!orphans.contains(&"mem1".to_string()));
        assert!(!orphans.contains(&"mem2".to_string()));
    }

    #[test]
    fn test_graph_stats() {
        let storage = GraphStorage::new();

        let edge = create_test_edge("mem1", "mem2", EdgeType::WikiLink);
        storage.add_edge(edge).unwrap();

        let stats = storage.get_stats();
        assert_eq!(stats.node_count, 2);
        assert_eq!(stats.edge_count, 1);
        assert_eq!(stats.orphan_count, 0);
    }
}
