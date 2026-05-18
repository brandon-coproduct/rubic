use std::sync::Arc;

use axum::{
    extract::{Path as AxPath, State},
    response::Json,
    routing::{get, post},
    Router,
};
use agent::{AgentClient, AgentResponse};
use core_ir::digest::{bytes_digest, goal_digest, hex, model_digest, plan_digest};
use core_ir::{EntityId, Goal, Model, Plan, PlanStep};
use planner::{graph, plan, EgraphSnapshot, PlanCandidate, PlanningOutcome, SnapshotKind};
use receipt::{Decision, Proof, Receipt, ReceiptStep, Rejection, PROOF_ALG, RECEIPT_VERSION};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::error::{ApiError, ApiResult};
use crate::state::SharedState;

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/model", get(get_model).post(set_model))
        .route("/api/model/load", post(set_model))
        .route("/api/plan", post(post_plan))
        .route("/api/agent/propose", post(post_agent_propose))
        .route("/api/replays", get(get_replays))
        .route("/api/validate", post(post_validate))
        .route("/api/graph", post(post_graph))
        .route("/api/receipt/{id}", get(get_receipt))
        .route("/api/receipt/{id}/verify", post(verify_receipt))
        .with_state(state)
}

// ── /healthz ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct Health {
    status: &'static str,
    kid: String,
    key_path: String,
}

async fn healthz(State(s): State<SharedState>) -> Json<Health> {
    Json(Health {
        status: "ok",
        kid: s.kid.clone(),
        key_path: s.key_path.display().to_string(),
    })
}

// ── /api/model ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ModelView {
    model: Model,
    digest: String,
    rules_digest: String,
}

async fn get_model(State(s): State<SharedState>) -> Json<ModelView> {
    let model = s.model.read().await.clone();
    let md = hex(&model_digest(&model));
    let rd = hex(&bytes_digest(egglog_engine::RULES_SRC.as_bytes()));
    Json(ModelView {
        model,
        digest: md,
        rules_digest: rd,
    })
}

#[derive(Deserialize)]
struct ModelLoad {
    /// Either inline TOML text…
    #[serde(default)]
    toml: Option<String>,
}

async fn set_model(
    State(s): State<SharedState>,
    Json(body): Json<ModelLoad>,
) -> ApiResult<Json<ModelView>> {
    let toml = body
        .toml
        .ok_or_else(|| ApiError::BadRequest("missing field: toml".into()))?;
    let m = Model::from_toml_str(&toml)?;
    *s.model.write().await = m.clone();
    let md = hex(&model_digest(&m));
    let rd = hex(&bytes_digest(egglog_engine::RULES_SRC.as_bytes()));
    Ok(Json(ModelView {
        model: m,
        digest: md,
        rules_digest: rd,
    }))
}

// ── /api/plan ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PlanRequest {
    goal: Goal,
    #[serde(default = "default_top_n")]
    top_n: usize,
    /// When false, server validates but does not persist the receipt.
    #[serde(default = "default_true")]
    persist: bool,
}

fn default_top_n() -> usize {
    3
}
fn default_true() -> bool {
    true
}

#[derive(Serialize)]
struct PlanResponse {
    outcome: PlanningOutcome,
    receipt: Receipt,
    receipt_id: Option<i64>,
    model_digest: String,
    rules_digest: String,
    goal_digest: String,
}

