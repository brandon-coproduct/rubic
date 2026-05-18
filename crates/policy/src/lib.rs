//! Deterministic policy invariants over (Model, Goal, Plan).
//!
//! Shape mirrors `ck-types::PolicyDiffReport`: violations carry a machine-
//! readable invariant kind plus a human-readable explanation so the receipt
//! can render structured rejection reasons.
//!
//! The check is total — it collects every violation rather than short-
//! circuiting at the first. This keeps rejection receipts informative.

use core_ir::{EntityId, Goal, Model, Permission, Plan, PlanStep};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum InvariantKind {
    /// Plan references a user that does not exist in the model.
    UserExists,
    /// Plan references a role that does not exist in the model.
    RoleExists,
    /// Goal references an (action, resource) no role in the model can satisfy.
    /// Detected pre-plan to give callers a clear "infeasible goal" signal.
    GoalReachable,
    /// After applying the plan, the user still cannot reach the goal
    /// permission.
    GoalSatisfied,
    /// Plan grants a permission listed in `policy.forbidden_permissions`.
    NoForbiddenPermission,
    /// Plan auto-grants a permission listed in `policy.requires_approval`.
    NoUnapprovedAutoGrant,
    /// Under `policy.least_privilege`, plan adds permissions strictly beyond
    /// what's needed to reach the goal.
    LeastPrivilegeMinimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyViolation {
    pub invariant: InvariantKind,
    pub explanation: String,
}

impl PolicyViolation {
    fn new(invariant: InvariantKind, explanation: impl Into<String>) -> Self {
        Self {
            invariant,
            explanation: explanation.into(),
        }
    }
}

/// Result of validating a (Model, Goal, Plan) tuple.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyReport {
    pub violations: Vec<PolicyViolation>,
}

