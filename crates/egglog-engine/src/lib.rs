//! Thin wrapper around `egglog::EGraph` that loads the RBAC ruleset, asserts
//! model-derived facts, applies a candidate plan, and queries reachability.
//!
//! The point of this crate is the trust boundary: the planner uses egglog as
//! the *authoritative* answer to "does this user reach this permission under
//! the assignment?" — not the LLM, not a hand-written walk over the IR. If
//! egglog can derive `(CanReach u (Perm a r))`, the plan is reachable.

use core_ir::{EntityId, Goal, Model, Permission, Plan, PlanStep};
use egglog::{EGraph, SerializeConfig};

/// Source of the RBAC ruleset; bound into receipts via `bytes_digest`.
pub const RULES_SRC: &str = include_str!("../../../examples/rules.egg");

/// Step in a derivation trace returned alongside `derives`.
/// Today we emit a single rule-firing summary; future work can plumb full
/// egglog extraction in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationStep {
    pub rule: String,
    pub bound: Vec<(String, String)>,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("egglog: {0}")]
    Egglog(String),
}

impl From<egglog::Error> for EngineError {
    fn from(e: egglog::Error) -> Self {
        Self::Egglog(format!("{e}"))
    }
}

/// Reachability engine for a single (Model, Plan) snapshot.
///
/// Construction loads the ruleset and asserts every `(HasPerm role perm)` and
/// `(Assigned user role)` fact from the model. `apply_plan` layers additional
/// `(Assigned ...)` facts before any `derives` call.
pub struct Engine {
    egraph: EGraph,
}

impl Engine {
    pub fn new(model: &Model) -> Result<Self, EngineError> {
        let mut egraph = EGraph::default();
        egraph.parse_and_run_program(None, RULES_SRC)?;

        let mut prelude = String::with_capacity(1024);
        for (rid, role) in &model.roles {
            for perm in &role.permissions {
                prelude.push_str(&fact_has_perm(rid, perm));
            }
        }
        for (uid, user) in &model.users {
            for r in &user.roles {
                prelude.push_str(&fact_assigned(uid, r));
            }
        }
        egraph.parse_and_run_program(None, &prelude)?;
        Ok(Self { egraph })
    }

    /// Apply each `(Assigned user role)` step from the plan, in order.
    /// Idempotent — egglog dedups facts.
    pub fn apply_plan(&mut self, plan: &Plan) -> Result<(), EngineError> {
        let mut buf = String::new();
        for step in &plan.steps {
            let PlanStep::AssignRole { user, role } = step;
            buf.push_str(&fact_assigned(user, role));
        }
        if !buf.is_empty() {
            self.egraph.parse_and_run_program(None, &buf)?;
        }
        Ok(())
    }

    /// Does the egraph derive `(CanReach user (Perm action resource))`?
    /// Runs the ruleset to fixpoint first.
    pub fn derives(
        &mut self,
        user: &EntityId,
        action: &str,
        resource: &str,
    ) -> Result<bool, EngineError> {
        // Saturate (a small iteration bound — the rule is linear in facts).
        self.egraph.parse_and_run_program(None, "(run 16)")?;

        let probe = format!(
            "(check (CanReach {} (Perm {} {})))\n",
            quote(user.as_str()),
            quote(action),
            quote(resource),
        );
        match self.egraph.parse_and_run_program(None, &probe) {
            Ok(_) => Ok(true),
            // `(check ...)` returns an error when the fact isn't derivable.
            // Treat that as "false," not as a pipeline failure.
            Err(_) => Ok(false),
        }
    }

    pub fn derives_goal(&mut self, goal: &Goal) -> Result<bool, EngineError> {
        self.derives(&goal.user, &goal.action, &goal.resource)
    }

    /// Stub trace: in v0.1 we just report the single rule that connects
    /// `Assigned` + `HasPerm` to `CanReach`. The plumbing exists so the
    /// receipt schema is forward-compatible with real extraction later.
    pub fn reasons(&self) -> Vec<DerivationStep> {
        vec![DerivationStep {
            rule: "Assigned(u,r) ∧ HasPerm(r,p) → CanReach(u,p)".to_string(),
            bound: Vec::new(),
        }]
    }

