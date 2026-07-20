<script lang="ts">
	import { ArrowDownRight, ArrowUpRight, Minus } from '@lucide/svelte';

	let {
		label,
		value,
		note,
		tone = 'neutral'
	} = $props<{
		label: string;
		value: string | number;
		note?: string;
		tone?: 'positive' | 'negative' | 'neutral';
	}>();

	const Icon = $derived(tone === 'positive' ? ArrowUpRight : tone === 'negative' ? ArrowDownRight : Minus);
</script>

<div class="metric">
	<div class="metric-head">
		<span>{label}</span>
		<Icon size={14} strokeWidth={1.8} aria-hidden="true" />
	</div>
	<strong>{value}</strong>
	{#if note}<p>{note}</p>{/if}
</div>

<style>
	.metric {
		min-width: 0;
		padding: var(--space-4) var(--space-5);
		border-right: 1px solid var(--line);
	}

	.metric:last-child {
		border-right: 0;
	}

	.metric-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		color: var(--ink-muted);
		font-size: 0.7rem;
		font-weight: 650;
		text-transform: uppercase;
	}

	strong {
		display: block;
		margin-top: var(--space-3);
		font-size: 1.55rem;
		font-weight: 720;
		line-height: 1;
		font-variant-numeric: tabular-nums;
	}

	p {
		margin: var(--space-2) 0 0;
		color: var(--ink-faint);
		font-size: 0.7rem;
	}

	@media (max-width: 680px) {
		.metric {
			border-right: 0;
			border-bottom: 1px solid var(--line);
		}

		.metric:last-child {
			border-bottom: 0;
		}
	}
</style>
