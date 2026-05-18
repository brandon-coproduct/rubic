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
    background: #0f172a;
    border: 1px solid #1e293b;
    border-left: 3px solid #d4a017;
    border-radius: 8px;
    padding: 14px 16px;
    color: #e2e8f0;
    font-size: 13px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
  }
  .version {
    background: #1e293b;
    padding: 2px 6px;
    border-radius: 4px;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 11px;
    margin-right: 8px;
    color: #cbd5e1;
  }
  .decision {
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
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
    gap: 6px;
  }
  button {
    background: #1e293b;
    border: 1px solid #334155;
    color: #cbd5e1;
    padding: 4px 10px;
    border-radius: 4px;
    font-size: 12px;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: #334155;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  dl {
    display: grid;
    grid-template-columns: 160px 1fr;
    column-gap: 12px;
    row-gap: 4px;
    margin: 8px 0;
    font-size: 12px;
  }
  dt {
    color: #94a3b8;
    font-family: ui-monospace, SFMono-Regular, monospace;
  }
  dd {
    margin: 0;
  }
  code {
    font-family: ui-monospace, SFMono-Regular, monospace;
    color: #cbd5e1;
  }
  h4 {
    margin: 12px 0 4px 0;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #94a3b8;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    font-size: 12px;
  }
  li {
    padding: 4px 0;
    border-bottom: 1px solid #1e293b;
  }
  li:last-child {
    border-bottom: none;
  }
  .just {
    margin-top: 2px;
    color: #94a3b8;
    font-size: 11px;
  }
  .verify-row {
    margin-top: 12px;
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .verify-result {
    margin-top: 8px;
    padding: 8px 10px;
    border-radius: 6px;
    font-size: 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
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
    margin-top: 8px;
    padding: 8px 10px;
    background: #2a0808;
    color: #f87171;
    border-radius: 6px;
    font-size: 12px;
  }
  .muted {
    color: #94a3b8;
  }
  .card {
    background: #0f172a;
    border: 1px solid #1e293b;
    border-radius: 8px;
    padding: 12px;
    font-size: 13px;
  }
</style>
