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
    background: var(--bg-2);
    border-radius: var(--radius);
    padding: var(--s-4) var(--s-5);
    color: var(--text);
    font-size: var(--fs-body);
  }
  .row {
    display: flex;
    gap: var(--s-5);
    align-items: flex-end;
    margin-bottom: var(--s-3);
  }
  .label {
    font-size: var(--fs-eyebrow);
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: var(--fw-medium);
  }
  code {
    font-family: var(--font-mono);
    background: transparent;
    padding: 0;
    color: var(--text-2);
    font-size: var(--fs-label);
  }
  button {
    margin-left: auto;
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
  .summary {
    font-size: var(--fs-label);
    padding-top: var(--s-2);
    border-top: 1px solid var(--border);
    margin-top: var(--s-2);
    color: var(--text-2);
  }
  .muted {
    color: var(--text-3);
    font-size: var(--fs-eyebrow);
    font-family: var(--font-mono);
    line-height: var(--leading-body);
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    margin-top: var(--s-2);
    background: var(--bg);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: var(--s-3);
    font-family: var(--font-mono);
    font-size: var(--fs-label);
    line-height: var(--leading-body);
    resize: vertical;
  }
  .actions {
    margin-top: var(--s-2);
    display: flex;
    justify-content: flex-end;
  }
  .err {
    margin-top: var(--s-2);
    padding: var(--s-3);
    background: #2a0808;
    color: #f87171;
    border-radius: var(--radius);
    font-size: var(--fs-label);
  }
</style>
