//! Deterministic canonical-bytes encoding for the IR.
//!
//! Pattern transcribed (not depended upon) from `nucleus-lineage::proof`:
//! - NUL-separated fields
//! - All collections sorted before emission
//! - Empty marker (single NUL) between variable-length sections so future
//!   appended fields cannot shift the meaning of trailing bytes
//! - Sentinel zero bytes for absent optional values
//!
//! Section order is locked. Adding fields requires appending a new section,
//! never reordering. This keeps already-issued receipts verifiable.

use crate::{Goal, Model, Permission, Plan, PlanStep};

const SECTION_TERMINATOR: u8 = 0;

fn push_field(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(s.as_bytes());
    out.push(0);
}

fn push_perm(out: &mut Vec<u8>, p: &Permission) {
    // action:resource (the colon is fine — neither side may contain NUL)
    push_field(out, &p.render());
}

/// Canonical bytes for a `Model`. The bytes a receipt signs over for the
/// `model_digest` field.
pub fn canonical_model_bytes(m: &Model) -> Vec<u8> {
    let mut out = Vec::with_capacity(1024);

    // §1 policy.least_privilege
    out.push(if m.policy.least_privilege { 1 } else { 0 });
    out.push(SECTION_TERMINATOR);

    // §2 forbidden_permissions (sorted)
    let mut forb = m.policy.forbidden_permissions.clone();
    forb.sort();
    for p in &forb {
        push_perm(&mut out, p);
    }
    out.push(SECTION_TERMINATOR);

    // §3 requires_approval (sorted)
    let mut req = m.policy.requires_approval.clone();
    req.sort();
    for p in &req {
        push_perm(&mut out, p);
    }
    out.push(SECTION_TERMINATOR);

    // §4 users (sorted; roles inside each user sorted)
    for (uid, user) in &m.users {
        push_field(&mut out, uid.as_str());
        let mut roles = user.roles.clone();
        roles.sort();
        for r in &roles {
            push_field(&mut out, r.as_str());
        }
        out.push(SECTION_TERMINATOR);
    }
    out.push(SECTION_TERMINATOR);

    // §5 roles (sorted; permissions inside each role sorted)
    for (rid, role) in &m.roles {
        push_field(&mut out, rid.as_str());
        let mut perms = role.permissions.clone();
        perms.sort();
        for p in &perms {
            push_perm(&mut out, p);
        }
        out.push(SECTION_TERMINATOR);
    }
    out.push(SECTION_TERMINATOR);

    out
}

/// Canonical bytes for a `Goal`. Distinct field separators so a permission
/// like `"read:foo"` and a goal `(read, foo)` hash differently.
pub fn canonical_goal_bytes(g: &Goal) -> Vec<u8> {
    let mut out = Vec::with_capacity(128);
    push_field(&mut out, g.user.as_str());
    push_field(&mut out, &g.action);
    push_field(&mut out, &g.resource);
    out
}

/// Canonical bytes for a `Plan`. Steps are emitted in given order — order is
/// part of the plan's identity.
pub fn canonical_plan_bytes(plan: &Plan) -> Vec<u8> {
    let mut out = Vec::with_capacity(128);
    for step in &plan.steps {
        match step {
            PlanStep::AssignRole { user, role } => {
                push_field(&mut out, "assign_role");
                push_field(&mut out, user.as_str());
                push_field(&mut out, role.as_str());
            }
        }
        out.push(SECTION_TERMINATOR);
    }
    out.push(SECTION_TERMINATOR);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityId;

    fn demo_model() -> Model {
        Model::from_toml_str(include_str!("../../../examples/rbac_demo.toml")).unwrap()
    }

    #[test]
    fn model_bytes_are_deterministic() {
        let m = demo_model();
        assert_eq!(canonical_model_bytes(&m), canonical_model_bytes(&m));
    }

    #[test]
    fn model_bytes_change_when_forbidden_changes() {
        let mut a = demo_model();
        let b = a.clone();
        a.policy
            .forbidden_permissions
            .push(Permission::new("write", "audit_log"));
        assert_ne!(canonical_model_bytes(&a), canonical_model_bytes(&b));
    }

    #[test]
    fn model_bytes_change_when_user_role_added() {
        let mut a = demo_model();
        let b = a.clone();
        a.users
            .get_mut(&EntityId::from("alice"))
            .unwrap()
            .roles
            .push(EntityId::from("finance_viewer"));
        assert_ne!(canonical_model_bytes(&a), canonical_model_bytes(&b));
    }

    #[test]
    fn user_role_order_inside_user_does_not_matter() {
        // Two semantically equivalent models (same roles, different declared
        // order in the user list) must produce identical bytes.
        let mut a = demo_model();
        a.users
            .get_mut(&EntityId::from("alice"))
            .unwrap()
            .roles
            .extend([EntityId::from("finance_viewer"), EntityId::from("employee")]);
        a.users
            .get_mut(&EntityId::from("alice"))
            .unwrap()
            .roles
            .sort();
        a.users
            .get_mut(&EntityId::from("alice"))
            .unwrap()
            .roles
            .dedup();

        let mut b = demo_model();
        b.users
            .get_mut(&EntityId::from("alice"))
            .unwrap()
            .roles
            .extend([EntityId::from("employee"), EntityId::from("finance_viewer")]);
        b.users
            .get_mut(&EntityId::from("alice"))
            .unwrap()
            .roles
            .sort();
        b.users
            .get_mut(&EntityId::from("alice"))
            .unwrap()
            .roles
            .dedup();

        assert_eq!(canonical_model_bytes(&a), canonical_model_bytes(&b));
    }

    #[test]
    fn goal_bytes_change_with_action() {
        let g1 = Goal {
            user: "alice".into(),
            action: "read".into(),
            resource: "payroll_summary".into(),
        };
        let g2 = Goal {
            user: "alice".into(),
            action: "write".into(),
            resource: "payroll_summary".into(),
        };
        assert_ne!(canonical_goal_bytes(&g1), canonical_goal_bytes(&g2));
    }

    #[test]
    fn plan_bytes_change_with_role() {
        let p1 = Plan::assign("alice", "finance_viewer");
        let p2 = Plan::assign("alice", "payroll_admin");
        assert_ne!(canonical_plan_bytes(&p1), canonical_plan_bytes(&p2));
    }
}
