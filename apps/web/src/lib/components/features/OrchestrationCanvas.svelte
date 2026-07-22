<script lang="ts">
	import {
		Background,
		BackgroundVariant,
		Controls,
		SvelteFlow,
		type Connection,
		type Edge,
		type Node,
		type Viewport
	} from '@xyflow/svelte';
	import '@xyflow/svelte/dist/style.css';
	import { t } from '$lib/i18n.svelte';
	import type {
		OrchestrationEdgeDto,
		OrchestrationNodeDto
	} from '$lib/api';
	import { canConnect } from '$lib/orchestration';
	import { getResolvedTheme, readCssVar } from '$lib/theme.svelte';
	import RuleNode from '$lib/flow/RuleNode.svelte';
	import GroupNode from '$lib/flow/GroupNode.svelte';
	import BuiltinNode from '$lib/flow/BuiltinNode.svelte';
	import StartNode from '$lib/flow/StartNode.svelte';
	import EndNode from '$lib/flow/EndNode.svelte';
	import OrchestrationMiniMap from '$lib/components/features/OrchestrationMiniMap.svelte';

	let {
		nodes = $bindable(),
		edges = $bindable(),
		viewport = $bindable(),
		fitOnInit = true,
		selectedNodeId = $bindable(null),
		selectedEdgeId = $bindable(null),
		onaddnode,
		onconnectrequest,
		onselectnode,
		onready,
		onchange,
		onbeforechange
	}: {
		nodes: OrchestrationNodeDto[];
		edges: OrchestrationEdgeDto[];
		viewport: Viewport;
		fitOnInit?: boolean;
		selectedNodeId: string | null;
		selectedEdgeId: string | null;
		onaddnode: (kind: 'rule' | 'node_group', position: { x: number; y: number }) => void;
		onconnectrequest?: (connection: Connection) => void;
		onselectnode?: (nodeId: string) => void;
		onready?: () => void;
		onchange?: () => void;
		onbeforechange?: () => void;
	} = $props();

	const nodeTypes = {
		start: StartNode,
		end: EndNode,
		rule: RuleNode,
		node_group: GroupNode,
		builtin: BuiltinNode
	};

	const colorMode = $derived(getResolvedTheme());
	const patternColor = $derived(readCssVar('--flow-dot', colorMode === 'dark' ? '#3a3a3a' : '#d4d4d4'));
	// CSS variables in SVG style attributes resolve live when the theme tokens change.
	const connectionLineStyle = $derived('stroke: var(--ink); stroke-width: 1.5');

	function onBeforeConnect(connection: Connection) {
		if (!onconnectrequest) return connection;
		onconnectrequest(connection);
		return false;
	}

	function onDrop(event: DragEvent) {
		event.preventDefault();
		const kind = event.dataTransfer?.getData('application/chaos-flow-node');
		if (kind !== 'rule' && kind !== 'node_group') return;
		const element = event.currentTarget as HTMLDivElement;
		const bounds = element.getBoundingClientRect();
		const zoom = viewport.zoom || 1;
		onaddnode(kind, {
			x: (event.clientX - bounds.left - viewport.x) / zoom - (kind === 'rule' ? 116 : 108),
			y: (event.clientY - bounds.top - viewport.y) / zoom - 52
		});
	}

	function onSelectionChange(event: { nodes: Node[]; edges: Edge[] }) {
		selectedNodeId = event.nodes[0]?.id ?? null;
		selectedEdgeId = selectedNodeId ? null : (event.edges[0]?.id ?? null);
	}

	function onDelete() {
		selectedNodeId = null;
		selectedEdgeId = null;
		onchange?.();
	}
</script>

<div
	class="flow-canvas"
	role="application"
	aria-label={t('flow.canvas.title')}
	ondragover={(event) => {
		event.preventDefault();
		if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
	}}
	ondrop={onDrop}
