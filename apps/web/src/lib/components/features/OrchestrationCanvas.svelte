<script lang="ts">
	import {
		Background,
		BackgroundVariant,
		Controls,
		MiniMap,
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
	import RuleNode from '$lib/flow/RuleNode.svelte';
	import GroupNode from '$lib/flow/GroupNode.svelte';
	import BuiltinNode from '$lib/flow/BuiltinNode.svelte';
	import StartNode from '$lib/flow/StartNode.svelte';
	import EndNode from '$lib/flow/EndNode.svelte';

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

	function minimapNodeColor(node: Node): string {
		switch (node.type) {
			case 'start':
				return '#111111';
			case 'end':
				return '#4a4a4a';
			case 'rule':
				return '#f0f0f0';
			case 'node_group':
				return '#ffffff';
			case 'builtin':
				return '#d0d0d0';
			default:
				return '#e8e8e8';
		}
	}

	function minimapNodeStroke(node: Node): string {
		return node.selected ? '#111111' : '#8a8a8a';
	}

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
		colorMode="light"
		minZoom={0.3}
		maxZoom={1.6}
		fitView={fitOnInit}
		snapGrid={[20, 20]}
		connectionRadius={28}
		connectionLineStyle="stroke: #111111; stroke-width: 1.4"
		defaultEdgeOptions={{ type: 'smoothstep' }}
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
		<Background variant={BackgroundVariant.Dots} gap={20} size={1} patternColor="#d4d4d4" />
		<Controls position="bottom-left" showLock={false} />
		<MiniMap
			position="bottom-right"
			width={168}
			height={112}
			pannable
			zoomable={false}
			inversePan={false}
			ariaLabel={t('flow.canvas.minimap')}
			nodeColor={minimapNodeColor}
			nodeStrokeColor={minimapNodeStroke}
			nodeStrokeWidth={2}
			nodeBorderRadius={2}
			maskColor="rgba(17, 17, 17, 0.1)"
			maskStrokeColor="#111111"
			maskStrokeWidth={1.25}
			bgColor="#f7f7f7"
		/>
	</SvelteFlow>
</div>

<style>
	.flow-canvas {
		position: relative;
		min-width: 0;
		height: 100%;
		min-height: 34rem;
		background: #fafafa;
		/* Keep wheel events inside the canvas so the page does not scroll. */
		overscroll-behavior: contain;
	}

	.flow-canvas :global(.svelte-flow) {
		background: #fafafa;
	}

	.flow-canvas :global(.svelte-flow__node) {
		font-family: var(--font-ui);
	}

	.flow-canvas :global(.svelte-flow__node:focus-visible) {
		outline: none;
	}

	.flow-canvas :global(.svelte-flow__edge-path) {
		stroke: var(--ink);
		stroke-width: 1.35;
	}

	.flow-canvas :global(.svelte-flow__edge.selected .svelte-flow__edge-path) {
		stroke-width: 2.25;
	}

	.flow-canvas :global(.svelte-flow__edge-textbg) {
		fill: #ffffff;
		stroke: #c8c8c8;
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
		background: #f7f7f7 !important;
		box-shadow: none;
	}

	.flow-canvas :global(.svelte-flow__minimap-mask) {
		fill: rgba(17, 17, 17, 0.1);
		stroke: #111111;
	}

	.flow-canvas :global(.svelte-flow__minimap-node) {
		stroke: #8a8a8a;
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
