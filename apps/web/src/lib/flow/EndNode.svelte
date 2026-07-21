<script lang="ts">
	import { LogOut } from '@lucide/svelte';
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';
	import type { OrchestrationEndData } from '$lib/api';
	import { t } from '$lib/i18n.svelte';

	let { data, selected = false }: NodeProps = $props();
	const details = $derived(data as unknown as OrchestrationEndData);
</script>

<div class="anchor-node" class:selected>
	<LogOut size={16} strokeWidth={1.8} aria-hidden="true" />
	<div><span>{t('flow.end')}</span><strong>{details.target_name || t('flow.fallback')}</strong><small>{t('flow.end.hint')}</small></div>
	<Handle type="source" position={Position.Right} id="out" />
</div>

<style>
	.anchor-node { position: relative; display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: .55rem; width: 11.5rem; min-height: 4.5rem; padding: .65rem .75rem; border: 1px solid var(--ink); border-radius: var(--radius-md); background: var(--surface); color: var(--ink); }
	.anchor-node.selected { box-shadow: 0 0 0 3px rgba(17, 17, 17, .18); }
	span, strong, small { display: block; }
	span, small { font-family: var(--font-mono); font-size: .55rem; }
	span { font-weight: 700; text-transform: uppercase; }
	strong { overflow: hidden; margin-top: .08rem; font-size: .7rem; text-overflow: ellipsis; white-space: nowrap; }
	small { margin-top: .2rem; color: var(--ink-faint); }
	.anchor-node :global(.svelte-flow__handle) { width: .62rem; height: .62rem; border: 2px solid var(--surface); background: var(--ink); }
</style>