>
	<SvelteFlow
		bind:nodes
		bind:edges
		bind:viewport
		{nodeTypes}
		{colorMode}
		minZoom={0.3}
		maxZoom={1.6}
		fitView={fitOnInit}
		snapGrid={[20, 20]}
		connectionRadius={28}
		connectionLineStyle={connectionLineStyle}
		defaultMarkerColor={null}
		defaultEdgeOptions={{
			type: 'smoothstep',
			style: 'stroke: var(--ink); stroke-width: 1.45'
		}}
		isValidConnection={(connection) => canConnect(connection, nodes, edges)}
		onbeforeconnect={onBeforeConnect}
		onselectionchange={onSelectionChange}
		onnodeclick={({ node }) => {
			selectedNodeId = node.id;
			selectedEdgeId = null;
			onselectnode?.(node.id);
		}}
		onedgeclick={({ edge }) => {
			selectedNodeId = null;
			selectedEdgeId = edge.id;
		}}
		onbeforedelete={async () => {
			onbeforechange?.();
			return true;
		}}
		ondelete={onDelete}
		onnodedragstart={() => onbeforechange?.()}
		onnodedragstop={() => onchange?.()}
		onpaneclick={() => {
			selectedNodeId = null;
			selectedEdgeId = null;
		}}
		deleteKey={['Backspace', 'Delete']}
		selectionKey="Shift"
		multiSelectionKey="Control"
		panOnDrag={true}
		panOnScroll={false}
		zoomOnScroll={true}
		zoomOnPinch={true}
		zoomOnDoubleClick={false}
		preventScrolling={true}
		oninit={onready}
		fitViewOptions={{ padding: 0.18, minZoom: 0.25, maxZoom: 1 }}
	>
		<Background variant={BackgroundVariant.Dots} gap={20} size={1} patternColor={patternColor} />
		<Controls position="bottom-left" showLock={false} />
		<OrchestrationMiniMap />
	</SvelteFlow>
</div>

<style>
	.flow-canvas {
		position: relative;
		min-width: 0;
		height: 100%;
		min-height: 34rem;
		background: var(--flow-canvas);
		/* Keep wheel events inside the canvas so the page does not scroll. */
		overscroll-behavior: contain;
	}

	.flow-canvas :global(.svelte-flow) {
		background: var(--flow-canvas);
		/* Keep xyflow defaults aligned with chaos tokens (especially dark mode). */
		--xy-background-color: var(--flow-canvas);
		--xy-background-pattern-dots-color: var(--flow-dot);
		--xy-edge-stroke: var(--ink);
		--xy-edge-stroke-default: var(--ink);
		--xy-edge-stroke-selected: var(--ink);
		--xy-edge-stroke-selected-default: var(--ink);
		--xy-edge-stroke-width: 1.45;
		--xy-connectionline-stroke: var(--ink);
		--xy-connectionline-stroke-default: var(--ink);
		--xy-minimap-background-color: var(--flow-minimap);
		--xy-minimap-node-background-color: var(--surface-subtle);
		--xy-minimap-node-stroke-color: var(--line-strong);
		--xy-controls-button-background-color: var(--surface);
		--xy-controls-button-background-color-hover: var(--surface-subtle);
		--xy-controls-button-color: var(--ink);
		--xy-controls-button-color-hover: var(--ink);
		--xy-controls-border-color: var(--line-strong);
	}

	.flow-canvas :global(.svelte-flow__node) {
		font-family: var(--font-ui);
	}

	.flow-canvas :global(.svelte-flow__node:focus-visible) {
		outline: none;
	}

	/* Inline edge style also sets stroke; keep class rules as a second layer. */
	.flow-canvas :global(.svelte-flow__edge-path),
	.flow-canvas :global(.svelte-flow__connection-path) {
		stroke: var(--ink) !important;
		stroke-width: 1.45;
	}

	.flow-canvas :global(.svelte-flow__edge.selected .svelte-flow__edge-path) {
		stroke-width: 2.25;
	}

	.flow-canvas :global(.svelte-flow__arrowhead polyline) {
		stroke: var(--ink) !important;
	}

	.flow-canvas :global(.svelte-flow__arrowhead polyline.arrowclosed) {
		fill: var(--ink) !important;
	}

	.flow-canvas :global(.svelte-flow__edge-textbg) {
		fill: var(--surface);
		stroke: var(--line-strong);
		stroke-width: 0.6;
	}

	.flow-canvas :global(.svelte-flow__controls) {
		margin: 0.75rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		box-shadow: none;
		overflow: hidden;
	}

	.flow-canvas :global(.svelte-flow__controls-button) {
		border-bottom-color: var(--line);
		background: var(--surface);
		color: var(--ink);
	}

	.flow-canvas :global(.svelte-flow__controls-button:hover) {
		background: var(--surface-subtle);
	}

	.flow-canvas :global(.svelte-flow__minimap) {
		margin: 0.75rem;
		overflow: hidden;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--flow-minimap) !important;
		box-shadow: none;
	}

	.flow-canvas :global(.svelte-flow__minimap-mask) {
		fill: color-mix(in srgb, var(--ink) 12%, transparent);
		stroke: var(--ink);
	}

	.flow-canvas :global(.svelte-flow__minimap-node) {
		stroke: var(--line-strong);
	}

	.flow-canvas :global(.svelte-flow__attribution) {
		display: none;
	}

	@media (max-width: 760px) {
		.flow-canvas {
			height: 34rem;
		}

		.flow-canvas :global(.svelte-flow__minimap) {
			width: 7.5rem !important;
			height: 5rem !important;
		}
	}
</style>
