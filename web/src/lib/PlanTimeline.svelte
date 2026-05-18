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
  <div class="card muted">No candidates generated.</div>
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
    gap: 10px;
  }
  .row {
    display: grid;
    grid-template-columns: 28px 1fr;
    gap: 12px;
    padding: 12px;
    background: #0f172a;
    border: 1px solid #1e293b;
    border-radius: 8px;
  }
  .row.accepted {
    border-left: 3px solid #16a34a;
  }
  .row.rejected {
    border-left: 3px solid #dc2626;
  }
  .bullet {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: #1e293b;
    color: #94a3b8;
    display: grid;
    place-items: center;
    font-size: 12px;
    font-weight: 600;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    font-size: 13px;
  }
  .head code {
    background: #1e293b;
    padding: 2px 6px;
    border-radius: 4px;
    color: #e2e8f0;
  }
  .badge {
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .badge.ok {
    background: #052e16;
    color: #4ade80;
  }
  .badge.bad {
    background: #2a0808;
    color: #f87171;
  }
  .cost,
  .derives {
    font-size: 11px;
    color: #94a3b8;
  }
  .delta {
    margin-top: 4px;
    font-size: 12px;
    color: #cbd5e1;
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    align-items: center;
  }
  .perm {
    background: #1e293b;
    padding: 1px 5px;
    border-radius: 3px;
    color: #d4a017;
    font-size: 11px;
  }
  .violations {
    margin: 6px 0 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .violations li {
    font-size: 12px;
    color: #fca5a5;
    line-height: 1.4;
  }
  .vkind {
    font-family: ui-monospace, SFMono-Regular, monospace;
    color: #f87171;
    margin-right: 6px;
  }
  .card {
    padding: 12px;
    border-radius: 8px;
    background: #0f172a;
    border: 1px solid #1e293b;
    color: #cbd5e1;
    font-size: 13px;
  }
  .card.unreachable {
    border-left: 3px solid #d4a017;
  }
  .muted {
    color: #94a3b8;
  }
</style>
