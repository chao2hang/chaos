<script lang="ts">
	import { LogIn } from '@lucide/svelte';
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';
	import type { OrchestrationAnchorData } from '$lib/api';
	import { t } from '$lib/i18n.svelte';

	let { data, selected = false }: NodeProps = $props();
	const details = $derived(data as unknown as OrchestrationAnchorData);
</script>

<div class="anchor-node" class:selected>
	<LogIn size={16} strokeWidth={1.8} aria-hidden="true" />
	<div><span>{t('flow.start')}</span><strong>{t('flow.start.hint')}</strong><small>{t('flow.ruleCount', { count: details.route_count ?? 0 })}</small></div>
	<Handle type="source" position={Position.Right} id="out" />
</div>

<style>
	.anchor-node { position: relative; display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: .55rem; width: 10.5rem; min-height: 4.5rem; padding: .65rem .75rem; border: 1px solid var(--ink); border-radius: var(--radius-md); background: var(--surface-inverse); color: var(--ink-inverse); }
	.anchor-node.selected { box-shadow: 0 0 0 3px rgba(17, 17, 17, .18); }
	span, strong, small { display: block; }
	span, small { font-family: var(--font-mono); font-size: .55rem; }
	span { font-weight: 700; text-transform: uppercase; }
	strong { margin-top: .08rem; font-size: .7rem; }
	small { margin-top: .2rem; opacity: .7; }
	.anchor-node :global(.svelte-flow__handle) { width: .62rem; height: .62rem; border: 2px solid var(--surface); background: var(--ink); }
</style>
