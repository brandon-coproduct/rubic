//! MCP (Model Context Protocol) surface for rubic.
//!
//! Mounted at `/mcp` on the same axum server. Users add one line to their
//! Claude config:
//!
//! ```json
//! { "mcpServers": { "rubic": { "url": "https://rubic.fly.dev/mcp" } } }
//! ```
//!
//! Tools exposed:
//!   - `propose_assignment(model_toml, goal)` — runs the full planner
//!     pipeline (policy + egglog + sign) on the user's own model.
//!   - `verify_receipt(receipt_json)` — checks signature against the
//!     server's Ed25519 verifying key.
//!
//! The MCP service is stateless across calls — each invocation parses its
//! own model, so users can experiment with bespoke RBAC schemas without
//! mutating the deployed demo model.

use std::sync::Arc;

use core_ir::digest::{bytes_digest, goal_digest, hex, model_digest, plan_digest};
use core_ir::{EntityId, Goal, Model};
use planner::plan;
use receipt::{Decision, Proof, Receipt, ReceiptStep, Rejection, PROOF_ALG, RECEIPT_VERSION};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    ErrorData as McpError, ServerHandler,
};
use serde_json::json;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::state::AppState;

/// MCP-facing goal — flat shape so we don't have to leak `core_ir::Goal`'s
/// JsonSchema impl across crates. We convert at the boundary.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct McpGoal {
    /// User id (must exist in the model's `[users.*]` table).
    pub user: String,
    /// Action verb, e.g. `read`, `write`, `delete`.
    pub action: String,
    /// Resource identifier, e.g. `payroll_summary`.
    pub resource: String,
}

