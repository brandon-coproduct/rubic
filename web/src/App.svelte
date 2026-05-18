<script lang="ts">
  import { onMount } from 'svelte';
  import {
    api,
    type AgentResponse,
    type Goal,
    type GraphView,
    type Model,
    type PlanResponse,
    type ReplayListing,
  } from './lib/api';
  import GraphViewC from './lib/GraphView.svelte';
  import PlanTimeline from './lib/PlanTimeline.svelte';
  import ReceiptPanel from './lib/ReceiptPanel.svelte';
  import ModelEditor from './lib/ModelEditor.svelte';
  import EgraphView from './lib/EgraphView.svelte';

  type VizTab = 'egraph' | 'graph';
  let vizTab = $state<VizTab>('egraph');

  let model = $state<Model | null>(null);
  let modelDigest = $state('');
  let rulesDigest = $state('');
  let kid = $state('');

  let user = $state('support-bot');
  let action = $state('write');
  let resource = $state('db.tickets');

  let planResp = $state<PlanResponse | null>(null);
  let graph = $state<GraphView | null>(null);
  let agentResp = $state<AgentResponse | null>(null);
  let agentDigest = $state<string | null>(null);
  let agentReplayId = $state<string | null>(null);
  let loading = $state(false);
  let askingAgent = $state(false);
  let err = $state<string | null>(null);
  let busy = $derived(loading || askingAgent);
  // Pre-fetched at mount: the set of goals the server has a recorded
  // replay for. Two surfaces consume this:
  //   - Always-visible chip row above the goal form (one-click try)
  //   - Fallback chips when an off-script goal hits 400
  let replays = $state<ReplayListing | null>(null);
  let showReplayChips = $state(false);
  let mcpOpen = $state(false);

  // The exact host the MCP config snippet should point at. Uses the page
  // origin so it stays right whether visitors hit rubic.fly.dev or a
  // local dev server.
  let mcpUrl = $derived(
    typeof window === 'undefined'
      ? 'https://rubic.fly.dev/mcp'
      : `${window.location.origin}/mcp`,
  );
  let mcpSnippet = $derived(
    JSON.stringify({ mcpServers: { rubic: { url: mcpUrl } } }, null, 2),
  );
  let mcpCopied = $state(false);

  function copyMcp() {
    navigator.clipboard.writeText(mcpSnippet);
    mcpCopied = true;
    setTimeout(() => (mcpCopied = false), 1200);
  }

  async function refreshModel() {
    const view = await api.getModel();
    model = view.model;
    modelDigest = view.digest;
    rulesDigest = view.rules_digest;
  }

  async function submitGoal() {
    loading = true;
    err = null;
    agentResp = null;
    agentDigest = null;
    try {
      const goal = { user, action, resource };
      const [pr, gv] = await Promise.all([api.plan(goal, 5), api.graph(goal, 5)]);
      planResp = pr;
      graph = gv;
      modelDigest = pr.model_digest;
      rulesDigest = pr.rules_digest;
    } catch (e) {
      err = (e as Error).message;
    } finally {
      loading = false;
    }
  }

  async function askAgent() {
    askingAgent = true;
    err = null;
    showReplayChips = false;
    try {
      const goal = { user, action, resource };
      const [ar, gv] = await Promise.all([api.agentPropose(goal), api.graph(goal, 5)]);
      planResp = {
        outcome: ar.outcome,
        receipt: ar.receipt,
        receipt_id: ar.receipt_id,
        model_digest: ar.model_digest,
        rules_digest: ar.rules_digest,
        goal_digest: ar.goal_digest,
      };
      graph = gv;
      agentResp = ar.agent;
      agentDigest = ar.agent_proposal_digest;
      agentReplayId = ar.replay_id;
      modelDigest = ar.model_digest;
      rulesDigest = ar.rules_digest;
    } catch (e) {
      const msg = (e as Error).message;
      err = msg;
      // 400 with "no recorded session" is our signal to show chips
      // instead of a raw error.
      if (msg.includes('no recorded session')) {
        showReplayChips = true;
      }
    } finally {
      askingAgent = false;
    }
  }

  function useReplay(g: Goal) {
    user = String(g.user);
    action = g.action;
    resource = g.resource;
    showReplayChips = false;
    err = null;
    askAgent();
  }

  async function loadModel(toml: string) {
    const view = await api.loadModel(toml);
    model = view.model;
    modelDigest = view.digest;
    rulesDigest = view.rules_digest;
    planResp = null;
    graph = null;
  }

  onMount(async () => {
    try {
      const h = await api.health();
      kid = h.kid;
      await refreshModel();
      replays = await api.replays();
    } catch (e) {
      err = (e as Error).message;
    }
  });
