<script lang="ts">
	import { MiniMap, useSvelteFlow, type Node } from '@xyflow/svelte';
	import { t } from '$lib/i18n.svelte';
	import { getResolvedTheme, readCssVar } from '$lib/theme.svelte';

	const { setCenter, getViewport } = useSvelteFlow();

	let pointerDown: { x: number; y: number } | null = null;

	const colorMode = $derived(getResolvedTheme());
	const ink = $derived(readCssVar('--ink', colorMode === 'dark' ? '#f0f0f0' : '#111111'));
	const lineStrong = $derived(
		readCssVar('--line-strong', colorMode === 'dark' ? '#555555' : '#a8a8a8')
	);
	const surface = $derived(
		readCssVar('--surface', colorMode === 'dark' ? '#141414' : '#ffffff')
	);
	const surfaceSubtle = $derived(
		readCssVar('--surface-subtle', colorMode === 'dark' ? '#1c1c1c' : '#f5f5f5')
	);
	const surfaceInverse = $derived(
		readCssVar('--surface-inverse', colorMode === 'dark' ? '#f0f0f0' : '#111111')
	);
	const inkMuted = $derived(
		readCssVar('--ink-muted', colorMode === 'dark' ? '#a8a8a8' : '#5f5f5f')
	);
	const minimapBg = $derived(
		readCssVar('--flow-minimap', colorMode === 'dark' ? '#161616' : '#f7f7f7')
	);
	const maskColor = $derived(
		colorMode === 'dark' ? 'rgba(0, 0, 0, 0.45)' : 'rgba(17, 17, 17, 0.1)'
	);

	function minimapNodeColor(node: Node): string {
		switch (node.type) {
			case 'start':
				return surfaceInverse;
			case 'end':
				return inkMuted;
			case 'rule':
				return surface;
			case 'node_group':
				return surface;
			case 'builtin':
				return surfaceSubtle;
			default:
				return surfaceSubtle;
		}
	}

	function minimapNodeStroke(node: Node): string {
		return node.selected ? ink : lineStrong;
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
	maskColor={maskColor}
	maskStrokeColor={ink}
	maskStrokeWidth={1.25}
	bgColor={minimapBg}
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
