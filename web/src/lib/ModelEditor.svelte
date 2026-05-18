<script lang="ts">
  import type { Model } from './api';

  let {
    model,
    digest,
    rulesDigest,
    onLoad,
  }: {
    model: Model | null;
    digest: string;
    rulesDigest: string;
    onLoad: (toml: string) => Promise<void>;
  } = $props();

  let editing = $state(false);
  let toml = $state('');
  let loading = $state(false);
  let err = $state<string | null>(null);

  async function submit() {
    loading = true;
    err = null;
    try {
      await onLoad(toml);
      editing = false;
    } catch (e) {
      err = (e as Error).message;
    } finally {
      loading = false;
    }
  }

  const short = (h: string) => (h ? `${h.slice(0, 12)}…${h.slice(-8)}` : '—');
</script>

<div class="editor">
  <div class="row">
    <div>
      <div class="label">model_digest</div>
      <code title={digest}>{short(digest)}</code>
    </div>
    <div>
      <div class="label">rules_digest</div>
      <code title={rulesDigest}>{short(rulesDigest)}</code>
    </div>
    <button onclick={() => (editing = !editing)}>
      {editing ? 'Cancel' : 'Edit TOML'}
    </button>
  </div>

  {#if model}
    <div class="summary">
      <div>
        <strong>{Object.keys(model.users).length}</strong> user(s),
        <strong>{Object.keys(model.roles).length}</strong> role(s).
        least_privilege:
        <code>{model.policy.least_privilege ? 'on' : 'off'}</code>
      </div>
      <div class="muted">
        forbidden: {model.policy.forbidden_permissions
          .map((p) => `${p.action}:${p.resource}`)
          .join(', ') || '—'}
      </div>
      <div class="muted">
        requires_approval: {model.policy.requires_approval
          .map((p) => `${p.action}:${p.resource}`)
          .join(', ') || '—'}
      </div>
    </div>
  {/if}

  {#if editing}
    <textarea
      bind:value={toml}
      rows="14"
      placeholder="paste TOML model here…"
    ></textarea>
    {#if err}<div class="err">{err}</div>{/if}
    <div class="actions">
      <button onclick={submit} disabled={loading || !toml}>
        {loading ? 'Loading…' : 'Load model'}
      </button>
    </div>
  {/if}
</div>

<style>
  .editor {
    background: #0f172a;
    border: 1px solid #1e293b;
    border-radius: 8px;
    padding: 12px;
    color: #e2e8f0;
    font-size: 13px;
  }
  .row {
    display: flex;
    gap: 16px;
    align-items: flex-end;
    margin-bottom: 8px;
  }
  .label {
    font-size: 11px;
    color: #94a3b8;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  code {
    font-family: ui-monospace, SFMono-Regular, monospace;
    background: #1e293b;
    padding: 2px 6px;
    border-radius: 4px;
    color: #cbd5e1;
    font-size: 11px;
  }
  button {
    margin-left: auto;
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
  .summary {
    font-size: 12px;
    padding-top: 6px;
    border-top: 1px solid #1e293b;
    margin-top: 6px;
  }
  .muted {
    color: #94a3b8;
    font-size: 11px;
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    margin-top: 8px;
    background: #0b1220;
    color: #e2e8f0;
    border: 1px solid #1e293b;
    border-radius: 6px;
    padding: 8px;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 12px;
    resize: vertical;
  }
  .actions {
    margin-top: 6px;
    display: flex;
    justify-content: flex-end;
  }
  .err {
    margin-top: 6px;
    padding: 8px;
    background: #2a0808;
    color: #f87171;
    border-radius: 6px;
    font-size: 12px;
  }
</style>