    /// Serialize the current egraph to the `egraph-serialize` JSON shape.
    /// This is what the `egraph-visualizer` npm package consumes — fields:
    /// `nodes`, `root_eclasses`, `class_data`. We round-trip through serde
    /// rather than reaching into egraph-serialize's types so the JSON we
    /// produce matches its own `to_json_file` output exactly.
    pub fn serialize_snapshot(&mut self) -> Result<serde_json::Value, EngineError> {
        // Make sure derivations are up to date before snapshotting.
        self.egraph.parse_and_run_program(None, "(run 16)")?;
        let out = self.egraph.serialize(SerializeConfig::default());
        serde_json::to_value(&out.egraph)
            .map_err(|e| EngineError::Egglog(format!("serialize: {e}")))
    }
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn fact_has_perm(role: &EntityId, perm: &Permission) -> String {
    format!(
        "(HasPerm {} (Perm {} {}))\n",
        quote(role.as_str()),
        quote(&perm.action),
        quote(&perm.resource),
    )
}

fn fact_assigned(user: &EntityId, role: &EntityId) -> String {
    format!(
        "(Assigned {} {})\n",
        quote(user.as_str()),
        quote(role.as_str()),
    )
}

/// Egglog string literal: double-quoted, with `\\` and `\"` escapes.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_model() -> Model {
        Model::from_toml_str(include_str!("../../../examples/rbac_demo.toml")).unwrap()
    }

    #[test]
    fn engine_loads_demo_model() {
        Engine::new(&demo_model()).expect("engine init");
    }

    #[test]
    fn alice_cannot_read_payroll_summary_initially() {
        let mut e = Engine::new(&demo_model()).unwrap();
        let alice = EntityId::from("alice");
        assert!(!e.derives(&alice, "read", "payroll_summary").unwrap());
    }

    #[test]
    fn alice_can_read_profile_via_employee_role() {
        let mut e = Engine::new(&demo_model()).unwrap();
        let alice = EntityId::from("alice");
        assert!(e.derives(&alice, "read", "profile").unwrap());
    }

    #[test]
    fn assigning_finance_viewer_grants_goal() {
        let mut e = Engine::new(&demo_model()).unwrap();
        e.apply_plan(&Plan::assign("alice", "finance_viewer"))
            .unwrap();
        let alice = EntityId::from("alice");
        assert!(e.derives(&alice, "read", "payroll_summary").unwrap());
    }

    #[test]
    fn assigning_payroll_admin_also_grants_goal() {
        // egglog is permission-neutral — it just reports reachability.
        // Policy is what rejects payroll_admin downstream.
        let mut e = Engine::new(&demo_model()).unwrap();
        e.apply_plan(&Plan::assign("alice", "payroll_admin"))
            .unwrap();
        let alice = EntityId::from("alice");
        assert!(e.derives(&alice, "read", "payroll_summary").unwrap());
        assert!(e.derives(&alice, "delete", "payroll").unwrap());
    }

    #[test]
    fn assigning_unknown_role_does_not_grant() {
        let mut e = Engine::new(&demo_model()).unwrap();
        e.apply_plan(&Plan::assign("alice", "ghost_role")).unwrap();
        let alice = EntityId::from("alice");
        assert!(!e.derives(&alice, "read", "payroll_summary").unwrap());
    }

    #[test]
    fn fresh_engine_each_query_is_independent() {
        // Two engines, two plans, must not leak facts into each other.
        let m = demo_model();
        let mut e1 = Engine::new(&m).unwrap();
        e1.apply_plan(&Plan::assign("alice", "finance_viewer")).unwrap();

        let mut e2 = Engine::new(&m).unwrap();
        let alice = EntityId::from("alice");
        assert!(!e2.derives(&alice, "read", "payroll_summary").unwrap());
    }

    #[test]
    fn snapshot_serializes_to_egraph_visualizer_shape() {
        let mut e = Engine::new(&demo_model()).unwrap();
        e.apply_plan(&Plan::assign("alice", "finance_viewer")).unwrap();
        let snap = e.serialize_snapshot().expect("serialize");
        // Top-level keys that egraph-visualizer expects.
        assert!(snap.get("nodes").is_some(), "missing `nodes`: {snap:#}");
        // Either present (possibly empty) or absent is OK for these two;
        // egraph-visualizer tolerates both.
        // At minimum we should have facts for the assigned role.
        let nodes = snap["nodes"].as_object().unwrap();
        assert!(!nodes.is_empty(), "expected at least one node");
        // The serialized form's `op` strings should include our RBAC
        // function names somewhere (HasPerm / Assigned / CanReach / Perm).
        let any_op = nodes
            .values()
            .filter_map(|n| n.get("op").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            any_op.contains("HasPerm") || any_op.contains("Assigned") || any_op.contains("Perm"),
            "no recognizable RBAC ops in snapshot: {any_op}"
        );
    }

    #[test]
    fn quote_escapes_backslash_and_doublequote() {
        assert_eq!(quote("plain"), "\"plain\"");
        assert_eq!(quote("with\"quote"), "\"with\\\"quote\"");
        assert_eq!(quote("with\\slash"), "\"with\\\\slash\"");
    }
}
