//! Skill dependency graph — petgraph DAG + topological sort + cycle detection
//!
//! Manages skill dependencies and execution order using petgraph.

use std::collections::{HashMap, HashSet};

use petgraph::Direction;
use petgraph::algo::{tarjan_scc, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use tracing::{debug, warn};

use wakey_types::{WakeyError, WakeyResult};

use crate::format::SkillManifest;

/// Skill node in the DAG
#[derive(Debug, Clone)]
pub struct SkillNode {
    /// Skill name
    pub name: String,

    /// Skill manifest (L0)
    pub manifest: SkillManifest,
}

/// Dependency edge in the DAG
#[derive(Debug, Clone, Copy)]
pub struct DependencyEdge {
    /// Source depends on target
    pub source_idx: NodeIndex,
    pub target_idx: NodeIndex,
}

/// Skill dependency graph using petgraph
pub struct SkillDag {
    /// Directed graph of skill dependencies
    graph: DiGraph<SkillNode, DependencyEdge>,

    /// Name to node index mapping
    name_to_idx: HashMap<String, NodeIndex>,
}

impl SkillDag {
    /// Build a skill DAG from a list of skill manifests
    ///
    /// Creates nodes for all skills and edges for dependencies.
    /// Skills with unknown dependencies get nodes created but are marked as broken.
    ///
    /// # Arguments
    /// * `skills` - List of skill manifests with dependencies
    ///
    /// # Returns
    /// SkillDag with all skills and dependency edges
    pub fn build(skills: &[SkillManifest]) -> Self {
        let mut graph = DiGraph::new();
        let mut name_to_idx = HashMap::new();

        // First pass: create all nodes
        for skill in skills {
            let idx = graph.add_node(SkillNode {
                name: skill.name.clone(),
                manifest: skill.clone(),
            });
            name_to_idx.insert(skill.name.clone(), idx);
        }

        // Second pass: add dependency edges
        for skill in skills {
            let source_idx = name_to_idx.get(&skill.name).expect("Node should exist");

            for dep_name in &skill.dependencies {
                if let Some(target_idx) = name_to_idx.get(dep_name) {
                    // Add edge: dep -> skill (dependency points to dependent)
                    // This ensures toposort puts dependencies first
                    graph.add_edge(
                        *target_idx, // dependency
                        *source_idx, // dependent
                        DependencyEdge {
                            source_idx: *target_idx,
                            target_idx: *source_idx,
                        },
                    );
                    debug!(skill = %skill.name, depends_on = %dep_name, "Added dependency edge");
                } else {
                    warn!(skill = %skill.name, missing_dep = %dep_name, "Missing dependency");
                }
            }
        }

        debug!(
            nodes = graph.node_count(),
            edges = graph.edge_count(),
            "Built skill DAG"
        );

        SkillDag { graph, name_to_idx }
    }

    /// Resolve execution order for a skill and its dependencies
    ///
    /// Returns skills in topological order (dependencies first).
    /// If the skill or any dependency is part of a cycle, returns error.
    ///
    /// # Arguments
    /// * `skill_name` - Root skill to resolve dependencies for
    ///
    /// # Returns
    /// Vec of skill names in execution order (dependencies first)
    pub fn resolve_order(&self, skill_name: &str) -> WakeyResult<Vec<String>> {
        let root_idx = self
            .name_to_idx
            .get(skill_name)
            .ok_or_else(|| WakeyError::Skill {
                skill: skill_name.into(),
                message: "Skill not found in DAG".into(),
            })?;

        // Find all reachable nodes (dependencies)
        let mut reachable = HashSet::new();
        self.collect_dependencies(*root_idx, &mut reachable);

        // Extract subgraph for toposort
        let _subgraph_indices: Vec<NodeIndex> = reachable.iter().cloned().collect();

        // Map to check if index is in subgraph
        let in_subgraph: HashSet<NodeIndex> = reachable.iter().cloned().collect();

        // Toposort the full graph and filter to subgraph
        match toposort(&self.graph, None) {
            Ok(sorted) => {
                let order: Vec<String> = sorted
                    .into_iter()
                    .filter(|idx| in_subgraph.contains(idx))
                    .map(|idx| self.graph[idx].name.clone())
                    .collect();

                debug!(skill = %skill_name, order = ?order, "Resolved dependency order");
                Ok(order)
            }
            Err(cycle) => Err(WakeyError::Skill {
                skill: skill_name.into(),
                message: format!("Dependency cycle detected at node {:?}", cycle.node_id()),
            }),
        }
    }

    /// Recursively collect all dependencies of a node
    fn collect_dependencies(&self, node: NodeIndex, visited: &mut HashSet<NodeIndex>) {
        if visited.contains(&node) {
            return;
        }
        visited.insert(node);

        // Walk incoming edges (dependencies point TO this node)
        for neighbor in self.graph.neighbors_directed(node, Direction::Incoming) {
            self.collect_dependencies(neighbor, visited);
        }
    }

    /// Detect cycles in the dependency graph
    ///
    /// Uses Tarjan's SCC algorithm to find strongly connected components.
    /// Any SCC with more than one node indicates a cycle.
    ///
    /// # Returns
    /// Vec of cycles, where each cycle is a list of skill names
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        let sccs = tarjan_scc(&self.graph);

        let cycles: Vec<Vec<String>> = sccs
            .into_iter()
            .filter(|scc| scc.len() > 1)
            .map(|scc| {
                scc.into_iter()
                    .map(|idx| self.graph[idx].name.clone())
                    .collect()
            })
            .collect();

        if !cycles.is_empty() {
            warn!(count = cycles.len(), "Detected cycles in skill DAG");
        }

        cycles
    }

    /// Find skills with broken dependencies
    ///
    /// Returns skills that reference dependencies not in the DAG.
    ///
    /// # Returns
    /// Vec of (skill_name, missing_dependency) pairs
    pub fn find_orphans(&self) -> Vec<(String, String)> {
        let mut orphans = Vec::new();

        for node in self.graph.node_indices() {
            let skill = &self.graph[node];

            for dep_name in &skill.manifest.dependencies {
                if !self.name_to_idx.contains_key(dep_name) {
                    orphans.push((skill.name.clone(), dep_name.clone()));
                }
            }
        }

        if !orphans.is_empty() {
            warn!(
                count = orphans.len(),
                "Found skills with missing dependencies"
            );
        }

        orphans
    }

    /// Get a skill node by name
    pub fn get(&self, name: &str) -> Option<&SkillNode> {
        self.name_to_idx.get(name).map(|idx| &self.graph[*idx])
    }

    /// Get all skill names in the DAG
    pub fn skill_names(&self) -> Vec<String> {
        self.name_to_idx.keys().cloned().collect()
    }

    /// Check if a skill exists in the DAG
    pub fn contains(&self, name: &str) -> bool {
        self.name_to_idx.contains_key(name)
    }

    /// Get dependencies of a skill
    pub fn get_dependencies(&self, name: &str) -> Option<Vec<String>> {
        let idx = self.name_to_idx.get(name)?;

        // Dependencies point TO the skill, so look for incoming edges
        let deps: Vec<String> = self
            .graph
            .neighbors_directed(*idx, Direction::Incoming)
            .map(|dep_idx| self.graph[dep_idx].name.clone())
            .collect();

        Some(deps)
    }

    /// Get dependents of a skill (skills that depend on it)
    pub fn get_dependents(&self, name: &str) -> Option<Vec<String>> {
        let idx = self.name_to_idx.get(name)?;

        // Dependents are pointed TO by this skill, so look for outgoing edges
        let dependents: Vec<String> = self
            .graph
            .neighbors_directed(*idx, Direction::Outgoing)
            .map(|dep_idx| self.graph[dep_idx].name.clone())
            .collect();

        Some(dependents)
    }

    /// Get DAG statistics
    pub fn stats(&self) -> DagStats {
        DagStats {
            node_count: self.graph.node_count(),
            edge_count: self.graph.edge_count(),
            cycle_count: self.detect_cycles().len(),
            orphan_count: self.find_orphans().len(),
        }
    }
}