impl From<McpGoal> for Goal {
    fn from(g: McpGoal) -> Self {
        Goal {
            user: EntityId::new(g.user),
            action: g.action,
            resource: g.resource,
        }
    }
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ProposeArgs {
    /// Full RBAC model as a TOML string (same schema as the demo's
    /// `examples/rbac_demo.toml` — `[users.*]`, `[roles.*]`, `[policy]`).
    pub model_toml: String,
    /// Who wants to do what to which resource.
    pub goal: McpGoal,
    /// Max number of ranked candidates to consider. Defaults to 5.
    #[serde(default)]
    pub top_n: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct VerifyArgs {
    /// A signed receipt JSON document (from `propose_assignment`'s output
    /// or the web UI's "Download" button).
    pub receipt_json: String,
}

#[derive(Clone)]
pub struct RubicMcp {
    state: Arc<AppState>,
    // Populated by the `#[tool_router]` macro; consumed indirectly by the
    // `#[tool_handler]` impl when MCP routes tool calls. Reads via the
    // macro-expanded path don't show up in dead_code analysis.
    #[allow(dead_code)]
    tool_router: ToolRouter<RubicMcp>,
}

#[tool_router]
impl RubicMcp {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Propose and validate a least-privilege role assignment for an RBAC \
                       goal. Runs the deterministic planner pipeline: enumerates candidates, \
                       runs policy invariant checks + egglog reachability per candidate, \
                       returns the ranked list plus a signed receipt that cryptographically \
                       witnesses the decision."
    )]
    async fn propose_assignment(
        &self,
        Parameters(args): Parameters<ProposeArgs>,
    ) -> Result<CallToolResult, McpError> {
        let model = Model::from_toml_str(&args.model_toml)
            .map_err(|e| McpError::invalid_params(format!("model_toml parse: {e}"), None))?;
        let goal: Goal = args.goal.into();

        let outcome = plan(&model, &goal, args.top_n.unwrap_or(5))
            .map_err(|e| McpError::internal_error(format!("planner: {e}"), None))?;

        let model_d = model_digest(&model);
        let rules_d = bytes_digest(egglog_engine::RULES_SRC.as_bytes());
        let goal_d = goal_digest(&goal);
        let accepted_plan_d = outcome.accepted().map(|c| plan_digest(&c.plan));

        let decision = if outcome.accepted().is_some() {
            Decision::Accepted
        } else {
            Decision::Rejected
        };

        let steps: Vec<ReceiptStep> = outcome
            .accepted()
            .map(|c| {
                c.plan
                    .steps
                    .iter()
                    .map(|step| match step {
                        core_ir::PlanStep::AssignRole { user, role } => ReceiptStep {
                            op: "assign_role".to_string(),
                            user: user.to_string(),
                            role: role.to_string(),
                            justification: format!(
                                "via mcp: egglog derives CanReach({user}, target); \
                                 policy clean; +{} permission(s) over current set",
                                c.granted_delta.len()
                            ),
                        },
                    })
                    .collect()
            })
            .unwrap_or_default();

        let rejections: Vec<Rejection> = outcome
            .candidates
            .iter()
            .filter(|c| !c.accepted)
            .map(|c| Rejection {
                candidate: c.role.to_string(),
                reason: c
                    .violations
                    .iter()
                    .map(|v| format!("{:?}: {}", v.invariant, v.explanation))
                    .collect::<Vec<_>>()
                    .join("; "),
            })
            .collect();

        let now = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

        let skeleton = Receipt {
            receipt_version: RECEIPT_VERSION.to_string(),
            model_digest: model_d,
            rules_digest: rules_d,
            goal_digest: goal_d,
            accepted_plan_digest: accepted_plan_d,
            timestamp: now,
            candidate_count: outcome.candidates.len() as u32,
            decision,
            steps,
            rejections,
            proof: Proof {
                kid: String::new(),
                alg: PROOF_ALG.to_string(),
                sig: Vec::new(),
                prev_hash: None, // MCP receipts aren't part of the SQLite chain
            },
        };
        let signed = skeleton.sign(&self.state.signing_key, &self.state.kid);

        // Strip the trace's egraph JSON from the MCP payload — it's
        // visualization-only and bloats the wire response. Keep the rest
        // of the outcome.
        let outcome_lean = json!({
            "goal_unreachable": outcome.goal_unreachable,
            "candidates": outcome.candidates,
            "accepted_index": outcome.accepted_index,
            "trace_frames": outcome.trace.len(),
        });

        let payload = json!({
            "outcome": outcome_lean,
            "receipt": signed,
            "model_digest": hex(&model_d),
            "rules_digest": hex(&rules_d),
            "goal_digest": hex(&goal_d),
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Verify a rubic receipt: parses the JSON, recomputes its canonical \
                       bytes, and checks the Ed25519 signature against this server's \
                       verifying key. Returns {signature_valid, kid, decision, notes}."
    )]
    async fn verify_receipt(
        &self,
        Parameters(args): Parameters<VerifyArgs>,
    ) -> Result<CallToolResult, McpError> {
        let r: Receipt = serde_json::from_str(&args.receipt_json)
            .map_err(|e| McpError::invalid_params(format!("receipt_json parse: {e}"), None))?;
        let signature_valid = r.verify(&self.state.signing_key.verifying_key());
        let payload = json!({
            "signature_valid": signature_valid,
            "kid_in_receipt": r.proof.kid,
            "kid_on_server":  self.state.kid,
            "decision": r.decision,
            "candidate_count": r.candidate_count,
            "notes": if signature_valid {
                vec!["signature checks out against the server's current key".to_string()]
            } else {
                vec![
                    "signature failed — either tampered, or signed by a different key".to_string(),
                ]
            },
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_default(),
        )]))
    }
}

#[tool_handler]
impl ServerHandler for RubicMcp {
    fn get_info(&self) -> ServerInfo {
        // `ServerInfo` is `#[non_exhaustive]`, so we can't use struct
        // literal syntax — build from Default and mutate.
        let mut info = ServerInfo::default();
        info.instructions = Some(
            "rubic — verified agentic RBAC planner. Propose role \
             assignments and verify signed decisions. The planner is \
             deterministic and cryptographically binds the model + rules \
             + goal into the receipt."
                .into(),
        );
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info
    }
}
