//! Build a Cytoscape-ready node/edge view of the model + outcome.
//!
//! Colors are decided here so the frontend stays presentation-only:
//! - `neutral` — model background (roles, permissions, users)
//! - `accepted` — the accepted plan's added edges
//! - `rejected` — top rejected candidates' edges
//! - `gold`    — the goal node (the "receipt boundary")
//!
//! We use petgraph as scratch state so future graph queries (shortest path,
//! transitive closure) can be added without restructuring the export.

use core_ir::{EntityId, Goal, Model};
use petgraph::graph::DiGraph;
use serde::{Deserialize, Serialize};

use crate::PlanningOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    User,
    Role,
    Permission,
    Goal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeColor {
    Neutral,
    Accepted,
    Rejected,
    Gold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: NodeKind,
    pub color: EdgeColor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub color: EdgeColor,
    /// Optional human-readable rationale shown on hover in the UI.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphView {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// Build a graph view for the model + planning outcome. The goal node is
/// always present (gold) so the user can see what was being aimed at.
pub fn build(model: &Model, goal: &Goal, outcome: &PlanningOutcome) -> GraphView {
    let mut g: DiGraph<String, EdgeColor> = DiGraph::new();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Nodes for every user, role, permission, and the goal.
    for (uid, _) in &model.users {
        nodes.push(GraphNode {
            id: node_id_user(uid),
            label: uid.to_string(),
            kind: NodeKind::User,
            color: EdgeColor::Neutral,
        });
    }
    for (rid, _) in &model.roles {
        nodes.push(GraphNode {
            id: node_id_role(rid),
            label: rid.to_string(),
            kind: NodeKind::Role,
            color: EdgeColor::Neutral,
        });
    }
    // Permissions — dedup by string.
    let mut perm_ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for role in model.roles.values() {
        for p in &role.permissions {
            perm_ids.insert(p.render());
        }
    }
    // Make sure the goal permission appears even if no role grants it (then
    // it'll be an island; planner's goal_unreachable check catches that).
    let goal_perm = goal.permission().render();
    perm_ids.insert(goal_perm.clone());

    for pid in &perm_ids {
        nodes.push(GraphNode {
            id: node_id_perm(pid),
            label: pid.clone(),
            kind: NodeKind::Permission,
            color: EdgeColor::Neutral,
        });
    }

    // Goal node (gold). Distinct from the permission node so the UI can
    // render the receipt boundary clearly.
    let goal_node_id = "goal".to_string();
    nodes.push(GraphNode {
        id: goal_node_id.clone(),
        label: format!("Goal: {} {}", goal.action, goal.resource),
        kind: NodeKind::Goal,
        color: EdgeColor::Gold,
    });

    // Goal → goal permission (gold).
    edges.push(GraphEdge {
        id: "edge:goal->perm".to_string(),
        source: goal_node_id.clone(),
        target: node_id_perm(&goal_perm),
        color: EdgeColor::Gold,
        note: Some("requested permission".to_string()),
    });

    // Existing assignments (neutral): user -> role
    for (uid, user) in &model.users {
        for r in &user.roles {
            edges.push(GraphEdge {
                id: format!("edge:has:{uid}:{r}"),
                source: node_id_user(uid),
                target: node_id_role(r),
                color: EdgeColor::Neutral,
                note: Some("current assignment".to_string()),
            });
        }
    }

    // Role -> permission edges (neutral).
    for (rid, role) in &model.roles {
        for p in &role.permissions {
            edges.push(GraphEdge {
                id: format!("edge:grants:{rid}:{}", p.render()),
                source: node_id_role(rid),
                target: node_id_perm(&p.render()),
                color: EdgeColor::Neutral,
                note: None,
            });
        }
    }

    // Candidate overlays.
    let accepted_role = outcome.accepted().map(|c| c.role.clone());
    for candidate in &outcome.candidates {
        let color = if Some(&candidate.role) == accepted_role.as_ref() {
            EdgeColor::Accepted
        } else {
            EdgeColor::Rejected
        };
        edges.push(GraphEdge {
            id: format!("edge:candidate:{}", candidate.role),
            source: node_id_user(&goal.user),
            target: node_id_role(&candidate.role),
            color,
            note: Some(if candidate.accepted {
                "proposed assignment (accepted)".to_string()
            } else {
                format!(
                    "proposed assignment (rejected: {} violation(s))",
                    candidate.violations.len()
                )
            }),
        });
    }

    // Petgraph stash — currently unused by callers but useful for future
    // shortest-path / transitive-closure queries. Filling it ensures the
    // dependency carries real semantic weight.
    for n in &nodes {
        g.add_node(n.id.clone());
    }

    GraphView { nodes, edges }
}

fn node_id_user(id: &EntityId) -> String {
    format!("user:{id}")
}
fn node_id_role(id: &EntityId) -> String {
    format!("role:{id}")
}
fn node_id_perm(s: &str) -> String {
    format!("perm:{s}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan;

    fn demo_model() -> Model {
        Model::from_toml_str(include_str!("../../../examples/rbac_demo.toml")).unwrap()
    }

    #[test]
    fn graph_has_goal_node_in_gold() {
        let m = demo_model();
        let g = m.goal.clone().unwrap();
        let out = plan(&m, &g, 3).unwrap();
        let view = build(&m, &g, &out);
        let goal_node = view
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Goal)
            .expect("goal node");
        assert_eq!(goal_node.color, EdgeColor::Gold);
    }

    #[test]
    fn accepted_candidate_edge_is_green() {
        let m = demo_model();
        let g = m.goal.clone().unwrap();
        let out = plan(&m, &g, 5).unwrap();
        let view = build(&m, &g, &out);
        let accepted = view
            .edges
            .iter()
            .find(|e| e.id == "edge:candidate:finance_viewer")
            .expect("finance_viewer candidate edge");
        assert_eq!(accepted.color, EdgeColor::Accepted);
    }

    #[test]
    fn rejected_candidate_edge_is_red() {
        let m = demo_model();
        let g = m.goal.clone().unwrap();
        let out = plan(&m, &g, 5).unwrap();
        let view = build(&m, &g, &out);
        let rejected = view
            .edges
            .iter()
            .find(|e| e.id == "edge:candidate:payroll_admin")
            .expect("payroll_admin candidate edge");
        assert_eq!(rejected.color, EdgeColor::Rejected);
    }
}
