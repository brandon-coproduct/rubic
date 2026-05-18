// Typed client for the rubic backend.
//
// Types mirror the Rust structs in core-ir / planner / receipt. JSON shape
// is the source of truth — keep field names in sync if either side moves.

export type EntityId = string;

export interface Permission {
  action: string;
  resource: string;
}

export interface User {
  id: EntityId;
  roles: EntityId[];
}

export interface Role {
  id: EntityId;
  permissions: Permission[];
}

export interface Goal {
  user: EntityId;
  action: string;
  resource: string;
}

export interface Policy {
  least_privilege: boolean;
  forbidden_permissions: Permission[];
  requires_approval: Permission[];
}

export interface Model {
  users: Record<string, User>;
  roles: Record<string, Role>;
  policy: Policy;
  goal?: Goal;
}

export type PlanStep = { op: 'assign_role'; user: EntityId; role: EntityId };

export interface Plan {
  steps: PlanStep[];
}

export type InvariantKind =
  | 'user_exists'
  | 'role_exists'
  | 'goal_reachable'
  | 'goal_satisfied'
  | 'no_forbidden_permission'
  | 'no_unapproved_auto_grant'
  | 'least_privilege_minimal';

export interface PolicyViolation {
  invariant: InvariantKind;
  explanation: string;
}

export interface PlanCandidate {
  plan: Plan;
  role: EntityId;
  accepted: boolean;
  granted_delta: Permission[];
  derives_goal: boolean;
  violations: PolicyViolation[];
  cost: number;
}

// Mirrors planner::EgraphSnapshot (Rust). `kind` is a serde-tagged enum:
// {kind: "initial"} or {kind: "candidate", role, accepted, derives_goal}.
export type SnapshotKind =
  | { kind: 'initial' }
  | { kind: 'candidate'; role: string; accepted: boolean; derives_goal: boolean };

export interface EgraphSnapshot extends Record<string, unknown> {
  label: string;
  // `kind` is flattened in the JSON (serde flatten + tag), so it's at the
  // top level alongside `label` and `graph`. The intersection above lets
  // us narrow on `kind` after the cast.
  kind: SnapshotKind['kind'];
  role?: string;
  accepted?: boolean;
  derives_goal?: boolean;
  graph: unknown; // egraph-serialize JSON; opaque to us, fed to visualizer
}

export interface PlanningOutcome {
  goal_unreachable: PolicyViolation | null;
  candidates: PlanCandidate[];
  accepted_index: number | null;
  trace: EgraphSnapshot[];
}

export interface ReceiptStep {
  op: string;
  user: string;
  role: string;
  justification: string;
}

export interface Rejection {
  candidate: string;
  reason: string;
}

export interface Proof {
  kid: string;
  alg: string;
  sig: string;
  prev_hash?: string;
}

export interface Receipt {
  receipt_version: string;
  model_digest: string;
  rules_digest: string;
  goal_digest: string;
  accepted_plan_digest?: string;
  timestamp: string;
  candidate_count: number;
  decision: 'accepted' | 'rejected';
  steps: ReceiptStep[];
  rejections: Rejection[];
  proof: Proof;
}

export interface PlanResponse {
  outcome: PlanningOutcome;
  receipt: Receipt;
  receipt_id: number | null;
  model_digest: string;
  rules_digest: string;
  goal_digest: string;
}

export interface AgentProposal {
  role: string;
  reasoning: string;
}

export interface AgentResponse {
  raw_json: string;
  proposals: AgentProposal[];
  model_used: string;
}

export interface AgentProposeResponse {
  agent: AgentResponse;
  agent_proposal_digest: string;
  /// When the response came from a recorded session, this is the replay id.
  /// `null` means a live `claude -p` call (only happens locally).
  replay_id: string | null;
  outcome: PlanningOutcome;
  receipt: Receipt;
  receipt_id: number | null;
  model_digest: string;
  rules_digest: string;
  goal_digest: string;
}

export interface ReplayListing {
  available: Goal[];
  allow_live_agent: boolean;
}

export type EdgeColor = 'neutral' | 'accepted' | 'rejected' | 'gold';
export type NodeKind = 'user' | 'role' | 'permission' | 'goal';

export interface GraphNode {
  id: string;
  label: string;
  kind: NodeKind;
  color: EdgeColor;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  color: EdgeColor;
  note?: string;
}

export interface GraphView {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface ModelView {
  model: Model;
  digest: string;
  rules_digest: string;
}

export interface VerifyView {
  id: number;
  signature_valid: boolean;
  chain_valid: boolean;
  notes: string[];
}

// ── fetch helpers ──────────────────────────────────────────────────────────

async function jsonFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    headers: { 'Content-Type': 'application/json', ...(init?.headers ?? {}) },
    ...init,
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`${res.status} ${res.statusText}: ${body}`);
  }
  return (await res.json()) as T;
}

export const api = {
  health: () => jsonFetch<{ status: string; kid: string; key_path: string }>('/healthz'),

  getModel: () => jsonFetch<ModelView>('/api/model'),

  loadModel: (toml: string) =>
    jsonFetch<ModelView>('/api/model/load', {
      method: 'POST',
      body: JSON.stringify({ toml }),
    }),

  plan: (goal: Goal, top_n = 3) =>
    jsonFetch<PlanResponse>('/api/plan', {
      method: 'POST',
      body: JSON.stringify({ goal, top_n }),
    }),

  agentPropose: (goal: Goal) =>
    jsonFetch<AgentProposeResponse>('/api/agent/propose', {
      method: 'POST',
      body: JSON.stringify({ goal }),
    }),

  replays: () => jsonFetch<ReplayListing>('/api/replays'),

  graph: (goal: Goal, top_n = 3) =>
    jsonFetch<GraphView>('/api/graph', {
      method: 'POST',
      body: JSON.stringify({ goal, top_n }),
    }),

  verifyReceipt: (id: number) =>
    jsonFetch<VerifyView>(`/api/receipt/${id}/verify`, { method: 'POST' }),
};
