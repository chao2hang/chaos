<script lang="ts">
	import { MiniMap, useSvelteFlow, type Node } from '@xyflow/svelte';
	import { t } from '$lib/i18n.svelte';

	const { setCenter, getViewport } = useSvelteFlow();

	let pointerDown: { x: number; y: number } | null = null;

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

	function onPointerDown(event: PointerEvent) {
		if (event.button !== 0) return;
		pointerDown = { x: event.clientX, y: event.clientY };
	}

	function onPointerUp(event: PointerEvent) {
		if (event.button !== 0 || !pointerDown) return;
		const dx = event.clientX - pointerDown.x;
		const dy = event.clientY - pointerDown.y;
		pointerDown = null;
		// Small movement = click-to-jump; larger movement keeps native minimap pan.
		if (Math.hypot(dx, dy) > 4) return;

		const target = event.currentTarget as HTMLElement;
		const svg = target.querySelector('svg.svelte-flow__minimap-svg') as SVGSVGElement | null;
		if (!svg) return;

		const ctm = svg.getScreenCTM();
		if (!ctm) return;

		const point = svg.createSVGPoint();
		point.x = event.clientX;
		point.y = event.clientY;
		const flowPoint = point.matrixTransform(ctm.inverse());
		const { zoom } = getViewport();
		void setCenter(flowPoint.x, flowPoint.y, { zoom, duration: 220 });
	}

	function onPointerCancel() {
		pointerDown = null;
	}
</script>

<MiniMap
	position="bottom-right"
	width={168}
	height={112}
	class="orchestration-minimap"
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
	onpointerdown={onPointerDown}
	onpointerup={onPointerUp}
	onpointercancel={onPointerCancel}
	onpointerleave={onPointerCancel}
/>

<style>
	:global(.svelte-flow__minimap.orchestration-minimap) {
		cursor: crosshair;
	}
</style>