async fn post_plan(
    State(s): State<SharedState>,
    Json(req): Json<PlanRequest>,
) -> ApiResult<Json<PlanResponse>> {
    let model = s.model.read().await.clone();
    let outcome = plan(&model, &req.goal, req.top_n)?;

    let model_d = model_digest(&model);
    let rules_d = bytes_digest(egglog_engine::RULES_SRC.as_bytes());
    let goal_d = goal_digest(&req.goal);

    let decision = if outcome.accepted().is_some() {
        Decision::Accepted
    } else {
        Decision::Rejected
    };

    let accepted_plan_digest = outcome.accepted().map(|c| plan_digest(&c.plan));

    let steps: Vec<ReceiptStep> = outcome
        .accepted()
        .map(|c| candidate_to_steps(c))
        .unwrap_or_default();

    let rejections: Vec<Rejection> = outcome
        .candidates
        .iter()
        .filter(|c| !c.accepted)
        .map(|c| Rejection {
            candidate: c.role.to_string(),
            reason: rejection_reason(c),
        })
        .collect();

    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

    let prev_hash = latest_chain_head(&s.db).await?;

    let skeleton = Receipt {
        receipt_version: RECEIPT_VERSION.to_string(),
        model_digest: model_d,
        rules_digest: rules_d,
        goal_digest: goal_d,
        accepted_plan_digest,
        timestamp: now.clone(),
        candidate_count: outcome.candidates.len() as u32,
        decision,
        steps,
        rejections,
        proof: Proof {
            kid: String::new(),
            alg: PROOF_ALG.to_string(),
            sig: Vec::new(),
            prev_hash,
        },
    };
    let signed = skeleton.sign(&s.signing_key, &s.kid);

    let mut receipt_id = None;
    if req.persist {
        let json = serde_json::to_string(&signed).map_err(|e| ApiError::Internal(e.to_string()))?;
        let this_hash = signed.this_hash();
        let row = sqlx::query(
            "INSERT INTO receipts (receipt_json, this_hash, prev_hash, created_at) \
             VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(&json)
        .bind(&this_hash[..])
        .bind(prev_hash.map(|h| h.to_vec()))
        .bind(&now)
        .fetch_one(&s.db)
        .await?;
        let id: i64 = sqlx::Row::get(&row, "id");
        receipt_id = Some(id);
    }

    Ok(Json(PlanResponse {
        outcome,
        receipt: signed,
        receipt_id,
        model_digest: hex(&model_d),
        rules_digest: hex(&rules_d),
        goal_digest: hex(&goal_d),
    }))
}

fn candidate_to_steps(c: &PlanCandidate) -> Vec<ReceiptStep> {
    candidate_to_steps_tagged(c, None)
}

/// Variant that appends an agent-provenance tag (the BLAKE3 hex of the
/// agent's raw JSON output) into the step's `justification`. The
/// justification string is part of the receipt's canonical bytes, so the
/// tag is cryptographically signed alongside the rest of the receipt —
/// tamper-evident binding of the (untrusted) agent output to the (signed)
/// server decision, without changing the receipt wire schema.
fn candidate_to_steps_tagged(
    c: &PlanCandidate,
    agent_digest_hex: Option<&str>,
) -> Vec<ReceiptStep> {
    c.plan
        .steps
        .iter()
        .map(|step| match step {
            core_ir::PlanStep::AssignRole { user, role } => {
                let base = format!(
                    "egglog derives CanReach({user}, target); policy clean; \
                     +{} permission(s) over current set",
                    c.granted_delta.len()
                );
                let justification = match agent_digest_hex {
                    Some(h) => format!("{base}; agent_proposal_digest={h}"),
                    None => base,
                };
                ReceiptStep {
                    op: "assign_role".to_string(),
                    user: user.to_string(),
                    role: role.to_string(),
                    justification,
                }
            }
        })
        .collect()
}

fn rejection_reason(c: &PlanCandidate) -> String {
    if c.violations.is_empty() && !c.derives_goal {
        return "candidate does not reach goal under egglog rules".to_string();
    }
    let reasons: Vec<String> = c
        .violations
        .iter()
        .map(|v| format!("{:?}: {}", v.invariant, v.explanation))
        .collect();
    reasons.join("; ")
}

async fn latest_chain_head(db: &sqlx::SqlitePool) -> ApiResult<Option<[u8; 32]>> {
    let row: Option<(Vec<u8>,)> =
        sqlx::query_as("SELECT this_hash FROM receipts ORDER BY id DESC LIMIT 1")
            .fetch_optional(db)
            .await?;
    Ok(row.and_then(|(b,)| b.try_into().ok()))
}

// ── /api/agent/propose ──────────────────────────────────────────────────────
//
// Trust model: the agent is untrusted. It returns role names + reasoning;
// we send each proposed role through the SAME validation pipeline as
// /api/plan (policy invariants + egglog reachability). Roles the agent
// invented or that violate policy get rejected with structured reasons.
// The agent's raw output is hashed and embedded in the signed receipt so
// the decision cryptographically witnesses exactly what the LLM produced.

#[derive(Deserialize)]
struct AgentProposeRequest {
    goal: Goal,
    #[serde(default = "default_true")]
    persist: bool,
}

#[derive(Serialize)]
struct AgentProposeResponse {
    agent: AgentResponse,
    agent_proposal_digest: String,
    /// Set when the agent response came from a pre-recorded replay rather
    /// than a live `claude -p` call. Frontend uses this to render a small
    /// "replay" badge so visitors know what they're seeing.
    replay_id: Option<String>,
    outcome: PlanningOutcome,
    receipt: Receipt,
    receipt_id: Option<i64>,
    model_digest: String,
    rules_digest: String,
    goal_digest: String,
}

async fn post_agent_propose(
    State(s): State<SharedState>,
    Json(req): Json<AgentProposeRequest>,
) -> ApiResult<Json<AgentProposeResponse>> {
    let model = s.model.read().await.clone();

    if !model.users.contains_key(&req.goal.user) {
        return Err(ApiError::BadRequest(format!(
            "user `{}` is not declared in the model",
            req.goal.user
        )));
    }

    // 1. Resolve the agent response.
    //    Prod path: serve a pre-recorded replay so visitors don't burn
    //    Anthropic tokens. Off-script goals return 404 with available
    //    goals, which the frontend renders as chips.
    //    Local dev (RUBIC_ALLOW_LIVE_AGENT=1): fall through to live `claude
    //    -p` on cache miss.
    let (agent_resp, replay_id) = match s.replays.find(&req.goal) {
        Some(r) => (r.agent.clone(), Some(r.id.clone())),
        None => {
            if s.allow_live_agent {
                let client = AgentClient::new();
                (client.propose(&model, &req.goal).await?, None)
            } else {
                let available: Vec<&Goal> = s.replays.available_goals();
                return Err(ApiError::BadRequest(format!(
                    "no recorded session for this goal. Try one of: {}",
                    available
                        .iter()
                        .map(|g| format!("{}/{}/{}", g.user, g.action, g.resource))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
    };
    let agent_digest = agent_resp.digest();
    let agent_digest_hex = hex_bytes(&agent_digest);

    // 2. Validate each agent-proposed role through the SAME pipeline as
    //    /api/plan: policy::check_plan + Engine::derives_goal. The agent's
    //    ordering is preserved so the UI can show its preferences.
    let current = model.permissions_of(&req.goal.user);
    let mut candidates: Vec<PlanCandidate> = Vec::new();
    let mut trace: Vec<EgraphSnapshot> = Vec::new();

    // Frame 0: initial egraph (no candidate applied) — same convention as
    // `planner::plan()`.
    {
        let mut e0 = egglog_engine::Engine::new(&model)
            .map_err(|e| ApiError::Internal(format!("engine init: {e}")))?;
        let graph_json = e0
            .serialize_snapshot()
            .map_err(|e| ApiError::Internal(format!("engine serialize: {e}")))?;
        trace.push(EgraphSnapshot {
            label: "initial: model facts asserted".to_string(),
            kind: SnapshotKind::Initial,
            graph: graph_json,
        });
    }

    for proposal in &agent_resp.proposals {
        let role_id = EntityId::new(&proposal.role);
        let plan_step = Plan {
            steps: vec![PlanStep::AssignRole {
                user: req.goal.user.clone(),
                role: role_id.clone(),
            }],
        };

        let granted_delta: Vec<core_ir::Permission> = match model.roles.get(&role_id) {
            Some(r) => r
                .permissions
                .iter()
                .filter(|p| !current.contains(p))
                .cloned()
                .collect(),
            None => Vec::new(),
        };

        let report = policy::check_plan(&model, &req.goal, &plan_step);

        // Fresh engine per candidate so facts don't bleed.
        let mut engine = egglog_engine::Engine::new(&model)
            .map_err(|e| ApiError::Internal(format!("engine init: {e}")))?;
        engine
            .apply_plan(&plan_step)
            .map_err(|e| ApiError::Internal(format!("engine apply: {e}")))?;
        let derives_goal = engine
            .derives_goal(&req.goal)
            .map_err(|e| ApiError::Internal(format!("engine derives: {e}")))?;

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
        let cost = planner::score(granted_delta.len() as u32, forbidden_hits, approval_hits);
        let accepted = report.violations.is_empty() && derives_goal;

        // Capture the post-apply egraph for the scrubber timeline.
        let graph_json = engine
            .serialize_snapshot()
            .map_err(|e| ApiError::Internal(format!("engine serialize: {e}")))?;
        let prefix = if accepted { "✓" } else { "✗" };
        trace.push(EgraphSnapshot {
            label: format!("{prefix} agent proposed: assign {}", role_id),
            kind: SnapshotKind::Candidate {
                role: role_id.clone(),
                accepted,
                derives_goal,
            },
            graph: graph_json,
        });

        candidates.push(PlanCandidate {
            plan: plan_step,
            role: role_id,
            accepted,
            granted_delta,
            derives_goal,
            violations: report.violations,
            cost,
        });
    }

    let accepted_index = candidates.iter().position(|c| c.accepted);
    let outcome = PlanningOutcome {
        goal_unreachable: None,
        candidates,
        accepted_index,
        trace,
    };

    // 3. Build the signed receipt — same shape as /api/plan, but step
    //    justifications carry the agent_proposal_digest tag so the bound
    //    LLM output is signed alongside the decision.
    let model_d = model_digest(&model);
    let rules_d = bytes_digest(egglog_engine::RULES_SRC.as_bytes());
    let goal_d = goal_digest(&req.goal);

    let decision = if outcome.accepted().is_some() {
        Decision::Accepted
    } else {
        Decision::Rejected
    };

    let accepted_plan_digest = outcome.accepted().map(|c| plan_digest(&c.plan));

    let steps: Vec<ReceiptStep> = outcome
        .accepted()
        .map(|c| candidate_to_steps_tagged(c, Some(&agent_digest_hex)))
        .unwrap_or_default();

    let rejections: Vec<Rejection> = outcome
        .candidates
        .iter()
        .filter(|c| !c.accepted)
        .map(|c| Rejection {
            candidate: c.role.to_string(),
            reason: rejection_reason(c),
        })
        .collect();

    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

    let prev_hash = latest_chain_head(&s.db).await?;

    let skeleton = Receipt {
        receipt_version: RECEIPT_VERSION.to_string(),
        model_digest: model_d,
        rules_digest: rules_d,
        goal_digest: goal_d,
        accepted_plan_digest,
        timestamp: now.clone(),
        candidate_count: outcome.candidates.len() as u32,
        decision,
        steps,
        rejections,
        proof: Proof {
            kid: String::new(),
            alg: PROOF_ALG.to_string(),
            sig: Vec::new(),
            prev_hash,
        },
    };
    let signed = skeleton.sign(&s.signing_key, &s.kid);

    let mut receipt_id = None;
    if req.persist {
        let json = serde_json::to_string(&signed)
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let this_hash = signed.this_hash();
        let row = sqlx::query(
            "INSERT INTO receipts (receipt_json, this_hash, prev_hash, created_at) \
             VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(&json)
        .bind(&this_hash[..])
        .bind(prev_hash.map(|h| h.to_vec()))
        .bind(&now)
        .fetch_one(&s.db)
        .await?;
        let id: i64 = sqlx::Row::get(&row, "id");
        receipt_id = Some(id);
    }

    Ok(Json(AgentProposeResponse {
        agent: agent_resp,
        agent_proposal_digest: agent_digest_hex,
        replay_id,
        outcome,
        receipt: signed,
        receipt_id,
        model_digest: hex(&model_d),
        rules_digest: hex(&rules_d),
        goal_digest: hex(&goal_d),
    }))
}

// ── /api/replays ────────────────────────────────────────────────────────────
// Lists the canonical goals the server has pre-recorded agent sessions for.
// Frontend reads this once at load to render the off-script chip suggestions.

#[derive(Serialize)]
struct ReplayListing {
    available: Vec<Goal>,
    allow_live_agent: bool,
}

async fn get_replays(State(s): State<SharedState>) -> Json<ReplayListing> {
    Json(ReplayListing {
        available: s.replays.available_goals().into_iter().cloned().collect(),
        allow_live_agent: s.allow_live_agent,
    })
}

// ── /api/validate ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ValidateRequest {
    goal: Goal,
    plan: Plan,
}

#[derive(Serialize)]
struct ValidateResponse {
    report: policy::PolicyReport,
}

async fn post_validate(
    State(s): State<SharedState>,
    Json(req): Json<ValidateRequest>,
) -> ApiResult<Json<ValidateResponse>> {
    let model = s.model.read().await.clone();
    let report = policy::check_plan(&model, &req.goal, &req.plan);
    Ok(Json(ValidateResponse { report }))
}

// ── /api/graph ──────────────────────────────────────────────────────────────

async fn post_graph(
    State(s): State<SharedState>,
    Json(req): Json<PlanRequest>,
) -> ApiResult<Json<graph::GraphView>> {
    let model = s.model.read().await.clone();
    let outcome = plan(&model, &req.goal, req.top_n)?;
    Ok(Json(graph::build(&model, &req.goal, &outcome)))
}

// ── /api/receipt/:id ────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ReceiptView {
    id: i64,
    receipt: Receipt,
    prev_hash_hex: Option<String>,
    this_hash_hex: String,
    created_at: String,
}

async fn get_receipt(
    State(s): State<SharedState>,
    AxPath(id): AxPath<i64>,
) -> ApiResult<Json<ReceiptView>> {
    let row: Option<(String, Vec<u8>, Option<Vec<u8>>, String)> = sqlx::query_as(
        "SELECT receipt_json, this_hash, prev_hash, created_at FROM receipts WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&s.db)
    .await?;
    let (json, this_hash, prev_hash, created_at) = row.ok_or(ApiError::NotFound)?;
    let r: Receipt =
        serde_json::from_str(&json).map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(ReceiptView {
        id,
        receipt: r,
        prev_hash_hex: prev_hash.map(|b| hex_bytes(&b)),
        this_hash_hex: hex_bytes(&this_hash),
        created_at,
    }))
}

#[derive(Serialize)]
struct VerifyView {
    id: i64,
    signature_valid: bool,
    chain_valid: bool,
    notes: Vec<String>,
}

async fn verify_receipt(
    State(s): State<SharedState>,
    AxPath(id): AxPath<i64>,
) -> ApiResult<Json<VerifyView>> {
    let row: Option<(String, Vec<u8>, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT receipt_json, this_hash, prev_hash FROM receipts WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&s.db)
    .await?;
    let (json, stored_this, stored_prev) = row.ok_or(ApiError::NotFound)?;
    let r: Receipt =
        serde_json::from_str(&json).map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut notes = Vec::new();

    // Signature check against the in-memory verifying key. Production would
    // resolve `r.proof.kid` against a JWKS; for v0.1 we serve a single key.
    let signature_valid = r.verify(&s.signing_key.verifying_key());
    if !signature_valid {
        notes.push("signature failed to verify against current server key".into());
    }

    // Chain check: re-compute this_hash from the stored JSON, compare with
    // the indexed column AND with the next row's prev_hash.
    let recomputed_this = r.this_hash();
    let stored_matches: bool = stored_this == recomputed_this.to_vec();
    if !stored_matches {
        notes.push("indexed this_hash diverges from recomputed canonical hash".into());
    }

    // Verify our prev_hash matches the predecessor row's this_hash.
    let prev_chain_ok = match stored_prev {
        None => true, // first receipt — nothing to compare against
        Some(stored_prev) => {
            let predecessor: Option<(Vec<u8>,)> = sqlx::query_as(
                "SELECT this_hash FROM receipts WHERE id < ? ORDER BY id DESC LIMIT 1",
            )
            .bind(id)
            .fetch_optional(&s.db)
            .await?;
            match predecessor {
                Some((prev_this,)) if prev_this == stored_prev => true,
                Some(_) => {
                    notes.push("prev_hash diverges from predecessor row's this_hash".into());
                    false
                }
                None => {
                    notes.push("receipt has prev_hash but no predecessor row".into());
                    false
                }
            }
        }
    };

    Ok(Json(VerifyView {
        id,
        signature_valid,
        chain_valid: stored_matches && prev_chain_ok,
        notes,
    }))
}

fn hex_bytes(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for byte in b {
        s.push_str(&format!("{:02x}", byte));
    }
    s
}

// Bring `Row::get` into scope for the INSERT...RETURNING handler.
#[allow(unused_imports)]
use sqlx::Row;

// Disambiguate a small lint: `Arc<AppState>` is the state type.
#[allow(dead_code)]
fn _typecheck(_: Arc<crate::state::AppState>) {}
