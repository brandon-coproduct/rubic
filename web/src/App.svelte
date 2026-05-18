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
    <div class="eyebrow">Open source · MIT · MCP-ready</div>
    <h1>rubic</h1>
    <div class="lede">Verified authorization for AI agent tool calls.</div>
    <div class="sub">
      An LLM proposes which tool-call capabilities to grant. An egglog
      rewrite system + policy invariants validate each proposal
      server-side. A signed, hash-chained receipt witnesses the decision.
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
    padding: var(--s-6) var(--s-7);
    border-bottom: 1px solid var(--border);
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    flex-wrap: wrap;
    gap: var(--s-4);
  }
  .title {
    flex: 1;
    min-width: 280px;
  }
  .eyebrow {
    font-size: var(--fs-eyebrow);
    font-weight: var(--fw-medium);
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    margin-bottom: var(--s-2);
  }
  h1 {
    margin: 0;
    font-size: var(--fs-head);
    letter-spacing: -0.02em;
    color: var(--text);
    font-weight: var(--fw-semibold);
    line-height: var(--leading-tight);
  }
  .lede {
    font-size: var(--fs-body);
    color: var(--text-2);
    margin-top: var(--s-2);
    font-weight: var(--fw-medium);
  }
  .sub {
    font-size: var(--fs-body);
    color: var(--text-3);
    margin-top: var(--s-2);
    line-height: var(--leading-body);
    max-width: 640px;
  }
  .meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: var(--s-2);
  }
  .meta-row {
    display: flex;
    align-items: center;
    gap: var(--s-2);
  }
  .repo-link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-3);
    text-decoration: none;
    font-size: var(--fs-label);
    padding: 6px 10px;
    border-radius: var(--radius);
    transition: color 120ms ease;
  }
  .repo-link:hover {
    color: var(--accent);
  }
  .mcp-btn {
    background: var(--accent);
    color: var(--bg);
    border: none;
    padding: 7px 14px;
    border-radius: var(--radius);
    font-size: var(--fs-label);
    font-weight: var(--fw-semibold);
    font-family: var(--font-sans);
    cursor: pointer;
    letter-spacing: 0;
    transition: opacity 120ms ease;
  }
  .mcp-btn:hover {
    opacity: 0.9;
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
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: var(--s-6);
    color: var(--text);
    z-index: 51;
    box-shadow: 0 24px 70px rgba(0, 0, 0, 0.55);
  }
  .modal-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--s-4);
  }
  .modal-head h3 {
    margin: 0;
    font-size: var(--fs-body);
    color: var(--text);
    font-weight: var(--fw-semibold);
    letter-spacing: -0.01em;
  }
  .modal-x {
    background: transparent;
    border: none;
    color: var(--text-3);
    font-size: var(--fs-body);
    cursor: pointer;
    padding: 4px 8px;
    font-family: var(--font-sans);
  }
  .modal-x:hover {
    color: var(--text);
    opacity: 1;
  }
  .modal-body {
    margin: 0 0 var(--s-3) 0;
    font-size: var(--fs-body);
    line-height: var(--leading-body);
    color: var(--text-2);
  }
  .modal-body code {
    background: var(--bg-3);
    padding: 1px 5px;
    border-radius: 3px;
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--fs-label);
  }
  .mcp-snippet {
    background: var(--bg);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: var(--s-3) var(--s-4);
    font-family: var(--font-mono);
    font-size: var(--fs-label);
    overflow-x: auto;
    margin: 0 0 var(--s-3) 0;
    line-height: 1.55;
  }
  .modal-actions {
    display: flex;
    gap: var(--s-3);
    align-items: center;
    justify-content: flex-end;
  }
  .copy-btn {
    background: var(--accent);
    color: var(--bg);
    border: none;
    padding: 7px 14px;
    border-radius: var(--radius);
    font-size: var(--fs-label);
    font-weight: var(--fw-semibold);
    font-family: var(--font-sans);
    cursor: pointer;
  }
  .docs-link {
    color: var(--text-3);
    font-size: var(--fs-label);
    text-decoration: none;
  }
  .docs-link:hover {
    color: var(--accent);
  }
  .try-chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s-2);
    align-items: center;
    margin-bottom: var(--s-3);
  }
  .try-label {
    font-size: var(--fs-eyebrow);
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: var(--fw-medium);
    margin-right: var(--s-1);
  }
  /* Flat chips — no border, just a slight background tint that brightens
     on hover. Reads as "this is a low-commitment shortcut," not a primary
     action that competes with the buttons. */
  .try-chip {
    background: var(--bg-2);
    border: none;
    color: var(--text-2);
    padding: 5px 10px;
    border-radius: var(--radius);
    font-size: var(--fs-label);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: var(--s-1);
    font-family: var(--font-mono);
    transition: background 120ms ease, color 120ms ease;
  }
  .try-chip:hover {
    background: var(--bg-3);
    color: var(--text);
  }
  .try-chip code {
    background: transparent;
    padding: 0;
    color: inherit;
    font-family: var(--font-mono);
    font-size: var(--fs-label);
  }
  .try-chip span {
    color: var(--text-4);
  }
  .kid {
    font-size: var(--fs-label);
    color: var(--text-4);
  }
  .kid code {
    color: var(--text-3);
    background: transparent;
    padding: 0;
    font-family: var(--font-mono);
  }
  main {
    max-width: 1400px;
    margin: 0 auto;
    padding: var(--s-6) var(--s-7);
    display: grid;
    grid-template-columns: minmax(0, 380px) minmax(0, 1fr);
    grid-template-rows: auto auto auto auto;
    grid-template-areas:
      'model goal'
      'model graph'
      'timeline graph'
      'receipt graph';
    gap: var(--s-5);
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
    font-size: var(--fs-eyebrow);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-3);
    margin: 0 0 var(--s-3) 0;
    font-weight: var(--fw-medium);
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
    gap: 0;
    background: var(--bg-2);
    border-radius: var(--radius);
    padding: 2px;
  }
  .tab {
    background: transparent;
    border: none;
    color: var(--text-3);
    padding: 4px 10px;
    border-radius: 4px;
    font-size: var(--fs-eyebrow);
    font-weight: var(--fw-medium);
    cursor: pointer;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    transition: color 120ms ease, background 120ms ease;
    font-family: var(--font-sans);
  }
  .tab.active {
    background: var(--bg-3);
    color: var(--text);
  }
  .tab:hover:not(.active) {
    color: var(--text);
  }
  .busy {
    margin-left: var(--s-2);
    font-size: var(--fs-eyebrow);
    color: var(--accent);
    font-weight: var(--fw-medium);
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
    gap: var(--s-1);
    font-size: var(--fs-eyebrow);
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-weight: var(--fw-medium);
  }
  input {
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text);
    padding: 8px 10px;
    border-radius: var(--radius);
    font-size: var(--fs-body);
    font-family: var(--font-mono);
    transition: border-color 120ms ease;
  }
  input:focus {
    outline: none;
    border-color: var(--accent);
  }
  button {
    background: var(--accent);
    color: var(--bg);
    border: none;
    padding: 8px 14px;
    border-radius: var(--radius);
    font-size: var(--fs-label);
    font-weight: var(--fw-semibold);
    font-family: var(--font-sans);
    cursor: pointer;
    transition: opacity 120ms ease;
  }
  button:hover:not(:disabled) {
    opacity: 0.9;
  }
  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .err {
    margin-top: var(--s-2);
    padding: var(--s-3);
    background: #2a0808;
    color: #f87171;
    border-radius: var(--radius);
    font-size: var(--fs-label);
  }
  /* Secondary action: solid but distinct from the primary gold so the
     two paths read as siblings, not as "primary vs outlined." */
  .agent-btn {
    background: var(--bg-3);
    color: var(--text);
    border: none;
  }
  .agent-btn:hover:not(:disabled) {
    background: var(--border-2);
    opacity: 1;
  }
  .agent-panel {
    margin-top: var(--s-3);
    padding: var(--s-4);
    background: var(--bg-2);
    border-radius: var(--radius);
    border-left: 2px solid var(--accent);
    font-size: var(--fs-label);
  }
  .agent-head {
    display: flex;
    gap: var(--s-3);
    align-items: baseline;
    margin-bottom: var(--s-2);
  }
  .agent-label {
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-size: var(--fs-eyebrow);
    font-weight: var(--fw-semibold);
  }
  .agent-model {
    color: var(--text-3);
    font-family: var(--font-mono);
    font-size: var(--fs-eyebrow);
    background: transparent;
    padding: 0;
  }
  .agent-hash {
    margin-left: auto;
    color: var(--text-4);
    font-size: var(--fs-eyebrow);
  }
  .agent-hash code {
    color: var(--text-3);
    background: transparent;
    padding: 0;
    font-family: var(--font-mono);
  }
  .agent-proposals {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .agent-proposals li {
    display: flex;
    gap: var(--s-2);
    align-items: baseline;
    padding: var(--s-1) 0;
  }
  .agent-proposals code {
    background: transparent;
    padding: 0;
    color: var(--accent);
    font-family: var(--font-mono);
    font-size: var(--fs-label);
    flex-shrink: 0;
  }
  .agent-reasoning {
    color: var(--text-2);
    font-size: var(--fs-label);
    line-height: var(--leading-body);
  }
  .agent-note {
    margin-top: var(--s-3);
    padding-top: var(--s-2);
    border-top: 1px solid var(--border);
    color: var(--text-4);
    font-size: var(--fs-eyebrow);
    line-height: var(--leading-body);
  }
  .replay-pill {
    background: var(--bg-3);
    padding: 1px 6px;
    border-radius: 3px;
    color: var(--text-3);
    font-size: var(--fs-eyebrow);
    font-family: var(--font-mono);
    letter-spacing: 0.03em;
  }
  .chips-panel {
    margin-top: var(--s-3);
    padding: var(--s-4);
    background: var(--bg-2);
    border-radius: var(--radius);
    border-left: 2px solid var(--accent);
  }
  .chips-head {
    font-size: var(--fs-label);
    color: var(--text-2);
    margin-bottom: var(--s-2);
    line-height: var(--leading-body);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s-2);
  }
  .chip {
    background: var(--bg-3);
    border: none;
    color: var(--text-2);
    padding: 5px 10px;
    border-radius: var(--radius);
    font-size: var(--fs-label);
    font-family: var(--font-mono);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: var(--s-1);
    transition: background 120ms ease;
  }
  .chip:hover {
    background: var(--border-2);
    color: var(--text);
  }
  .chip code {
    background: transparent;
    padding: 0;
    color: var(--accent);
    font-family: var(--font-mono);
    font-size: var(--fs-label);
  }
  .chip span {
    color: var(--text-4);
    font-size: var(--fs-label);
  }
</style>