</script>

<header>
  <div class="title">
    <h1>rubic — Verified authorization for AI agent tool calls</h1>
    <div class="sub">
      An LLM proposes which tool-call capabilities to grant. An egglog
      rewrite system + policy invariants validate each proposal
      server-side. A signed, hash-chained receipt cryptographically
      witnesses the decision. LLMs propose; algebra disposes.
    </div>
  </div>
  <div class="meta">
    <div class="meta-row">
      <a
        class="repo-link"
        href="https://github.com/brandon-coproduct/rubic"
        target="_blank"
        rel="noreferrer"
        title="Source on GitHub (MIT licensed)"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="14"
          height="14"
          viewBox="0 0 16 16"
          fill="currentColor"
          aria-hidden="true"
        >
          <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8z"/>
        </svg>
        <span>source</span>
      </a>
      <button class="mcp-btn" onclick={() => (mcpOpen = true)}>
        Use from Claude →
      </button>
    </div>
    <span class="kid">signing key <code title="ed25519 verifying-key id used to sign receipts on this machine">{kid || '—'}</code></span>
  </div>
</header>

{#if mcpOpen}
  <div
    class="modal-backdrop"
    onclick={() => (mcpOpen = false)}
    role="presentation"
  ></div>
  <div class="modal" role="dialog" aria-labelledby="mcp-title">
    <div class="modal-head">
      <h3 id="mcp-title">Drive rubic from your own Claude</h3>
      <button class="modal-x" onclick={() => (mcpOpen = false)} aria-label="Close">✕</button>
    </div>
    <p class="modal-body">
      Rubic exposes a <code>Model Context Protocol</code> server at
      <code>{mcpUrl}</code>. Add this to your Claude config (Claude Code's
      <code>.claude/settings.json</code> or Claude Desktop's
      <code>claude_desktop_config.json</code>), restart, and ask your Claude
      to use the <code>rubic</code> tools — <code>propose_assignment</code> takes
      a TOML model + a goal and returns a signed receipt;
      <code>verify_receipt</code> checks any receipt against this server's key.
    </p>
    <p class="modal-body" style="color:#94a3b8; font-size:12px;">
      Meta-loop: the rubic MCP server you're about to configure is itself an
      MCP surface. A production rubic deployment would gate this and every
      other MCP tool call your agent makes.
    </p>
    <pre class="mcp-snippet">{mcpSnippet}</pre>
    <div class="modal-actions">
      <button class="copy-btn" onclick={copyMcp}>
        {mcpCopied ? '✓ Copied' : 'Copy config'}
      </button>
      <a
        class="docs-link"
        href="https://modelcontextprotocol.io/docs/getting-started/user-quickstart"
        target="_blank"
        rel="noreferrer"
      >MCP docs ↗</a>
    </div>
  </div>
{/if}

<main>
  <section class="model">
    <h2>Semantic model</h2>
    <ModelEditor
      {model}
      digest={modelDigest}
      {rulesDigest}
      onLoad={loadModel}
    />
  </section>

  <section class="goal">
    <h2>Goal</h2>
    {#if replays && replays.available.length > 0}
      <div class="try-chips">
        <span class="try-label">Try a recorded tool-call →</span>
        {#each replays.available as g}
          <button
            class="try-chip"
            onclick={() => useReplay(g)}
            title="Loads {g.user} / {g.action} / {g.resource} and asks the agent (pre-recorded, no API call)"
          >
            <code>{g.user}</code>
            <span>/</span>
            <code>{g.action}:{g.resource}</code>
          </button>
        {/each}
      </div>
    {/if}
    <div class="goal-form">
      <label title="The AI agent persona requesting the tool call"
        >agent
        <input bind:value={user} placeholder="support-bot" />
      </label>
      <label title="The tool verb (read, write, delete, exec, fetch, …)"
        >action
        <input bind:value={action} placeholder="write" />
      </label>
      <label title="The tool target (db.tickets, github.pulls, https://api.example.com/v1, …)"
        >resource
        <input bind:value={resource} placeholder="db.tickets" />
      </label>
      <button
        onclick={submitGoal}
        disabled={loading || askingAgent}
        title="Deterministic enumeration: tries every role in the model, ranks by least privilege, runs policy + egglog checks per candidate."
      >
        {loading ? 'Planning…' : 'Propose plan'}
      </button>
      <button
        class="agent-btn"
        onclick={askAgent}
        disabled={loading || askingAgent}
        title="Asks Claude to propose tool-grant assignments. Server runs each proposal through the same policy + egglog pipeline as the deterministic path."
      >
        {askingAgent ? 'Asking agent…' : 'Ask agent'}
      </button>
    </div>
    {#if err && !showReplayChips}<div class="err">{err}</div>{/if}
    {#if showReplayChips && replays}
      <div class="chips-panel">
        <div class="chips-head">
          No recorded session for that goal — pick one of these to see the agent's actual response{replays.allow_live_agent ? ', or set up live mode' : ''}:
        </div>
        <div class="chips">
          {#each replays.available as g}
            <button class="chip" onclick={() => useReplay(g)}>
              <code>{g.user}</code>
              <span>→</span>
              <code>{g.action}:{g.resource}</code>
            </button>
          {/each}
        </div>
      </div>
    {/if}
    {#if agentResp}
      <div class="agent-panel">
        <div class="agent-head">
          <span class="agent-label">LLM proposal (untrusted)</span>
          <code class="agent-model">{agentResp.model_used}</code>
          {#if agentReplayId}
            <span
              class="replay-pill"
              title="Pre-recorded session — no live API call. Same JSON the agent really emitted."
            >replay · {agentReplayId}</span>
          {/if}
          <span class="agent-hash">
            digest <code title={agentDigest ?? ''}>
              {agentDigest ? `${agentDigest.slice(0, 8)}…${agentDigest.slice(-6)}` : '—'}
            </code>
          </span>
        </div>
        <ul class="agent-proposals">
          {#each agentResp.proposals as p}
            <li>
              <code>{p.role}</code>
              <span class="agent-reasoning">{p.reasoning}</span>
            </li>
          {/each}
        </ul>
        <div class="agent-note">
          Server-side validation result appears in <em>Candidate plans</em> below.
          The LLM's verbatim output is BLAKE3-hashed and embedded in the signed
          receipt — even though the proposal text is never trusted, the decision
          cryptographically binds to exactly what the LLM said.
        </div>
      </div>
    {/if}
  </section>

  <section class="timeline">
    <h2>Candidate plans</h2>
    <PlanTimeline outcome={planResp?.outcome ?? null} />
  </section>

  <section class="graph">
    <div class="viz-header">
      <h2>
        {#if vizTab === 'egraph'}Egraph trace{:else}Role graph{/if}
        {#if busy}<span class="busy">· planning…</span>{/if}
      </h2>
      {#if planResp && planResp.outcome.trace.length > 0}
        <div class="viz-tabs">
          <button
            class="tab"
            class:active={vizTab === 'egraph'}
            onclick={() => (vizTab = 'egraph')}
            title="The actual egglog state per candidate, scrub through frames"
          >egraph</button>
          <button
            class="tab"
            class:active={vizTab === 'graph'}
            onclick={() => (vizTab = 'graph')}
            title="Role → permission DAG, final state colored by outcome"
          >role graph</button>
        </div>
      {/if}
    </div>
    {#if planResp && planResp.outcome.trace.length > 0 && vizTab === 'egraph'}
      <EgraphView snapshots={planResp.outcome.trace} />
    {:else}
      <GraphViewC {graph} />
    {/if}
  </section>

  <section class="receipt">
    <h2>Receipt</h2>
    <ReceiptPanel
      receipt={planResp?.receipt ?? null}
      receiptId={planResp?.receipt_id ?? null}
    />
  </section>
</main>

<style>
  header {
    padding: 16px 24px;
    border-bottom: 1px solid #1e293b;
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }
  .title {
    flex: 1;
    min-width: 260px;
  }
  h1 {
    margin: 0;
    font-size: 18px;
    letter-spacing: -0.01em;
    color: #e2e8f0;
    font-weight: 600;
  }
  .sub {
    font-size: 12px;
    color: #94a3b8;
    margin-top: 4px;
    line-height: 1.5;
    max-width: 720px;
  }
  .meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
  }
  .meta-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .repo-link {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: #94a3b8;
    text-decoration: none;
    font-size: 12px;
    padding: 5px 8px;
    border-radius: 4px;
    border: 1px solid #334155;
  }
  .repo-link:hover {
    color: #d4a017;
    border-color: #d4a017;
  }
  .mcp-btn {
    background: #1e293b;
    color: #d4a017;
    border: 1px solid #d4a017;
    padding: 5px 12px;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    text-transform: none;
    letter-spacing: 0;
  }
  .mcp-btn:hover {
    background: #334155;
  }
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
    z-index: 50;
  }
  .modal {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(560px, 92vw);
    background: #0f172a;
    border: 1px solid #1e293b;
    border-left: 3px solid #d4a017;
    border-radius: 8px;
    padding: 18px 20px;
    color: #e2e8f0;
    z-index: 51;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
  }
  .modal-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
  }
  .modal-head h3 {
    margin: 0;
    font-size: 14px;
    color: #d4a017;
  }
  .modal-x {
    background: transparent;
    border: none;
    color: #94a3b8;
    font-size: 16px;
    cursor: pointer;
    padding: 2px 6px;
  }
  .modal-body {
    margin: 0 0 12px 0;
    font-size: 13px;
    line-height: 1.55;
    color: #cbd5e1;
  }
  .modal-body code {
    background: #1e293b;
    padding: 1px 5px;
    border-radius: 3px;
    color: #cbd5e1;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 11px;
  }
  .mcp-snippet {
    background: #050913;
    color: #cbd5e1;
    border: 1px solid #1e293b;
    border-radius: 6px;
    padding: 10px 12px;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 12px;
    overflow-x: auto;
    margin: 0 0 10px 0;
  }
  .modal-actions {
    display: flex;
    gap: 10px;
    align-items: center;
    justify-content: flex-end;
  }
  .copy-btn {
    background: #d4a017;
    color: #0b1220;
    border: none;
    padding: 6px 14px;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .docs-link {
    color: #94a3b8;
    font-size: 12px;
    text-decoration: none;
  }
  .docs-link:hover {
    color: #d4a017;
  }
  .try-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
    margin-bottom: 10px;
  }
  .try-label {
    font-size: 10px;
    color: #94a3b8;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin-right: 4px;
  }
  .try-chip {
    background: #0f172a;
    border: 1px solid #334155;
    color: #cbd5e1;
    padding: 3px 8px;
    border-radius: 4px;
    font-size: 11px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .try-chip:hover {
    border-color: #d4a017;
    background: #1e293b;
  }
  .try-chip code {
    background: transparent;
    padding: 0;
    color: #d4a017;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 11px;
  }
  .try-chip span {
    color: #64748b;
  }
  .kid {
    font-size: 11px;
    color: #94a3b8;
  }
  .kid code {
    background: #1e293b;
    padding: 2px 6px;
    border-radius: 4px;
    color: #cbd5e1;
    font-family: ui-monospace, SFMono-Regular, monospace;
  }
  main {
    max-width: 1400px;
    margin: 0 auto;
    padding: 20px 24px;
    display: grid;
    grid-template-columns: minmax(0, 380px) minmax(0, 1fr);
    grid-template-rows: auto auto auto auto;
    grid-template-areas:
      'model goal'
      'model graph'
      'timeline graph'
      'receipt graph';
    gap: 16px;
  }
  /* On viewports too narrow for the 2-column layout, stack everything in
     a single column. Visualization-first ordering: form → graph → details. */
  @media (max-width: 960px) {
    main {
      grid-template-columns: 1fr;
      grid-template-areas:
        'goal'
        'graph'
        'timeline'
        'receipt'
        'model';
      padding: 14px;
    }
    .graph {
      min-height: 360px;
    }
    .goal-form {
      grid-template-columns: 1fr 1fr;
      grid-template-rows: auto auto auto;
    }
    .goal-form label:nth-child(3) {
      grid-column: 1 / -1;
    }
    .goal-form button {
      grid-column: 1 / -1;
    }
  }
  .model {
    grid-area: model;
  }
  .goal {
    grid-area: goal;
  }
  .graph {
    grid-area: graph;
    min-height: 520px;
    display: flex;
    flex-direction: column;
  }
  .timeline {
    grid-area: timeline;
  }
  .receipt {
    grid-area: receipt;
  }
  section h2 {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #94a3b8;
    margin: 0 0 8px 0;
    font-weight: 600;
  }
  .graph :global(.graph) {
    flex: 1;
  }
  .viz-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }
  .viz-header h2 {
    margin: 0;
  }
  .viz-tabs {
    display: flex;
    gap: 2px;
    background: #0f172a;
    border: 1px solid #1e293b;
    border-radius: 4px;
    padding: 2px;
  }
  .tab {
    background: transparent;
    border: none;
    color: #94a3b8;
    padding: 3px 10px;
    border-radius: 3px;
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    text-transform: lowercase;
    letter-spacing: 0.04em;
  }
  .tab.active {
    background: #1e293b;
    color: #d4a017;
  }
  .tab:hover:not(.active) {
    color: #cbd5e1;
  }
  .busy {
    margin-left: 8px;
    font-size: 10px;
    color: #d4a017;
    font-weight: 500;
    text-transform: lowercase;
    letter-spacing: 0.06em;
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    0%, 100% { opacity: 0.5; }
    50%      { opacity: 1; }
  }
  .goal-form {
    display: grid;
    grid-template-columns: 1fr 1fr 2fr auto;
    gap: 8px;
    align-items: end;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: #94a3b8;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  input {
    background: #0f172a;
    border: 1px solid #1e293b;
    color: #e2e8f0;
    padding: 6px 8px;
    border-radius: 4px;
    font-size: 13px;
    font-family: ui-monospace, SFMono-Regular, monospace;
  }
  input:focus {
    outline: none;
    border-color: #d4a017;
  }
  button {
    background: #d4a017;
    color: #0b1220;
    border: none;
    padding: 7px 12px;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .err {
    margin-top: 8px;
    padding: 8px;
    background: #2a0808;
    color: #f87171;
    border-radius: 6px;
    font-size: 12px;
  }
  .agent-btn {
    background: #1e293b;
    color: #d4a017;
    border: 1px solid #d4a017;
  }
  .agent-panel {
    margin-top: 10px;
    padding: 10px 12px;
    background: #0f172a;
    border: 1px solid #1e293b;
    border-left: 3px solid #d4a017;
    border-radius: 6px;
    font-size: 12px;
  }
  .agent-head {
    display: flex;
    gap: 10px;
    align-items: baseline;
    margin-bottom: 6px;
  }
  .agent-label {
    color: #d4a017;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-size: 10px;
    font-weight: 600;
  }
  .agent-model {
    background: #1e293b;
    padding: 1px 6px;
    border-radius: 3px;
    color: #cbd5e1;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 10px;
  }
  .agent-hash {
    margin-left: auto;
    color: #94a3b8;
    font-size: 10px;
  }
  .agent-hash code {
    background: #1e293b;
    padding: 1px 4px;
    border-radius: 3px;
    color: #cbd5e1;
    font-family: ui-monospace, SFMono-Regular, monospace;
  }
  .agent-proposals {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .agent-proposals li {
    display: flex;
    gap: 8px;
    align-items: baseline;
    padding: 3px 0;
  }
  .agent-proposals code {
    background: #1e293b;
    padding: 1px 6px;
    border-radius: 3px;
    color: #d4a017;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 11px;
  }
  .agent-reasoning {
    color: #cbd5e1;
    font-size: 11px;
    line-height: 1.4;
  }
  .agent-note {
    margin-top: 8px;
    padding-top: 6px;
    border-top: 1px solid #1e293b;
    color: #94a3b8;
    font-size: 10px;
    line-height: 1.5;
  }
  .replay-pill {
    background: #0b1220;
    border: 1px solid #334155;
    padding: 1px 6px;
    border-radius: 3px;
    color: #94a3b8;
    font-size: 10px;
    font-family: ui-monospace, SFMono-Regular, monospace;
    letter-spacing: 0.03em;
  }
  .chips-panel {
    margin-top: 10px;
    padding: 10px 12px;
    background: #0f172a;
    border: 1px solid #1e293b;
    border-left: 3px solid #d4a017;
    border-radius: 6px;
  }
  .chips-head {
    font-size: 12px;
    color: #cbd5e1;
    margin-bottom: 8px;
    line-height: 1.45;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    background: #1e293b;
    border: 1px solid #334155;
    color: #cbd5e1;
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 11px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .chip:hover {
    background: #334155;
    border-color: #d4a017;
  }
  .chip code {
    background: transparent;
    padding: 0;
    color: #d4a017;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 11px;
  }
  .chip span {
    color: #64748b;
    font-size: 11px;
  }
</style>
