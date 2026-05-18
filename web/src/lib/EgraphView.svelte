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
  // across frames so the eye can track equivalences. Tuned for white
  // background: muted pastels with high enough chroma to discriminate
  // ~8 distinct eclasses, low enough not to read as decorative.
  function eclassColor(eclass: string | undefined): string {
    if (!eclass) return '#e4e4e7';
    let h = 0;
    for (let i = 0; i < eclass.length; i++) {
      h = (h * 31 + eclass.charCodeAt(i)) | 0;
    }
    const hue = Math.abs(h) % 360;
    return `hsl(${hue}, 35%, 88%)`;
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
    // Pull live design tokens from CSS so the egraph re-themes if the
    // page palette is ever swapped — single source of truth in app.css.
    const css = getComputedStyle(document.documentElement);
    const tokenAccent = css.getPropertyValue('--accent').trim() || '#1e40af';
    const tokenBorder = css.getPropertyValue('--border-2').trim() || '#d4d4d8';
    const tokenText = css.getPropertyValue('--text').trim() || '#09090b';
    const tokenEdge = css.getPropertyValue('--text-4').trim() || '#71717a';

    cy = cytoscape({
      container,
      style: [
        {
          selector: 'node',
          style: {
            'background-color': 'data(color)',
            'border-color': tokenBorder,
            'border-width': 1,
            label: 'data(label)',
            color: tokenText,
            'font-size': 10,
            'font-weight': 500,
            'text-valign': 'center',
            'text-halign': 'center',
            'text-margin-y': 0,
            'text-wrap': 'wrap',
            'text-max-width': '100px',
            width: 96,
            height: 36,
            shape: 'round-rectangle',
          } as never,
        },
        {
          selector: 'node[?isNew]',
          style: {
            // New-in-this-frame: bold accent border + a soft halo so it
            // pops without the gaudy gold glow used in dark mode.
            'border-color': tokenAccent,
            'border-width': 2,
            'overlay-color': tokenAccent,
            'overlay-padding': 6,
            'overlay-opacity': 0.18,
          } as never,
        },
        {
          selector: 'edge',
          style: {
            width: 1,
            'line-color': tokenEdge,
            'target-arrow-color': tokenEdge,
            'target-arrow-shape': 'triangle',
            'curve-style': 'bezier',
            'arrow-scale': 0.7,
            opacity: 0.6,
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
      <span class="hint">drag to scrub · blue ring = new in this frame</span>
    </div>
  </div>
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 460px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .viz {
    flex: 1;
    min-height: 360px;
    background: var(--bg-1);
  }
  .controls {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--s-3);
    padding: var(--s-3) var(--s-4);
    background: var(--bg-2);
    border-top: 1px solid var(--border);
  }
  .left {
    display: flex;
    gap: 2px;
  }
  .ctrl {
    width: 30px;
    height: 28px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-2);
    border-radius: var(--radius);
    font-size: var(--fs-label);
    cursor: pointer;
    display: grid;
    place-items: center;
    font-family: var(--font-sans);
    transition: border-color 120ms ease, color 120ms ease;
  }
  .ctrl.play {
    color: var(--accent);
  }
  .ctrl:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--text);
  }
  .ctrl:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .middle {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    min-width: 0;
  }
  input[type='range'] {
    width: 100%;
    accent-color: var(--accent);
  }
  .frame-label {
    display: flex;
    gap: var(--s-3);
    align-items: center;
    font-size: var(--fs-label);
    color: var(--text-2);
    min-height: 18px;
  }
  .counter {
    font-family: var(--font-mono);
    font-size: var(--fs-eyebrow);
    color: var(--text-3);
  }
  .label {
    font-family: var(--font-mono);
    font-size: var(--fs-label);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
  }
  .badge {
    padding: 2px 7px;
    border-radius: 3px;
    font-size: var(--fs-eyebrow);
    font-weight: var(--fw-semibold);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--accent-fg);
  }
  .right {
    text-align: right;
  }
  .hint {
    font-size: var(--fs-eyebrow);
    color: var(--text-4);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    font-weight: var(--fw-medium);
  }
</style>
