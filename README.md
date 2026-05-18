# rubic

**Verified Agentic RBAC Planner.** An LLM proposes role assignments; an
egglog rewrite system + policy invariants validate each one server-side;
the decision is sealed in a signed, hash-chained receipt that
cryptographically witnesses *exactly* what the LLM said and how the
algebra responded.

LLMs propose. Algebra disposes.

**Live demo:** <https://rubic.fly.dev>
**MCP endpoint:** `https://rubic.fly.dev/mcp` (one-line install in your Claude config — see below)

---

## Why this exists

Agentic AI systems are increasingly trusted to take actions on real
authorization surfaces — granting access, modifying policy, mutating
production data. The standard pattern today is *"let the model decide
and hope the prompt was good enough."* That doesn't compose with how
authorization actually works in adults-in-the-room environments: an
auditor doesn't want a transcript, they want a proof.

Rubic is a small, sharp demonstration of a different pattern:

- The LLM emits a **proposal** in structured JSON. Untrusted, treated
  as input.
- A deterministic, algebraic verifier runs **two independent checks**:
  policy invariants (forbidden permissions, approval-required actions,
  least-privilege deltas) and **egglog** reachability — proving the
  proposed assignment would actually let the user reach their goal,
  not just plausibly.
- The decision is signed with **Ed25519**. The receipt binds the
  model, the ruleset, the goal, the LLM's raw proposal, and the
  accepted plan into a single document anyone can re-verify against
  the server's public key. Receipts chain via `prev_hash` for
  tamper-evident audit logs.

The technique is small enough to fit on one page; the value is that
*the trust boundary is mathematical, not prompt-engineered*.

---

## What you can do with the live demo

1. **Click `Propose plan`** — pure deterministic path. Server enumerates
   every role for the user, ranks by least privilege, runs all checks,
   signs a receipt.
2. **Click `Ask agent`** — same pipeline, but the candidate roles come
   from Claude (pre-recorded sessions; no API spend per visit).
   Try `alice / read / payroll_summary` (accept), `alice / write / payroll`
   (rejected — required-approval + insufficient role), or
   `alice / delete / payroll` (rejected — forbidden permission).
3. **Scrub the egraph trace** — watch the egglog egraph evolve frame by
   frame as each candidate's `(Assigned ...)` fact is asserted and
   `CanReach` derives. New nodes glow gold.
4. **Verify the receipt** — server-side signature check + hash-chain
   walk against the previous receipt's `this_hash`. Tamper any field
   (try editing the role name in the JSON and re-uploading) and the
   signature breaks.
5. **Drive it from your own Claude via MCP** (see below).

---

## MCP — drive rubic from your Claude

Rubic exposes a Model Context Protocol server. Add one line to
`.claude/settings.json` (Claude Code) or
`claude_desktop_config.json` (Claude Desktop):

```json
{
  "mcpServers": {
    "rubic": { "url": "https://rubic.fly.dev/mcp" }
  }
}
```

Restart your Claude. Then ask: *"use the rubic tool to propose a
least-privilege role for alice to read payroll_summary."* Two tools are
exposed:

- `propose_assignment(model_toml, goal)` — runs the full validator
  pipeline against your own TOML model and returns the ranked
  candidates + a signed receipt.
- `verify_receipt(receipt_json)` — checks any receipt's signature
  against the server's current Ed25519 key.

The MCP path is the most opinionated way to play with rubic — you bring
your own RBAC model, the deployed verifier does the work, you get a
receipt back.

---

## Architecture

```
                            Fly machine (rubic.fly.dev)
                            ┌───────────────────────────────────────┐
Browser  ────── HTTPS ──────┤ axum                                  │
                            │  ├── /              SPA (Svelte 5)    │
                            │  ├── /api/plan      deterministic     │
                            │  ├── /api/agent/propose  replay-backed│
                            │  ├── /api/receipt/:id/verify          │
                            │  └── /mcp           rmcp HTTP         │
                            │                                       │
Claude   ── MCP (HTTP/SSE) ─┤   tools: propose_assignment,          │
                            │          verify_receipt               │
                            └───────────────────────────────────────┘
```

