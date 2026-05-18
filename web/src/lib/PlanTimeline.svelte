<script lang="ts">
  import type { PlanningOutcome } from './api';

  let { outcome }: { outcome: PlanningOutcome | null } = $props();
</script>

{#if outcome?.goal_unreachable}
  <div class="card unreachable">
    <strong>Goal infeasible.</strong>
    {outcome.goal_unreachable.explanation}
  </div>
{:else if outcome && outcome.candidates.length === 0}
  <div class="card muted">No candidate tool-grants generated.</div>
{:else if outcome}
  <div class="timeline">
    {#each outcome.candidates as c, i (c.role)}
      <div class="row" class:accepted={c.accepted} class:rejected={!c.accepted}>
        <div class="bullet">{i + 1}</div>
        <div class="body">
          <div class="head">
            <code>{c.role}</code>
            <span class="badge {c.accepted ? 'ok' : 'bad'}">
              {c.accepted ? 'accepted' : 'rejected'}
            </span>
            <span class="cost">cost {c.cost}</span>
            <span class="derives" title="egglog reachability">
              {c.derives_goal ? '⊢ reaches goal' : '⊬ does not reach goal'}
            </span>
          </div>
          <div class="delta">
            +
            {#each c.granted_delta as p}
              <code class="perm">{p.action}:{p.resource}</code>
            {/each}
            {#if c.granted_delta.length === 0}
              <span class="muted">no permission change</span>
            {/if}
          </div>
          {#if c.violations.length > 0}
            <ul class="violations">
              {#each c.violations as v}
                <li>
                  <span class="vkind">{v.invariant}</span>
                  <span>{v.explanation}</span>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      </div>
    {/each}
  </div>
{:else}
  <div class="card muted">Submit a goal to see candidates.</div>
{/if}

<style>
  .timeline {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }
  .row {
    display: grid;
    grid-template-columns: 28px 1fr;
    gap: var(--s-3);
    padding: var(--s-4);
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .row.accepted {
    border-left: 2px solid var(--accept-edge);
  }
  .row.rejected {
    border-left: 2px solid var(--reject-edge);
  }
  .bullet {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--bg-2);
    color: var(--text-3);
    display: grid;
    place-items: center;
    font-size: var(--fs-label);
    font-weight: var(--fw-semibold);
    font-family: var(--font-mono);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    flex-wrap: wrap;
    font-size: var(--fs-body);
  }
  .head code {
    background: transparent;
    padding: 0;
    color: var(--text);
    font-family: var(--font-mono);
    font-weight: var(--fw-medium);
  }
  .badge {
    padding: 2px 7px;
    border-radius: 3px;
    font-size: var(--fs-eyebrow);
    font-weight: var(--fw-semibold);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .badge.ok {
    background: var(--accept-bg);
    color: var(--accept-fg);
  }
  .badge.bad {
    background: var(--reject-bg);
    color: var(--reject-fg);
  }
  .cost,
  .derives {
    font-size: var(--fs-eyebrow);
    color: var(--text-3);
    font-family: var(--font-mono);
  }
  .delta {
    margin-top: var(--s-1);
    font-size: var(--fs-label);
    color: var(--text-2);
    display: flex;
    gap: var(--s-2);
    flex-wrap: wrap;
    align-items: center;
  }
  .perm {
    background: transparent;
    padding: 0;
    color: var(--accent);
    font-size: var(--fs-label);
    font-family: var(--font-mono);
    font-weight: var(--fw-medium);
  }
  .violations {
    margin: var(--s-2) 0 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
  }
  .violations li {
    font-size: var(--fs-label);
    color: var(--reject-fg);
    line-height: var(--leading-body);
  }
  .vkind {
    font-family: var(--font-mono);
    color: var(--reject);
    margin-right: var(--s-1);
    font-weight: var(--fw-semibold);
  }
  .card {
    padding: var(--s-4);
    border-radius: var(--radius);
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-2);
    font-size: var(--fs-body);
  }
  .card.unreachable {
    border-left: 3px solid #d4a017;
  }
  .muted {
    color: #94a3b8;
  }
</style>
