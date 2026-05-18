<script lang="ts">
  // Native egraph renderer using the Cytoscape we already ship.
  //
  // Why not use @saulshanabrook/egraph-visualizer:
  //   - 13.8MB raw (pulls React + ELK), peer-depends on Tailwind, and
  //     its unstyled-without-tailwind empty state leaks through our UI.
  //   - We already render graphs with Cytoscape in GraphView.svelte;
  //     reusing that engine keeps the bundle tight and the theme consistent.
  //
  // What we render: every node in the egraph-serialize JSON becomes a
  // Cytoscape node labeled with its `op` and tinted by eclass (a stable
  // hash → HSL color so nodes in the same equivalence class share a hue).
  // Edges are parent → child for each child in `children`. New nodes
  // between frames flash gold for ~600ms so the scrubber actually shows
  // *what changed* per candidate, not just a static dump.

  import { onDestroy, onMount } from 'svelte';
  import cytoscape, { type Core, type ElementDefinition } from 'cytoscape';
  // @ts-expect-error — cytoscape-dagre has no shipped types
  import dagre from 'cytoscape-dagre';
  import type { EgraphSnapshot } from './api';

  cytoscape.use(dagre);

  let {
    snapshots,
    autoplay = true,
  }: {
    snapshots: EgraphSnapshot[];
    autoplay?: boolean;
  } = $props();

  let container: HTMLDivElement;
  let cy: Core | null = null;
  let index = $state(0);
  let playing = $state(false);
  let playTimer: ReturnType<typeof setInterval> | null = null;
  // Track nodes that appeared in the most recent frame transition so we
  // can flash-highlight them. Keyed by node id.
  let recentlyAdded = new Set<string>();

  // egraph-serialize node shape; we keep it loose because the package's
  // JSON contains some opaque metadata we don't need to type.
  type Node = { op: string; children?: string[]; eclass?: string };
  type Graph = { nodes?: Record<string, Node> };

  let current = $derived<EgraphSnapshot | undefined>(snapshots[index]);
  let outcomeBadge = $derived.by<{ label: string; color: string } | null>(() => {
    if (!current || current.kind !== 'candidate') return null;
    return current.accepted
      ? { label: 'accepted', color: '#16a34a' }
      : { label: 'rejected', color: '#dc2626' };
  });

  let stopAt = $derived.by(() => {
    const accIdx = snapshots.findIndex(
      (s) => s.kind === 'candidate' && s.accepted === true,
    );
    return accIdx >= 0 ? accIdx : snapshots.length - 1;
  });

  // Stable HSL color for an eclass — keeps the same class the same color
  // across frames so the eye can track equivalences.
  function eclassColor(eclass: string | undefined): string {
    if (!eclass) return '#475569';
    let h = 0;
    for (let i = 0; i < eclass.length; i++) {
      h = (h * 31 + eclass.charCodeAt(i)) | 0;
    }
    const hue = Math.abs(h) % 360;
    return `hsl(${hue}, 55%, 55%)`;
  }

  // Strip surrounding quotes for nicer labels — egglog literals come in as
  // `"alice"`, `"read"`, etc.
  function prettyOp(op: string): string {
    if (op.length >= 2 && op[0] === '"' && op[op.length - 1] === '"') {
      return op.slice(1, -1);
    }
    return op;
  }

  function buildElements(graph: Graph): ElementDefinition[] {
    const els: ElementDefinition[] = [];
    const nodes = graph.nodes ?? {};
    for (const [id, n] of Object.entries(nodes)) {
      els.push({
        data: {
          id,
          label: prettyOp(n.op),
          eclass: n.eclass ?? '',
          color: eclassColor(n.eclass),
          isNew: recentlyAdded.has(id),
        },
      });
    }
    for (const [id, n] of Object.entries(nodes)) {
      for (const child of n.children ?? []) {
        if (!nodes[child]) continue;
        els.push({
          data: {
            id: `${id}->${child}`,
            source: id,
            target: child,
          },
        });
      }
    }
    return els;
  }

  function render(i: number) {
    if (!cy || !snapshots[i]) return;
    const graph = snapshots[i].graph as Graph;
    cy.elements().remove();
    cy.add(buildElements(graph));
    cy.layout({
      name: 'dagre',
      rankDir: 'LR',
      nodeSep: 30,
      rankSep: 60,
      // padding around the whole graph so labels don't kiss the panel edge
      fit: true,
      padding: 16,
    } as never).run();
  }

  // Compute the set of node ids that exist in `next` but not in `prev`.
  function newIds(prev: Graph | undefined, next: Graph): Set<string> {
    const out = new Set<string>();
    const prevNodes = prev?.nodes ?? {};
    for (const id of Object.keys(next.nodes ?? {})) {
      if (!(id in prevNodes)) out.add(id);
    }
    return out;
  }

  function moveTo(next: number) {
    if (next < 0 || next >= snapshots.length || !cy) return;
    const prevGraph = snapshots[index]?.graph as Graph | undefined;
    const nextGraph = snapshots[next].graph as Graph;
    recentlyAdded = newIds(prevGraph, nextGraph);
    index = next;
    // The $effect on `index` will trigger re-render.
    // Clear the highlight after the layout settles.
    setTimeout(() => {
      recentlyAdded = new Set();
      // Re-render without the highlight glow.
      if (cy) {
        cy.nodes().forEach((n) => {
          n.data('isNew', false);
        });
      }
    }, 650);
  }

  function startPlay() {
    if (snapshots.length <= 1) return;
    playing = true;
    playTimer = setInterval(() => {
      if (index >= stopAt) {
        stopPlay();
        return;
      }
      moveTo(index + 1);
    }, 950);
  }

  function stopPlay() {
    if (playTimer) clearInterval(playTimer);
    playTimer = null;
    playing = false;
  }

  function togglePlay() {
    if (playing) stopPlay();
    else if (index >= snapshots.length - 1) {
      moveTo(0);
      startPlay();
    } else startPlay();
  }

  function step(dir: -1 | 1) {
    stopPlay();
    moveTo(index + dir);
  }

  function onSlider(e: Event) {
    stopPlay();
    const v = Number((e.target as HTMLInputElement).value);
    moveTo(v);
  }

  onMount(() => {
    if (!container) return;
    cy = cytoscape({
      container,
      style: [
        {
          selector: 'node',
          style: {
            'background-color': 'data(color)',
            'border-color': '#0b1220',
            'border-width': 2,
            label: 'data(label)',
            color: '#0b1220',
            'font-size': 10,
            'font-weight': 600,
            'text-valign': 'center',
            'text-halign': 'center',
            'text-margin-y': 0,
            'text-wrap': 'wrap',
            'text-max-width': '100px',
            // Fixed dimensions instead of the deprecated `'label'` value.
            // Wide enough for the longest realistic egglog op name in our
            // ruleset; text-wrap handles overflow gracefully.
            width: 96,
            height: 36,
            shape: 'round-rectangle',
          } as never,
        },
        {
          selector: 'node[?isNew]',
          style: {
            'border-color': '#d4a017',
            'border-width': 4,
            // `shadow-*` was removed in newer Cytoscape; an overlay gives
            // the same "glow" effect for the new-node highlight.
            'overlay-color': '#d4a017',
            'overlay-padding': 8,
            'overlay-opacity': 0.35,
          } as never,
        },
        {
          selector: 'edge',
          style: {
            width: 1.5,
            'line-color': '#475569',
            'target-arrow-color': '#475569',
            'target-arrow-shape': 'triangle',
            'curve-style': 'bezier',
            'arrow-scale': 0.8,
            opacity: 0.7,
          } as never,
        },
      ],
    });
    render(index);
    if (autoplay && snapshots.length > 1) startPlay();
  });

  onDestroy(() => {
    stopPlay();
    cy?.destroy();
    cy = null;
  });

  $effect(() => {
    if (cy) render(index);
  });

  // Reset when snapshots themselves change (new planning run).
  let lastRef: EgraphSnapshot[] | null = null;
  $effect(() => {
    if (snapshots !== lastRef) {
      lastRef = snapshots;
      stopPlay();
      recentlyAdded = new Set();
      index = 0;
      if (cy) render(0);
      if (autoplay && snapshots.length > 1) startPlay();
    }
  });
