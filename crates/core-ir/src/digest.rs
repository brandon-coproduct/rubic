//! BLAKE3 digests over canonical bytes.

use crate::canonical::{canonical_goal_bytes, canonical_model_bytes, canonical_plan_bytes};
use crate::{Goal, Model, Plan};

pub type Digest = [u8; 32];

pub fn model_digest(m: &Model) -> Digest {
    *blake3::hash(&canonical_model_bytes(m)).as_bytes()
}

pub fn goal_digest(g: &Goal) -> Digest {
    *blake3::hash(&canonical_goal_bytes(g)).as_bytes()
}

pub fn plan_digest(p: &Plan) -> Digest {
    *blake3::hash(&canonical_plan_bytes(p)).as_bytes()
}

/// BLAKE3 of arbitrary bytes — used by callers to digest the egglog ruleset
/// source so it can be bound into the receipt.
pub fn bytes_digest(b: &[u8]) -> Digest {
    *blake3::hash(b).as_bytes()
}

pub fn hex(d: &Digest) -> String {
    let mut s = String::with_capacity(64);
    for byte in d {
        s.push_str(&format!("{:02x}", byte));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_model() -> Model {
        Model::from_toml_str(include_str!("../../../examples/rbac_demo.toml")).unwrap()
    }

    #[test]
    fn model_digest_is_stable() {
        let m = demo_model();
        assert_eq!(model_digest(&m), model_digest(&m));
    }

    #[test]
    fn model_digest_changes_when_model_changes() {
        let mut m = demo_model();
        let before = model_digest(&m);
        m.policy.least_privilege = !m.policy.least_privilege;
        assert_ne!(before, model_digest(&m));
    }

    #[test]
    fn hex_is_64_chars() {
        let m = demo_model();
        assert_eq!(hex(&model_digest(&m)).len(), 64);
    }
}
