//! Signed receipts with hash-chained `prev_hash`, mirroring the
//! `nucleus-lineage::Proof { kid, alg, sig, prev_hash }` envelope.
//!
//! A receipt binds a (model, rules, goal, accepted-plan) tuple to a signed
//! decision. Receipts form a hash chain via `proof.prev_hash`, so tampering
//! with one row also invalidates every later receipt that referenced it.
//!
//! Wire format is JSON. Canonical bytes (what gets signed) are a NUL-
//! separated structural encoding — never raw JSON, so future cosmetic
//! changes to serialization can never break verification.

use core_ir::digest::Digest;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

pub const RECEIPT_VERSION: &str = "rubic-rbac-1";
pub const PROOF_ALG: &str = "Ed25519";

/// Domain-separation prefix prepended to every receipt's canonical bytes.
/// Ensures a signature over a rubic receipt can never be confused with a
/// signature over any other format that happens to use the same Proof
/// envelope (e.g. `nucleus-lineage::LineageEdge`). A third-party verifier
/// that reads the leading bytes knows immediately what payload schema to
/// expect — no field-by-field guessing, no cross-version forgery risk.
pub const DOMAIN_SEPARATOR: &[u8] = b"rubic-rbac-1\0";

// ── Proof envelope (nucleus-lineage shape) ──────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proof {
    pub kid: String,
    pub alg: String,
    #[serde(with = "base64_bytes")]
    pub sig: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "opt_hex32")]
    pub prev_hash: Option<[u8; 32]>,
}

// ── Receipt body ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Accepted,
    Rejected,
}

impl Decision {
    fn as_str(self) -> &'static str {
        match self {
            Decision::Accepted => "accepted",
            Decision::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptStep {
    pub op: String,
    pub user: String,
    pub role: String,
    pub justification: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rejection {
    pub candidate: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub receipt_version: String,
    #[serde(with = "hex32")]
    pub model_digest: Digest,
    #[serde(with = "hex32")]
    pub rules_digest: Digest,
    #[serde(with = "hex32")]
    pub goal_digest: Digest,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "opt_hex32")]
    pub accepted_plan_digest: Option<Digest>,
    /// RFC3339-formatted UTC timestamp.
    pub timestamp: String,
    pub candidate_count: u32,
    pub decision: Decision,
    pub steps: Vec<ReceiptStep>,
    pub rejections: Vec<Rejection>,
    pub proof: Proof,
}

impl Receipt {
    /// Canonical bytes that the proof signs over. Excludes `proof`.
    /// Stable across cosmetic JSON changes.
    ///
    /// Format:
    ///   `DOMAIN_SEPARATOR ‖ receipt_version\0 ‖ <fields...>`
    ///
    /// The domain separator at the front is what makes rubic receipts
    /// safely verifiable by the same Ed25519 verifier that handles
    /// `nucleus-lineage::Proof` payloads: the cryptographic layer is
    /// identical (`Proof { kid, alg, sig, prev_hash }`), but the bytes
    /// being signed are unambiguously labeled, so a verifier can refuse
    /// a payload whose domain it doesn't recognize.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(512);

        out.extend_from_slice(DOMAIN_SEPARATOR);
        push_field(&mut out, &self.receipt_version);
        out.extend_from_slice(&self.model_digest);
        out.push(0);
        out.extend_from_slice(&self.rules_digest);
        out.push(0);
        out.extend_from_slice(&self.goal_digest);
        out.push(0);
        match &self.accepted_plan_digest {
            Some(d) => out.extend_from_slice(d),
            None => out.extend_from_slice(&[0u8; 32]),
        }
        out.push(0);
        push_field(&mut out, &self.timestamp);
        out.extend_from_slice(&self.candidate_count.to_le_bytes());
        out.push(0);
        push_field(&mut out, self.decision.as_str());

        // Steps in declared order — order is part of the plan's identity.
        for s in &self.steps {
            push_field(&mut out, &s.op);
            push_field(&mut out, &s.user);
            push_field(&mut out, &s.role);
            push_field(&mut out, &s.justification);
            out.push(0); // step terminator
        }
        out.push(0); // section terminator (no more steps)

        for r in &self.rejections {
            push_field(&mut out, &r.candidate);
            push_field(&mut out, &r.reason);
            out.push(0);
        }
        out.push(0);

