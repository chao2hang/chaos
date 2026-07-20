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
		rule: RuleNode,
		node_group: GroupNode,
		builtin: BuiltinNode
	};
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
		panOnScroll
		zoomOnScroll={false}
		zoomOnDoubleClick={false}
		oninit={onready}
		fitViewOptions={{ padding: 0.18, minZoom: 0.25, maxZoom: 1 }}
	>
		<Background variant={BackgroundVariant.Dots} gap={20} size={1} patternColor="#d4d4d4" />
		<Controls />
		<MiniMap pannable zoomable nodeColor={(node) => (node.type === 'builtin' ? '#d7d7d7' : '#ffffff')} />
	</SvelteFlow>
</div>

<style>
	.flow-canvas {
		position: relative;
		min-width: 0;
		height: 100%;
		min-height: 34rem;
		background: #fafafa;
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
		width: 8.5rem;
		height: 5.5rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: rgba(255, 255, 255, 0.94) !important;
		box-shadow: none;
	}

	.flow-canvas :global(.svelte-flow__attribution) {
		display: none;
	}

	@media (max-width: 760px) {
		.flow-canvas {
			height: 34rem;
		}

		.flow-canvas :global(.svelte-flow__minimap) {
			display: none;
		}
	}
</style>
