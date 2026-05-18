<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import cytoscape, { type Core, type ElementDefinition } from 'cytoscape';
  // @ts-expect-error — cytoscape-dagre has no shipped types and the stub is deprecated.
  import dagre from 'cytoscape-dagre';
  import type { GraphView } from './api';

  cytoscape.use(dagre);

  let { graph }: { graph: GraphView | null } = $props();
  let container: HTMLDivElement;
  let cy: Core | null = null;

  const colorFor = (c: string) => {
    switch (c) {
      case 'accepted':
        return '#16a34a'; // green-600
      case 'rejected':
        return '#dc2626'; // red-600
      case 'gold':
        return '#d4a017'; // muted gold (Notion-y)
      default:
        return '#94a3b8'; // slate-400
    }
  };

  const widthFor = (c: string) => (c === 'neutral' ? 1.5 : 3);

  function toElements(view: GraphView): ElementDefinition[] {
    const els: ElementDefinition[] = [];
    for (const n of view.nodes) {
      els.push({
        data: { id: n.id, label: n.label, kind: n.kind, color: n.color },
      });
    }
    for (const e of view.edges) {
      els.push({
        data: {
          id: e.id,
          source: e.source,
          target: e.target,
          color: e.color,
          note: e.note ?? '',
        },
      });
    }
    return els;
  }

  function render(view: GraphView) {
    if (!container) return;
    cy?.destroy();
    cy = cytoscape({
      container,
      elements: toElements(view),
      layout: { name: 'dagre', rankDir: 'LR', nodeSep: 35, rankSep: 95 } as never,
      style: [
        {
          selector: 'node',
          style: {
            'background-color': '#1e293b',
            'border-color': (ele: cytoscape.NodeSingular) => colorFor(ele.data('color')),
            'border-width': 2,
            label: 'data(label)',
            color: '#e2e8f0',
            'font-size': 11,
            'text-valign': 'center',
            'text-halign': 'center',
            'text-margin-y': 0,
            'text-outline-color': '#0f172a',
            'text-outline-width': 1.5,
            'text-wrap': 'wrap',
            'text-max-width': '160px',
            padding: '6px',
            width: 'label',
            height: 'label',
            shape: (ele: cytoscape.NodeSingular) => {
              const k = ele.data('kind');
              if (k === 'goal') return 'diamond';
              if (k === 'user') return 'round-rectangle';
              if (k === 'role') return 'ellipse';
              return 'round-tag';
            },
          } as never,
        },
        {
          selector: 'node[kind = "goal"]',
          style: {
            'background-color': '#3b2f0b',
            'border-color': '#d4a017',
            color: '#f5e7b8',
            'font-weight': 600,
          } as never,
        },
        {
          selector: 'edge',
          style: {
            width: (ele: cytoscape.EdgeSingular) => widthFor(ele.data('color')),
            'line-color': (ele: cytoscape.EdgeSingular) => colorFor(ele.data('color')),
            'target-arrow-color': (ele: cytoscape.EdgeSingular) => colorFor(ele.data('color')),
            'target-arrow-shape': 'triangle',
            'curve-style': 'bezier',
            opacity: 0.95,
          } as never,
        },
      ],
    });
  }

  $effect(() => {
    if (graph) render(graph);
  });

  onMount(() => {
    if (graph) render(graph);
  });

  onDestroy(() => {
    cy?.destroy();
    cy = null;
  });
</script>

<div class="graph" bind:this={container}></div>

<style>
  .graph {
    width: 100%;
    height: 100%;
    min-height: 360px;
    background: #0b1220;
    border-radius: 8px;
  }
</style>
