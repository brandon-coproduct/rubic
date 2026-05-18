//! Candidate role-assignment enumeration and least-privilege ranking.
//!
//! The planner is intentionally dumb: it enumerates every role the user does
//! not already hold, builds a one-step `AssignRole` plan, then asks two
//! oracles whether to accept it — `policy::check_plan` (structural rules) and
//! `egglog_engine::Engine::derives_goal` (reachability). The cost function
//! determines ranking; acceptance is binary.
//!
//! This is the right place to surface "no candidate works" too — the planner
//! returns the full ranked list, including rejections, so the receipt can
//! explain why nothing was accepted.

use core_ir::{EntityId, Goal, Model, Permission, Plan, PlanStep};
use egglog_engine::Engine;
use policy::{check_goal_reachable, check_plan, PolicyViolation};
use serde::{Deserialize, Serialize};

pub mod graph;

#[derive(Debug, thiserror::Error)]
pub enum PlannerError {
    #[error("engine: {0}")]
    Engine(#[from] egglog_engine::EngineError),
    #[error("goal user {0} is not declared in the model")]
    UnknownGoalUser(EntityId),
}

/// One enumerated candidate plan + everything we learned about it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanCandidate {
    pub plan: Plan,
    /// Role this candidate assigns (convenience for UI; redundant with `plan`).
    pub role: EntityId,
    /// True iff policy + egglog both green-lit this plan.
    pub accepted: bool,
    /// Permissions added to the user beyond what they already had.
    pub granted_delta: Vec<Permission>,
    /// Whether egglog could derive `CanReach(user, goal_perm)` post-plan.
    pub derives_goal: bool,
    /// Structured policy violations (empty for accepted plans).
    pub violations: Vec<PolicyViolation>,
    /// Lower is better. See `score` in this module for the formula.
    pub cost: u32,
}