impl PolicyReport {
    pub fn ok(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Pre-plan feasibility check: does *any* role in the model grant the goal
/// permission? Run this before enumeration so callers see "infeasible goal"
/// instead of "no candidates."
pub fn check_goal_reachable(model: &Model, goal: &Goal) -> Option<PolicyViolation> {
    let target = goal.permission();
    let any_role_grants = model
        .roles
        .values()
        .any(|r| r.permissions.contains(&target));
    if any_role_grants {
        None
    } else {
        Some(PolicyViolation::new(
            InvariantKind::GoalReachable,
            format!(
                "no role in the model grants {target}; goal is infeasible \
                 without first defining a role that includes it"
            ),
        ))
    }
}

/// Validate a candidate plan against the model and goal. Returns every
/// violation found, in a stable order (one pass per step, in step order).
pub fn check_plan(model: &Model, goal: &Goal, plan: &Plan) -> PolicyReport {
    let mut violations = Vec::new();

    for step in &plan.steps {
        match step {
            PlanStep::AssignRole { user, role } => {
                check_assign_role(model, goal, user, role, &mut violations);
            }
        }
    }

    // Post-plan reachability + least-privilege are global checks over the
    // user's resulting permissions, computed once after all steps applied.
    let Some(goal_user) = plan_target_user(plan) else {
        return PolicyReport { violations };
    };

    let resulting = resulting_permissions(model, plan, &goal_user);
    let target = goal.permission();

    if !resulting.contains(&target) {
        violations.push(PolicyViolation::new(
            InvariantKind::GoalSatisfied,
            format!(
                "after applying plan, user {goal_user} still cannot {target}"
            ),
        ));
    }

    if model.policy.least_privilege {
        let current = model.permissions_of(&goal_user);
        let added: Vec<&Permission> = resulting
            .iter()
            .filter(|p| !current.contains(p))
            .collect();
        let extraneous: Vec<&&Permission> = added
            .iter()
            .filter(|p| ***p != target)
            .collect();
        if !extraneous.is_empty() {
            let names: Vec<String> = extraneous.iter().map(|p| p.render()).collect();
            violations.push(PolicyViolation::new(
                InvariantKind::LeastPrivilegeMinimal,
                format!(
                    "least_privilege=true: plan grants {} permission(s) beyond the goal \
                     ({}); a tighter role would suffice",
                    extraneous.len(),
                    names.join(", ")
                ),
            ));
        }
    }

    PolicyReport { violations }
}

fn check_assign_role(
    model: &Model,
    goal: &Goal,
    user: &EntityId,
    role: &EntityId,
    out: &mut Vec<PolicyViolation>,
) {
    if !model.users.contains_key(user) {
        out.push(PolicyViolation::new(
            InvariantKind::UserExists,
            format!("user {user} is not declared in the model"),
        ));
    }
    // We don't reject the goal user vs plan user mismatch as an invariant —
    // the planner will only emit plans whose user matches the goal — but if a
    // hand-crafted plan slips through, the GoalSatisfied check catches it.
    if user != &goal.user {
        out.push(PolicyViolation::new(
            InvariantKind::UserExists,
            format!(
                "plan targets user {user} but goal targets {}; \
                 inconsistent assignment",
                goal.user
            ),
        ));
    }

    let Some(role_obj) = model.roles.get(role) else {
        out.push(PolicyViolation::new(
            InvariantKind::RoleExists,
            format!("role {role} is not declared in the model"),
        ));
        return;
    };

    for perm in &role_obj.permissions {
        if model.policy.forbidden_permissions.contains(perm) {
            out.push(PolicyViolation::new(
                InvariantKind::NoForbiddenPermission,
                format!(
                    "role {role} grants forbidden permission {perm}"
                ),
            ));
        }
        if model.policy.requires_approval.contains(perm) {
            out.push(PolicyViolation::new(
                InvariantKind::NoUnapprovedAutoGrant,
                format!(
                    "role {role} grants {perm} which requires explicit approval; \
                     automatic assignment is not allowed"
                ),
            ));
        }
    }
}

fn plan_target_user(plan: &Plan) -> Option<EntityId> {
    plan.steps.iter().find_map(|step| match step {
        PlanStep::AssignRole { user, .. } => Some(user.clone()),
    })
}

/// Compute the user's permission set assuming the plan is applied on top of
/// the model. Does not mutate the model.
fn resulting_permissions(model: &Model, plan: &Plan, user: &EntityId) -> Vec<Permission> {
    let mut perms = model.permissions_of(user);
    for step in &plan.steps {
        let PlanStep::AssignRole { user: u, role } = step;
        if u != user {
            continue;
        }
        if let Some(r) = model.roles.get(role) {
            for p in &r.permissions {
                if !perms.contains(p) {
                    perms.push(p.clone());
                }
            }
        }
    }
    perms.sort();
    perms
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
    fn finance_viewer_plan_is_clean() {
        let report = check_plan(
            &demo_model(),
            &demo_goal(),
            &Plan::assign("alice", "finance_viewer"),
        );
        assert!(
            report.ok(),
            "expected no violations, got {:?}",
            report.violations
        );
    }

    #[test]
    fn payroll_admin_plan_rejected_with_forbidden() {
        let report = check_plan(
            &demo_model(),
            &demo_goal(),
            &Plan::assign("alice", "payroll_admin"),
        );
        assert!(!report.ok());
        assert!(report
            .violations
            .iter()
            .any(|v| v.invariant == InvariantKind::NoForbiddenPermission));
    }

    #[test]
    fn payroll_admin_plan_also_flagged_requires_approval() {
        let report = check_plan(
            &demo_model(),
            &demo_goal(),
            &Plan::assign("alice", "payroll_admin"),
        );
        assert!(report
            .violations
            .iter()
            .any(|v| v.invariant == InvariantKind::NoUnapprovedAutoGrant));
    }

    #[test]
    fn payroll_admin_violates_least_privilege_too() {
        let report = check_plan(
            &demo_model(),
            &demo_goal(),
            &Plan::assign("alice", "payroll_admin"),
        );
        assert!(report
            .violations
            .iter()
            .any(|v| v.invariant == InvariantKind::LeastPrivilegeMinimal));
    }

    #[test]
    fn unknown_role_rejected() {
        let report = check_plan(
            &demo_model(),
            &demo_goal(),
            &Plan::assign("alice", "ghost_role"),
        );
        assert!(report
            .violations
            .iter()
            .any(|v| v.invariant == InvariantKind::RoleExists));
    }

    #[test]
    fn unknown_user_rejected() {
        let report = check_plan(
            &demo_model(),
            &demo_goal(),
            &Plan::assign("bob", "finance_viewer"),
        );
        assert!(report
            .violations
            .iter()
            .any(|v| v.invariant == InvariantKind::UserExists));
    }

    #[test]
    fn empty_plan_fails_goal_satisfied() {
        // No steps → no target user inferable, no GoalSatisfied check fires.
        // But assigning the existing `employee` role (which Alice already has)
        // shouldn't satisfy the payroll goal.
        let report = check_plan(
            &demo_model(),
            &demo_goal(),
            &Plan::assign("alice", "employee"),
        );
        assert!(report
            .violations
            .iter()
            .any(|v| v.invariant == InvariantKind::GoalSatisfied));
    }

    #[test]
    fn goal_reachable_on_demo() {
        assert!(check_goal_reachable(&demo_model(), &demo_goal()).is_none());
    }

    #[test]
    fn goal_unreachable_when_no_role_grants_it() {
        let m = demo_model();
        let infeasible = Goal {
            user: "alice".into(),
            action: "exfiltrate".into(),
            resource: "trade_secrets".into(),
        };
        assert!(check_goal_reachable(&m, &infeasible).is_some());
    }
}