</script>

<div class="wrap">
  <div class="viz" bind:this={container}></div>

  <div class="controls">
    <div class="left">
      <button class="ctrl" onclick={() => step(-1)} disabled={index === 0} title="Previous frame">‹</button>
      <button class="ctrl play" onclick={togglePlay} title={playing ? 'Pause' : 'Play'}>
        {playing ? '❚❚' : '▶'}
      </button>
      <button
        class="ctrl"
        onclick={() => step(1)}
        disabled={index >= snapshots.length - 1}
        title="Next frame"
      >›</button>
    </div>

    <div class="middle">
      <input
        type="range"
        min="0"
        max={Math.max(0, snapshots.length - 1)}
        value={index}
        oninput={onSlider}
        disabled={snapshots.length <= 1}
      />
      <div class="frame-label">
        <span class="counter">
          {snapshots.length === 0 ? '—' : `${index + 1} / ${snapshots.length}`}
        </span>
        <span class="label">{current?.label ?? ''}</span>
        {#if outcomeBadge}
          <span class="badge" style="background: {outcomeBadge.color}">
            {outcomeBadge.label}
          </span>
        {/if}
      </div>
    </div>

    <div class="right">
      <span class="hint">drag to scrub · gold ring = new in this frame</span>
    </div>
  </div>
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 460px;
    background: #0b1220;
    border-radius: 8px;
    overflow: hidden;
  }
  .viz {
    flex: 1;
    min-height: 360px;
    background: #0b1220;
  }
  .controls {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: #0f172a;
    border-top: 1px solid #1e293b;
  }
  .left {
    display: flex;
    gap: 4px;
  }
  .ctrl {
    width: 30px;
    height: 26px;
    background: #1e293b;
    border: 1px solid #334155;
    color: #cbd5e1;
    border-radius: 4px;
    font-size: 12px;
    cursor: pointer;
    display: grid;
    place-items: center;
  }
  .ctrl.play {
    color: #d4a017;
    border-color: #d4a017;
  }
  .ctrl:hover:not(:disabled) {
    background: #334155;
  }
  .ctrl:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .middle {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  input[type='range'] {
    width: 100%;
    accent-color: #d4a017;
  }
  .frame-label {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 12px;
    color: #cbd5e1;
    min-height: 18px;
  }
  .counter {
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 11px;
    color: #94a3b8;
  }
  .label {
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #e2e8f0;
  }
  .badge {
    padding: 1px 7px;
    border-radius: 3px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #0b1220;
  }
  .right {
    text-align: right;
  }
  .hint {
    font-size: 10px;
    color: #64748b;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
</style>
