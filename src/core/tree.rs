use std::collections::HashMap;

use crate::pyjson::PyValue;

pub const ROOT_COMMIT: &str = "initial";

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub commit_id: String,
    pub id: usize,
    pub parent_id: Option<usize>,
    pub children: Vec<usize>,
    pub utility_measures: Vec<f64>,
}

impl Node {
    pub fn num_evals(&self) -> usize {
        self.utility_measures.len()
    }

    pub fn mean_utility(&self) -> f64 {
        if self.utility_measures.is_empty() {
            return f64::INFINITY;
        }
        self.utility_measures.iter().sum::<f64>() / self.num_evals() as f64
    }

    pub fn pseudo_evals(&self, num_pseudo: usize) -> Vec<f64> {
        if self.num_evals() < num_pseudo {
            self.utility_measures.clone()
        } else {
            vec![self.mean_utility(); num_pseudo]
        }
    }

    pub fn is_expandable(&self) -> bool {
        let mean = self.mean_utility();
        mean.is_finite() && mean > 0.0
    }

    pub fn save_as_dict(&self) -> PyValue {
        PyValue::dict([
            ("commit_id", PyValue::str(&self.commit_id)),
            ("id", PyValue::Int(self.id as i64)),
            (
                "parent_id",
                self.parent_id
                    .map_or(PyValue::Null, |p| PyValue::Int(p as i64)),
            ),
            ("mean_utility", PyValue::Float(self.mean_utility())),
            ("num_evals", PyValue::Int(self.num_evals() as i64)),
        ])
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tree {
    order: Vec<usize>,
    nodes: HashMap<usize, Node>,
}

impl Tree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_root() -> Self {
        let mut tree = Self::new();
        tree.insert(ROOT_COMMIT, None, None);
        tree
    }

    pub fn insert(
        &mut self,
        commit_id: &str,
        parent_id: Option<usize>,
        id: Option<usize>,
    ) -> usize {
        let id = id.unwrap_or(self.order.len());
        let node = Node {
            commit_id: commit_id.to_string(),
            id,
            parent_id,
            children: Vec::new(),
            utility_measures: Vec::new(),
        };
        if self.nodes.insert(id, node).is_none() {
            self.order.push(id);
        }
        id
    }

    pub fn add_child(&mut self, parent: usize, commit_id: &str) -> usize {
        let id = self.insert(commit_id, Some(parent), None);
        if let Some(p) = self.nodes.get_mut(&parent) {
            p.children.push(id);
        }
        id
    }

    pub fn link_children(&mut self) {
        let links: Vec<(usize, usize)> = self
            .nodes()
            .filter_map(|n| n.parent_id.map(|p| (p, n.id)))
            .collect();
        for (parent, child) in links {
            if let Some(p) = self.nodes.get_mut(&parent) {
                p.children.push(child);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn get(&self, id: usize) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn get_mut(&mut self, id: usize) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.order.iter().map(|id| &self.nodes[id])
    }

    pub fn sub_tree(&self, id: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut pending = vec![id];
        while let Some(next) = pending.pop() {
            let Some(node) = self.nodes.get(&next) else {
                continue;
            };
            out.push(next);
            pending.extend(node.children.iter().rev());
        }
        out
    }

    pub fn descendant_evals(&self, id: usize, num_pseudo: usize) -> Vec<f64> {
        let Some(node) = self.nodes.get(&id) else {
            return Vec::new();
        };
        let mut evals = node.pseudo_evals(num_pseudo);
        for descendant in self.sub_tree(id).into_iter().skip(1) {
            evals.extend_from_slice(&self.nodes[&descendant].utility_measures);
        }
        evals
    }

    pub fn total_evals(&self) -> usize {
        self.order.first().map_or(0, |&root| {
            self.sub_tree(root)
                .iter()
                .map(|id| self.nodes[id].num_evals())
                .sum()
        })
    }
}
