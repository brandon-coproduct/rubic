//! IR for the Verified Agentic RBAC Planner.
//!
//! Loads a semantic model from TOML, normalizes it deterministically, and
//! produces a BLAKE3 digest that uniquely identifies the model state. The
//! canonical-bytes encoding mirrors `nucleus-lineage`: NUL-separated, sorted
//! collections, with sentinel zero bytes for absent fields. Two semantically
//! equal models always hash identically.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub mod canonical;
pub mod digest;

// ── Identifiers ─────────────────────────────────────────────────────────────

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct EntityId(pub String);

impl EntityId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for EntityId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Permission ──────────────────────────────────────────────────────────────

/// `action:resource` — e.g. `read:payroll_summary`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Permission {
    pub action: String,
    pub resource: String,
}

impl Permission {
    pub fn new(action: impl Into<String>, resource: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            resource: resource.into(),
        }
    }

    /// Parse `"action:resource"` from TOML strings. Returns `None` if the
    /// input lacks a single colon.
    pub fn parse(s: &str) -> Option<Self> {
        let (a, r) = s.split_once(':')?;
        if a.is_empty() || r.is_empty() {
            return None;
        }
        Some(Self::new(a, r))
    }

    pub fn render(&self) -> String {
        format!("{}:{}", self.action, self.resource)
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.action, self.resource)
    }
}

// ── Core entities ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: EntityId,
    /// Roles currently assigned to the user.
    #[serde(default)]
    pub roles: Vec<EntityId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub id: EntityId,
    #[serde(default)]
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub user: EntityId,
    pub action: String,
    pub resource: String,
}

impl Goal {
    pub fn permission(&self) -> Permission {
        Permission::new(&self.action, &self.resource)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Policy {
    #[serde(default)]
    pub least_privilege: bool,
    #[serde(default)]
    pub forbidden_permissions: Vec<Permission>,
    #[serde(default)]
    pub requires_approval: Vec<Permission>,
}

// ── Plan ────────────────────────────────────────────────────────────────────

/// An atomic change a plan can request. Today we only need role assignment;
/// future ops (revoke, create_role) slot in as new variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PlanStep {
    AssignRole { user: EntityId, role: EntityId },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Plan {
    pub steps: Vec<PlanStep>,
}

impl Plan {
    pub fn assign(user: impl Into<EntityId>, role: impl Into<EntityId>) -> Self {
        Self {
            steps: vec![PlanStep::AssignRole {
                user: user.into(),
                role: role.into(),
            }],
        }
    }
}

// ── Model ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Model {
    pub users: BTreeMap<EntityId, User>,
    pub roles: BTreeMap<EntityId, Role>,
    #[serde(default)]
    pub policy: Policy,
    /// Optional default goal carried alongside the model for the demo file.
    /// Production callers POST their own goal to `/api/plan`.
    #[serde(default)]
    pub goal: Option<Goal>,
}

impl Model {
    /// Load from a TOML file using the demo schema (`[users.<id>]`,
    /// `[roles.<id>]`, `[policy]`, `[goal]`).
    pub fn from_toml_str(input: &str) -> Result<Self, ModelError> {
        let wire: WireModel = toml::from_str(input).map_err(ModelError::Toml)?;
        Ok(wire.into_model())
    }

    pub fn from_toml_path(path: impl AsRef<Path>) -> Result<Self, ModelError> {
        let text = std::fs::read_to_string(path).map_err(ModelError::Io)?;
        Self::from_toml_str(&text)
    }

    /// Resolved permission set granted to a user under the current model
    /// (union of all assigned roles' permissions).
    pub fn permissions_of(&self, user: &EntityId) -> Vec<Permission> {
        let Some(u) = self.users.get(user) else {
            return Vec::new();
        };
        let mut out: Vec<Permission> = Vec::new();
        for r in &u.roles {
            if let Some(role) = self.roles.get(r) {
                for p in &role.permissions {
                    if !out.contains(p) {
                        out.push(p.clone());
                    }
                }
            }
        }
        out.sort();
        out
    }
}

// ── TOML wire format ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct WireModel {
    #[serde(default)]
    users: BTreeMap<String, WireUser>,
    #[serde(default)]
    roles: BTreeMap<String, WireRole>,
    #[serde(default)]
    policy: WirePolicy,
    #[serde(default)]
    goal: Option<WireGoal>,
}

#[derive(Debug, Default, Deserialize)]
struct WireUser {
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WireRole {
    #[serde(default)]
    permissions: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WirePolicy {
    #[serde(default)]
    least_privilege: bool,
    #[serde(default)]
    forbidden_permissions: Vec<String>,
    #[serde(default)]
    requires_approval: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct WireGoal {
    user: String,
    action: String,
    resource: String,
}

impl WireModel {
    fn into_model(self) -> Model {
        let users = self
            .users
            .into_iter()
            .map(|(id, u)| {
                let eid = EntityId::new(&id);
                (
                    eid.clone(),
                    User {
                        id: eid,
                        roles: u.roles.into_iter().map(EntityId::new).collect(),
                    },
                )
            })
            .collect();
        let roles = self
            .roles
            .into_iter()
            .map(|(id, r)| {
                let eid = EntityId::new(&id);
                (
                    eid.clone(),
                    Role {
                        id: eid,
                        permissions: r
                            .permissions
                            .into_iter()
                            .filter_map(|s| Permission::parse(&s))
                            .collect(),
                    },
                )
            })
            .collect();
        let policy = Policy {
            least_privilege: self.policy.least_privilege,
            forbidden_permissions: self
                .policy
                .forbidden_permissions
                .into_iter()
                .filter_map(|s| Permission::parse(&s))
                .collect(),
            requires_approval: self
                .policy
                .requires_approval
                .into_iter()
                .filter_map(|s| Permission::parse(&s))
                .collect(),
        };
        let goal = self.goal.map(|g| Goal {
            user: EntityId::new(g.user),
            action: g.action,
            resource: g.resource,
        });
        Model {
            users,
            roles,
            policy,
            goal,
        }
    }
}

// ── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("toml parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO: &str = include_str!("../../../examples/rbac_demo.toml");

    #[test]
    fn parses_demo_model() {
        let m = Model::from_toml_str(DEMO).expect("parse");
        assert!(m.users.contains_key(&EntityId::from("alice")));
        assert!(m.roles.contains_key(&EntityId::from("finance_viewer")));
        assert!(m.roles.contains_key(&EntityId::from("payroll_admin")));
        let payroll_admin = &m.roles[&EntityId::from("payroll_admin")];
        assert!(payroll_admin
            .permissions
            .contains(&Permission::new("delete", "payroll")));
        assert_eq!(
            m.goal.as_ref().unwrap().permission(),
            Permission::new("read", "payroll_summary")
        );
    }

    #[test]
    fn alice_starts_with_read_profile_only() {
        let m = Model::from_toml_str(DEMO).unwrap();
        let perms = m.permissions_of(&EntityId::from("alice"));
        assert_eq!(perms, vec![Permission::new("read", "profile")]);
    }

    #[test]
    fn permission_parse_rejects_malformed() {
        assert!(Permission::parse("noseparator").is_none());
        assert!(Permission::parse(":").is_none());
        assert!(Permission::parse(":res").is_none());
        assert!(Permission::parse("act:").is_none());
        assert_eq!(
            Permission::parse("read:payroll_summary").unwrap(),
            Permission::new("read", "payroll_summary")
        );
    }
}