The server is a single Rust binary that serves the SPA, the JSON API,
and the MCP endpoint. The trust boundary lives in three crates:

- **`policy/`** — total, deterministic invariant checker. Returns every
  violation, not just the first; structured by `InvariantKind` so the
  UI can render specific reasons (`NoForbiddenPermission`,
  `NoUnapprovedAutoGrant`, `LeastPrivilegeMinimal`, …).
- **`egglog-engine/`** — wraps the [egglog](https://github.com/egraphs-good/egglog) e-graph.
  Asserts model facts, applies the candidate `(Assigned user role)`,
  saturates the ruleset, queries `(check (CanReach user (Perm action resource)))`.
- **`receipt/`** — `Proof { kid, alg, sig, prev_hash }` envelope
  modeled on [`nucleus-lineage`](https://github.com/anthropics/nucleus). Canonical
  bytes are NUL-separated and exclude the proof block; signature is
  over those bytes. Hash chain: `prev_hash[n+1] = BLAKE3(canonical_bytes[n] || sig[n])`.

The remaining crates (`core-ir`, `planner`, `agent`, `replay`, `server`)
are plumbing.

---

## Run locally

```bash
# Backend (Rust workspace) on :3000
cargo run -p server

# Frontend (Vite + Svelte 5) on :5173, proxies /api/* to backend
pnpm --dir web install
pnpm --dir web dev
```

Then open <http://localhost:5173/>. Submit
`alice / read / payroll_summary`; expect `finance_viewer` accepted,
`payroll_admin` rejected.

For the agent path locally, you need either:
- the `claude` CLI on PATH (OAuth-backed; no API key needed), or
- `ANTHROPIC_API_KEY` exported and `RUBIC_ALLOW_LIVE_AGENT=1` set

Without one of these, the agent endpoint serves pre-recorded sessions
matched by goal — same as production.

### Record new replays

```bash
cargo run --bin record-replay -p agent -- \
  --user alice --action read --resource payroll_summary
```

Writes JSON to `replays/`, which gets bundled into the binary on next
build (via `include_dir!`).

---

## Tests

```bash
cargo test --workspace          # 60 unit tests across 7 crates
pnpm --dir web exec svelte-check
pnpm --dir web build
```

---

## Receipt format

```json
{
  "receipt_version": "rbac-1",
  "model_digest":        "blake3 of normalized RBAC model",
  "rules_digest":        "blake3 of egglog ruleset",
  "goal_digest":         "blake3 of canonical (user, action, resource)",
  "accepted_plan_digest": "blake3 of the accepted role-assignment plan",
  "timestamp":           "RFC3339 UTC",
  "candidate_count":     3,
  "decision":            "accepted | rejected",
  "steps":     [ { "op": "assign_role", "user": "...", "role": "...",
                   "justification": "...; agent_proposal_digest=<hex>" } ],
  "rejections": [ { "candidate": "...", "reason": "structured policy citations" } ],
  "proof":     { "kid": "...", "alg": "Ed25519",
                 "sig": "base64", "prev_hash": "hex|null" }
}
```

Canonical bytes are NUL-separated and exclude `proof`. The signature is
over those bytes. The `agent_proposal_digest` (BLAKE3 of the LLM's raw
JSON output) rides inside the step's `justification` string, which is
itself inside the signed canonical bytes — so the receipt is
tamper-evident with respect to the agent's untrusted input, with no
schema rev required.

---

## Deploy

The included `Dockerfile` is a multi-stage build (Node → Rust →
Debian slim) producing a ~31MB image. The `fly.toml` deploys to Fly.io
in a single region with scale-to-zero. SQLite + the Ed25519 key live in
`/tmp` (ephemeral by design for the demo; mount a Fly Volume if you
want persistence).

```bash
fly apps create rubic
fly deploy --remote-only
```

---

## What's not (yet) here

- AWS IAM / Cedar / Rego export
- JWKS-backed multi-key verifier (today: single in-server key)
- Versioned receipt schema (`rbac-2`) with a typed `agent_proposal_digest` field
- TEE attestation (the binary's hash isn't part of the receipt)
- Persistent receipt log across deploys (use a Fly Volume)

---

## License

MIT — see [LICENSE](./LICENSE).