/// DAG statistics
#[derive(Debug, Clone, Copy)]
pub struct DagStats {
    /// Number of skills (nodes)
    pub node_count: usize,

    /// Number of dependency edges
    pub edge_count: usize,

    /// Number of cycles detected
    pub cycle_count: usize,

    /// Number of skills with broken dependencies
    pub orphan_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest(name: &str, deps: Vec<&str>) -> SkillManifest {
        SkillManifest {
            name: name.into(),
            description: format!("{} skill", name),
            version: "1.0.0".into(),
            dependencies: deps.into_iter().map(String::from).collect(),
            tags: vec![],
            platforms: vec![],
        }
    }

    #[test]
    fn test_dag_build() {
        let skills = vec![
            make_manifest("app", vec!["db", "config"]),
            make_manifest("db", vec!["config"]),
            make_manifest("config", vec![]),
        ];

        let dag = SkillDag::build(&skills);

        assert_eq!(dag.skill_names().len(), 3);
        assert!(dag.contains("app"));
        assert!(dag.contains("db"));
        assert!(dag.contains("config"));
    }

    #[test]
    fn test_resolve_order() {
        let skills = vec![
            make_manifest("app", vec!["db", "config"]),
            make_manifest("db", vec!["config"]),
            make_manifest("config", vec![]),
        ];

        let dag = SkillDag::build(&skills);
        let order = dag.resolve_order("app").expect("Resolve order");

        // config should come before db and app
        // db should come before app
        let config_pos = order
            .iter()
            .position(|n| n == "config")
            .expect("config in order");
        let db_pos = order.iter().position(|n| n == "db").expect("db in order");
        let app_pos = order.iter().position(|n| n == "app").expect("app in order");

        assert!(config_pos < db_pos, "config before db");
        assert!(config_pos < app_pos, "config before app");
        assert!(db_pos < app_pos, "db before app");
    }

    #[test]
    fn test_detect_cycles() {
        let skills = vec![
            make_manifest("a", vec!["b"]),
            make_manifest("b", vec!["c"]),
            make_manifest("c", vec!["a"]), // Creates cycle
        ];

        let dag = SkillDag::build(&skills);
        let cycles = dag.detect_cycles();

        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 3);
    }

    #[test]
    fn test_find_orphans() {
        let skills = vec![make_manifest("app", vec!["missing-dep"])];

        let dag = SkillDag::build(&skills);
        let orphans = dag.find_orphans();

        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0], ("app".into(), "missing-dep".into()));
    }

    #[test]
    fn test_get_dependencies() {
        let skills = vec![
            make_manifest("app", vec!["db", "config"]),
            make_manifest("db", vec![]),
            make_manifest("config", vec![]),
        ];

        let dag = SkillDag::build(&skills);
        let deps = dag.get_dependencies("app").expect("Get deps");

        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&"db".into()));
        assert!(deps.contains(&"config".into()));
    }

    #[test]
    fn test_get_dependents() {
        let skills = vec![
            make_manifest("app", vec!["config"]),
            make_manifest("db", vec!["config"]),
            make_manifest("config", vec![]),
        ];

        let dag = SkillDag::build(&skills);
        let dependents = dag.get_dependents("config").expect("Get dependents");

        assert_eq!(dependents.len(), 2);
        assert!(dependents.contains(&"app".into()));
        assert!(dependents.contains(&"db".into()));
    }
}
