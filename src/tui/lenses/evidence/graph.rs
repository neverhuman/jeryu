use std::collections::BTreeSet;

use crate::{
    api::entity::EntityRef,
    tui::lenses::evidence::{EvidenceLensInput, ReceiptHit},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceGraph {
    pub nodes: Vec<EvidenceGraphNode>,
    pub edges: Vec<EvidenceGraphEdge>,
}

impl EvidenceGraph {
    pub fn first_entity(&self) -> Option<EntityRef> {
        self.nodes.iter().find_map(|node| node.entity.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceGraphNode {
    pub id: String,
    pub label: String,
    pub kind: EvidenceGraphNodeKind,
    pub entity: Option<EntityRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceGraphNodeKind {
    Entity,
    Proof,
    Receipt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceGraphEdge {
    pub from: String,
    pub to: String,
    pub label: &'static str,
}

pub fn build_entity_proof_graph(input: EvidenceLensInput<'_>) -> EvidenceGraph {
    let mut graph = EvidenceGraph {
        nodes: Vec::new(),
        edges: Vec::new(),
    };
    let mut node_ids = BTreeSet::new();
    let mut edge_ids = BTreeSet::new();

    for proof in input.proof_hits() {
        let proof_id = proof_node_id(&proof.proof_id);
        push_node(
            &mut graph,
            &mut node_ids,
            EvidenceGraphNode {
                id: proof_id.clone(),
                label: proof.proof_id.clone(),
                kind: EvidenceGraphNodeKind::Proof,
                entity: None,
            },
        );

        if let Some(entity) = proof.entity {
            let entity_id = entity_node_id(&entity);
            push_entity_node(&mut graph, &mut node_ids, &entity);
            push_edge(
                &mut graph,
                &mut edge_ids,
                EvidenceGraphEdge {
                    from: entity_id,
                    to: proof_id,
                    label: "supports",
                },
            );
        }
    }

    for receipt in input.receipt_hits() {
        push_receipt(&mut graph, &mut node_ids, &mut edge_ids, &receipt);
    }

    EvidenceGraph {
        nodes: graph.nodes,
        edges: graph.edges,
    }
}

fn push_receipt(
    graph: &mut EvidenceGraph,
    node_ids: &mut BTreeSet<String>,
    edge_ids: &mut BTreeSet<String>,
    receipt: &ReceiptHit,
) {
    let receipt_id = receipt_node_id(&receipt.receipt_id);
    push_node(
        graph,
        node_ids,
        EvidenceGraphNode {
            id: receipt_id.clone(),
            label: receipt.receipt_id.clone(),
            kind: EvidenceGraphNodeKind::Receipt,
            entity: None,
        },
    );

    if let Some(entity) = &receipt.affected_entity {
        let entity_id = entity_node_id(entity);
        push_entity_node(graph, node_ids, entity);
        push_edge(
            graph,
            edge_ids,
            EvidenceGraphEdge {
                from: entity_id,
                to: receipt_id.clone(),
                label: "receipt",
            },
        );
    }

    for proof in &receipt.evidence_created {
        let proof_id = proof_node_id(proof);
        push_node(
            graph,
            node_ids,
            EvidenceGraphNode {
                id: proof_id.clone(),
                label: proof.clone(),
                kind: EvidenceGraphNodeKind::Proof,
                entity: None,
            },
        );
        push_edge(
            graph,
            edge_ids,
            EvidenceGraphEdge {
                from: receipt_id.clone(),
                to: proof_id,
                label: "created",
            },
        );
    }
}

fn push_entity_node(
    graph: &mut EvidenceGraph,
    node_ids: &mut BTreeSet<String>,
    entity: &EntityRef,
) {
    push_node(
        graph,
        node_ids,
        EvidenceGraphNode {
            id: entity_node_id(entity),
            label: entity.display(),
            kind: EvidenceGraphNodeKind::Entity,
            entity: Some(entity.clone()),
        },
    );
}

fn push_node(graph: &mut EvidenceGraph, node_ids: &mut BTreeSet<String>, node: EvidenceGraphNode) {
    if node_ids.insert(node.id.clone()) {
        graph.nodes.push(node);
    }
}

fn push_edge(graph: &mut EvidenceGraph, edge_ids: &mut BTreeSet<String>, edge: EvidenceGraphEdge) {
    let key = format!("{}>{}:{}", edge.from, edge.to, edge.label);
    if edge_ids.insert(key) {
        graph.edges.push(edge);
    }
}

fn entity_node_id(entity: &EntityRef) -> String {
    format!("entity:{}", entity.display())
}

fn proof_node_id(proof_id: &str) -> String {
    format!("proof:{proof_id}")
}

fn receipt_node_id(receipt_id: &str) -> String {
    format!("receipt:{receipt_id}")
}
