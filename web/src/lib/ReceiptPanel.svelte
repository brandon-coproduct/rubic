<script lang="ts">
  import { api, type Receipt, type VerifyView } from './api';

  let {
    receipt,
    receiptId,
  }: { receipt: Receipt | null; receiptId: number | null } = $props();

  let verifyState: VerifyView | null = $state(null);
  let verifyError: string | null = $state(null);
  let verifying = $state(false);

  async function doVerify() {
    if (!receiptId) return;
    verifying = true;
    verifyState = null;
    verifyError = null;
    try {
      verifyState = await api.verifyReceipt(receiptId);
    } catch (e) {
      verifyError = (e as Error).message;
    } finally {
      verifying = false;
    }
  }

  function copyJson() {
    if (!receipt) return;
    navigator.clipboard.writeText(JSON.stringify(receipt, null, 2));
  }

  function downloadJson() {
    if (!receipt) return;
    const blob = new Blob([JSON.stringify(receipt, null, 2)], {
      type: 'application/json',
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `receipt-${receiptId ?? 'unsigned'}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  const short = (h?: string) => (h ? `${h.slice(0, 12)}…${h.slice(-8)}` : '—');
</script>

{#if !receipt}
  <div class="muted card">No receipt yet.</div>
{:else}
  <div class="receipt">
    <div class="head">
      <div>
        <span class="version">{receipt.receipt_version}</span>
        <span class="decision {receipt.decision}">{receipt.decision}</span>
      </div>
      <div class="actions">
        <button onclick={copyJson}>Copy JSON</button>
        <button onclick={downloadJson}>Download</button>
      </div>
    </div>

    <dl>
      <dt title="BLAKE3 of the normalized RBAC model (users, roles, policy). Binds the receipt to exactly which permissions existed when the decision was made — any later edit of the model changes this hash.">model_digest</dt>
      <dd><code title={receipt.model_digest}>{short(receipt.model_digest)}</code></dd>
      <dt title="BLAKE3 of the egglog ruleset source. Proves which derivation rules (e.g. Assigned + HasPerm → CanReach) the server was running. Lets you detect a server that swapped its rules out from under you.">rules_digest</dt>
      <dd><code title={receipt.rules_digest}>{short(receipt.rules_digest)}</code></dd>
      <dt title="BLAKE3 of the canonical (user, action, resource) triple this receipt answered for.">goal_digest</dt>
      <dd><code title={receipt.goal_digest}>{short(receipt.goal_digest)}</code></dd>
      <dt title="BLAKE3 of the role-assignment plan the server accepted. None for rejection receipts.">accepted_plan_digest</dt>
      <dd>
        {#if receipt.accepted_plan_digest}
          <code title={receipt.accepted_plan_digest}>
            {short(receipt.accepted_plan_digest)}
          </code>
        {:else}
          <span class="muted">none (rejected)</span>
        {/if}
      </dd>
      <dt title="RFC3339 UTC instant the decision was signed.">timestamp</dt>
      <dd><code>{receipt.timestamp}</code></dd>
      <dt title="How many candidate role assignments the planner considered for this goal.">candidate_count</dt>
      <dd>{receipt.candidate_count}</dd>
      <dt title="Short id of the ed25519 verifying key that signed this receipt. Matches the server's /healthz kid until the key is rotated.">proof.kid</dt>
      <dd><code>{receipt.proof.kid}</code></dd>
      <dt title="Signature algorithm — always Ed25519 in this version.">proof.alg</dt>
      <dd><code>{receipt.proof.alg}</code></dd>
      <dt title="BLAKE3 of the previous receipt's canonical bytes + sig — forms a hash-chained audit log. None for the first receipt in a chain.">proof.prev_hash</dt>
      <dd>
        {#if receipt.proof.prev_hash}
          <code title={receipt.proof.prev_hash}>{short(receipt.proof.prev_hash)}</code>
        {:else}
          <span class="muted">none (first in chain)</span>
        {/if}
      </dd>
    </dl>

    {#if receipt.steps.length > 0}
      <h4>Steps</h4>
      <ul>
        {#each receipt.steps as s}
          <li>
            <code>{s.op}</code> <code>{s.user}</code> → <code>{s.role}</code>
            <div class="just">{s.justification}</div>
          </li>
        {/each}
      </ul>
    {/if}

    {#if receipt.rejections.length > 0}
      <h4>Rejections</h4>
      <ul>
        {#each receipt.rejections as r}
          <li>
            <code>{r.candidate}</code>: {r.reason}
          </li>
        {/each}
      </ul>
    {/if}

    <div class="verify-row">
      <button onclick={doVerify} disabled={!receiptId || verifying}>
        {verifying ? 'Verifying…' : 'Verify receipt'}
      </button>
      {#if receiptId === null}
        <span class="muted">not persisted — verify is N/A</span>
      {/if}
    </div>

    {#if verifyError}
      <div class="verify-fail">verify error: {verifyError}</div>
    {/if}
    {#if verifyState}
      <div
        class="verify-result"
        class:ok={verifyState.signature_valid && verifyState.chain_valid}
        class:bad={!(verifyState.signature_valid && verifyState.chain_valid)}
      >
        <span>signature: {verifyState.signature_valid ? '✓' : '✗'}</span>
        <span>chain: {verifyState.chain_valid ? '✓' : '✗'}</span>
        {#if verifyState.notes.length > 0}
          <ul>
            {#each verifyState.notes as n}<li>{n}</li>{/each}
          </ul>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .receipt {
    background: var(--bg-2);
    border-radius: var(--radius);
    border-left: 2px solid var(--accent);
    padding: var(--s-4) var(--s-5);
    color: var(--text);
    font-size: var(--fs-body);
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--s-3);
  }
  .version {
    padding: 0;
    font-family: var(--font-mono);
    font-size: var(--fs-eyebrow);
    margin-right: var(--s-2);
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .decision {
    padding: 2px 8px;
    border-radius: 3px;
    font-size: var(--fs-eyebrow);
    font-weight: var(--fw-semibold);
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }
  .decision.accepted {
    background: #052e16;
    color: #4ade80;
  }
  .decision.rejected {
    background: #2a0808;
    color: #f87171;
  }
  .actions {
    display: flex;
    gap: var(--s-2);
  }
  button {
    background: var(--bg-3);
    border: none;
    color: var(--text-2);
    padding: 5px 12px;
    border-radius: var(--radius);
    font-size: var(--fs-label);
    font-family: var(--font-sans);
    cursor: pointer;
    transition: background 120ms ease, color 120ms ease;
  }
  button:hover:not(:disabled) {
    background: var(--border-2);
    color: var(--text);
  }
  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  dl {
    display: grid;
    grid-template-columns: 160px 1fr;
    column-gap: var(--s-3);
    row-gap: var(--s-1);
    margin: var(--s-3) 0;
    font-size: var(--fs-label);
  }
  dt {
    color: var(--text-3);
    font-family: var(--font-mono);
  }
  dd {
    margin: 0;
  }
  code {
    font-family: var(--font-mono);
    color: var(--text-2);
  }
  h4 {
    margin: var(--s-4) 0 var(--s-2) 0;
    font-size: var(--fs-eyebrow);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-3);
    font-weight: var(--fw-medium);
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    font-size: var(--fs-label);
  }
  li {
    padding: var(--s-2) 0;
    border-bottom: 1px solid var(--border);
  }
  li:last-child {
    border-bottom: none;
  }
  .just {
    margin-top: 2px;
    color: var(--text-3);
    font-size: var(--fs-eyebrow);
    line-height: var(--leading-body);
  }
  .verify-row {
    margin-top: var(--s-4);
    display: flex;
    gap: var(--s-3);
    align-items: center;
  }
  .verify-result {
    margin-top: var(--s-2);
    padding: var(--s-3);
    border-radius: var(--radius);
    font-size: var(--fs-label);
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
  }
  .verify-result.ok {
    background: #052e16;
    color: #4ade80;
  }
  .verify-result.bad {
    background: #2a0808;
    color: #f87171;
  }
  .verify-fail {
    margin-top: var(--s-2);
    padding: var(--s-3);
    background: #2a0808;
    color: #f87171;
    border-radius: var(--radius);
    font-size: var(--fs-label);
  }
  .muted {
    color: var(--text-3);
  }
  .card {
    background: var(--bg-2);
    border-radius: var(--radius);
    padding: var(--s-4);
    font-size: var(--fs-body);
  }
</style>