/// One frame of the egglog trace — the egraph state at a specific point
/// in the planner's search. Frames are ordered: `Initial` first, then one
/// `Candidate` frame per role the planner tried.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SnapshotKind {
    /// Initial egraph — model facts asserted, no candidate plan applied.
    Initial,
    /// State after `(Assigned user role)` for one candidate.
    Candidate {
        role: EntityId,
        accepted: bool,
        derives_goal: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgraphSnapshot {
    pub label: String,
    #[serde(flatten)]
    pub kind: SnapshotKind,
    /// `egraph-serialize` JSON, consumable directly by the `egraph-visualizer`
    /// npm package.
    pub graph: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningOutcome {
    /// Pre-enumeration: if the goal is unreachable in principle (no role in
    /// the model grants it), this is `Some` and `candidates` is empty.
    pub goal_unreachable: Option<PolicyViolation>,
    /// All candidates considered, ranked best-first.
    pub candidates: Vec<PlanCandidate>,
    /// The top accepted candidate, if any. This is the plan the server will
    /// emit a positive receipt for.
    pub accepted_index: Option<usize>,
    /// Trace of egraph states the planner produced. Frame 0 is `Initial`,
    /// frames 1..=N are one per candidate in cost-ranked order. Used by the
    /// frontend's `EgraphView` scrubber.
    #[serde(default)]
    pub trace: Vec<EgraphSnapshot>,
}

impl PlanningOutcome {
    pub fn accepted(&self) -> Option<&PlanCandidate> {
        self.accepted_index.and_then(|i| self.candidates.get(i))
    }
}

/// Enumerate, validate, and rank candidate role assignments for a goal.
/// Returns at most `top_n` candidates.
pub fn plan(model: &Model, goal: &Goal, top_n: usize) -> Result<PlanningOutcome, PlannerError> {
    if !model.users.contains_key(&goal.user) {
        return Err(PlannerError::UnknownGoalUser(goal.user.clone()));
    }

    if let Some(v) = check_goal_reachable(model, goal) {
        return Ok(PlanningOutcome {
            goal_unreachable: Some(v),
            candidates: Vec::new(),
            accepted_index: None,
            trace: Vec::new(),
        });
    }

    // Frame 0: the initial egraph — model facts, no candidates applied.
    let mut trace: Vec<EgraphSnapshot> = Vec::new();
    {
        let mut initial_engine = Engine::new(model)?;
        let graph = initial_engine
            .serialize_snapshot()
            .map_err(PlannerError::Engine)?;
        trace.push(EgraphSnapshot {
            label: "initial: model facts asserted".to_string(),
            kind: SnapshotKind::Initial,
            graph,
        });
    }

    let current = model.permissions_of(&goal.user);
    let already_has: Vec<&EntityId> = model
        .users
        .get(&goal.user)
        .map(|u| u.roles.iter().collect())
        .unwrap_or_default();

    let mut candidates = Vec::new();

    for (rid, role) in &model.roles {
        if already_has.contains(&rid) {
            continue;
        }

        let plan = Plan {
            steps: vec![PlanStep::AssignRole {
                user: goal.user.clone(),
                role: rid.clone(),
            }],
        };

        // Permission delta: what new perms does this role grant the user?
        let granted_delta: Vec<Permission> = role
            .permissions
            .iter()
            .filter(|p| !current.contains(p))
            .cloned()
            .collect();

        // Policy check — collect every violation.
        let report = check_plan(model, goal, &plan);

        // Reachability oracle — fresh engine per candidate so facts don't bleed.
        let mut engine = Engine::new(model)?;
        engine.apply_plan(&plan)?;
        let derives_goal = engine.derives_goal(goal)?;

        let forbidden_hits = report
            .violations
            .iter()
            .filter(|v| v.invariant == policy::InvariantKind::NoForbiddenPermission)
            .count() as u32;
        let approval_hits = report
            .violations
            .iter()
            .filter(|v| v.invariant == policy::InvariantKind::NoUnapprovedAutoGrant)
            .count() as u32;

        let cost = score(granted_delta.len() as u32, forbidden_hits, approval_hits);

        // Accepted iff: (a) policy clean, (b) egglog derives goal.
        let accepted = report.violations.is_empty() && derives_goal;

        candidates.push(PlanCandidate {
            plan,
            role: rid.clone(),
            accepted,
            granted_delta,
            derives_goal,
            violations: report.violations,
            cost,
        });
    }

    // Rank: accepted first, then by cost ascending, then by role id for
    // determinism.
    candidates.sort_by(|a, b| {
        b.accepted
            .cmp(&a.accepted)
            .then_with(|| a.cost.cmp(&b.cost))
            .then_with(|| a.role.cmp(&b.role))
    });
    candidates.truncate(top_n.max(1));

    let accepted_index = candidates.iter().position(|c| c.accepted);

    // Frames 1..=N: one snapshot per ranked candidate (same order the UI
    // sees in the timeline). Each snapshot is the egraph after asserting
    // (Assigned goal.user candidate.role) — i.e. the algebraic state the
    // validator looked at when deciding accept/reject.
    for cand in &candidates {
        let mut engine = Engine::new(model)?;
        engine.apply_plan(&cand.plan)?;
        let graph = engine
            .serialize_snapshot()
            .map_err(PlannerError::Engine)?;
        let prefix = if cand.accepted { "✓" } else { "✗" };
        trace.push(EgraphSnapshot {
            label: format!("{prefix} assign {}", cand.role),
            kind: SnapshotKind::Candidate {
                role: cand.role.clone(),
                accepted: cand.accepted,
                derives_goal: cand.derives_goal,
            },
            graph,
        });
    }

    Ok(PlanningOutcome {
        goal_unreachable: None,
        candidates,
        accepted_index,
        trace,
    })
}

/// Cost formula. Lower = preferred.
///
/// `cost = |granted_perms| + 10·|forbidden_hits| + 5·|requires_approval_hits|`
///
/// Forbidden hits dominate so a forbidden-violating plan is always ranked
/// below a clean plan, even if its raw permission delta happens to be smaller.
pub fn score(granted: u32, forbidden_hits: u32, approval_hits: u32) -> u32 {
    granted + 10 * forbidden_hits + 5 * approval_hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_model() -> Model {
        Model::from_toml_str(include_str!("../../../examples/rbac_demo.toml")).unwrap()
    }

    fn demo_goal() -> Goal {
        demo_model().goal.unwrap()
    }

    #[test]
    fn accepted_candidate_is_finance_viewer() {
        let out = plan(&demo_model(), &demo_goal(), 3).unwrap();
        let acc = out.accepted().expect("expected an accepted plan");
        assert_eq!(acc.role.as_str(), "finance_viewer");
        assert!(acc.derives_goal);
        assert!(acc.violations.is_empty());
    }

    #[test]
    fn payroll_admin_is_present_but_rejected() {
        let out = plan(&demo_model(), &demo_goal(), 5).unwrap();
        let admin = out
            .candidates
            .iter()
            .find(|c| c.role.as_str() == "payroll_admin")
            .expect("payroll_admin must be enumerated");
        assert!(!admin.accepted);
        // Egglog still says it would *technically* reach the goal — the
        // rejection comes from policy, not reachability.
        assert!(admin.derives_goal);
        assert!(admin
            .violations
            .iter()
            .any(|v| v.invariant == policy::InvariantKind::NoForbiddenPermission));
    }

    #[test]
    fn already_held_roles_are_not_re_enumerated() {
        let out = plan(&demo_model(), &demo_goal(), 10).unwrap();
        assert!(out
            .candidates
            .iter()
            .all(|c| c.role.as_str() != "employee"));
    }

    #[test]
    fn ranking_puts_finance_viewer_above_payroll_admin() {
        let out = plan(&demo_model(), &demo_goal(), 5).unwrap();
        let fv = out
            .candidates
            .iter()
            .position(|c| c.role.as_str() == "finance_viewer")
            .unwrap();
        let pa = out
            .candidates
            .iter()
            .position(|c| c.role.as_str() == "payroll_admin")
            .unwrap();
        assert!(fv < pa, "finance_viewer must rank above payroll_admin");
    }

    #[test]
    fn infeasible_goal_returns_unreachable() {
        let m = demo_model();
        let g = Goal {
            user: "alice".into(),
            action: "exfiltrate".into(),
            resource: "trade_secrets".into(),
        };
        let out = plan(&m, &g, 3).unwrap();
        assert!(out.goal_unreachable.is_some());
        assert!(out.candidates.is_empty());
        assert!(out.accepted_index.is_none());
    }

    #[test]
    fn unknown_user_is_a_planner_error() {
        let m = demo_model();
        let g = Goal {
            user: "bob".into(),
            action: "read".into(),
            resource: "payroll_summary".into(),
        };
        match plan(&m, &g, 3) {
            Err(PlannerError::UnknownGoalUser(u)) => assert_eq!(u.as_str(), "bob"),
            other => panic!("expected UnknownGoalUser, got {other:?}"),
        }
    }

    #[test]
    fn score_penalizes_forbidden_more_than_approval() {
        assert!(score(2, 1, 0) > score(2, 0, 1));
    }

    #[test]
    fn trace_has_initial_plus_one_per_candidate() {
        let out = plan(&demo_model(), &demo_goal(), 10).unwrap();
        // Initial frame + N candidate frames.
        assert_eq!(out.trace.len(), out.candidates.len() + 1);
        assert!(matches!(out.trace[0].kind, SnapshotKind::Initial));
        // Every candidate has a corresponding snapshot.
        for (i, cand) in out.candidates.iter().enumerate() {
            match &out.trace[i + 1].kind {
                SnapshotKind::Candidate { role, accepted, .. } => {
                    assert_eq!(role, &cand.role);
                    assert_eq!(accepted, &cand.accepted);
                }
                _ => panic!("expected Candidate at frame {}", i + 1),
            }
        }
    }

    #[test]
    fn trace_snapshots_are_real_egraph_json() {
        let out = plan(&demo_model(), &demo_goal(), 3).unwrap();
        for frame in &out.trace {
            // egraph-visualizer requires a `nodes` map.
            assert!(
                frame.graph.get("nodes").is_some(),
                "frame {:?} missing nodes",
                frame.label
            );
        }
    }

    #[test]
    fn unreachable_goal_returns_empty_trace() {
        let m = demo_model();
        let g = Goal {
            user: "alice".into(),
            action: "exfiltrate".into(),
            resource: "trade_secrets".into(),
        };
        let out = plan(&m, &g, 3).unwrap();
        assert!(out.trace.is_empty());
    }
}