        out
    }

    /// Sign an unsigned receipt skeleton, producing a verifiable receipt.
    /// `kid` is a free-form identifier (e.g. the verifying key's hex prefix).
    pub fn sign(mut self, signing_key: &SigningKey, kid: impl Into<String>) -> Self {
        // Clear any pre-existing proof bytes so canonicalization is well-defined.
        self.proof = Proof {
            kid: String::new(),
            alg: PROOF_ALG.to_string(),
            sig: Vec::new(),
            prev_hash: self.proof.prev_hash, // preserve chain link
        };
        let bytes = self.canonical_bytes();
        let sig: Signature = signing_key.sign(&bytes);
        self.proof.kid = kid.into();
        self.proof.sig = sig.to_bytes().to_vec();
        self
    }

    /// Verify the receipt's signature against a public key.
    pub fn verify(&self, vk: &VerifyingKey) -> bool {
        let bytes = {
            // Re-canonicalize with proof.sig cleared, matching what `sign` did.
            let cleared = Receipt {
                proof: Proof {
                    kid: String::new(),
                    alg: self.proof.alg.clone(),
                    sig: Vec::new(),
                    prev_hash: self.proof.prev_hash,
                },
                ..self.clone()
            };
            cleared.canonical_bytes()
        };
        let sig_bytes: [u8; 64] = match self.proof.sig.as_slice().try_into() {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig = Signature::from_bytes(&sig_bytes);
        vk.verify(&bytes, &sig).is_ok()
    }

    /// Hash chain link: BLAKE3 over (canonical_bytes || sig). The next
    /// receipt should set `proof.prev_hash = Some(this_hash(receipt_n))`.
    pub fn this_hash(&self) -> Digest {
        let mut h = blake3::Hasher::new();
        h.update(&self.canonical_bytes());
        h.update(&self.proof.sig);
        *h.finalize().as_bytes()
    }

    /// Re-verify the chain link from this receipt to its predecessor.
    /// Returns true if `self.proof.prev_hash == this_hash(prev)`.
    pub fn chain_follows(&self, prev: &Receipt) -> bool {
        match self.proof.prev_hash {
            Some(p) => p == prev.this_hash(),
            None => false, // a chained receipt must declare its predecessor
        }
    }
}

/// Payload-agnostic Ed25519 verification — given the canonical bytes
/// (however they were computed), a Proof envelope, and a verifying key,
/// is this signature valid?
///
/// This is the function a generic verifier (or `nucleus-lineage`) would
/// call. The signature math doesn't care what bytes mean; it only cares
/// that they're the same bytes the signer used. Exposing this helper
/// makes explicit that rubic's Proof envelope is interchangeable with
/// any other Ed25519-based receipt format: pull out `proof`, recompute
/// the canonical bytes per the format's spec, call this function.
pub fn verify_canonical_bytes(
    bytes: &[u8],
    proof: &Proof,
    vk: &VerifyingKey,
) -> bool {
    let sig_bytes: [u8; 64] = match proof.sig.as_slice().try_into() {
        Ok(b) => b,
        Err(_) => return false,
    };
    let sig = Signature::from_bytes(&sig_bytes);
    vk.verify(bytes, &sig).is_ok()
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn push_field(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(s.as_bytes());
    out.push(0);
}

// ── serde modules for binary fields ────────────────────────────────────────

mod hex32 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex_encode(v))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        hex_decode(&s)
            .map_err(serde::de::Error::custom)
            .and_then(|b| {
                b.try_into()
                    .map_err(|v: Vec<u8>| serde::de::Error::custom(format!("len {}", v.len())))
            })
    }
    fn hex_encode(b: &[u8]) -> String {
        let mut s = String::with_capacity(b.len() * 2);
        for byte in b {
            s.push_str(&format!("{:02x}", byte));
        }
        s
    }
    fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
        if s.len() % 2 != 0 {
            return Err("odd hex length".to_string());
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
            .collect()
    }
}

mod opt_hex32 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &Option<[u8; 32]>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(b) => super::hex32::serialize(b, s),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<[u8; 32]>, D::Error> {
        let opt = Option::<String>::deserialize(d)?;
        match opt {
            None => Ok(None),
            Some(s) => {
                // delegate
                let de = serde::de::value::StrDeserializer::<D::Error>::new(&s);
                super::hex32::deserialize(de).map(Some)
            }
        }
    }
}

mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serializer};
    const STD: base64_lite::Engine = base64_lite::Engine;
    mod base64_lite {
        pub struct Engine;
        impl Engine {
            pub fn encode(&self, b: &[u8]) -> String {
                const T: &[u8; 64] =
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
                let mut out = String::with_capacity((b.len() + 2) / 3 * 4);
                let mut i = 0;
                while i + 3 <= b.len() {
                    let n = ((b[i] as u32) << 16) | ((b[i + 1] as u32) << 8) | (b[i + 2] as u32);
                    out.push(T[((n >> 18) & 63) as usize] as char);
                    out.push(T[((n >> 12) & 63) as usize] as char);
                    out.push(T[((n >> 6) & 63) as usize] as char);
                    out.push(T[(n & 63) as usize] as char);
                    i += 3;
                }
                if i < b.len() {
                    let rem = b.len() - i;
                    let n = ((b[i] as u32) << 16)
                        | (if rem > 1 { (b[i + 1] as u32) << 8 } else { 0 });
                    out.push(T[((n >> 18) & 63) as usize] as char);
                    out.push(T[((n >> 12) & 63) as usize] as char);
                    if rem == 2 {
                        out.push(T[((n >> 6) & 63) as usize] as char);
                    } else {
                        out.push('=');
                    }
                    out.push('=');
                }
                out
            }
            pub fn decode(&self, s: &str) -> Result<Vec<u8>, String> {
                fn idx(c: u8) -> Result<u8, String> {
                    Ok(match c {
                        b'A'..=b'Z' => c - b'A',
                        b'a'..=b'z' => c - b'a' + 26,
                        b'0'..=b'9' => c - b'0' + 52,
                        b'+' => 62,
                        b'/' => 63,
                        b'=' => 0,
                        other => return Err(format!("bad b64 char {other}")),
                    })
                }
                let bytes = s.as_bytes();
                if bytes.len() % 4 != 0 {
                    return Err("b64 length not multiple of 4".to_string());
                }
                let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
                for chunk in bytes.chunks(4) {
                    let a = idx(chunk[0])?;
                    let b = idx(chunk[1])?;
                    let c = idx(chunk[2])?;
                    let d = idx(chunk[3])?;
                    let n = ((a as u32) << 18)
                        | ((b as u32) << 12)
                        | ((c as u32) << 6)
                        | (d as u32);
                    out.push((n >> 16) as u8);
                    if chunk[2] != b'=' {
                        out.push((n >> 8) as u8);
                    }
                    if chunk[3] != b'=' {
                        out.push(n as u8);
                    }
                }
                Ok(out)
            }
        }
    }

    pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STD.encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STD.decode(&s).map_err(serde::de::Error::custom)
    }
}

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn keypair() -> SigningKey {
        SigningKey::generate(&mut OsRng)
    }

    fn skel(accepted: bool) -> Receipt {
        Receipt {
            receipt_version: RECEIPT_VERSION.to_string(),
            model_digest: [1u8; 32],
            rules_digest: [2u8; 32],
            goal_digest: [3u8; 32],
            accepted_plan_digest: if accepted { Some([4u8; 32]) } else { None },
            timestamp: "2026-05-17T10:00:00Z".to_string(),
            candidate_count: 3,
            decision: if accepted {
                Decision::Accepted
            } else {
                Decision::Rejected
            },
            steps: if accepted {
                vec![ReceiptStep {
                    op: "assign_role".to_string(),
                    user: "alice".to_string(),
                    role: "finance_viewer".to_string(),
                    justification: "derives CanReach(alice, read:payroll_summary)".to_string(),
                }]
            } else {
                vec![]
            },
            rejections: vec![Rejection {
                candidate: "payroll_admin".to_string(),
                reason: "grants forbidden permission delete:payroll".to_string(),
            }],
            proof: Proof {
                kid: String::new(),
                alg: PROOF_ALG.to_string(),
                sig: Vec::new(),
                prev_hash: None,
            },
        }
    }

    #[test]
    fn signed_receipt_verifies() {
        let sk = keypair();
        let r = skel(true).sign(&sk, "kid-test");
        assert!(r.verify(&sk.verifying_key()));
    }

    #[test]
    fn tampered_step_fails_verify() {
        let sk = keypair();
        let mut r = skel(true).sign(&sk, "kid-test");
        r.steps[0].role = "payroll_admin".to_string(); // adversarial swap
        assert!(!r.verify(&sk.verifying_key()));
    }

    #[test]
    fn tampered_digest_fails_verify() {
        let sk = keypair();
        let mut r = skel(true).sign(&sk, "kid-test");
        r.model_digest[0] ^= 0xFF;
        assert!(!r.verify(&sk.verifying_key()));
    }

    #[test]
    fn json_round_trip_preserves_verification() {
        let sk = keypair();
        let r = skel(true).sign(&sk, "kid-test");
        let json = serde_json::to_string(&r).unwrap();
        let back: Receipt = serde_json::from_str(&json).unwrap();
        assert!(back.verify(&sk.verifying_key()));
        assert_eq!(r, back);
    }

    #[test]
    fn chain_link_holds_then_breaks_on_tamper() {
        let sk = keypair();
        let r1 = skel(true).sign(&sk, "kid-test");
        let h1 = r1.this_hash();
        let r2 = {
            let mut s = skel(false);
            s.proof.prev_hash = Some(h1);
            s.sign(&sk, "kid-test")
        };
        assert!(r2.chain_follows(&r1));

        // Now tamper r1 — chain should break (h1 changes).
        let mut r1_tampered = r1.clone();
        r1_tampered.steps.clear();
        assert!(!r2.chain_follows(&r1_tampered));
    }

    #[test]
    fn wrong_key_fails_verify() {
        let sk1 = keypair();
        let sk2 = keypair();
        let r = skel(true).sign(&sk1, "kid-test");
        assert!(!r.verify(&sk2.verifying_key()));
    }

    #[test]
    fn rejected_receipt_has_no_accepted_plan_digest() {
        let sk = keypair();
        let r = skel(false).sign(&sk, "kid-test");
        assert!(r.accepted_plan_digest.is_none());
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            !json.contains("accepted_plan_digest"),
            "absent field should be omitted, got {json}"
        );
    }

    #[test]
    fn canonical_bytes_begin_with_domain_separator() {
        let r = skel(true);
        let bytes = r.canonical_bytes();
        assert!(
            bytes.starts_with(DOMAIN_SEPARATOR),
            "canonical bytes must lead with the domain separator so any \
             generic Ed25519 verifier can confirm the format before \
             attempting to interpret payload fields"
        );
    }

    #[test]
    fn receipt_version_is_namespaced() {
        // We bumped from `rbac-1` to `rubic-rbac-1` to reduce ambiguity
        // with any other RBAC-flavored receipt format in the ecosystem.
        assert_eq!(RECEIPT_VERSION, "rubic-rbac-1");
        let r = skel(true);
        assert_eq!(r.receipt_version, RECEIPT_VERSION);
    }

    /// Demonstrates the payload-agnostic verification path — the same
    /// shape a third-party verifier (or `nucleus-lineage`) would use:
    /// pull out `proof`, recompute canonical bytes via the format spec,
    /// verify. Nothing here depends on the `Receipt` Rust type.
    #[test]
    fn interop_verifier_can_authenticate_without_receipt_type() {
        let sk = keypair();
        let r = skel(true).sign(&sk, "kid-interop");

        // Serialize then deserialize to make sure the bytes round-trip
        // cleanly — what a foreign verifier would actually see.
        let json = serde_json::to_string(&r).unwrap();
        let back: Receipt = serde_json::from_str(&json).unwrap();

        // The foreign verifier extracts the proof, recomputes the
        // canonical bytes from the receipt payload, and calls the
        // generic helper. This is exactly what nucleus-lineage's
        // verifier shape would do.
        let canonical = back.canonical_bytes();
        let proof = back.proof.clone();
        let vk = sk.verifying_key();

        assert!(
            verify_canonical_bytes(&canonical, &proof, &vk),
            "payload-agnostic verification must succeed"
        );

        // Tampering anywhere in the canonical bytes — including in our
        // domain separator — must break the signature.
        let mut tampered = canonical.clone();
        tampered[0] ^= 0xFF;
        assert!(
            !verify_canonical_bytes(&tampered, &proof, &vk),
            "a single-bit flip in the domain separator must invalidate"
        );
    }
}
