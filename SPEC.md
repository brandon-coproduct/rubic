# Verified Agentic RBAC Planner

## One-line demo
An AI agent asks: "How can Alice read the payroll report with least privilege?" The system derives a minimal lawful sequence of role/permission rewrites, validates it server-side with egglog + policy checks, visualizes the path, and emits a signed receipt.

## Core claim
LLMs propose. Algebra validates. The product does not trust generated text; it trusts only checked transformations.

## Stack
- Rust workspace (axum, serde, tokio, sqlx-SQLite, egglog 2.x, petgraph, utoipa, blake3, ed25519-dalek v2)
- SvelteKit + TypeScript + Cytoscape.js (dagre layout)

## Architecture
```
[ AI agent ]  --(JSON proposal)-->  [ axum /api/plan ]
                                          |
                                          v
                            +------ planner crate -------+
                            |  enumerate candidates      |
                            |  rank by cost              |
                            |  validate each via:        |
                            |   - policy invariants      |
                            |   - egglog reachability    |
                            +----------------------------+
                                          |
                                          v
                          [ receipt crate signs decision ]
                                          |
                                          v
                       [ SQLite hash-chained receipt log ]
                                          |
                                          v
                  [ SvelteKit UI: graph + timeline + receipt ]
```

## Demo flow
1. Load `examples/rbac_demo.toml`.
2. Goal: Alice, read, payroll_summary.
3. Server enumerates `{finance_viewer, payroll_admin}` candidates.
4. `finance_viewer` accepted (minimal permission delta, no forbidden hits).
5. `payroll_admin` rejected (`NoForbiddenPermission: delete:payroll`).
6. Receipt signed and logged. UI shows green/red/gold graph + downloadable receipt JSON.

## Non-goals
- No production IAM integration.
- No claims of full formal verification.
- No dynamic self-modifying rules.
- AI agent has no authority to mutate the model; it only proposes JSON.

## Receipt schema (`receipt_version = "rbac-1"`)
```json
{
  "receipt_version": "rbac-1",
  "model_digest": "...",
  "rules_digest": "...",
  "goal_digest": "...",
  "accepted_plan_digest": "...",
  "timestamp": "...",
  "candidate_count": 3,
  "decision": "accepted",
  "steps": [
    { "op": "assign_role", "user": "alice", "role": "finance_viewer",
      "justification": "derives CanReach(alice, read:payroll_summary)" }
  ],
  "rejections": [
    { "candidate": "payroll_admin",
      "reason": "grants forbidden permission delete:payroll" }
  ],
  "proof": {
    "kid": "...", "alg": "Ed25519", "sig": "...", "prev_hash": "..."
  }
}
```
